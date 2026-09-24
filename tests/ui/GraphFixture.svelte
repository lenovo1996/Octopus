<script lang="ts">
  // Browser-only QA entry; not imported by the production application.
  import HistoryPane from "../../src/lib/components/HistoryPane.svelte";
  import { layoutGraph } from "../../src/lib/graph/layout";
  import type { CommitRow } from "../../src/lib/ipc/types";

  let selectedOid = $state<string | null>(null);
  let wide = $state(false);
  let hidden = $state(false);
  const rows: CommitRow[] = Array.from({ length: 10_000 }, (_, i) => ({
    oid: `commit-${i}`, parents: i === 9999 ? [] : i === 0 ? Array.from({ length: 24 }, (_, j) => `commit-${j + 1}`) : [`commit-${Math.max(25, i + 1)}`],
    subject: `Commit ${i}: verify graph continuity and virtual scrolling`,
    authorName: "QA fixture", committedAt: "2026-09-23", authoredAt: "2026-09-23", refs: i === 0 ? ["refs/heads/main"] : [], boundary: false
  }));
  const graph = layoutGraph(rows, new Map());
  const noop = () => {};
</script>

<div class="fixture">
  <header>
    <strong>QA fixture · 10,000 commits · 24 lanes</strong>
    <button onclick={() => (wide = !wide)}>Toggle width</button>
    <button onclick={() => (hidden = !hidden)}>Toggle panel</button>
    <button onclick={() => (selectedOid = "commit-9999")}>Select oldest</button>
    <output>{selectedOid ?? "No selection"}</output>
  </header>
  <div class="panel" class:wide hidden={hidden}>
    <HistoryPane {rows} laid={graph.rows} laneCount={graph.laneCount}
      loading={false} loadingMore={false} error={null} hasMore={false} pageTruncated={false}
      totalHint="10000 loaded" emptyHint="Empty" {selectedOid} refLabels={new Map([["refs/heads/main", "main"]])}
      scopeValue="all" scopeOptions={[{ value: "all", label: "All refs" }]} onScopeChange={noop}
      searchActive={false} searchQuery="" searchRows={[]} searchLoading={false} searchLoadingMore={false}
      searchError={null} searchHasMore={false} searchIncomplete={false}
      onSearchLoadMore={noop} onSearchRetry={noop} onShowInGraph={noop} onClearSearch={noop}
      onCreateBranch={noop} onCommitAction={noop}
      onSelect={(oid) => (selectedOid = oid)} onRetry={noop} onLoadMore={noop} />
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
