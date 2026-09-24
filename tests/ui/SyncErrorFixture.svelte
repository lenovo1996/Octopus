<script lang="ts">
  import GitToolbar from "../../src/lib/components/GitToolbar.svelte";

  let retries = $state(0);
</script>

<main>
  <GitToolbar
    onBranches={() => {}}
    remoteLabel="origin/main · ↑1 ↓0"
    remoteTitle="https://example.com/acme/repo.git"
    syncDisabled={false}
    syncDisabledReason=""
    jobActive={false}
    jobLabel=""
    jobProgress={null}
    onFetch={() => {}}
    onPull={() => {}}
    onPush={() => {}}
    onMerge={() => {}}
    onStash={() => {}}
    onCancel={() => {}}
    syncError="Push authentication was rejected. Refresh the access token in your Git credential helper and verify repository access, then retry."
    syncErrorCode="AUTH_REQUIRED"
    syncRetryLabel="Retry push"
    onRetrySync={() => (retries += 1)}
    bitbucketAvailable={false}
    onConnectBitbucket={() => {}}
    logOpen={false}
    onToggleLog={() => {}}
    logLoading={false}
    logEntries={[]}
    logError={null}
  />
  <output aria-label="Retry count">Retry count: {retries}</output>
</main>

<style>
  main { min-height: 100vh; background: var(--gd-canvas); color: var(--gd-text); }
  output { display: block; padding: 20px; font: 13px var(--gd-font-ui); }
</style>
