import { describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { realAdapter } from "../../src/lib/ipc/real";
import { createMockAdapter } from "../../src/lib/ipc/mock";
import { demoSession } from "../../src/mocks/demoSession";
import { aliasValidationError, repositoryTabName } from "../../src/lib/repositories/alias";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("repository aliases", () => {
  it("keeps aliases separate from Git identity and other worktrees, including reopened sessions", async () => {
    const snapshot = { ...demoSession, repoId: "alias-first", workspaceKey: "alias:first" };
    const first = createMockAdapter(snapshot);
    const second = createMockAdapter({ ...snapshot, repoId: "alias-linked", workspaceKey: "alias:linked" });
    expect(await first.repoAliasSet(snapshot.repoId, "  API Việt Nam 🐙  ")).toEqual({ workspaceKey: snapshot.workspaceKey, alias: "API Việt Nam 🐙" });
    expect((await first.repoSnapshot()).displayName).toBe(snapshot.displayName);
    expect((await second.repoAliasGet("alias-linked")).alias).toBeNull();
    const reopened = createMockAdapter({ ...snapshot, repoId: "new-session" });
    expect((await reopened.repoAliasGet("new-session")).alias).toBe("API Việt Nam 🐙");
    expect((await reopened.repoAliasSet("new-session", "   ")).alias).toBeNull();
    expect(repositoryTabName({ snapshot, alias: null })).toBe(snapshot.displayName);
    expect(repositoryTabName({ snapshot, alias: "API" })).toBe("API");
  });

  it("validates code points and rejects controls without replacing a saved alias", async () => {
    expect(aliasValidationError("🐙".repeat(80))).toBeNull();
    for (const value of ["a".repeat(81), "x\ny", "x\0y", "x\u007fy", "x\u2028y"]) expect(aliasValidationError(value)).not.toBeNull();
    const adapter = createMockAdapter({ ...demoSession, workspaceKey: "alias:invalid" });
    await adapter.repoAliasSet(demoSession.repoId, "Original");
    await expect(adapter.repoAliasSet(demoSession.repoId, "bad\nname")).rejects.toMatchObject({code: "INVALID_ARGUMENT"});
    expect((await adapter.repoAliasGet(demoSession.repoId)).alias).toBe("Original");
  });

  it("uses typed commands bound to a repository session for reads, writes and reset", async () => {
    vi.mocked(invoke).mockImplementation(async (_command, args) => {
      const { request } = args as { request: { requestId: string } };
      return { ok: true, requestId: request.requestId, data: { workspaceKey: "worktree-v1:61", alias: "API" } };
    });
    await realAdapter.repoAliasGet("repo-1");
    expect(invoke).toHaveBeenLastCalledWith("repo_alias_get", { request: { requestId: expect.any(String), repoId: "repo-1" } });
    for (const alias of ["API", null]) {
      await realAdapter.repoAliasSet("repo-1", alias);
      expect(invoke).toHaveBeenLastCalledWith("repo_alias_set", { request: { requestId: expect.any(String), repoId: "repo-1", alias } });
    }
  });
});
