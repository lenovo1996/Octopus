import { invokeCommand, newRequestId } from "./client";
import type {
  BitbucketConnectionResult,
  BranchCreateResult,
  ClosedResponse,
  CommitDetails,
  CommitResult,
  ConfirmationDetails,
  DiffDocument,
  DiffTarget,
  IdentityInfo,
  HistoryPage,
  HistoryScope,
  ConflictAcceptResult,
  ConflictList,
  ConflictPreview,
  MergeCompleteResult,
  MergeStartResult,
  SettingsV1,
  OperationLogPage,
  OperationRecord,
  OperationStarted,
  PullRequestResult,
  OpenWorkspaceEntry,
  PreflightData,
  RebasePlanEntry,
  RemoteStatus,
  WorkspacesRestoreResult,
  WorkspacesSaved,
  StashApplyResult,
  StashEntry,
  StashSaveResult,
  RecentEntry,
  RefItem,
  RemovedResponse,
  RepoSnapshot,
  RepoId,
  SearchResults,
  StatusData
} from "./types";

/** Real native adapter: thin wrappers over the typed repo commands. */
export const realAdapter = {
  adapter: "real" as const,
  async appPreflight(): Promise<PreflightData> {
    return invokeCommand<PreflightData>("app_preflight", { requestId: newRequestId() });
  },
  async repoOpen(selectedPath: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("repo_open", { requestId: newRequestId(), selectedPath });
  },
  async repoInit(selectedPath: string, initialBranch: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("repo_init", {
      requestId: newRequestId(),
      selectedPath,
      initialBranch
    });
  },
  async repoClose(repoId: RepoId): Promise<ClosedResponse> {
    return invokeCommand<ClosedResponse>("repo_close", { requestId: newRequestId(), repoId });
  },
  async repoSnapshot(repoId: RepoId, refresh: boolean): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("repo_snapshot", {
      requestId: newRequestId(),
      repoId,
      refresh
    });
  },
  async repoTrustSet(repoId: RepoId, trusted: boolean): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("repo_trust_set", {
      requestId: newRequestId(),
      repoId,
      trusted
    });
  },
  async repoRecentList(): Promise<RecentEntry[]> {
    return invokeCommand<RecentEntry[]>("repo_recent_list", { requestId: newRequestId() });
  },
  async repoRecentRemove(entryId: string): Promise<RemovedResponse> {
    return invokeCommand<RemovedResponse>("repo_recent_remove", {
      requestId: newRequestId(),
      entryId
    });
  },
  async workspacesSave(entries: OpenWorkspaceEntry[], activeKey: string | null): Promise<WorkspacesSaved> {
    return invokeCommand<WorkspacesSaved>("workspaces_save", {
      requestId: newRequestId(),
      entries,
      activeKey
    });
  },
  async workspacesRestore(): Promise<WorkspacesRestoreResult> {
    return invokeCommand<WorkspacesRestoreResult>("workspaces_restore", {
      requestId: newRequestId()
    });
  },
  async repoRefs(repoId: RepoId): Promise<RefItem[]> {
    return invokeCommand<RefItem[]>("repo_refs", { requestId: newRequestId(), repoId });
  },
  async historyPage(
    repoId: RepoId,
    scope: HistoryScope,
    cursor: string | null,
    limit: number
  ): Promise<HistoryPage> {
    return invokeCommand<HistoryPage>("history_page", {
      requestId: newRequestId(),
      repoId,
      scope,
      cursor,
      limit
    });
  },
  async historySearch(
    repoId: RepoId,
    scope: HistoryScope,
    query: string,
    cursor: string | null,
    limit: number
  ): Promise<SearchResults> {
    return invokeCommand<SearchResults>("history_search", {
      requestId: newRequestId(),
      repoId,
      scope,
      query,
      cursor,
      limit
    });
  },
  async commitDetails(
    repoId: RepoId,
    oid: string,
    parentIndex: number | null
  ): Promise<CommitDetails> {
    return invokeCommand<CommitDetails>("commit_details", {
      requestId: newRequestId(),
      repoId,
      oid,
      parentIndex
    });
  },
  async repoStatus(repoId: RepoId): Promise<StatusData> {
    return invokeCommand<StatusData>("repo_status", { requestId: newRequestId(), repoId });
  },
  async operationGet(operationId: string): Promise<OperationRecord> {
    return invokeCommand<OperationRecord>("operation_get", {
      requestId: newRequestId(),
      operationId
    });
  },
  async operationCancel(operationId: string): Promise<{ cancelRequested: boolean }> {
    return invokeCommand<{ cancelRequested: boolean }>("operation_cancel", {
      requestId: newRequestId(),
      operationId
    });
  },
  async diffRead(repoId: RepoId, target: DiffTarget): Promise<DiffDocument> {
    return invokeCommand<DiffDocument>("diff_read", {
      requestId: newRequestId(),
      repoId,
      target
    });
  },
  async indexStage(repoId: RepoId, expectedVersion: number, pathIds: string[]): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("index_stage", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      pathIds
    });
  },
  async indexUnstage(repoId: RepoId, expectedVersion: number, pathIds: string[]): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("index_unstage", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      pathIds
    });
  },
  async worktreeDiscardFile(repoId: RepoId, expectedVersion: number, pathId: string, confirmationToken: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("worktree_discard_file", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      pathId,
      confirmationToken
    });
  },
  async diffHunkStage(repoId: RepoId, expectedVersion: number, pathId: string, hunkId: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("diff_hunk_stage", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      pathId,
      hunkId
    });
  },
  async diffHunkDiscard(repoId: RepoId, expectedVersion: number, pathId: string, hunkId: string, confirmationToken: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("diff_hunk_discard", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      pathId,
      hunkId,
      confirmationToken
    });
  },
  async identityRead(repoId: RepoId): Promise<IdentityInfo> {
    return invokeCommand<IdentityInfo>("identity_read", { requestId: newRequestId(), repoId });
  },
  async commitCreate(repoId: RepoId, expectedVersion: number, subject: string, body: string): Promise<CommitResult> {
    return invokeCommand<CommitResult>("commit_create", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      subject,
      body
    });
  },
  async branchCreate(repoId: RepoId, expectedVersion: number, name: string, startOid: string, switchAfterCreate: boolean): Promise<BranchCreateResult> {
    return invokeCommand<BranchCreateResult>("branch_create", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      name,
      startOid,
      switchAfterCreate
    });
  },
  async branchSwitch(repoId: RepoId, expectedVersion: number, refId: string, newLocalName: string | null): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("branch_switch", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      refId,
      newLocalName
    });
  },
  async branchDelete(repoId: RepoId, expectedVersion: number, refId: string, confirmationToken: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("branch_delete", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      refId,
      confirmationToken
    });
  },
  async branchMove(repoId: RepoId, expectedVersion: number, refId: string, oid: string, confirmationToken: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("branch_move", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      refId,
      oid,
      confirmationToken
    });
  },
  async branchRename(repoId: RepoId, expectedVersion: number, refId: string, newName: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("branch_rename", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      refId,
      newName
    });
  },
  async branchSetUpstream(repoId: RepoId, expectedVersion: number, refId: string, upstreamRefId: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("branch_set_upstream", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      refId,
      upstreamRefId
    });
  },
  async branchPush(repoId: RepoId, expectedVersion: number, refId: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("branch_push", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      refId
    });
  },
  async confirmationPrepare(repoId: RepoId, expectedVersion: number, action: string, targets: string[]): Promise<ConfirmationDetails> {
    return invokeCommand<ConfirmationDetails>("confirmation_prepare", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      action,
      targets
    });
  },
  async remoteStatus(repoId: RepoId): Promise<RemoteStatus> {
    return invokeCommand<RemoteStatus>("remote_status", { requestId: newRequestId(), repoId });
  },
  async bitbucketConnect(
    repoId: RepoId,
    expectedVersion: number,
    remote: string | null,
    apiToken: string
  ): Promise<BitbucketConnectionResult> {
    return invokeCommand<BitbucketConnectionResult>("bitbucket_connect", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      remote,
      apiToken
    });
  },
  async pullRequestCreate(
    repoId: RepoId,
    expectedVersion: number,
    remote: string | null,
    sourceBranch: string,
    targetBranch: string,
    title: string,
    description: string
  ): Promise<PullRequestResult> {
    return invokeCommand<PullRequestResult>("pull_request_create", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      remote,
      sourceBranch,
      targetBranch,
      title,
      description
    });
  },
  async remoteFetch(repoId: RepoId, expectedVersion: number, remote: string | null): Promise<OperationStarted> {
    return invokeCommand<OperationStarted>("remote_fetch", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      remote
    });
  },
  async remotePull(repoId: RepoId, expectedVersion: number): Promise<OperationStarted> {
    return invokeCommand<OperationStarted>("remote_pull", {
      requestId: newRequestId(),
      repoId,
      expectedVersion
    });
  },
  async remotePush(repoId: RepoId, expectedVersion: number, remote: string | null, setUpstream: boolean): Promise<OperationStarted> {
    return invokeCommand<OperationStarted>("remote_push", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      remote,
      setUpstream
    });
  },
  async operationLog(repoId: RepoId, cursor: number): Promise<OperationLogPage> {
    return invokeCommand<OperationLogPage>("operation_log", {
      requestId: newRequestId(),
      repoId,
      cursor
    });
  },
  async stashList(repoId: RepoId): Promise<StashEntry[]> {
    return invokeCommand<StashEntry[]>("stash_list", { requestId: newRequestId(), repoId });
  },
  async stashSave(repoId: RepoId, expectedVersion: number, message: string, includeUntracked: boolean): Promise<StashSaveResult> {
    return invokeCommand<StashSaveResult>("stash_save", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      message,
      includeUntracked
    });
  },
  async stashApply(repoId: RepoId, expectedVersion: number, stashId: string, expectedOid: string, mode: "apply" | "pop"): Promise<StashApplyResult> {
    return invokeCommand<StashApplyResult>("stash_apply", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      stashId,
      expectedOid,
      mode
    });
  },
  async conflictList(repoId: RepoId): Promise<ConflictList> {
    return invokeCommand<ConflictList>("conflict_list", { requestId: newRequestId(), repoId });
  },
  async conflictPreview(repoId: RepoId, pathId: string): Promise<ConflictPreview> {
    return invokeCommand<ConflictPreview>("conflict_preview", {
      requestId: newRequestId(),
      repoId,
      pathId
    });
  },
  async conflictAccept(repoId: RepoId, expectedVersion: number, pathId: string, side: string, workingFingerprint: string, confirmationToken: string): Promise<ConflictAcceptResult> {
    return invokeCommand<ConflictAcceptResult>("conflict_accept", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      pathId,
      side,
      workingFingerprint,
      confirmationToken
    });
  },
  async conflictMarkResolved(repoId: RepoId, expectedVersion: number, pathId: string, workingFingerprint: string, resolution: "workingFile" | "deletion"): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("conflict_mark_resolved", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      pathId,
      workingFingerprint,
      resolution
    });
  },
  async mergeStart(repoId: RepoId, expectedVersion: number, sourceRefId: string, confirmedTargetOid: string): Promise<MergeStartResult> {
    return invokeCommand<MergeStartResult>("merge_start", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      sourceRefId,
      confirmedTargetOid
    });
  },
  async mergeComplete(repoId: RepoId, expectedVersion: number, subject: string, body: string, stagedReviewed: boolean, confirmedHeadOid: string): Promise<MergeCompleteResult> {
    return invokeCommand<MergeCompleteResult>("merge_complete", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      subject,
      body,
      stagedReviewed,
      confirmedHeadOid
    });
  },
  async mergeAbort(repoId: RepoId, expectedVersion: number, confirmationToken: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("merge_abort", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      confirmationToken
    });
  },
  async historyCheckout(repoId: RepoId, expectedVersion: number, oid: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("history_checkout", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid
    });
  },
  async tagCreateAt(repoId: RepoId, expectedVersion: number, oid: string, name: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("tag_create_at", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid,
      name
    });
  },
  async historyPushTo(repoId: RepoId, expectedVersion: number, oid: string, remote: string, destBranch: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("history_push_to", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid,
      remote,
      destBranch
    });
  },
  async cherryPickOnto(repoId: RepoId, expectedVersion: number, oid: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("cherry_pick_onto", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid
    });
  },
  async revertCommit(repoId: RepoId, expectedVersion: number, oid: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("revert_commit", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid
    });
  },
  async mergeCommit(repoId: RepoId, expectedVersion: number, oid: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("merge_commit", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid
    });
  },
  async rebaseOnto(repoId: RepoId, expectedVersion: number, oid: string, confirmationToken: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("rebase_onto", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid,
      confirmationToken
    });
  },
  async rewordMessage(repoId: RepoId, expectedVersion: number, oid: string, subject: string, body: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("reword_message", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid,
      subject,
      body
    });
  },
  async modifyCommit(repoId: RepoId, expectedVersion: number, oid: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("modify_commit", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid
    });
  },
  async editAuthor(repoId: RepoId, expectedVersion: number, oid: string, name: string, email: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("edit_author", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid,
      name,
      email
    });
  },
  async splitCommit(repoId: RepoId, expectedVersion: number, oid: string, confirmationToken: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("split_commit", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid,
      confirmationToken
    });
  },
  async moveToBranch(repoId: RepoId, expectedVersion: number, oid: string, name: string, switchAfterCreate: boolean): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("move_to_branch", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid,
      name,
      switchAfterCreate
    });
  },
  async rebaseInteractiveFrom(repoId: RepoId, expectedVersion: number, oid: string, plan: RebasePlanEntry[], confirmationToken: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("rebase_interactive_from", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid,
      plan,
      confirmationToken
    });
  },
  async resetSoft(repoId: RepoId, expectedVersion: number, oid: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("reset_soft", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid
    });
  },
  async resetMixed(repoId: RepoId, expectedVersion: number, oid: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("reset_mixed", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid
    });
  },
  async resetHard(repoId: RepoId, expectedVersion: number, oid: string, confirmationToken: string): Promise<RepoSnapshot> {
    return invokeCommand<RepoSnapshot>("reset_hard", {
      requestId: newRequestId(),
      repoId,
      expectedVersion,
      oid,
      confirmationToken
    });
  },
  async settingsGet(): Promise<SettingsV1> {
    return invokeCommand<SettingsV1>("settings_get", { requestId: newRequestId() });
  },
  async settingsUpdate(settingsVersion: number, fontScale: number | null): Promise<SettingsV1> {
    return invokeCommand<SettingsV1>("settings_update", {
      requestId: newRequestId(),
      settingsVersion,
      patch: { fontScale }
    });
  }
};
