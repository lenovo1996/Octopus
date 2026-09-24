// Sidebar branch context menu (Local/Remote rows). Mirrors the requested
// desktop-Git grouping: checkout, merge/rebase/cherry-pick/revert,
// create here, move, resets, rename/upstream/push, copies, reveal, delete.
// Every entry routes to a real flow; entries without a backend stay out.
// Tags keep a smaller menu (show + copies).
import type { RefItem } from "../ipc/types";
import type { ContextMenuItem } from "../context-menu/model";

export type BranchMenuAction =
  | "checkout"
  | "merge"
  | "rebase"
  | "cherry-pick"
  | "revert"
  | "create-branch"
  | "create-tag"
  | "move"
  | "reset-soft"
  | "reset-mixed"
  | "reset-hard"
  | "rename"
  | "upstream"
  | "push"
  | "push-to"
  | "reveal"
  | "delete";

export interface BranchMenuContext {
  actionsDisabled: boolean;
  /** Selected graph commit; move targets it. Null disables move. */
  selectedCommitOid: string | null;
}

function copyItems(
  ref: RefItem,
  onCopy: (text: string) => void
): ContextMenuItem[] {
  return [
    {
      id: "copy-name",
      label: ref.kind === "tag" ? "Copy tag name" : "Copy branch name",
      action: () => onCopy(ref.label)
    },
    {
      id: "copy-sha",
      label: "Copy commit SHA",
      action: () => onCopy(ref.oid)
    }
  ];
}

export function buildBranchMenuItems(
  ref: RefItem,
  ctx: BranchMenuContext,
  onAction: (action: BranchMenuAction, ref: RefItem) => void,
  onCopy: (text: string) => void
): ContextMenuItem[] {
  if (ref.kind === "tag") {
    return [
      { id: "show", label: "Show in graph", action: () => onAction("reveal", ref) },
      ...copyItems(ref, onCopy)
    ];
  }
  const local = ref.kind === "local";
  const locked = ctx.actionsDisabled;
  const go = (id: BranchMenuAction) => () => onAction(id, ref);
  const moveReady =
    ctx.selectedCommitOid !== null && ctx.selectedCommitOid !== ref.oid;

  const items: ContextMenuItem[] = [
    local
      ? {
          id: "checkout",
          label: `Checkout: ${ref.label}`,
          disabled: locked || ref.current,
          title: ref.current ? "Already checked out" : undefined,
          action: go("checkout")
        }
      : {
          id: "checkout",
          label: `Checkout: ${ref.label}`,
          disabled: locked,
          action: go("checkout")
        },
    {
      id: "merge",
      label: `Merge into current: ${ref.label}`,
      disabled: locked,
      action: go("merge")
    },
    {
      id: "rebase",
      label: "Rebase current onto this…",
      disabled: locked,
      action: go("rebase")
    },
    {
      id: "cherry-pick",
      label: "Cherry-pick onto HEAD…",
      disabled: locked,
      action: go("cherry-pick")
    },
    {
      id: "revert",
      label: "Revert…",
      disabled: locked,
      action: go("revert")
    },
    {
      id: "create-branch",
      label: "Create branch here…",
      separatorBefore: true,
      action: go("create-branch")
    },
    {
      id: "create-tag",
      label: "Create tag here…",
      disabled: locked,
      action: go("create-tag")
    }
  ];
  if (local) {
    items.push({
      id: "move",
      label: "Move branch to selected commit…",
      danger: true,
      disabled: locked || !moveReady,
      title: !moveReady
        ? "Select a different commit in the history first"
        : undefined,
      action: go("move")
    });
  }
  items.push(
    {
      id: "reset-soft",
      label: "Reset current to here (soft)",
      separatorBefore: true,
      disabled: locked,
      action: go("reset-soft")
    },
    {
      id: "reset-mixed",
      label: "Reset current to here (mixed)",
      disabled: locked,
      action: go("reset-mixed")
    },
    {
      id: "reset-hard",
      label: "Reset current to here (hard)",
      danger: true,
      disabled: locked,
      action: go("reset-hard")
    }
  );
  if (local) {
    items.push(
      {
        id: "rename",
        label: "Rename…",
        separatorBefore: true,
        disabled: locked || ref.checkedOutElsewhere,
        title: ref.checkedOutElsewhere
          ? "Checked out in another worktree"
          : undefined,
        action: go("rename")
      },
      {
        id: "upstream",
        label: "Set upstream to…",
        disabled: locked,
        action: go("upstream")
      },
      {
        id: "push",
        label: "Push",
        disabled: locked,
        action: go("push")
      }
    );
  }
  items.push({
    id: "push-to",
    label: "Push to…",
    separatorBefore: !local,
    disabled: locked,
    action: go("push-to")
  });
  items.push(...copyItems(ref, onCopy));
  items.push({
    id: "reveal",
    label: "Reveal commit in History",
    separatorBefore: true,
    action: go("reveal")
  });
  if (local) {
    items.push({
      id: "delete",
      label: `Delete Branch: ${ref.label}`,
      danger: true,
      disabled: locked || ref.current || ref.checkedOutElsewhere,
      title:
        ref.current || ref.checkedOutElsewhere
          ? "Checked-out branches cannot be deleted"
          : undefined,
      action: go("delete")
    });
  }
  return items;
}
