<script lang="ts">
  import App from "../../src/app/App.svelte";
  import { createMockAdapter } from "../../src/lib/ipc/mock";
  import { demoSession } from "../../src/mocks/demoSession";
  import type { AppError } from "../../src/lib/ipc/types";
  let failRead = $state(false);
  let failSave = $state(false);
  let slow = $state(false);
  let writes = $state(0);
  const error: AppError = { code: "IO_ERROR", message: "Fixture: could not save repository alias.", recovery: "inspectState", retryable: false };
  const adapterFor = (repoId: string) => createMockAdapter({ ...demoSession, repoId, workspaceKey: `demo-worktree:${repoId.slice(5)}` });
  const aliasCommands = {
    async repoAliasGet(repoId: string) {
      if (slow) await new Promise(resolve => setTimeout(resolve, 3000));
      if (failRead) throw { ...error, message: "Fixture: could not load repository alias." };
      return adapterFor(repoId).repoAliasGet(repoId);
    },
    async repoAliasSet(repoId: string, value: string | null) {
      writes += 1;
      if (slow) await new Promise(resolve => setTimeout(resolve, 3000));
      if (failSave) throw error;
      return adapterFor(repoId).repoAliasSet(repoId, value);
    }
  };
</script>
<App {aliasCommands} />
<aside aria-label="Alias fixture controls">
  <label><input type="checkbox" bind:checked={failRead} />Fail alias read</label>
  <label><input type="checkbox" bind:checked={failSave} />Fail alias save</label>
  <label><input type="checkbox" bind:checked={slow} />Slow alias requests</label>
  <output>Alias writes: {writes}</output>
</aside>
<style>
  aside { position: fixed; bottom: 0; right: 0; z-index: 100; display: flex; gap: 12px; padding: 8px; background: var(--gd-panel); color: var(--gd-text); border: 1px solid var(--gd-border); font: 12px var(--gd-font-ui); }
</style>
