import { describe, expect, it, vi } from "vitest";
import { DiffController, emptyDiffState, type DiffState } from "../../src/lib/diff/controller";
import type { DiffDocument, DiffTarget } from "../../src/lib/ipc/types";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

const document = (path: string): DiffDocument => ({ kind: "text", displayPath: path, additions: 0, deletions: 0, hunks: [], truncated: false });
const selection = (path: string, kind: "worktree" | "index" = "worktree", repoId = "repo") => ({ repoId, path, target: { kind, pathId: `token:${path}` } satisfies DiffTarget });

describe("main panel diff lifecycle", () => {
  it("uses the exact source/token and clears the previous content while loading", async () => {
    const second = deferred<DiffDocument>();
    const read = vi.fn().mockResolvedValueOnce(document("file")).mockReturnValueOnce(second.promise);
    let state: DiffState = emptyDiffState();
    const controller = new DiffController(read, (next) => { state = next; });
    await controller.open(selection("file"));
    const pending = controller.open(selection("file", "index"));
    expect(state.doc).toBeNull();
    expect(state.loading).toBe(true);
    expect(read).toHaveBeenLastCalledWith("repo", { kind: "index", pathId: "token:file" });
    second.resolve(document("staged"));
    await pending;
    expect(state.doc?.displayPath).toBe("staged");
  });

  it.each(["success", "failure"])("ignores stale %s after another file opens", async (result) => {
    const first = deferred<DiffDocument>();
    const second = deferred<DiffDocument>();
    const read = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    let state: DiffState = emptyDiffState();
    const controller = new DiffController(read, (next) => { state = next; });
    const a = controller.open(selection("a"));
    const b = controller.open(selection("b"));
    if (result === "success") first.resolve(document("a"));
    else first.reject({ code: "GIT_ERROR", message: "old failure" });
    await a;
    expect(state.loading).toBe(true);
    expect(state.error).toBeNull();
    second.resolve(document("b"));
    await b;
    expect(state.doc?.displayPath).toBe("b");
  });

  it("does not reopen when a read finishes after close or a repository switch", async () => {
    const old = deferred<DiffDocument>();
    let state: DiffState = emptyDiffState();
    const read = vi.fn().mockReturnValueOnce(old.promise).mockResolvedValueOnce(document("new"));
    const controller = new DiffController(read, (next) => { state = next; });
    const pending = controller.open(selection("file"));
    controller.close();
    expect(state).toEqual(emptyDiffState());
    await controller.open(selection("new", "worktree", "other-repo"));
    old.resolve(document("old"));
    await pending;
    expect(state.selection?.repoId).toBe("other-repo");
    expect(state.doc?.displayPath).toBe("new");
  });

  it("retries the selected commit comparison after an error", async () => {
    const error = { code: "GIT_ERROR", message: "Read failed" };
    const read = vi.fn().mockRejectedValueOnce(error).mockResolvedValueOnce(document("file"));
    let state: DiffState = emptyDiffState();
    const controller = new DiffController(read, (next) => { state = next; });
    const chosen = { repoId: "repo", path: "file", target: { kind: "commit", oid: "merge", parentIndex: 1, pathId: "exact-token" } satisfies DiffTarget };
    await controller.open(chosen);
    expect(state.error).toEqual(error);
    await controller.retry();
    expect(read).toHaveBeenLastCalledWith("repo", chosen.target);
    expect(state.error).toBeNull();
    expect(state.doc?.displayPath).toBe("file");
    controller.close();
    await controller.retry();
    expect(read).toHaveBeenCalledTimes(2);
  });
});
