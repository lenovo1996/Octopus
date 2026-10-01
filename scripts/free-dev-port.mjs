#!/usr/bin/env node
/* global console: readonly, setTimeout: readonly */
// Frees a TCP port (default 1420) before the Vite dev server starts.
// Linux-first: resolves LISTEN owners via /proc, falls back to fuser/lsof.
// Always exits 0 when the port ends up free; exits 1 only if it stays busy.
import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import net from "node:net";
import process from "node:process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const LISTEN_STATE = "0A";
const TERM_GRACE_MS = 3000;
const KILL_GRACE_MS = 2000;
const POLL_MS = 100;

function parsePort(argv) {
  const raw = argv[2] ?? "1420";
  const port = Number.parseInt(raw, 10);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    console.error(`free-dev-port: invalid port "${raw}"`);
    process.exit(1);
  }
  return port;
}

async function isPortFree(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.listen(port, () => {
      server.close(() => resolve(true));
    });
  });
}

async function waitForFree(port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    if (await isPortFree(port)) return true;
    if (Date.now() >= deadline) return await isPortFree(port);
    await new Promise((r) => setTimeout(r, POLL_MS));
  }
}

// Collect socket inodes in LISTEN state on the given port from /proc/net/tcp*.
async function listenInodes(port) {
  const inodes = new Set();
  const hexPort = port.toString(16).toUpperCase().padStart(4, "0");
  for (const file of ["/proc/net/tcp", "/proc/net/tcp6"]) {
    let text;
    try {
      text = await fs.readFile(file, "utf8");
    } catch {
      return null; // No /proc (non-Linux): caller tries external tools.
    }
    for (const line of text.split("\n").slice(1)) {
      const cols = line.trim().split(/\s+/);
      if (cols.length < 10) continue;
      const local = cols[1] ?? "";
      const state = cols[3] ?? "";
      const inode = cols[9] ?? "";
      if (state !== LISTEN_STATE || inode === "0") continue;
      const sep = local.lastIndexOf(":");
      if (sep === -1) continue;
      if (local.slice(sep + 1).toUpperCase() !== hexPort) continue;
      inodes.add(inode);
    }
  }
  return inodes;
}

// Map socket inodes to owner PIDs by scanning /proc/<pid>/fd symlinks.
async function pidsForInodes(inodes) {
  const pids = new Set();
  if (inodes.size === 0) return pids;
  let entries;
  try {
    entries = await fs.readdir("/proc");
  } catch {
    return pids;
  }
  await Promise.all(
    entries
      .filter((name) => /^\d+$/.test(name))
      .map(Number)
      .filter((pid) => pid !== process.pid)
      .map(async (pid) => {
        let fds;
        try {
          fds = await fs.readdir(`/proc/${pid}/fd`);
        } catch {
          return; // Exited or permission denied.
        }
        for (const fd of fds) {
          let link;
          try {
            link = await fs.readlink(`/proc/${pid}/fd/${fd}`);
          } catch {
            continue;
          }
          const match = /^socket:\[(\d+)\]$/.exec(link);
          if (match && inodes.has(match[1])) {
            pids.add(pid);
            return;
          }
        }
      }),
  );
  return pids;
}

function killPid(pid, signal) {
  try {
    process.kill(pid, signal);
    return true;
  } catch {
    return false; // Exited already or permission denied.
  }
}

async function killViaProc(port) {
  const inodes = await listenInodes(port);
  if (inodes === null) return false; // No /proc: let the caller try fallbacks.
  const pids = await pidsForInodes(inodes);
  if (pids.size === 0) return true; // Nothing we own to kill; recheck decides.
  for (const pid of pids) killPid(pid, "SIGTERM");
  if (await waitForFree(port, TERM_GRACE_MS)) {
    console.log(`free-dev-port: freed TCP ${port} (SIGTERM ${[...pids].join(", ")})`);
    return true;
  }
  for (const pid of pids) killPid(pid, "SIGKILL");
  if (await waitForFree(port, KILL_GRACE_MS)) {
    console.log(`free-dev-port: freed TCP ${port} (SIGKILL ${[...pids].join(", ")})`);
    return true;
  }
  return true; // Recheck below reports the final state.
}

async function killViaFallbacks(port) {
  // fuser (Linux, psmisc) then lsof (macOS/Linux): best effort, ignore errors.
  try {
    await execFileAsync("fuser", ["-k", "-TERM", `${port}/tcp`]);
  } catch {
    // Either no match, no fuser, or kill failed; keep going.
  }
  if (await waitForFree(port, TERM_GRACE_MS)) return;
  try {
    const { stdout } = await execFileAsync("lsof", ["-ti", `tcp:${port}`]);
    const pids = stdout
      .split(/\s+/)
      .map(Number)
      .filter((pid) => Number.isInteger(pid) && pid > 0 && pid !== process.pid);
    for (const pid of pids) killPid(pid, "SIGTERM");
  } catch {
    // No lsof or no match.
  }
  if (await waitForFree(port, TERM_GRACE_MS)) return;
  try {
    const { stdout } = await execFileAsync("lsof", ["-ti", `tcp:${port}`]);
    const pids = stdout
      .split(/\s+/)
      .map(Number)
      .filter((pid) => Number.isInteger(pid) && pid > 0 && pid !== process.pid);
    for (const pid of pids) killPid(pid, "SIGKILL");
  } catch {
    // Nothing left to try.
  }
  await waitForFree(port, KILL_GRACE_MS);
}

const port = parsePort(process.argv);
if (await isPortFree(port)) {
  console.log(`free-dev-port: TCP ${port} is already free`);
  process.exit(0);
}
await killViaProc(port);
if (!(await isPortFree(port))) {
  await killViaFallbacks(port);
}
if (await isPortFree(port)) {
  console.log(`free-dev-port: TCP ${port} is free`);
  process.exit(0);
}
console.error(`free-dev-port: TCP ${port} is still busy; stop the holding process and retry`);
process.exit(1);
