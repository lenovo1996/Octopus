import { describe, expect, it } from "vitest";
import { COMMIT_ACTION_FORMS, historyActionToast, validateCommitForm } from "../../src/lib/history/commit-action-forms";
import { COMMIT_ACTIONS, type CommitActionId } from "../../src/lib/history/commit-menu";

describe("commit action forms", () => {
  it("covers every menu action", () => {
    const menuIds = COMMIT_ACTIONS.map((item) => item.id).sort();
    expect(Object.keys(COMMIT_ACTION_FORMS).sort()).toEqual(menuIds);
  });

  it("requires tokens for destructive rewrites and typing for hard reset", () => {
    expect(COMMIT_ACTION_FORMS["reset-hard"].confirmAction).toBe("history_reset_hard");
    expect(COMMIT_ACTION_FORMS["reset-hard"].typeToConfirm).toBe(true);
    expect(COMMIT_ACTION_FORMS.rebase.confirmAction).toBe("history_rebase");
    expect(COMMIT_ACTION_FORMS.split.confirmAction).toBe("history_split");
    expect(COMMIT_ACTION_FORMS["interactive-rebase"].confirmAction).toBe(
      "history_rebase_interactive"
    );
    expect(COMMIT_ACTION_FORMS["reset-soft"].confirmAction).toBeUndefined();
  });

  it("marks the HEAD-scoped amend family", () => {
    for (const id of ["reword", "modify", "edit-author", "split"] as CommitActionId[]) {
      expect(COMMIT_ACTION_FORMS[id].headOnly).toBe(true);
    }
    expect(COMMIT_ACTION_FORMS["interactive-rebase"].headOnly).toBeUndefined();
  });

  it("validates required fields and author email", () => {
    expect(validateCommitForm(COMMIT_ACTION_FORMS["create-tag"], { name: "" })).toContain("Tag name");
    expect(validateCommitForm(COMMIT_ACTION_FORMS["create-tag"], { name: "v1" })).toBeNull();
    expect(
      validateCommitForm(COMMIT_ACTION_FORMS["edit-author"], { name: "A", email: "nope" })
    ).toContain("@");
    expect(
      validateCommitForm(COMMIT_ACTION_FORMS["edit-author"], { name: "A", email: "a@x" })
    ).toBeNull();
    expect(validateCommitForm(COMMIT_ACTION_FORMS.reword, { subject: "", body: "" })).toContain(
      "Subject"
    );
  });

  it("toasts the submitted history action with a short oid", () => {
    const oid = "abcdef1234567890";
    expect(historyActionToast("cherry-pick", oid, {}, 0)).toBe("Cherry-picked abcdef1");
    expect(historyActionToast("reset-hard", oid, {}, 0)).toBe("Hard reset to abcdef1");
    expect(historyActionToast("create-tag", oid, { name: " v1 " }, 0)).toBe("Created tag v1");
    expect(historyActionToast("push-to", oid, { remote: "origin", destBranch: "main" }, 0)).toBe(
      "Pushed abcdef1 to origin/main"
    );
    expect(historyActionToast("interactive-rebase", oid, {}, 1)).toBe("Rebased 1 commit");
    expect(historyActionToast("interactive-rebase", oid, {}, 3)).toBe("Rebased 3 commits");
    expect(historyActionToast("move-to-branch", oid, { name: "feat" }, 0)).toBe(
      "Moved abcdef1 to feat"
    );
  });
});
