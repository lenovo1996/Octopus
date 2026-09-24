<script lang="ts">
  // History action dialog (T18): one modal drives every commit-row action
  // except create-branch (branches dialog). Destructive actions show the
  // backend confirmation summary; hard reset additionally requires typing
  // the target short OID. Escape/close defaults to cancel, never mutate.
  import type { AppError, RebasePlanEntry } from "../ipc/types";
  import { COMMIT_ACTION_FORMS, validateCommitForm, type BranchFormOverride, type CommitActionForm } from "../history/commit-action-forms";
  import type { CommitActionId } from "../history/commit-menu";

  interface PlanRow {
    oid: string;
    subject: string;
  }

  interface Props {
    action: CommitActionId | "branch-form";
    oid: string;
    subject: string;
    isHead: boolean;
    busy: boolean;
    error: AppError | string | null;
    confirmSummary: string | null;
    planRows: PlanRow[];
    formOverride?: BranchFormOverride | null;
    onSubmit: (values: Record<string, string>, plan: RebasePlanEntry[]) => void;
    onClose: () => void;
  }

  let { action, oid, subject, isHead, busy, error, confirmSummary, planRows, formOverride = null, onSubmit, onClose }: Props = $props();

  const form: CommitActionForm | BranchFormOverride = $derived(
    formOverride ?? COMMIT_ACTION_FORMS[action as CommitActionId]
  );
  const shortOid = $derived(oid.slice(0, 7));
  let values: Record<string, string> = $state({ switchAfter: "true" });
  let planActions: Record<string, string> = $state({});
  let planMessages: Record<string, string> = $state({});
  let typeConfirm = $state("");
  let localError: string | null = $state(null);

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") onClose();
  }

  function actionFor(rowOid: string): string {
    return planActions[rowOid] ?? "pick";
  }

  function buildPlan(): RebasePlanEntry[] {
    return planRows.map((row) => {
      const planAction = actionFor(row.oid) as RebasePlanEntry["action"];
      const message = planAction === "reword" ? (planMessages[row.oid] ?? "").trim() : null;
      return { oid: row.oid, action: planAction, message };
    });
  }

  function submit(): void {
    localError = null;
    if (form.typeToConfirm && typeConfirm.trim() !== shortOid) {
      localError = `Type ${shortOid} to confirm.`;
      return;
    }
    if (action === "interactive-rebase") {
      const plan = buildPlan();
      if (plan.length === 0) {
        localError = "The plan is empty.";
        return;
      }
      for (const entry of plan) {
        if (!["pick", "reword", "squash", "fixup", "drop"].includes(entry.action)) {
          localError = "Unknown plan action.";
          return;
        }
        if (entry.action === "reword" && !(entry.message ?? "").trim()) {
          localError = "Reword needs a message.";
          return;
        }
      }
      if (plan.every((entry) => entry.action === "drop")) {
        localError = "Dropping every commit would delete the branch.";
        return;
      }
      onSubmit(values, plan);
      return;
    }
    const invalid = validateCommitForm(form, values);
    if (invalid) {
      localError = invalid;
      return;
    }
    onSubmit(values, []);
  }

  function errorText(e: AppError | string): string {
    return typeof e === "string" ? e : `${e.code}: ${e.message}`;
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal" role="dialog" aria-modal="true" aria-label={form.title}>
    <h2>{form.title}</h2>
    <p class="gd-muted">
      <code>{shortOid}</code> — {subject}
      {#if form.headOnly} · HEAD only in v1{/if}
      {#if !isHead && !form.headOnly} · target selected row{/if}
    </p>
    <p class="gd-muted">{form.description}</p>

    {#if error}
      <p class="gd-error" role="alert">{errorText(error)}</p>
    {/if}
    {#if localError}
      <p class="gd-error" role="alert">{localError}</p>
    {/if}
    {#if confirmSummary}
      <pre class="gd-summary">{confirmSummary}</pre>
    {/if}

    {#if action === "interactive-rebase"}
      <ol class="gd-plan">
        {#each planRows as row (row.oid)}
          <li>
            <code>{row.oid.slice(0, 7)}</code>
            <span class="gd-plan-subject">{row.subject}</span>
            <select
              aria-label={`Action for ${row.oid.slice(0, 7)}`}
              value={actionFor(row.oid)}
              onchange={(e) => (planActions = { ...planActions, [row.oid]: e.currentTarget.value })}
              disabled={busy}
            >
              <option value="pick">pick</option>
              <option value="reword">reword</option>
              <option value="squash">squash</option>
              <option value="fixup">fixup</option>
              <option value="drop">drop</option>
            </select>
            {#if actionFor(row.oid) === "reword"}
              <input
                type="text"
                aria-label={`New message for ${row.oid.slice(0, 7)}`}
                placeholder="New message"
                value={planMessages[row.oid] ?? ""}
                oninput={(e) => (planMessages = { ...planMessages, [row.oid]: e.currentTarget.value })}
                disabled={busy}
              />
            {/if}
          </li>
        {/each}
      </ol>
    {:else}
      {#each form.fields as field (field.key)}
        {#if field.kind === "textarea"}
          <label>
            {field.label}
            <textarea
              value={values[field.key] ?? ""}
              placeholder={field.placeholder ?? ""}
              oninput={(e) => (values = { ...values, [field.key]: e.currentTarget.value })}
              disabled={busy}
            ></textarea>
          </label>
        {:else if field.kind === "checkbox"}
          <label class="gd-check">
            <input
              type="checkbox"
              checked={(values[field.key] ?? "true") === "true"}
              onchange={(e) => (values = { ...values, [field.key]: String(e.currentTarget.checked) })}
              disabled={busy}
            />
            {field.label}
          </label>
        {:else if field.kind === "text"}
          <label>
            {field.label}
            <input
              type="text"
              value={values[field.key] ?? ""}
              placeholder={field.placeholder ?? ""}
              oninput={(e) => (values = { ...values, [field.key]: e.currentTarget.value })}
              disabled={busy}
            />
          </label>
        {:else if field.kind === "select"}
          <label>
            {field.label}
            <select
              value={values[field.key] ?? ""}
              onchange={(e) => (values = { ...values, [field.key]: e.currentTarget.value })}
              disabled={busy}
            >
              <option value="">Choose…</option>
              {#each field.options as option (option.value)}
                <option value={option.value}>{option.label}</option>
              {/each}
            </select>
          </label>
        {/if}
      {/each}
    {/if}

    {#if form.typeToConfirm}
      <label>
        Type <code>{shortOid}</code> to confirm
        <input
          type="text"
          value={typeConfirm}
          oninput={(e) => (typeConfirm = e.currentTarget.value)}
          disabled={busy}
        />
      </label>
    {/if}

    <div class="gd-modal-actions">
      <button type="button" onclick={onClose} disabled={busy}>Cancel</button>
      <button
        type="button"
        class:gd-danger-btn={form.danger}
        onclick={submit}
        disabled={busy || (form.typeToConfirm && typeConfirm.trim() !== shortOid)}
      >
        {busy ? "Working…" : form.submitLabel}
      </button>
    </div>
  </div>
</div>

<style>
  .gd-modal-backdrop {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
    z-index: 20;
  }
  .gd-modal {
    width: 440px;
    max-width: calc(100vw - 48px);
    max-height: calc(100vh - 96px);
    overflow-y: auto;
    padding: var(--gd-space-4);
    background: var(--gd-panel);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-panel);
  }
  .gd-modal h2 {
    margin: 0 0 var(--gd-space-2);
    font-size: var(--gd-font-size-title);
  }
  .gd-modal label {
    display: flex;
    flex-direction: column;
    gap: var(--gd-space-1);
    font-size: var(--gd-font-size-small);
    margin-top: var(--gd-space-2);
  }
  .gd-check {
    flex-direction: row !important;
    align-items: center;
  }
  .gd-modal input[type="text"],
  .gd-modal textarea,
  .gd-modal select {
    padding: 6px 10px;
    color: var(--gd-text);
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    font: inherit;
  }
  .gd-modal textarea {
    min-height: 64px;
    resize: vertical;
  }
  .gd-summary {
    font-size: var(--gd-font-size-small);
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    padding: var(--gd-space-2);
    white-space: pre-wrap;
  }
  .gd-muted {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
  }
  .gd-error {
    color: var(--gd-danger);
    font-size: var(--gd-font-size-small);
  }
  .gd-modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--gd-space-2);
    margin-top: var(--gd-space-3);
  }
  .gd-modal-actions button {
    padding: 6px 14px;
    border-radius: var(--gd-radius-control);
    cursor: pointer;
    color: var(--gd-text);
    background: transparent;
    border: 1px solid var(--gd-border);
  }
  .gd-modal-actions button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .gd-danger-btn {
    color: white !important;
    background: var(--gd-danger) !important;
    border: 0 !important;
  }
  code {
    font-family: var(--gd-font-code);
  }
  .gd-plan {
    list-style: none;
    margin: 8px 0;
    padding: 0;
    max-height: 240px;
    overflow-y: auto;
  }
  .gd-plan li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
  }
  .gd-plan-subject {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
