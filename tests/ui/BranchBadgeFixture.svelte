<script lang="ts">
  import { onMount } from "svelte";
  import RepositoryWorkspace from "../../src/app/RepositoryWorkspace.svelte";
  import { createMockAdapter } from "../../src/lib/ipc/mock";
  import { demoSession } from "../../src/mocks/demoSession";
  import { demoCommits } from "../../src/mocks/demoRepo";
  import type { AppError, RepoSnapshot } from "../../src/lib/ipc/types";

  let snapshot: RepoSnapshot | null = $state(null);
  let failCheckout = $state(false);
  let holdCheckout = $state(false);
  let releaseCheckout: (() => void) | null = $state(null);
  const seed = { ...demoSession, repoId: "branch-badge-fixture", workspaceKey: "fixture:branch-badge" };
  const base = createMockAdapter(seed);
  const adapter = {
    ...base,
    async branchSwitch(...args: Parameters<typeof base.branchSwitch>) {
      if (holdCheckout) await new Promise<void>((resolve) => { releaseCheckout = resolve; });
      releaseCheckout = null;
      if (failCheckout) throw { code: "GIT_ERROR", message: "Fixture checkout failed", recovery: "retryRead", retryable: true } satisfies AppError;
      return base.branchSwitch(...args);
    }
  };

  onMount(() => {
    void base.branchCreate(seed.repoId, seed.version, "B", demoCommits[0].oid, false)
      .then(result => { snapshot = result.snapshot; });
  });
</script>

<nav aria-label="Branch badge fixture controls">
  <label><input type="checkbox" bind:checked={failCheckout} />Fail checkout</label>
  <label><input type="checkbox" bind:checked={holdCheckout} />Hold checkout</label>
  <button disabled={releaseCheckout === null} onclick={() => releaseCheckout?.()}>Complete checkout</button>
</nav>
<main>
  {#if snapshot}
    <RepositoryWorkspace initialSession={snapshot} active={true} mockAdapter={adapter}
      onOpenRepository={() => {}} onInitRepository={() => {}} onCloseRepository={() => {}} onWorkspaceChange={() => {}} />
  {:else}<p>Preparing main, origin/main, and B at the same commit…</p>{/if}
</main>

<style>
  nav { display: flex; gap: 12px; padding: 10px; font: 12px var(--gd-font-ui); }
  main { height: calc(100vh - 48px); min-width: 1100px; }
</style>
