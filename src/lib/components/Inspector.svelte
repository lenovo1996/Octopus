<script lang="ts">
  // InspectorPanel: three states per UI spec §4 — Working changes, Commit
  // details, Merge conflict. T02 renders static demo fixtures; T07–T13 wire real IPC.
  import WorkingChangesPanel from "./WorkingChangesPanel.svelte";
  import CommitDetailsPanel from "./CommitDetailsPanel.svelte";
  import type {
    AppError,
    ChangedFile,
    CommitDetails,
    ConflictFile,
    ConflictPreview,
    ResolvedConflictFile,
    RefItem,
    DiffTarget
  } from "../ipc/types";
  import ConflictPanel from "./ConflictPanel.svelte";
  import type { InspectorState } from "../state/shell";

  interface Props {
    width: number;
    state: InspectorState;
    branchName: string;
    refs: RefItem[];
    onParentCommit: (oid: string) => void;
    commitMessage: string;
    commitDetails: CommitDetails | null;
    detailsLoading: boolean;
    detailsError: AppError | null;
    /** Live worktree rows; null renders a loading/unavailable state. */
    statusFiles: ChangedFile[] | null;
    statusLoading: boolean;
    statusError: AppError | null;
    /** True while the repo is read-only: counts read "unavailable", not zero. */
    trustBlocked: boolean;
    onRefreshStatus: () => void;
    selectedDiffTarget: DiffTarget | null;
    onWorktreeDiff: (pathId: string, kind: "worktree" | "index", displayPath: string) => void;
    onCommitDiff: (pathId: string) => void;
    /** T09 index mutation state: selection is never cleared by errors. */
    indexBusy: boolean;
    indexError: AppError | null;
    onStageFiles: (ids: string[]) => void;
    onUnstageFiles: (ids: string[]) => void;
    onDiscardFile: (id: string) => void;
    onDiscardAllFiles: () => void;
    /** T10 commit editor: draft lives in App and survives failures. */
    commitBody: string;
    commitBusy: boolean;
    commitError: AppError | null;
    amendOid: string | null;
    amendLoading: boolean;
    amendUnavailable: string | null;
    amendStale: boolean;
    onAmend: (enabled: boolean) => void;
    identityLabel: string | null;
    headDetached: boolean;
    onCommitBody: (value: string) => void;
    onCommit: () => void;
    onStateChange: (state: InspectorState) => void;
    onCommitMessage: (value: string) => void;
    onParentChange: (parentIndex: number | null) => void;
    onRetryDetails: () => void;
    /** T13 conflict inspector: live unmerged state, null until loaded. */
    mergeBanner: string | null;
    conflictFiles: ConflictFile[] | null;
    conflictResolvedFiles: ResolvedConflictFile[];
    conflictCurrentLabel: string;
    conflictIncomingLabel: string;
    conflictFilesLoading: boolean;
    conflictFilesError: AppError | null;
    conflictSelected: string | null;
    conflictPreview: ConflictPreview | null;
    conflictPreviewLoading: boolean;
    conflictPreviewError: AppError | null;
    conflictBusy: string | null;
    conflictActionError: AppError | null;
    conflictNotice: string | null;
    mergeSubject: string;
    mergeBody: string;
    reviewedStaged: boolean;
    canComplete: boolean;
    canAbort: boolean;
    abortReason: string | null;
    abortConfirm: string | null;
    acceptConfirm: { side: string; summary: string; token: string } | null;
    onSelectConflict: (pathId: string) => void;
    onReloadConflicts: () => void;
    onSelectResolvedConflict: (displayPath: string) => void;
    onMarkAllResolved: () => void;
    onAskAccept: (side: string) => void;
    onConfirmAccept: () => void;
    onCancelAccept: () => void;
    onMarkWorking: () => void;
    onMarkDeletion: () => void;
    onMergeSubject: (value: string) => void;
    onMergeBody: (value: string) => void;
    onCompleteMerge: () => void;
    onAskAbort: () => void;
    onConfirmAbort: () => void;
    onCancelAbort: () => void;
  }

  let {
    width,
    state,
    branchName,
    refs,
    onParentCommit,
    commitMessage,
    commitDetails,
    detailsLoading,
    detailsError,
    statusFiles,
    statusLoading,
    statusError,
    trustBlocked,
    onRefreshStatus,
    selectedDiffTarget,
    onWorktreeDiff,
    onCommitDiff,
    indexBusy,
    indexError,
    onStageFiles,
    onUnstageFiles,
    onDiscardFile,
    onDiscardAllFiles,
    commitBody,
    commitBusy,
    commitError,
    amendOid,
    amendLoading,
    amendUnavailable,
    amendStale,
    onAmend,
    identityLabel,
    headDetached,
    onCommitBody,
    onCommit,
    onStateChange,
    onCommitMessage,
    onParentChange,
    onRetryDetails,
    mergeBanner,
    conflictFiles,
    conflictResolvedFiles,
    conflictCurrentLabel,
    conflictIncomingLabel,
    conflictFilesLoading,
    conflictFilesError,
    conflictSelected,
    conflictPreview,
    conflictPreviewLoading,
    conflictPreviewError,
    conflictBusy,
    conflictActionError,
    mergeSubject,
    mergeBody,
    reviewedStaged,
    canComplete,
    canAbort,
    abortReason,
    abortConfirm,
    acceptConfirm,
    onSelectConflict,
    onReloadConflicts,
    onSelectResolvedConflict,
    onMarkAllResolved,
    onAskAccept,
    onConfirmAccept,
    onCancelAccept,
    onMarkWorking,
    onMarkDeletion,
    onMergeSubject,
    onMergeBody,
    onCompleteMerge,
    onAskAbort,
    onConfirmAbort,
    onCancelAbort
  }: Props = $props();

</script>

<aside class="gd-inspector" style="width: {width}px" aria-label="Inspector">
  {#if state === "working"}
    <WorkingChangesPanel {branchName} files={statusFiles} loading={statusLoading} error={statusError} {trustBlocked}
      selectedTarget={selectedDiffTarget} {indexBusy} {indexError} subject={commitMessage} body={commitBody}
      {commitBusy} {commitError} {identityLabel} {headDetached} onRefresh={onRefreshStatus}
      {amendOid} {amendLoading} {amendUnavailable} {amendStale} {onAmend}
      onOpen={onWorktreeDiff} onStage={onStageFiles} onUnstage={onUnstageFiles} onSubject={onCommitMessage}
      onDiscard={onDiscardFile} onDiscardAll={onDiscardAllFiles}
      onBody={onCommitBody} {onCommit} onConflicts={() => onStateChange("conflict")} />
  {:else if state === "commit"}
    <CommitDetailsPanel details={commitDetails} loading={detailsLoading} error={detailsError} {refs}
      selectedTarget={selectedDiffTarget} onOpen={onCommitDiff} {onParentChange} {onParentCommit}
      onRetry={onRetryDetails} onBack={() => onStateChange("working")} />
  {:else}
    <ConflictPanel
      {mergeBanner}
      files={conflictFiles ?? []}
      resolvedFiles={conflictResolvedFiles}
      currentLabel={conflictCurrentLabel}
      incomingLabel={conflictIncomingLabel}
      filesLoading={conflictFilesLoading}
      filesError={conflictFilesError}
      selectedPathId={conflictSelected}
      preview={conflictPreview}
      previewLoading={conflictPreviewLoading}
      previewError={conflictPreviewError}
      busy={conflictBusy}
      actionError={conflictActionError}
      notice={""}
      {mergeSubject}
      {mergeBody}
      {reviewedStaged}
      {canComplete}
      {canAbort}
      {abortReason}
      {abortConfirm}
      {acceptConfirm}
      {trustBlocked}
      onSelectFile={onSelectConflict}
      onReload={onReloadConflicts}
      onSelectResolved={onSelectResolvedConflict}
      onMarkAll={onMarkAllResolved}
      {onAskAccept}
      {onConfirmAccept}
      {onCancelAccept}
      {onMarkWorking}
      {onMarkDeletion}
      {onMergeSubject}
      {onMergeBody}
      onComplete={onCompleteMerge}
      {onAskAbort}
      {onConfirmAbort}
      {onCancelAbort}
      onBack={() => onStateChange("working")}
    />
  {/if}
</aside>

<style>
  .gd-inspector {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    background: var(--gd-panel);
    border-left: 1px solid var(--gd-border);
    min-height: 0;
    min-width: 0;
  }
</style>
