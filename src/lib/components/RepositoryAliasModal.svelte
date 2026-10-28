<script lang="ts">
  import { onMount, tick } from "svelte";
  import type { AppError } from "../ipc/types";
  import { aliasValidationError, MAX_ALIAS_LENGTH } from "../repositories/alias";

  let { name, path, value, loading, loaded, busy, error, onChange, onSave, onClose, onRetry }: {
    name: string; path: string; value: string; loading: boolean; loaded: boolean; busy: boolean;
    error: AppError | null; onChange: (value: string) => void; onSave: () => void;
    onClose: () => void; onRetry: () => void;
  } = $props();
  let input: HTMLInputElement;
  let form: HTMLFormElement;
  const validation = $derived(aliasValidationError(value));
  $effect(() => {
    if (!loading && loaded) void tick().then(() => { input?.focus(); input?.select(); });
  });
  onMount(() => { form?.focus(); });

  function keyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault(); event.stopImmediatePropagation();
      if (!busy) onClose();
    }
    if (event.key === "Tab") {
      const controls = Array.from(form.querySelectorAll<HTMLElement>("input:not(:disabled), button:not(:disabled)"));
      const first = controls[0], last = controls.at(-1);
      if (event.shiftKey && (document.activeElement === first || document.activeElement === form)) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }
  }
</script>

<svelte:window onkeydown={keyDown} />
<div class="gd-alias-backdrop">
  <div class="gd-alias-modal" role="dialog" aria-modal="true" aria-labelledby="alias-title">
  <form bind:this={form} tabindex="-1" onsubmit={event => { event.preventDefault(); if (loaded && !loading && !busy && !validation) onSave(); }}>
    <h2 id="alias-title">Repository alias</h2>
    <p class="gd-path" title={path}>{path}</p>
    {#if loading}<p role="status">Loading alias…</p>{/if}
    <label for="repository-alias">Tab name</label>
    <input id="repository-alias" bind:this={input} value={value} placeholder={name} autocomplete="off"
      disabled={loading || busy || !loaded} aria-invalid={!!validation} aria-describedby="alias-help"
      oninput={event => onChange(event.currentTarget.value)} />
    <p id="alias-help">Saved for this repository on this device. Leave blank to use “{name}”. Up to {MAX_ALIAS_LENGTH} characters.</p>
    {#if validation}<p class="gd-error" role="alert">{validation}</p>{/if}
    {#if error}<p class="gd-error" role="alert">{error.message}</p>
      {#if !loaded}<button type="button" onclick={onRetry} disabled={loading}>Retry loading alias</button>{/if}
    {/if}
    <div class="gd-actions">
      <button class="gd-reset" type="button" onclick={() => onChange("")} disabled={loading || busy || !loaded || !value}>Use folder name</button>
      <button type="button" onclick={onClose} disabled={busy}>Cancel</button>
      <button class="gd-primary" type="submit" disabled={loading || busy || !loaded || !!validation}>{busy ? "Saving…" : "Save"}</button>
    </div>
  </form>
  </div>
</div>

<style>
  .gd-alias-backdrop { position: fixed; inset: 0; display: flex; align-items: center; justify-content: center; background: #0009; z-index: 70; }
  .gd-alias-modal { width: 440px; max-width: calc(100vw - 32px); padding: 20px; background: var(--gd-panel); border: 1px solid var(--gd-border); border-radius: var(--gd-radius-panel); font: var(--gd-font-size) var(--gd-font-ui); color: var(--gd-text); outline: none; }
  h2 { margin: 0 0 8px; font-size: var(--gd-font-size-title); }
  form { outline: none; }
  p { font-size: var(--gd-font-size-small); color: var(--gd-text-secondary); line-height: 1.6; }
  .gd-path { overflow-wrap: anywhere; margin-bottom: 18px; }
  label { display: block; margin-bottom: 6px; }
  input { width: 100%; padding: 8px 10px; background: var(--gd-canvas); color: var(--gd-text); border: 1px solid var(--gd-border); border-radius: var(--gd-radius-control); font: inherit; }
  .gd-error { color: var(--gd-danger); }
  .gd-actions { display: flex; gap: 8px; margin-top: 20px; }
  button { padding: 6px 12px; color: var(--gd-text); background: transparent; border: 1px solid var(--gd-border); border-radius: var(--gd-radius-control); font: inherit; cursor: pointer; }
  .gd-reset { margin-right: auto; }
  .gd-primary { background: var(--gd-accent); color: var(--gd-on-accent); border-color: var(--gd-accent); }
  button:disabled, input:disabled { opacity: .5; cursor: not-allowed; }
  button:focus-visible, input:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 2px; }
</style>
