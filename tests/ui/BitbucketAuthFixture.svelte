<script lang="ts">
  import BitbucketAuthModal from "../../src/lib/components/BitbucketAuthModal.svelte";
  import GitToolbar from "../../src/lib/components/GitToolbar.svelte";

  let open = $state(false);
  let token = $state("");
  let saved = $state(0);
</script>

<main>
  <GitToolbar
    onBranches={() => {}}
    remoteLabel="origin/main · ↑1 ↓0"
    remoteTitle="https://bitbucket.org/acme/repository.git"
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
    syncError="Push authentication was rejected by Bitbucket. Connect a scoped API token with Repository Read and Write permissions, then retry."
    syncErrorCode="AUTH_REQUIRED"
    syncRetryLabel="Retry push"
    onRetrySync={() => {}}
    bitbucketAvailable={true}
    onConnectBitbucket={() => (open = true)}
    logOpen={false}
    onToggleLog={() => {}}
    logLoading={false}
    logEntries={[]}
    logError={null}
  />
  <output aria-label="Saved count">Saved credentials: {saved}</output>
  {#if open}
    <BitbucketAuthModal
      remoteUrl="https://bitbucket.org/acme/repository.git"
      {token}
      busy={false}
      error={null}
      retryKind="push"
      onToken={(value) => (token = value)}
      onCreateToken={() => {}}
      onSubmit={() => { saved += 1; token = ""; open = false; }}
      onClose={() => { token = ""; open = false; }}
    />
  {/if}
</main>

<style>
  main { min-height: 100vh; background: var(--gd-canvas); color: var(--gd-text); }
  output { display: block; padding: 20px; font: 13px var(--gd-font-ui); }
</style>
