<script lang="ts">
  import type { AppError, ConflictFile, ConflictPreview, ResolvedConflictFile } from "../ipc/types";
  import { neighborRowIndex } from "./file-row-nav";

  const componentId = $props.id();
  const summaryId = `${componentId}-summary`;

  interface AcceptConfirm {
    side: string;
    summary: string;
    token: string;
  }

  interface Props {
    mergeBanner: string | null;
    files: ConflictFile[];
    resolvedFiles: ResolvedConflictFile[];
    currentLabel: string;
    incomingLabel: string;
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
    mergeBody: string;
    reviewedStaged: boolean;
    canComplete: boolean;
    canAbort: boolean;
    abortReason: string | null;
    abortConfirm: string | null;
    acceptConfirm: AcceptConfirm | null;
    trustBlocked: boolean;
    onSelectFile: (pathId: string) => void;
    onSelectResolved: (displayPath: string) => void;
    onReload: () => void;
    onMarkAll: () => void;
    onAskAccept: (side: string) => void;
    onConfirmAccept: () => void;
    onCancelAccept: () => void;
    onMarkWorking: () => void;
    onMarkDeletion: () => void;
    onMergeSubject: (value: string) => void;
    onMergeBody: (value: string) => void;
    onComplete: () => void;
    onAskAbort: () => void;
    onConfirmAbort: () => void;
    onCancelAbort: () => void;
    onBack: () => void;
  }

  let {
    mergeBanner,
    files,
    resolvedFiles,
    currentLabel,
    incomingLabel,
    filesLoading,
    filesError,
    selectedPathId,
    previewError,
    busy,
    actionError,
    notice,
    mergeSubject,
    mergeBody,
    reviewedStaged,
    canComplete,
    canAbort,
    abortReason,
    abortConfirm,
    trustBlocked,
    onSelectFile,
    onSelectResolved,
    onReload,
    onMarkAll,
    onMergeSubject,
    onMergeBody,
    onComplete,
    onAskAbort,
    onConfirmAbort,
    onCancelAbort
  }: Props = $props();

  let conflictsCollapsed = $state(false);
  let resolvedCollapsed = $state(false);

  const showFooter = $derived(canComplete || canAbort || abortReason !== null);
  const completeHint = $derived(trustBlocked
    ? "Trust the repository first"
    : files.length > 0 || !canComplete
      ? "Resolve every conflicted file first"
      : !reviewedStaged
        ? "Loading the conflict list counts as review"
        : mergeSubject.trim() === ""
          ? "Write a merge subject first"
          : "Create the merge commit");
  const canSubmitComplete = $derived(
    canComplete && busy === null && !trustBlocked && reviewedStaged && mergeSubject.trim() !== ""
  );
  const contextLabel = $derived(
    currentLabel && incomingLabel
      ? `Merging ${incomingLabel} into ${currentLabel}`
      : (mergeBanner ?? "No merge in progress")
  );

  function fileListKey(event: KeyboardEvent): void {
    const list = event.currentTarget as HTMLElement;
    const rows = [...list.querySelectorAll<HTMLButtonElement>(".gd-merge-file")];
    const current = rows.indexOf((event.target as Element).closest(".gd-merge-file") as HTMLButtonElement);
    if (current < 0) return;
    const next = neighborRowIndex(rows.length, current, event.key);
    if (next === null) return;
    event.preventDefault();
    rows[next].focus();
    rows[next].click();
  }

  function errorText(error: AppError): string {
    return `${error.code}: ${error.message}`;
  }

  function pathParts(path: string): { directory: string; name: string } {
    const splitAt = path.lastIndexOf("/");
    return splitAt < 0
      ? { directory: "", name: path }
      : { directory: path.slice(0, splitAt + 1), name: path.slice(splitAt + 1) };
  }
</script>

<section class="gd-conflict-panel" aria-label="Merge conflicts">
  <div class="gd-conflict-scroll">
    <p class="gd-merge-context" title={contextLabel}>
      {#if currentLabel && incomingLabel}
        Merging <strong>{incomingLabel}</strong> into <span>{currentLabel}</span>
      {:else}
        {mergeBanner ?? "No merge in progress"}
      {/if}
    </p>

    {#if trustBlocked}
      <p class="gd-notice">Trust this repository to resolve conflicts.</p>
    {:else if filesError}
      <div class="gd-error" role="alert"><p>{errorText(filesError)}</p><button type="button" class="gd-text-action" onclick={onReload}>Retry</button></div>
    {/if}
    {#if actionError}<p class="gd-error" role="alert">{errorText(actionError)}</p>{/if}
    {#if notice}<p class="gd-notice" role="status">{notice}</p>{/if}
    {#if previewError}<p class="gd-error" role="alert">{errorText(previewError)}</p>{/if}

    <section class="gd-file-group" aria-label="Conflicted files">
      <div class="gd-group-heading">
        <button type="button" class="gd-group-toggle" aria-expanded={!conflictsCollapsed} onclick={() => (conflictsCollapsed = !conflictsCollapsed)}>
          <span class="gd-chevron" aria-hidden="true">{conflictsCollapsed ? "▸" : "▾"}</span>
          <span>Conflicted Files ({files.length})</span>
        </button>
        <button type="button" class="gd-mark-all" onclick={onMarkAll} disabled={files.length === 0 || filesLoading || busy !== null || trustBlocked}>
          {busy === "resolve-all" ? "Marking…" : "Mark All Resolved"}
        </button>
      </div>
      {#if !conflictsCollapsed}
        <div class="gd-group-list gd-conflicted-list">
          {#if filesLoading && files.length === 0}
            <p class="gd-empty" role="status">Loading conflicts…</p>
          {:else if files.length === 0}
            <p class="gd-empty">{canComplete ? "All files are resolved. Review the result and commit the merge." : "No conflicted files."}</p>
          {:else}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions: delegates navigation to file buttons -->
            <ul onkeydown={fileListKey}>
              {#each files as file (file.pathId)}
                {@const parts = pathParts(file.displayPath)}
                <li>
                  <button type="button" class="gd-merge-file" class:selected={file.pathId === selectedPathId} onclick={() => onSelectFile(file.pathId)} title={file.displayPath}>
                    <svg class="gd-file-status gd-status-conflict" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3 2.6 20h18.8L12 3Z"/><path d="M12 8v6M12 17.2v.1"/></svg>
                    <span class="gd-file-path"><span class="gd-path-dir">{parts.directory}</span><span>{parts.name}</span></span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </section>

    <section class="gd-file-group" aria-label="Resolved files">
      <div class="gd-group-heading">
        <button type="button" class="gd-group-toggle" aria-expanded={!resolvedCollapsed} onclick={() => (resolvedCollapsed = !resolvedCollapsed)}>
          <span class="gd-chevron" aria-hidden="true">{resolvedCollapsed ? "▸" : "▾"}</span>
          <span>Resolved Files ({resolvedFiles.length})</span>
        </button>
      </div>
      {#if !resolvedCollapsed}
        <div class="gd-group-list gd-resolved-list">
          {#if resolvedFiles.length === 0}
            <p class="gd-empty">Resolved files appear here after they enter the merge result.</p>
          {:else}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions: delegates navigation to file buttons -->
            <ul onkeydown={fileListKey}>
              {#each resolvedFiles as file, index (`${file.status}:${file.displayPath}:${index}`)}
                {@const parts = pathParts(file.displayPath)}
                <li>
                  <button type="button" class="gd-merge-file" onclick={() => onSelectResolved(file.displayPath)} title={`Open staged diff · ${file.displayPath}`}>
                    {#if file.status === "A"}
                      <span class="gd-file-status gd-status-add" aria-hidden="true">+</span>
                    {:else if file.status === "D"}
                      <span class="gd-file-status gd-status-delete" aria-hidden="true">−</span>
                    {:else}
                      <svg class="gd-file-status gd-status-modify" viewBox="0 0 24 24" aria-hidden="true"><path d="m4 20 4.4-1 10.5-10.5-3.4-3.4L5 15.6 4 20Z"/><path d="m13.8 6.8 3.4 3.4"/></svg>
                    {/if}
                    <span class="gd-file-path"><span class="gd-path-dir">{parts.directory}</span><span>{parts.name}</span></span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </section>
  </div>

  {#if showFooter}
    <footer class="gd-commit-editor">
      <div class="gd-editor-heading"><h3>Merge commit</h3><span>Resolved files only</span></div>
      <label for={summaryId}>Summary</label>
      <input id={summaryId} type="text" aria-label="Merge subject" value={mergeSubject} maxlength={500} placeholder="What changed?" disabled={busy !== null} oninput={(event) => onMergeSubject(event.currentTarget.value)} />
      <details open={mergeBody !== ""}><summary>Add description <span>optional</span></summary>
        <textarea aria-label="Merge description" value={mergeBody} placeholder="Why was this merge needed?" rows="3" disabled={busy !== null} oninput={(event) => onMergeBody(event.currentTarget.value)}></textarea>
      </details>
      {#if abortConfirm}
        <section class="gd-confirm" aria-label="Confirm abort">
          <pre>{abortConfirm}</pre>
          <div class="gd-confirm-actions"><button type="button" class="gd-danger" disabled={busy !== null} onclick={onConfirmAbort}>Confirm abort</button><button type="button" onclick={onCancelAbort}>Cancel</button></div>
        </section>
      {:else}
        <div class="gd-footer-actions">
          <button type="button" class="gd-commit-button" disabled={!canSubmitComplete} title={completeHint} onclick={onComplete}>{busy === "complete" ? "Committing…" : "Commit and Merge"}</button>
          <button type="button" class="gd-abort-button" disabled={!canAbort || busy !== null || trustBlocked} title={canAbort ? "Abort the app-started merge" : (abortReason ?? "This merge cannot be aborted here")} onclick={onAskAbort}>{busy === "abort" ? "Aborting…" : "Abort Merge"}</button>
        </div>
      {/if}
      {#if !canAbort && abortReason}<p class="gd-commit-hint">{abortReason}</p>{/if}
    </footer>
  {/if}
</section>

<style>
  .gd-conflict-panel { display: flex; flex: 1; min-height: 0; flex-direction: column; background: var(--gd-panel); }
  .gd-conflict-scroll { flex: 1; min-height: 0; overflow-y: auto; padding: 10px 12px 0; }
  .gd-file-status { width: 18px; height: 18px; flex: 0 0 18px; fill: var(--gd-warning); stroke: var(--gd-canvas); stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .gd-merge-context { overflow: hidden; margin: 8px 0 18px; text-align: center; color: var(--gd-text-secondary); text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
  .gd-merge-context strong { padding: 2px 5px; background: color-mix(in srgb, var(--gd-accent) 35%, transparent); color: var(--gd-text); font-weight: 600; }
  .gd-merge-context span { margin-left: 7px; color: var(--gd-text); }
  .gd-file-group { margin-bottom: 18px; }
  .gd-group-heading { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding-bottom: 7px; border-bottom: 1px solid var(--gd-border); }
  .gd-group-toggle { display: flex; min-width: 0; align-items: center; gap: 6px; padding: 2px 0; border: 0; background: transparent; color: var(--gd-text); cursor: pointer; font: 500 13px var(--gd-font-ui); }
  .gd-chevron { width: 12px; color: var(--gd-text-secondary); }
  .gd-mark-all { flex: 0 0 auto; padding: 4px 7px; border: 1px solid var(--gd-warning); border-radius: 3px; background: color-mix(in srgb, var(--gd-warning) 8%, transparent); color: var(--gd-text); cursor: pointer; font: 600 10px var(--gd-font-ui); }
  .gd-group-list { height: 248px; overflow-y: auto; overflow-x: hidden; padding-top: 6px; }
  ul { margin: 0; padding: 0; list-style: none; }
  .gd-merge-file { display: flex; width: 100%; min-width: 0; min-height: 31px; align-items: center; gap: 8px; padding: 4px 7px 4px 8px; border: 1px solid transparent; border-radius: 3px; background: transparent; color: var(--gd-text); cursor: pointer; text-align: left; }
  .gd-merge-file:hover { background: var(--gd-surface-hover); }
  .gd-merge-file.selected { border-color: var(--gd-border); background: transparent; }
  .gd-file-path { display: flex; min-width: 0; overflow: hidden; white-space: nowrap; font-size: 12px; }
  .gd-file-path > span { overflow: hidden; text-overflow: ellipsis; }
  .gd-file-path > span:last-child { flex: 0 0 auto; max-width: 70%; }
  .gd-path-dir { color: var(--gd-text-secondary); }
  .gd-status-add, .gd-status-delete { display: grid; place-items: center; font: 700 18px/1 var(--gd-font-code); }
  .gd-status-add { color: var(--gd-success); }
  .gd-status-delete { color: var(--gd-danger); }
  .gd-status-modify { fill: none; stroke: var(--gd-warning); stroke-width: 1.8; }
  .gd-error, .gd-notice { margin: 0 0 10px; padding: 8px 10px; border-left: 2px solid var(--gd-danger); background: var(--gd-canvas); color: var(--gd-danger); font-size: 11px; line-height: 1.45; }
  .gd-error p { margin: 0; }
  .gd-notice { border-color: var(--gd-warning); color: var(--gd-warning); }
  .gd-text-action { border: 0; padding: 3px 0 0; background: transparent; color: var(--gd-accent); cursor: pointer; font-size: 11px; }
  .gd-empty { margin: 0; padding: 10px 8px; color: var(--gd-text-secondary); font-size: 11px; line-height: 1.45; }
  .gd-commit-editor { flex: 0 0 auto; padding: 10px 12px; border-top: 1px solid var(--gd-border); background: color-mix(in srgb, var(--gd-panel) 65%, var(--gd-canvas)); }
  .gd-editor-heading { display: flex; align-items: center; justify-content: space-between; margin-bottom: 14px; }
  .gd-editor-heading h3 { margin: 0; font-size: 12px; font-weight: 600; }
  .gd-editor-heading > span { color: var(--gd-text-secondary); font-size: 10px; }
  label { display: block; margin-bottom: 6px; color: var(--gd-text-secondary); font-size: 11px; }
  .gd-commit-editor input, .gd-commit-editor textarea { box-sizing: border-box; width: 100%; border: 1px solid var(--gd-border); border-radius: 4px; padding: 7px 8px; background: var(--gd-canvas); color: var(--gd-text); font: var(--gd-font-size-small)/1.4 var(--gd-font-ui); }
  .gd-commit-editor textarea { min-height: 64px; max-height: 140px; margin-top: 8px; resize: vertical; }
  details { margin-top: 10px; color: var(--gd-text-secondary); font-size: 11px; }
  summary { cursor: pointer; }
  summary > span { margin-left: 4px; opacity: .7; }
  .gd-footer-actions { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-top: 10px; }
  .gd-commit-button, .gd-abort-button { min-height: 36px; border-radius: 3px; font: 600 11px var(--gd-font-ui); cursor: pointer; }
  .gd-commit-button { border: 1px solid transparent; background: var(--gd-success); color: var(--gd-on-accent); }
  .gd-abort-button { border: 1px solid var(--gd-danger); background: color-mix(in srgb, var(--gd-danger) 14%, transparent); color: var(--gd-text); }
  .gd-commit-hint { margin: 7px 0 0; color: var(--gd-text-secondary); font-size: 10px; line-height: 1.35; }
  .gd-confirm { margin-top: 10px; }
  .gd-confirm pre { max-height: 90px; overflow: auto; margin: 0; padding: 8px; border: 1px solid var(--gd-border); background: var(--gd-canvas); white-space: pre-wrap; font: 10px/1.4 var(--gd-font-code); }
  .gd-confirm-actions { display: flex; gap: 8px; margin-top: 8px; }
  .gd-confirm-actions button { padding: 6px 9px; border: 1px solid var(--gd-border); border-radius: 3px; background: transparent; color: var(--gd-text); cursor: pointer; }
  .gd-confirm-actions .gd-danger { border-color: var(--gd-danger); color: var(--gd-danger); }
  button:disabled { opacity: .42; cursor: not-allowed; }
  button:focus-visible, input:focus-visible, textarea:focus-visible, summary:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 2px; }
</style>
