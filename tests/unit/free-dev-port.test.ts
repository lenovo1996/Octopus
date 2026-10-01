import { execFile, spawn } from "node:child_process";
import net from "node:net";
import path from "node:path";
import process from "node:process";
import { promisify } from "node:util";
import { afterEach, describe, expect, it } from "vitest";

const execFileAsync = promisify(execFile);
const SCRIPT = path.resolve("scripts/free-dev-port.mjs");
const children: ReturnType<typeof spawn>[] = [];

afterEach(() => {
  for (const child of children.splice(0)) {
    try {
      child.kill("SIGKILL");
    } catch {
      // Already exited.
    }
  }
});

async function pickFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => {
        if (address && typeof address === "object") resolve(address.port);
        else reject(new Error("no port assigned"));
      });
    });
  });
}

async function waitForPort(port: number, wantFree: boolean, timeoutMs = 5000): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const free = await new Promise<boolean>((resolve) => {
      const probe = net.createServer();
      probe.once("error", () => resolve(false));
      probe.listen(port, "127.0.0.1", () => {
        probe.close(() => resolve(true));
      });
    });
    if (free === wantFree) return true;
    if (Date.now() >= deadline) return false;
    await new Promise((r) => setTimeout(r, 50));
  }
}

function spawnListener(port: number) {
  const child = spawn(
    process.execPath,
    ["-e", `require("node:net").createServer().listen(${port}, "127.0.0.1");`],
    { stdio: "ignore" },
  );
  children.push(child);
  return child;
}

describe("free-dev-port.mjs", () => {
  it(
    "kills the listener on a busy temp port and exits 0",
    async () => {
      const port = await pickFreePort();
      expect(port).not.toBe(1420);
      const child = spawnListener(port);
      expect(await waitForPort(port, false)).toBe(true);

      const { stdout } = await execFileAsync(process.execPath, [SCRIPT, String(port)], {
        timeout: 15000,
      });
      expect(stdout).toContain(`TCP ${port}`);

      const exited = await new Promise<boolean>((resolve) => {
        if (child.exitCode !== null || child.signalCode !== null) {
          resolve(true);
          return;
        }
        child.once("exit", () => resolve(true));
        setTimeout(() => resolve(false), 5000);
      });
      expect(exited).toBe(true);
      expect(await waitForPort(port, true)).toBe(true);
    },
    20000,
  );

  it("exits 0 when the port is already free", async () => {
    const port = await pickFreePort();
    const { stdout } = await execFileAsync(process.execPath, [SCRIPT, String(port)], {
      timeout: 15000,
    });
    expect(stdout).toContain("already free");
  });

  it("exits 1 for an invalid port", async () => {
    await expect(
      execFileAsync(process.execPath, [SCRIPT, "not-a-port"], { timeout: 15000 }),
    ).rejects.toThrow();
  });
});
