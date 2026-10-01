<script lang="ts">
  import RepositoryWorkspace from "../../src/app/RepositoryWorkspace.svelte";
  import { createMockAdapter } from "../../src/lib/ipc/mock";
  import { demoSession } from "../../src/mocks/demoSession";
  import type { AppError, OperationRecord, RepoSnapshot } from "../../src/lib/ipc/types";

  let failPush = $state(true);
  let failForce = $state(false);
  let failPull = $state(false);
  let generation = $state(0);
  let calls: string[] = $state([]);
  let seed: RepoSnapshot = $state.raw({ ...demoSession, repoId: "push-fixture", workspaceKey: "fixture:push" });

  function record(operationId: string, kind: string, failed: AppError | null): OperationRecord {
    return {
      operationId,
      requestId: "fixture-request",
      repoId: seed.repoId,
      kind,
      state: failed ? "failed" : "succeeded",
      stage: failed ? "failed" : "done",
      progress: null,
      errorCode: failed?.code ?? null,
      error: failed
    };
  }

  const rejected: AppError = {
    code: "DIVERGED",
    message: "Push was rejected because the remote tip differs from yours. Fetch and merge, or force-push if you rewrote local history",
    recovery: "refresh",
    retryable: true
  };
  const pullBlocked: AppError = {
    code: "GIT_ERROR",
    message: "Pull could not fast-forward; the branches have diverged",
    recovery: "refresh",
    retryable: true
  };

  let adapter = $derived.by(() => {
    const base = createMockAdapter(seed);
    return {
      ...base,
      async remotePush() {
        calls = [...calls, "push"];
        await new Promise((resolve) => setTimeout(resolve, 50));
        return { operationId: "fixture-push" };
      },
      async remotePushForce() {
        calls = [...calls, "force"];
        await new Promise((resolve) => setTimeout(resolve, 50));
        return { operationId: "fixture-push-force" };
      },
      async remotePull() {
        calls = [...calls, "pull"];
        await new Promise((resolve) => setTimeout(resolve, 50));
        return { operationId: "fixture-pull" };
      },
      async operationGet(operationId: string) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        if (operationId === "fixture-push") return record(operationId, "push", failPush ? rejected : null);
        if (operationId === "fixture-push-force") return record(operationId, "push", failForce ? rejected : null);
        if (operationId === "fixture-pull") return record(operationId, "pull", failPull ? pullBlocked : null);
        return base.operationGet(operationId);
      }
    };
  });

  function reset() {
    generation += 1;
    calls = [];
    seed = { ...demoSession, repoId: `push-fixture-${generation}`, workspaceKey: `fixture:push:${generation}` };
  }
</script>

<nav aria-label="Fixture controls">
  <label><input type="checkbox" bind:checked={failPush} />Fail push (rejected)</label>
  <label><input type="checkbox" bind:checked={failForce} />Fail force push</label>
  <label><input type="checkbox" bind:checked={failPull} />Fail pull</label>
  <button onclick={reset}>Reset workspace</button>
</nav>
<output aria-label="Sync calls">{calls.join(",")}</output>
<main>
  {#key generation}
    <RepositoryWorkspace initialSession={seed} active={true} mockAdapter={adapter}
      onOpenRepository={() => {}} onInitRepository={() => {}} onCloseRepository={() => {}} onWorkspaceChange={() => {}} />
  {/key}
</main>

<style>
  nav { display: flex; flex-wrap: wrap; gap: 12px; padding: 10px; font: 12px var(--gd-font-ui); }
  output { display: block; padding: 0 10px 10px; font: 12px var(--gd-font-code); color: var(--gd-text-secondary); }
  main { height: calc(100vh - 110px); min-width: 1100px; }
</style>
