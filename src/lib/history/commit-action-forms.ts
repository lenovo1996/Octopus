// Per-action form metadata for the commit history modal (T18). Pure data
// plus small validators so the modal stays thin and unit-testable.
import type { CommitActionId } from "./commit-menu";

export type CommitFormField =
  | { kind: "text"; key: string; label: string; placeholder?: string }
  | { kind: "textarea"; key: string; label: string; placeholder?: string }
  | { kind: "checkbox"; key: string; label: string }
  | { kind: "select"; key: string; label: string; options: { value: string; label: string }[] }
  | { kind: "plan"; key: string; label: string };

/** Branch-row forms (rename / set-upstream / move / push) reuse the same modal. */
export interface BranchFormOverride {
  title: string;
  description: string;
  fields: CommitFormField[];
  submitLabel: string;
  danger?: boolean;
  headOnly?: boolean;
  typeToConfirm?: boolean;
}

export interface CommitActionForm {
  id: CommitActionId;
  title: string;
  description: string;
  fields: CommitFormField[];
  submitLabel: string;
  danger?: boolean;
  /** Backend confirmation_prepare action; absent means no token needed. */
  confirmAction?: string;
  /** Destructive confirm additionally requires typing the short OID. */
  typeToConfirm?: boolean;
  headOnly?: boolean;
}

export const COMMIT_ACTION_FORMS: Record<CommitActionId, CommitActionForm> = {
  checkout: {
    id: "checkout",
    title: "Checkout this commit",
    description: "Detaches HEAD at the exact commit. Needs a clean worktree.",
    fields: [],
    submitLabel: "Checkout commit"
  },
  "create-branch": {
    id: "create-branch",
    title: "Create branch here",
    description: "Handled by the branches dialog.",
    fields: [],
    submitLabel: "Create branch"
  },
  "create-tag": {
    id: "create-tag",
    title: "Create tag here",
    description: "Creates a lightweight tag pointing at the exact commit.",
    fields: [{ kind: "text", key: "name", label: "Tag name", placeholder: "v1.0.0" }],
    submitLabel: "Create tag"
  },
  "push-to": {
    id: "push-to",
    title: "Push this commit to…",
    description: "Pushes the exact commit to an explicit remote branch. Never force-pushes.",
    fields: [
      { kind: "text", key: "remote", label: "Remote", placeholder: "origin" },
      { kind: "text", key: "destBranch", label: "Destination branch", placeholder: "feature/backport" }
    ],
    submitLabel: "Push commit"
  },
  "cherry-pick": {
    id: "cherry-pick",
    title: "Cherry-pick onto HEAD",
    description: "Stages the commit on HEAD without committing, so the result can be reviewed first. Conflicts stop with guidance.",
    fields: [],
    submitLabel: "Cherry-pick"
  },
  revert: {
    id: "revert",
    title: "Revert commit",
    description: "Stages the inverse of the commit on HEAD without committing. HEAD itself cannot be reverted.",
    fields: [],
    submitLabel: "Revert"
  },
  merge: {
    id: "merge",
    title: "Merge into current branch",
    description: "Merges the exact commit with --no-ff --no-commit. Uses the same review/complete/abort flow as branch merges.",
    fields: [],
    submitLabel: "Start merge"
  },
  rebase: {
    id: "rebase",
    title: "Rebase current branch onto this",
    description: "Replays the current branch on top of the commit. Rewrites IDs; conflicts finish in a terminal.",
    fields: [],
    submitLabel: "Rebase",
    danger: true,
    confirmAction: "history_rebase"
  },
  reword: {
    id: "reword",
    title: "Reword message",
    description: "Replaces the message of HEAD. Older commits go through Interactive rebase.",
    fields: [
      { kind: "text", key: "subject", label: "Subject", placeholder: "Short summary" },
      { kind: "textarea", key: "body", label: "Body (optional)" }
    ],
    submitLabel: "Reword",
    headOnly: true
  },
  modify: {
    id: "modify",
    title: "Modify commit",
    description: "Folds the currently staged changes into HEAD. Stage follow-up changes first.",
    fields: [],
    submitLabel: "Amend into HEAD",
    headOnly: true
  },
  "edit-author": {
    id: "edit-author",
    title: "Edit author",
    description: "Replaces the author of HEAD without touching the message.",
    fields: [
      { kind: "text", key: "name", label: "Author name" },
      { kind: "text", key: "email", label: "Author email", placeholder: "name@example.com" }
    ],
    submitLabel: "Update author",
    headOnly: true
  },
  split: {
    id: "split",
    title: "Split commit",
    description: "Uncommits HEAD into the worktree (mixed reset to its parent) so changes can be re-committed in parts.",
    fields: [],
    submitLabel: "Split HEAD",
    danger: true,
    confirmAction: "history_split",
    headOnly: true
  },
  "move-to-branch": {
    id: "move-to-branch",
    title: "Move to branch",
    description: "Creates a branch at the exact commit, optionally switching to it.",
    fields: [
      { kind: "text", key: "name", label: "Branch name", placeholder: "feature/rescued" },
      { kind: "checkbox", key: "switchAfter", label: "Switch to the new branch" }
    ],
    submitLabel: "Move to branch"
  },
  "interactive-rebase": {
    id: "interactive-rebase",
    title: "Interactive rebase from here",
    description: "Replays pick, reword, squash, fixup and drop steps starting at the commit. Drops and squashes are permanent.",
    fields: [{ kind: "plan", key: "plan", label: "Plan" }],
    submitLabel: "Run rebase plan",
    danger: true,
    confirmAction: "history_rebase_interactive"
  },
  "reset-soft": {
    id: "reset-soft",
    title: "Reset HEAD (soft) to here",
    description: "Moves the branch pointer; index and worktree stay untouched.",
    fields: [],
    submitLabel: "Soft reset"
  },
  "reset-mixed": {
    id: "reset-mixed",
    title: "Reset HEAD (mixed) to here",
    description: "Moves the branch pointer and unstages; worktree files stay untouched.",
    fields: [],
    submitLabel: "Mixed reset"
  },
  "reset-hard": {
    id: "reset-hard",
    title: "Reset HEAD (hard) to here",
    description: "Moves the branch pointer and overwrites index and tracked worktree files. Uncommitted work is destroyed.",
    fields: [],
    submitLabel: "Hard reset",
    danger: true,
    confirmAction: "history_reset_hard",
    typeToConfirm: true
  }
};

export function validateCommitForm(
  form: CommitActionForm | BranchFormOverride,
  values: Record<string, string>
): string | null {
  for (const field of form.fields) {
    if (field.kind === "checkbox" || field.kind === "plan") continue;
    const value = (values[field.key] ?? "").trim();
    if (field.key === "body") continue;
    if (value === "") return `${field.label} is required.`;
  }
  if ("id" in form && form.id === "edit-author") {
    const email = (values.email ?? "").trim();
    if (!email.includes("@")) return "Author email must contain @.";
  }
  return null;
}
