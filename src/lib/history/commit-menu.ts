export type CommitActionId =
  | "checkout"
  | "create-branch"
  | "create-tag"
  | "push-to"
  | "cherry-pick"
  | "revert"
  | "merge"
  | "rebase"
  | "reword"
  | "modify"
  | "edit-author"
  | "split"
  | "move-to-branch"
  | "interactive-rebase"
  | "reset-soft"
  | "reset-mixed"
  | "reset-hard";

export interface CommitActionDefinition {
  id: CommitActionId;
  label: string;
  available: boolean;
  separatorBefore?: boolean;
  danger?: boolean;
  unavailableReason?: string;
}

/**
 * Every commit action is backed by an exact typed IPC flow (T18); the menu
 * communicates scope without pretending that a click will do work. Hard
 * reset stays visually destructive and additionally requires a typed
 * confirmation plus re-typing the target short OID.
 */
export const COMMIT_ACTIONS: readonly CommitActionDefinition[] = [
  { id: "checkout", label: "Checkout this commit…", available: true },
  { id: "create-branch", label: "Create branch here…", available: true },
  { id: "create-tag", label: "Create tag here…", available: true },
  { id: "push-to", label: "Push to…", available: true },
  { id: "cherry-pick", label: "Cherry-pick onto HEAD…", available: true, separatorBefore: true },
  { id: "revert", label: "Revert commit…", available: true },
  { id: "merge", label: "Merge into current branch…", available: true, separatorBefore: true },
  { id: "rebase", label: "Rebase current branch onto this…", available: true },
  { id: "reword", label: "Reword message…", available: true, separatorBefore: true },
  { id: "modify", label: "Modify commit…", available: true },
  { id: "edit-author", label: "Edit author…", available: true },
  { id: "split", label: "Split commit…", available: true },
  { id: "move-to-branch", label: "Move to branch…", available: true },
  { id: "interactive-rebase", label: "Interactive rebase from here…", available: true },
  { id: "reset-soft", label: "Reset HEAD (soft) to here", available: true, separatorBefore: true },
  { id: "reset-mixed", label: "Reset HEAD (mixed) to here", available: true },
  {
    id: "reset-hard",
    label: "Reset HEAD (hard) to here",
    available: true,
    danger: true
  }
];
