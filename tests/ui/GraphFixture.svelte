<script lang="ts">
  // Browser-only QA entry; not imported by the production application.
  import HistoryPane from "../../src/lib/components/HistoryPane.svelte";
  import { layoutGraph } from "../../src/lib/graph/layout";
  import type { CommitRow, RefItem } from "../../src/lib/ipc/types";

  let selectedOid = $state<string | null>(null);
  let wide = $state(false);
  let hidden = $state(false);
  let historyState = $state("loaded");
  const rows: CommitRow[] = Array.from({ length: 10_000 }, (_, i) => ({
    oid: `commit-${i}`, parents: i === 9999 ? [] : i === 0 ? Array.from({ length: 24 }, (_, j) => `commit-${j + 1}`) : [`commit-${Math.max(25, i + 1)}`],
    subject: `Commit ${i}: verify graph continuity and virtual scrolling`,
    authorName: "QA fixture", committedAt: "2026-09-23", authoredAt: "2026-09-23", refs: i === 0 ? ["refs/heads/main"] : [], boundary: false
  }));
  const refs: RefItem[] = rows.slice(0, 25).map((row, i) => {
    const kind = i === 0 || i % 3 === 0 ? "local" : i % 3 === 1 ? "remote" : "tag";
    const label = i === 0 ? "main" : `${kind === "remote" ? "origin/" : ""}feature/long-branch-name-${i}`;
    const fullName = `refs/${kind === "local" ? "heads" : kind === "remote" ? "remotes" : "tags"}/${label}`;
    return { refId: fullName, fullName, label, kind, oid: row.oid, current: i === 0, checkedOutElsewhere: false };
  });
  refs.push({ ...refs[0], refId: "refs/remotes/origin/main", fullName: "refs/remotes/origin/main", label: "origin/main", kind: "remote", current: false });
  const graph = layoutGraph(rows, new Map());
  const noop = () => {};
</script>

<div class="fixture">
  <header>
    <strong>QA fixture · 10,000 commits · 24 lanes</strong>
    <button onclick={() => (wide = !wide)}>Toggle width</button>
    <button onclick={() => (hidden = !hidden)}>Toggle panel</button>
    <button onclick={() => (selectedOid = "commit-9999")}>Select oldest</button>
    <select aria-label="History state" bind:value={historyState}>
      <option value="loaded">Loaded</option><option value="loading">Loading</option>
      <option value="empty">Empty</option><option value="error">Error</option>
    </select>
    <output>{selectedOid ?? "No selection"}</output>
  </header>
  <div class="panel" class:wide hidden={hidden}>
    <HistoryPane rows={historyState === "loaded" ? rows : []} {refs} laid={graph.rows} laneCount={graph.laneCount}
      loading={historyState === "loading"} loadingMore={false}
      error={historyState === "error" ? { code: "GIT_ERROR", message: "Fixture read failure", recovery: "retryRead", retryable: true } : null}
      hasMore={false} pageTruncated={false}
      totalHint={historyState === "loaded" ? "10000 loaded" : "0 loaded"} emptyHint="Empty" {selectedOid} refLabels={new Map(refs.map(ref => [ref.refId, ref.label]))}
      scopeValue="all" scopeOptions={[{ value: "all", label: "All refs" }]} onScopeChange={noop}
      searchActive={false} searchQuery="" searchRows={[]} searchLoading={false} searchLoadingMore={false}
      searchError={null} searchHasMore={false} searchIncomplete={false}
      onSearchLoadMore={noop} onSearchRetry={noop} onShowInGraph={noop} onClearSearch={noop}
      onCreateBranch={noop} onCommitAction={noop} branchActionsDisabled={false}
      onSelect={(oid) => (selectedOid = oid)} onRetry={() => (historyState = "loaded")} onLoadMore={noop} />
  </div>
</div>

<style>
  .fixture { padding: 16px; font: 13px var(--gd-font-ui); }
  header { display: flex; gap: 12px; align-items: center; margin-bottom: 16px; }
  button { padding: 6px 10px; }
  .panel { display: flex; width: 500px; height: 650px; border: 1px solid var(--gd-border); }
  .panel.wide { width: 1000px; }
  .panel[hidden] { display: none; }
</style>
