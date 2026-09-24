import { describe, expect, it } from "vitest";
import { COMMIT_ACTION_FORMS, validateCommitForm } from "../../src/lib/history/commit-action-forms";
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
});
