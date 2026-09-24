<script lang="ts">
  // Conflict inspector panel (T13): file list, base/current/incoming
  // preview, whole-file accept behind confirmation, mark resolved
  // (working file or confirmed deletion), complete and abort.
  import type { AppError, ConflictFile, ConflictPreview } from "../ipc/types";

  interface AcceptConfirm {
    side: string;
    summary: string;
    token: string;
  }

  interface Props {
    mergeBanner: string | null;
    files: ConflictFile[];
    filesLoading: boolean;
    filesError: AppError | null;
    selectedPathId: string | null;
    preview: ConflictPreview | null;
    previewLoading: boolean;
    previewError: AppError | null;
    busy: string | null;
    actionError: AppError | null;
    notice: string | null;
    mergeSubject: string;
    reviewedStaged: boolean;
    canComplete: boolean;
    canAbort: boolean;
    abortReason: string | null;
    abortConfirm: string | null;
    acceptConfirm: AcceptConfirm | null;
    trustBlocked: boolean;
    onSelectFile: (pathId: string) => void;
    onReload: () => void;
    onAskAccept: (side: string) => void;
    onConfirmAccept: () => void;
    onCancelAccept: () => void;
    onMarkWorking: () => void;
    onMarkDeletion: () => void;
    onMergeSubject: (value: string) => void;
    onComplete: () => void;
    onAskAbort: () => void;
    onConfirmAbort: () => void;
    onCancelAbort: () => void;
    onBack: () => void;
  }

  let {
    mergeBanner,
    files,
    filesLoading,
    filesError,
    selectedPathId,
    preview,
    previewLoading,
    previewError,
    busy,
    actionError,
    notice,
    mergeSubject,
    reviewedStaged,
    canComplete,
    canAbort,
    abortReason,
    abortConfirm,
    acceptConfirm,
    trustBlocked,
    onSelectFile,
    onReload,
    onAskAccept,
    onConfirmAccept,
    onCancelAccept,
    onMarkWorking,
    onMarkDeletion,
    onMergeSubject,
    onComplete,
    onAskAbort,
    onConfirmAbort,
    onCancelAbort,
    onBack
  }: Props = $props();

  function errorText(e: AppError): string {
    return `${e.code}: ${e.message}`;
  }

  function kindLabel(kind: string): string {
    if (kind === "text") return "Text conflict";
    if (kind === "addAdd") return "Added on both sides";
    if (kind === "modifyDelete") return "Modified here, deleted there";
    if (kind === "binary") return "Binary conflict";
    if (kind === "symlink") return "Symlink conflict";
    if (kind === "submodule") return "Submodule conflict";
    return "Unsupported conflict";
  }
</script>

<div class="gd-inspector-body">
  {#if mergeBanner}
    <p class="gd-banner" role="alert">{mergeBanner}</p>
  {/if}
  {#if actionError}
    <p class="gd-error" role="alert">{errorText(actionError)}</p>
  {/if}
  {#if notice}
    <p class="gd-notice" role="status">{notice}</p>
  {/if}

  <div class="gd-work-head">
    <h2>Conflicts ({files.length})</h2>
    <button type="button" class="gd-back" onclick={onReload} disabled={filesLoading} title="Re-read the unmerged index">
      {filesLoading ? "Loading…" : "Refresh"}
    </button>
  </div>
  {#if filesError}
    <p class="gd-error" role="alert">{errorText(filesError)}</p>
  {:else if filesLoading && files.length === 0}
    <p class="gd-muted" role="status">Loading conflicts…</p>
  {:else if files.length === 0}
    <p class="gd-muted">
      {#if canComplete}
        No unmerged files. Review the staged result, then complete the merge below.
      {:else}
        No unmerged files and no merge in progress.
      {/if}
    </p>
  {:else}
    <ul class="gd-conflict-list">
      {#each files as file (file.pathId)}
        <li>
          <button
            type="button"
            class="gd-file"
            class:selected={selectedPathId === file.pathId}
            onclick={() => onSelectFile(file.pathId)}
          >
            {file.displayPath}
            <span class="gd-muted"> · {kindLabel(file.kind)}</span>
            {#if !file.supported}<span class="gd-muted"> · external</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if previewLoading}
    <p class="gd-muted" role="status">Loading preview…</p>
  {:else if previewError}
    <p class="gd-error" role="alert">{errorText(previewError)}</p>
  {:else if preview}
    <section aria-label="Conflict preview">
      <h3>{preview.displayPath}</h3>
      {#if !preview.supportedActions.length && preview.supportReason}
        <p class="gd-muted">{preview.supportReason}</p>
      {/if}
      {#if preview.base}
        <h4>Base</h4>
        <pre class="gd-stage">{preview.base.text}{#if preview.base.truncated}<span class="gd-muted">(truncated)</span>{/if}</pre>
      {:else}
        <p class="gd-muted">No base (added on one or both sides).</p>
      {/if}
      {#if preview.current}
        <h4>Current ({preview.currentLabel})</h4>
        <pre class="gd-stage">{preview.current.text}{#if preview.current.truncated}<span class="gd-muted">(truncated)</span>{/if}</pre>
      {/if}
      {#if preview.incoming}
        <h4>Incoming ({preview.incomingLabel})</h4>
        <pre class="gd-stage">{preview.incoming.text}{#if preview.incoming.truncated}<span class="gd-muted">(truncated)</span>{/if}</pre>
      {/if}
      {#if preview.supportedActions.length > 0}
        <div class="gd-conflict-actions">
          {#if preview.supportedActions.includes("current")}
            <button type="button" disabled={busy !== null || trustBlocked} onclick={() => onAskAccept("current")} title="Overwrite the working file with the current version (asks confirmation)">
              {busy === "accept-current" ? "Working…" : `Use current (${preview.currentLabel})`}
            </button>
          {/if}
          {#if preview.supportedActions.includes("incoming")}
            <button type="button" disabled={busy !== null || trustBlocked} onclick={() => onAskAccept("incoming")} title="Overwrite the working file with the incoming version (asks confirmation)">
              {busy === "accept-incoming" ? "Working…" : `Use incoming (${preview.incomingLabel})`}
            </button>
          {/if}
        </div>
        {#if acceptConfirm}
          <section aria-label="Confirm accept">
            <pre class="gd-summary">{acceptConfirm.summary}</pre>
            <div class="gd-conflict-actions">
              <button type="button" disabled={busy !== null} onclick={onConfirmAccept}>Confirm overwrite</button>
              <button type="button" onclick={onCancelAccept}>Cancel</button>
            </div>
          </section>
        {/if}
        <div class="gd-conflict-actions">
          <button type="button" disabled={busy !== null || trustBlocked} onclick={onMarkWorking} title="Stage the working file as resolved (only this file)">
            {busy === "resolve" ? "Staging…" : "Mark resolved (working file)"}
          </button>
          <button type="button" disabled={busy !== null || trustBlocked} onclick={onMarkDeletion} title="Stage the deletion (the file must already be removed)">
            Mark resolved (deletion)
          </button>
        </div>
      {:else if preview.supportReason}
        <div class="gd-conflict-actions">
          <button type="button" disabled={busy !== null || trustBlocked} onclick={onMarkWorking} title="Stage after resolving outside the app">
            Mark resolved (working file)
          </button>
          <button type="button" disabled={busy !== null || trustBlocked} onclick={onMarkDeletion} title="Stage the deletion (the file must already be removed)">
            Mark resolved (deletion)
          </button>
        </div>
      {/if}
    </section>
  {/if}

  {#if canComplete}
    <section aria-label="Complete merge">
      <h3>Complete merge</h3>
      <label class="gd-field">
        <span>Subject</span>
        <input
          type="text"
          value={mergeSubject}
          maxlength={500}
          placeholder="Merge feature into main"
          oninput={(e) => onMergeSubject(e.currentTarget.value)}
        />
      </label>
      <button
        type="button"
        disabled={busy !== null || trustBlocked || !reviewedStaged || mergeSubject.trim() === ""}
        title={trustBlocked
          ? "Trust the repository first"
          : !reviewedStaged
            ? "Loading the conflict list counts as review"
            : mergeSubject.trim() === ""
              ? "Write a merge subject first"
              : "Create the merge commit (must have two parents)"}
        onclick={onComplete}
      >
        {busy === "complete" ? "Completing…" : "Complete merge"}
      </button>
    </section>
  {/if}

  {#if canAbort}
    {#if abortConfirm}
      <section aria-label="Confirm abort">
        <pre class="gd-summary">{abortConfirm}</pre>
        <div class="gd-conflict-actions">
          <button type="button" disabled={busy !== null} onclick={onConfirmAbort}>Confirm abort</button>
          <button type="button" onclick={onCancelAbort}>Cancel</button>
        </div>
      </section>
    {:else}
      <button type="button" disabled={busy !== null || trustBlocked} onclick={onAskAbort} title="Abort the app-started merge and return to the pre-merge HEAD (asks confirmation)">
        Abort merge
      </button>
    {/if}
  {:else if abortReason}
    <p class="gd-muted">{abortReason}</p>
  {/if}
</div>

<style>
  .gd-inspector-body {
    flex: 1 1 auto;
    overflow-y: auto;
    padding: var(--gd-space-3);
  }
  .gd-inspector-body h2 {
    margin: 0 0 var(--gd-space-2);
    font-size: var(--gd-font-size-title);
  }
  .gd-inspector-body h3 {
    margin: var(--gd-space-3) 0 var(--gd-space-1);
    font-size: 13px;
  }
  .gd-inspector-body h4 {
    margin: var(--gd-space-2) 0 4px;
    font-size: var(--gd-font-size-small);
    color: var(--gd-text-secondary);
  }
  .gd-banner {
    background: var(--gd-surface-raised);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    padding: var(--gd-space-2) var(--gd-space-3);
    font-size: var(--gd-font-size-small);
  }
  .gd-error {
    color: var(--gd-danger);
    font-size: var(--gd-font-size-small);
  }
  .gd-notice {
    color: var(--gd-warning);
    font-size: var(--gd-font-size-small);
  }
  .gd-muted {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
  }
  .gd-work-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--gd-space-2);
  }
  .gd-conflict-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .gd-file {
    background: transparent;
    border: none;
    color: var(--gd-text);
    cursor: pointer;
    padding: 4px 0;
    text-align: left;
  }
  .gd-file.selected {
    font-weight: bold;
  }
  .gd-stage {
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    padding: var(--gd-space-2);
    max-height: 180px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-size: var(--gd-font-size-small);
  }
  .gd-conflict-actions {
    display: flex;
    gap: var(--gd-space-2);
    margin: var(--gd-space-2) 0;
    flex-wrap: wrap;
  }
  .gd-summary {
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    padding: var(--gd-space-2);
    white-space: pre-wrap;
    font-size: var(--gd-font-size-small);
  }
  .gd-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: var(--gd-space-2);
    font-size: var(--gd-font-size-small);
  }
  .gd-field input {
    background: var(--gd-background);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    color: var(--gd-text);
    padding: 6px 8px;
  }
  .gd-back {
    margin-top: var(--gd-space-2);
  }
</style>
