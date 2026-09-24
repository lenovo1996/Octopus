import { describe, expect, it } from "vitest";
import { createMockAdapter } from "../../src/lib/ipc/mock";
import { demoSession } from "../../src/mocks/demoSession";

describe("independent repository workspaces", () => {
  it("keeps file mutations, branch selection and snapshot identity in their own repository", async () => {
    const first = createMockAdapter({...demoSession, repoId:"first", workspaceKey:"worktree:first", displayName:"first"});
    const second = createMockAdapter({...demoSession, repoId:"second", workspaceKey:"worktree:second", displayName:"second"});
    const firstFiles = await first.repoStatus("first");
    const secondFiles = await second.repoStatus("second");
    const file = firstFiles.files.find(file=>file.worktreeStatus==="?")!;
    const changed = await first.indexStage("first",1,[file.pathId]);
    expect(changed.repoId).toBe("first");
    expect(changed.workspaceKey).toBe("worktree:first");
    expect(await second.repoStatus("second")).toEqual(secondFiles);
    await first.branchSwitch("first",1,"refs/heads/feature/ui");
    expect((await first.repoSnapshot()).head).toMatchObject({name:"feature/ui"});
    expect((await second.repoSnapshot()).head).toMatchObject({name:"main"});
    await first.repoClose();
    expect((await second.repoOpen("second")).repoId).toBe("second");
  });
  it("rejects a file token from another repository before changing its index", async () => {
    const first = createMockAdapter({...demoSession,repoId:"first"});
    const second = createMockAdapter({...demoSession,repoId:"second"});
    const firstFiles = await first.repoStatus("first");
    const before = await second.repoStatus("second");
    await expect(second.indexStage("second",1,[firstFiles.files[0].pathId])).rejects.toMatchObject({code:"STALE_STATE"});
    expect(await second.repoStatus("second")).toEqual(before);
  });
});
