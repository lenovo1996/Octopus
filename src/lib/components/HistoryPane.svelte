<script lang="ts">
  // HistoryPane: virtualized commit list + lane graph (T05) with scope
  // selector, search-result mode and keyboard navigation (T06).
  // Search results render as a plain list on purpose: result rows and graph
  // topology never share a gutter, so topology cannot leak into results.
  import { onMount, untrack } from "svelte";
  import type { AppError, CommitRow, RefItem } from "../ipc/types";
  import type { WorkingSummary } from "../status/partition";
  import { windowRows, type LaidRow } from "../graph/layout";

  import ColumnResize from "./ColumnResize.svelte";
  import { COLUMNS_KEY, COLUMN_LIMITS, clampColumn, defaultColumns, restoreColumns, type HistoryColumn } from "../history/columns";
  import { formatDateTime } from "../format/date";
  import { badgeLabel, branchTipPlacement, primaryBadge, refItemForBadge, refsByCommit, type RefBadge } from "../history/refs";
  import RefKindIcon from "./RefKindIcon.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import { isContextMenuKey, pointFromContextEvent, type ContextMenuItem } from "../context-menu/model";
  import { COMMIT_ACTIONS, type CommitActionId } from "../history/commit-menu";
  import { buildBranchMenuItems, canCheckoutRef, type BranchMenuAction } from "../refs/branch-menu";

  export interface ScopeOption {
    value: string;
    label: string;
  }

  interface Props {
    rows: CommitRow[];
    laid: LaidRow[];
    laneCount: number;
    loading: boolean;
    loadingMore: boolean;
    error: AppError | null;
    hasMore: boolean;
    pageTruncated: boolean;
    totalHint: string;
    emptyHint: string;
    selectedOid: string | null;
    refLabels: Map<string, string>;
    refs?: RefItem[];
    preferredRefId?: string | null;
    scopeValue: string;
    scopeOptions: ScopeOption[];
    onScopeChange: (value: string) => void;
    searchActive: boolean;
    searchQuery: string;
    searchRows: CommitRow[];
    searchLoading: boolean;
    searchLoadingMore: boolean;
    searchError: AppError | null;
    searchHasMore: boolean;
    searchIncomplete: boolean;
    onSearchLoadMore: () => void;
    onSearchRetry: () => void;
    onShowInGraph: (oid: string) => void;
    onClearSearch: () => void;
    onCreateBranch: (oid: string) => void;
    onCommitAction: (action: CommitActionId, row: CommitRow) => void;
    onSelect: (oid: string) => void;
    onRetry: () => void;
    onLoadMore: () => void;
    /** Same lock as the sidebar branch menu. */
    branchActionsDisabled: boolean;
    /** Merge target label for the badge menu; null disables merge. */
    mergeTarget?: string | null;
    mergeBusy?: boolean;
    /** When set, right-clicking a branch badge opens the branch menu. */
    onBranchAction?: (action: BranchMenuAction, ref: RefItem) => void;
    /** Working-changes summary pinned under the header; null hides the bar. */
    workBar?: WorkingSummary | null;
    onOpenWorkingChanges?: () => void;
  }

  let {
    rows,
    laid,
    laneCount,
    loading,
    loadingMore,
    error,
    hasMore,
    totalHint,
    emptyHint,
    selectedOid,
    refLabels,
    refs = [],
    preferredRefId = null,
    scopeValue,
    scopeOptions,
    onScopeChange,
    searchActive,
    searchQuery,
    searchRows,
    searchLoading,
    searchLoadingMore,
    searchError,
    searchHasMore,
    onSearchLoadMore,
    onSearchRetry,
    onShowInGraph,
    onClearSearch,
    onCreateBranch,
    onCommitAction,
    onSelect,
    onRetry,
    onLoadMore,
    branchActionsDisabled,
    onBranchAction,
    mergeTarget = null,
    mergeBusy = false,
    workBar = null,
    onOpenWorkingChanges
  }: Props = $props();

  const rowHeight = 28;
  const headerHeight = 28;
  const laneWidth = 20;
  const overscan = 8;
  let scrollTop = $state(0);
  let viewportHeight = $state(600);
  let graphScrollLeft = $state(0);
  let graphScrollEl: HTMLDivElement | undefined = $state(undefined);
  let viewportWidth = $state(760);
  let columns = $state(defaultColumns());
  let viewportEl: HTMLElement | undefined = $state(undefined);
  // Keyboard focus ring, separate from the details selection.
  let focusOid: string | null = $state(null);
  let rowMenu = $state<{ row: CommitRow; x: number; y: number } | null>(null);
  let badgeMenu = $state<{ ref: RefItem; x: number; y: number } | null>(null);
  let branchTip = $state<{ x: number; y: number; above: boolean; badges: RefBadge[] } | null>(null);

  const laidByOid = $derived(new Map(laid.map((r) => [r.oid, r])));
  const window = $derived(windowRows(activeRows(), scrollTop, Math.max(0, viewportHeight - headerHeight), rowHeight, overscan));
  const visible = $derived(activeRows().slice(window.start, window.end));
  // The column is a viewport, not a minimum imposed by the repository's lane count.
  const graphWidth = $derived(columns.graph ?? 84);
  const graphContentWidth = $derived(Math.max(graphWidth, laneCount * laneWidth + 16));
  const branchWidth = $derived(columns.branches ?? 170);
  const authorWidth = $derived(columns.author ?? 140);
  const dateWidth = $derived(columns.date ?? COLUMN_LIMITS.date.initial);
  const subjectWidth = $derived(columns.subject ?? Math.max(200, viewportWidth - authorWidth - (searchActive ? 110 : graphWidth + branchWidth + dateWidth)));
  const tableWidth = $derived(subjectWidth + authorWidth + (searchActive ? 110 : graphWidth + branchWidth + dateWidth));
  const columnTemplate = $derived(`${searchActive ? "" : `${branchWidth}px ${graphWidth}px `}${subjectWidth}px ${authorWidth}px${searchActive ? " 110px" : ` ${dateWidth}px`}`);
  const badgesByOid = $derived(refsByCommit(refs));
  const refById = $derived(new Map(refs.map((ref) => [ref.refId, ref])));
  const headerColumns = $derived((searchActive ? ["subject", "author"] : ["branches", "graph", "subject", "author", "date"]) as HistoryColumn[]);
  const labels = { branches: "Branch", graph: "Graph", subject: "Subject", author: "Author", date: "Date" };

  onMount(() => {
    try { columns = restoreColumns(localStorage.getItem(COLUMNS_KEY)); } catch { /* Storage unavailable. */ }
  });
  function widthOf(column: HistoryColumn): number {
    return { branches: branchWidth, graph: graphWidth, subject: subjectWidth, author: authorWidth, date: dateWidth }[column];
  }
  function resizeColumn(column: HistoryColumn, delta: number): void {
    columns = { ...columns, [column]: clampColumn(column, widthOf(column) + delta) };
    saveColumns();
  }
  function resetColumn(column: HistoryColumn): void {
    columns = { ...columns, [column]: defaultColumns()[column] };
    saveColumns();
  }
  function saveColumns(): void {
    try { localStorage.setItem(COLUMNS_KEY, JSON.stringify(columns)); } catch { /* Best-effort UI preference. */ }
  }
  function badgesFor(row: CommitRow): RefBadge[] {
    return badgesByOid.get(row.oid) ?? row.refs.map(id => ({
      id, name: refLabels.get(id) ?? id.replace(/^refs\/(heads|remotes|tags)\//, ""),
      source: id.startsWith("refs/remotes/") ? "remote" : id.startsWith("refs/tags/") ? "tag" : "local",
      kind: id.startsWith("refs/remotes/") ? "remote" : id.startsWith("refs/tags/") ? "tag" : "local",
      fullName: id,
      current: refById.get(id)?.current ?? false
    }));
  }

  const laneColors = ["var(--gd-lane-1)", "var(--gd-lane-2)", "var(--gd-lane-3)", "var(--gd-lane-4)", "var(--gd-lane-5)", "var(--gd-lane-6)"];

  function laneX(lane: number): number {
    return 8 + lane * laneWidth + laneWidth / 2;
  }

  function color(lane: number): string {
    return laneColors[lane % laneColors.length];
  }

  function edgePath(fromLane: number, toLane: number): string {
    const from = laneX(fromLane);
    const to = laneX(toLane);
    const center = rowHeight / 2;
    if (from === to) return `M ${from} ${center} V ${rowHeight}`;
    const direction = Math.sign(to - from);
    const radius = Math.min(6, Math.abs(to - from) / 2, rowHeight / 4);
    return `M ${from} ${center} H ${to - direction * radius} Q ${to} ${center}, ${to} ${center + radius} V ${rowHeight}`;
  }

  function onGraphScrollKeyDown(event: KeyboardEvent): void {
    const el = event.currentTarget as HTMLElement;
    const step = event.shiftKey ? 160 : 40;
    let next: number;
    if (event.key === "ArrowLeft") next = el.scrollLeft - step;
    else if (event.key === "ArrowRight") next = el.scrollLeft + step;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = el.scrollWidth - el.clientWidth;
    else return;
    event.preventDefault();
    event.stopPropagation();
    el.scrollLeft = next;
    graphScrollLeft = el.scrollLeft;
  }

  function onScroll(e: Event): void {
    rowMenu = null;
    badgeMenu = null;
    hideBranchTip();
    const el = e.currentTarget as HTMLElement;
    scrollTop = el.scrollTop;
    viewportHeight = el.clientHeight;
    const nearBottom = el.scrollTop + el.clientHeight > el.scrollHeight - 200;
    if (searchActive) {
      if (nearBottom && searchHasMore && !searchLoadingMore && !searchLoading) onSearchLoadMore();
    } else if (nearBottom && hasMore && !loadingMore && !loading) {
      onLoadMore();
    }
  }

  function shortOid(oid: string): string {
    return oid.slice(0, 7);
  }

  function openRowMenu(event: MouseEvent | KeyboardEvent, row: CommitRow): void {
    event.preventDefault();
    event.stopPropagation();
    badgeMenu = null;
    hideBranchTip();
    focusOid = row.oid;
    const point = pointFromContextEvent(event);
    rowMenu = { row, x:point.left, y:point.top };
  }

  let tipEl: HTMLDivElement | undefined = $state(undefined);
  let tipTimer: ReturnType<typeof setTimeout> | null = null;

  function showBranchTip(event: MouseEvent | FocusEvent, badges: RefBadge[]): void {
    if (badges.length < 2) return;
    if (tipTimer !== null) {
      clearTimeout(tipTimer);
      tipTimer = null;
    }
    const el = event.currentTarget as HTMLElement;
    const rect = el.getBoundingClientRect();
    const at = branchTipPlacement(
      { left: rect.left, top: rect.top, bottom: rect.bottom },
      document.documentElement.clientWidth,
      document.documentElement.clientHeight
    );
    branchTip = { ...at, badges };
  }

  function hideBranchTip(): void {
    if (tipTimer !== null) {
      clearTimeout(tipTimer);
      tipTimer = null;
    }
    branchTip = null;
  }

  /** Grace period for moving the pointer or focus from the badge into the panel. */
  function scheduleHideBranchTip(): void {
    if (tipTimer !== null) clearTimeout(tipTimer);
    tipTimer = setTimeout(() => {
      branchTip = null;
      tipTimer = null;
    }, 250);
  }

  function cancelHideBranchTip(): void {
    if (tipTimer !== null) {
      clearTimeout(tipTimer);
      tipTimer = null;
    }
  }

  function tipFocusOut(event: FocusEvent): void {
    const to = event.relatedTarget;
    if (to instanceof Node && tipEl?.contains(to)) return;
    hideBranchTip();
  }

  function doubleClickBadge(badge: RefBadge): void {
    if (branchActionsDisabled || !onBranchAction) return;
    const ref = refItemForBadge(refs, badge);
    if (!ref || ref.current || !canCheckoutRef(ref)) return;
    onBranchAction("checkout", ref);
  }

  function openBadgeMenu(event: MouseEvent, badge: RefBadge): void {
    event.preventDefault();
    event.stopPropagation();
    if (!onBranchAction) return;
    const ref = refItemForBadge(refs, badge);
    if (!ref) return;
    rowMenu = null;
    const point = pointFromContextEvent(event);
    badgeMenu = { ref, x: point.left, y: point.top };
  }

  function badgeMenuItems(ref: RefItem): ContextMenuItem[] {
    return buildBranchMenuItems(
      ref,
      { actionsDisabled: branchActionsDisabled, selectedCommitOid: selectedOid, mergeTarget, mergeBusy },
      (action, target) => onBranchAction?.(action, target),
      (text) => {
        void navigator.clipboard.writeText(text);
      }
    );
  }
  function rowMenuKey(event: KeyboardEvent, row: CommitRow): void {
    if (isContextMenuKey(event)) openRowMenu(event, row);
  }

  function rowMenuItems(row: CommitRow): ContextMenuItem[] {
    const items: ContextMenuItem[] = COMMIT_ACTIONS.map((item) => ({
      id: item.id,
      label: item.label,
      disabled: !item.available,
      danger: item.danger,
      separatorBefore: item.separatorBefore,
      hint: item.available ? undefined : "Planned",
      title: item.unavailableReason,
      action: () => runCommitAction(item.id, row)
    }));
    items.push({ id:"details", label:"Open commit details", separatorBefore:true, action:() => onSelect(row.oid) });
    if (searchActive) items.push({ id:"show", label:"Show in graph", action:() => onShowInGraph(row.oid) });
    items.push(
      { id:"copy-id", label:"Copy commit ID", separatorBefore:true, action:() => navigator.clipboard.writeText(row.oid) },
      { id:"copy-subject", label:"Copy subject", action:() => navigator.clipboard.writeText(row.subject) }
    );
    return items;
  }

  function runCommitAction(action: CommitActionId, row: CommitRow): void {
    if (action === "create-branch") onCreateBranch(row.oid);
    else onCommitAction(action, row);
  }

  function activeRows(): CommitRow[] {
    return searchActive ? searchRows : rows;
  }

  function moveFocus(delta: -1 | 1): void {
    const list = activeRows();
    if (list.length === 0) return;
    const index = list.findIndex((r) => r.oid === focusOid);
    const next = index < 0 ? (delta < 0 ? list.length - 1 : 0) : Math.min(list.length - 1, Math.max(0, index + delta));
    focusOid = list[next].oid;
    revealRow(focusOid);
  }

  function revealRow(oid: string): void {
    if (!viewportEl) return;
    const index = activeRows().findIndex((row) => row.oid === oid);
    if (index < 0) return;
    const top = index * rowHeight;
    const bottom = top + rowHeight;
    if (top < viewportEl.scrollTop) viewportEl.scrollTop = top;
    else if (bottom > viewportEl.scrollTop + viewportEl.clientHeight - headerHeight) {
      viewportEl.scrollTop = bottom - viewportEl.clientHeight + headerHeight;
    }
    scrollTop = viewportEl.scrollTop;
    const lane = laidByOid.get(oid)?.lane;
    if (!searchActive && lane !== undefined && graphScrollEl) {
      const x = laneX(lane);
      if (x - 8 < graphScrollLeft) graphScrollEl.scrollLeft = Math.max(0, x - 8);
      else if (x + 8 > graphScrollLeft + graphWidth) graphScrollEl.scrollLeft = x + 8 - graphWidth;
      graphScrollLeft = graphScrollEl.scrollLeft;
    }
  }

  $effect(() => {
    const oid = selectedOid;
    if (oid) untrack(() => revealRow(oid));
  });

  function onViewportKeyDown(e: KeyboardEvent): void {
    if (e.defaultPrevented) return;
    if (graphScrollEl?.contains(e.target as Node)) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveFocus(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      moveFocus(-1);
    } else if ((e.key === "Enter" || e.key === " ") && focusOid !== null) {
      e.preventDefault();
      onSelect(focusOid);
    } else if (e.key === "Escape" && searchActive) {
      e.preventDefault();
      onClearSearch();
    }
  }

  // Keep the focus ring on a visible row when the list changes.
  $effect(() => {
    const list = activeRows();
    if (focusOid !== null && !list.some((r) => r.oid === focusOid)) {
      focusOid = selectedOid && list.some((r) => r.oid === selectedOid) ? selectedOid : null;
    }
  });

  // A new search or scope starts at the top of the list.
  $effect(() => {
    searchQuery;
    scopeValue;
    // Scroll-element mounts during resizing must not restart the history.
    untrack(() => {
      rowMenu = null;
      scrollTop = 0;
      graphScrollLeft = 0;
      graphScrollEl?.scrollTo({ left: 0 });
      viewportEl?.scrollTo({ top: 0, left: 0 });
    });
  });

  $effect(() => {
    const max = graphContentWidth - graphWidth;
    untrack(() => {
      graphScrollLeft = Math.min(graphScrollLeft, max);
      if (graphScrollEl) graphScrollEl.scrollLeft = graphScrollLeft;
    });
  });
</script>

<section class="gd-history" aria-label="Commit history" style={`--history-width: ${tableWidth}px; --history-columns: ${columnTemplate}`}>
  <div class="gd-history-bar">
    <div class="gd-scope">
      <label for="gd-history-scope">Showing</label>
      <select
        id="gd-history-scope"
        value={scopeValue}
        onchange={(e) => onScopeChange(e.currentTarget.value)}
      >
        {#each scopeOptions as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </div>
    <span class="gd-total">{totalHint}</span>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions: keyboard navigation in virtual commit list -->
  <div class="gd-viewport" role="region" aria-label="Scrollable commit history"
    onscroll={onScroll} onkeydown={onViewportKeyDown} tabindex="0" bind:clientHeight={viewportHeight} bind:clientWidth={viewportWidth} bind:this={viewportEl}>
    <div class="gd-history-header">
      {#each headerColumns as column (column)}
        <div class="gd-column-head" class:gd-graph-head={column === "graph" && graphContentWidth > graphWidth}>
          <span>{labels[column]}</span>
          {#if column === "graph" && graphContentWidth > graphWidth}
            <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions: native horizontal scroll area with scoped keyboard navigation -->
            <div class="gd-lane-scroll" role="region" aria-label="Scroll graph lanes" tabindex="0"
              title="Scroll to see more graph lanes" bind:this={graphScrollEl}
              onkeydown={onGraphScrollKeyDown}
              onscroll={(event) => (graphScrollLeft = event.currentTarget.scrollLeft)}>
              <div style={`width: ${graphContentWidth}px; height: 1px`}></div>
            </div>
          {/if}
          <ColumnResize label={labels[column]} width={widthOf(column)}
            min={COLUMN_LIMITS[column].min} max={COLUMN_LIMITS[column].max}
            onResize={(delta) => resizeColumn(column, delta)} onReset={() => resetColumn(column)} />
        </div>
      {/each}
      {#if searchActive}<span></span>{/if}
    </div>
    {#if workBar}
      <button
        type="button"
        class="gd-workbar"
        title="Open working changes"
        aria-label={`${workBar.total} uncommitted changes. Open working changes.`}
        onclick={() => onOpenWorkingChanges?.()}
      >
        <strong>{workBar.total} changed</strong>
        <span class="gd-workbar-stats">
          {#if workBar.added}<span class="gd-added">+{workBar.added} added</span>{/if}
          {#if workBar.modified}<span class="gd-modified">{workBar.modified} modified</span>{/if}
          {#if workBar.deleted}<span class="gd-deleted">−{workBar.deleted} deleted</span>{/if}
        </span>
        <span class="gd-workbar-go" aria-hidden="true">→</span>
      </button>
    {/if}
  {#if searchActive ? searchLoading && searchRows.length === 0 : loading && rows.length === 0}
    <p class="gd-state" role="status">{searchActive ? "Searching…" : "Loading history…"}</p>
  {:else if searchActive ? searchError : error}
    {@const failure = searchActive ? searchError : error}
    <div class="gd-state" role="alert"><p>{searchActive ? "Search" : "History"} failed ({failure?.code}): {failure?.message}</p>
      <button onclick={searchActive ? onSearchRetry : onRetry}>Retry</button>
    </div>
  {:else if activeRows().length === 0}
    <p class="gd-state">{searchActive ? `No commits match “${searchQuery}”.` : emptyHint}</p>
  {:else}
    <div role="list" aria-label={searchActive ? `Search results, ${searchRows.length} shown` : `Commits, ${totalHint}`}>
      <div style={`height: ${window.start * rowHeight}px`}></div>
      {#each visible as row, i (row.oid)}
        {@const laidRow = laidByOid.get(row.oid)}
        {@const badges = badgesFor(row)}
        {@const committedAt = formatDateTime(row.committedAt)}
        {@const primary = primaryBadge(badges, preferredRefId)}
        {@const nodeRadius = row.parents.length > 1 ? 5 : 4}
        {@const nodeX = laneX(laidRow?.lane ?? 0) - graphScrollLeft}
        {@const showBranchLink = primary && laidRow && nodeX >= nodeRadius && nodeX <= graphWidth - nodeRadius}
        <div class="gd-list-row" class:contexted={rowMenu?.row.oid === row.oid} role="listitem"
          aria-setsize={activeRows().length} aria-posinset={window.start + i + 1}
          oncontextmenu={(event) => openRowMenu(event, row)} onkeydown={(event) => rowMenuKey(event, row)}>
          <button type="button" class="gd-commit-row" class:selected={selectedOid === row.oid} class:focused={focusOid === row.oid && selectedOid !== row.oid}
            style:--commit-color={color(laidRow?.lane ?? 0)}
            aria-current={selectedOid === row.oid ? "true" : undefined}
            onclick={() => { focusOid = row.oid; onSelect(row.oid); }}
            aria-label={`${row.subject}, ${row.authorName}, ${row.parents.length} parent${row.parents.length === 1 ? "" : "s"}`}>
            {#if !searchActive}
              <span class="gd-branches">
                {#if primary}
                  <span class="gd-ref-labels">
                  <span
                    class="gd-ref-badge"
                    class:current={primary.current}
                    tabindex="0"
                    role="button"
                    aria-label={`${primary.source} ${primary.name}`}
                    oncontextmenu={(event) => openBadgeMenu(event, primary)}
                    ondblclick={() => doubleClickBadge(primary)}
                    onmouseenter={(event) => showBranchTip(event, badges)}
                    onmouseleave={scheduleHideBranchTip}
                    onfocus={(event) => showBranchTip(event, badges)}
                    onblur={scheduleHideBranchTip}
                    onkeydown={(event) => {
                      if (event.key === "Escape") hideBranchTip();
                      else if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        doubleClickBadge(primary);
                      }
                    }}
                  >
                    <span class="gd-ref-source"><RefKindIcon kind={primary.kind} /></span><span class="gd-ref-name">{badgeLabel(primary)}</span>
                  </span>
                  {#if badges.length > 1}<span class="gd-ref-extra">+{badges.length - 1}</span>{/if}
                  </span>
                  <svg class="gd-branch-link" class:visible={showBranchLink} height={rowHeight} aria-hidden="true">
                    <rect y={rowHeight / 2} width="100%" height="1" fill="var(--commit-color)" opacity="0.5" shape-rendering="crispEdges" />
                  </svg>
                {/if}
              </span>
            <span class="gd-graph-clip">
            <svg
              class="gd-graph"
              style={`width: ${graphContentWidth}px; transform: translateX(${-graphScrollLeft}px)`}
              height={rowHeight}
              aria-hidden="true"
            >
              <g class="gd-graph-links">
                {#if showBranchLink}
                  <rect class="gd-graph-branch-link" x={graphScrollLeft} y={rowHeight / 2}
                    width={nodeX - nodeRadius} height="1" fill="var(--commit-color)" opacity="0.5" shape-rendering="crispEdges" />
                {/if}
                {#each laidRow?.rails ?? [] as rail (rail)}
                  <line
                    x1={laneX(rail)}
                    y1="0"
                    x2={laneX(rail)}
                    y2={rowHeight}
                    stroke={color(rail)}
                    stroke-width="2"
                    stroke-opacity="0.9"
                    stroke-linecap="round"
                  />
                {/each}
                {#if laidRow?.incoming}
                  <line x1={laneX(laidRow.lane)} y1="0" x2={laneX(laidRow.lane)} y2={rowHeight / 2}
                    stroke={color(laidRow.lane)} stroke-width="2" />
                {/if}
                {#each laidRow?.edges ?? [] as edge, edgeIndex (edge.parentOid)}
                  <path
                    d={edgePath(edge.fromLane, edge.toLane)}
                    fill="none"
                    stroke={color(edgeIndex === 0 ? edge.fromLane : edge.toLane)}
                    stroke-width="2"
                    stroke-opacity="0.9"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                {/each}
              </g>
              <circle
                cx={laneX(laidRow?.lane ?? 0)}
                cy={rowHeight / 2}
                r={nodeRadius}
                fill={row.parents.length > 1 ? "var(--commit-color)" : "var(--gd-canvas)"}
                stroke="var(--commit-color)"
                stroke-width="2.5"
              />
            </svg>
            </span>
            {/if}
            <span class="gd-subject" title={row.subject}>{#if row.boundary}<span title="Shallow boundary">◇ </span>{/if}{row.subject}</span>
            <span class="gd-author" title={`${row.authorName} · ${committedAt} · ${row.oid}`}>{row.authorName}<small>{shortOid(row.oid)}</small></span>
            {#if !searchActive}
              <span class="gd-date" title={committedAt}>{committedAt}</span>
            {/if}
          </button>
          {#if searchActive}<button class="gd-show-graph" onclick={() => onShowInGraph(row.oid)}>Show in graph</button>{/if}
        </div>
      {/each}
      <div style={`height: ${(activeRows().length - window.end) * rowHeight}px`}></div>
      {#if searchActive ? searchLoadingMore : loadingMore}<p class="gd-more" role="status">Loading more…</p>{/if}
    </div>
  {/if}
  </div>
  {#if rowMenu}<ContextMenu x={rowMenu.x} y={rowMenu.y} items={rowMenuItems(rowMenu.row)}
    label={`Commit actions for ${shortOid(rowMenu.row.oid)}`} onClose={() => (rowMenu = null)} />{/if}
  {#if badgeMenu}<ContextMenu x={badgeMenu.x} y={badgeMenu.y} items={badgeMenuItems(badgeMenu.ref)}
    label={`Branch actions for ${badgeMenu.ref.label}`} onClose={() => (badgeMenu = null)} />{/if}
  {#if branchTip}
    <div
      class="gd-branch-tip"
      class:above={branchTip.above}
      style={`left: ${branchTip.x}px; top: ${branchTip.y}px`}
      role="menu"
      aria-label="Branches on this commit"
      tabindex="-1"
      bind:this={tipEl}
      onmouseenter={cancelHideBranchTip}
      onmouseleave={hideBranchTip}
      onfocusin={cancelHideBranchTip}
      onfocusout={tipFocusOut}
      onkeydown={(event) => { if (event.key === "Escape") hideBranchTip(); }}
    >
      {#each branchTip.badges as badge (badge.id)}
        {@const tipRef = refItemForBadge(refs, badge)}
        {@const tipDisabled = tipRef === null || tipRef.current || branchActionsDisabled || !canCheckoutRef(tipRef)}
        <button
          type="button"
          role="menuitem"
          class="gd-tip-branch"
          class:remote={badge.kind === "remote"}
          class:tag={badge.kind === "tag"}
          disabled={tipDisabled}
          title={tipRef?.current ? "Already checked out" : badge.kind === "tag" ? "Tags cannot be checked out" : `Check out ${badge.name}`}
          onclick={() => {
            hideBranchTip();
            doubleClickBadge(badge);
          }}
        >
          <RefKindIcon kind={badge.kind} /><span class="gd-tip-name">{badgeLabel(badge)}</span>{#if tipRef?.current}<span class="gd-tip-current" aria-label="current">•</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</section>

<style>
  .gd-history { flex: 1 1 auto; display: flex; flex-direction: column; min-width: 0; min-height: 0; overflow: hidden; background: var(--gd-canvas); }
  .gd-history-bar { display: flex; align-items: center; gap: 8px; min-height: 28px; padding: 1px 12px; border-bottom: 1px solid var(--gd-border); font-size: var(--gd-font-size-small); flex: 0 0 auto; }
  .gd-scope { display: flex; align-items: center; gap: 8px; }
  .gd-scope label { color: var(--gd-text-secondary); white-space: nowrap; }
  select { max-width: 220px; padding: 1px 4px; border: 1px solid var(--gd-border); border-radius: 4px; background: var(--gd-panel); color: var(--gd-text); font: inherit; }
  .gd-total { margin-left: auto; color: var(--gd-text-secondary); white-space: nowrap; }
  .gd-show-graph, .gd-state button { background: var(--gd-panel); color: var(--gd-accent); border: 1px solid var(--gd-border); border-radius: 4px; padding: 4px 8px; cursor: pointer; font: inherit; }
  .gd-history-header, .gd-commit-row { display: grid; grid-template-columns: var(--history-columns); width: var(--history-width); }
  .gd-history-header { position: sticky; top: 0; z-index: 3; height: 28px; grid-template-rows: minmax(0, 1fr); background: var(--gd-panel); border-bottom: 1px solid var(--gd-border); color: var(--gd-text-secondary); font-size: var(--gd-font-size-small); }
  .gd-workbar {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--gd-space-3);
    position: sticky;
    top: 28px;
    left: 0;
    z-index: 2;
    width: 100%;
    min-height: 28px;
    padding: 4px var(--gd-space-3);
    color: var(--gd-text);
    background: var(--gd-surface-raised);
    border: 0;
    border-bottom: 1px solid var(--gd-border);
    cursor: pointer;
    font-size: var(--gd-font-size-small);
    text-align: left;
  }
  .gd-workbar:hover { background: var(--gd-surface-hover); }
  .gd-workbar:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: -2px; }
  .gd-workbar-stats { display: flex; gap: var(--gd-space-2); color: var(--gd-text-secondary); }
  .gd-workbar .gd-added { color: var(--gd-accent); }
  .gd-workbar .gd-modified { color: var(--gd-warning); }
  .gd-workbar .gd-deleted { color: var(--gd-danger); }
  .gd-workbar-go { color: var(--gd-text-secondary); }
  .gd-column-head { position: relative; display: flex; align-items: center; min-width: 0; min-height: 0; overflow: hidden; }
  .gd-column-head > span { padding-left: 12px; }
  .gd-graph-head { flex-direction: column; align-items: stretch; justify-content: space-between; }
  .gd-graph-head > span { line-height: 14px; flex: 0 0 auto; }
  .gd-lane-scroll { width: 100%; flex: 1 1 0; min-height: 0; overflow-x: scroll; overflow-y: hidden; }
  .gd-lane-scroll:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: -2px; }
  .gd-viewport { flex: 1 1 0; overflow-x: scroll; overflow-y: auto; min-width: 0; min-height: 0; scrollbar-gutter: stable; }
  .gd-list-row { position: relative; width: var(--history-width); }
  .gd-list-row.contexted .gd-commit-row { background: var(--gd-surface-hover); box-shadow: inset 3px 0 var(--gd-focus), inset 0 -1px color-mix(in srgb, var(--gd-border) 45%, transparent); }
  .gd-commit-row { align-items: center; height: 28px; padding: 0; color: var(--gd-text); background: transparent; border: 0; box-shadow: inset 0 -1px color-mix(in srgb, var(--gd-border) 45%, transparent); text-align: left; font: inherit; cursor: pointer; }
  .gd-commit-row:hover { background: var(--gd-surface-hover); }
  .gd-commit-row.selected { background: var(--gd-surface-selected); }
  .gd-commit-row.focused, button:focus-visible, .gd-viewport:focus-visible, select:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: -2px; }
  .gd-branches { display: flex; align-items: center; height: 28px; min-width: 0; overflow: hidden; padding: 1px 0 1px 8px; }
  .gd-ref-labels { display: inline-flex; align-items: center; gap: 4px; flex: 0 1 auto; min-width: 0; }
  .gd-branch-link { display: block; flex: 1 0 8px; width: 8px; height: 28px; visibility: hidden; pointer-events: none; }
  .gd-branch-link.visible { visibility: visible; }
  .gd-ref-badge { display: inline-flex; align-items: center; gap: 4px; flex: 0 1 auto; min-width: 0; max-width: 100%; border: 1px solid var(--commit-color); border-radius: var(--gd-radius-control); background: transparent; color: var(--commit-color); font-size: 11px; line-height: 1; padding: 3px 6px; white-space: nowrap; }
  .gd-ref-badge.current { background: var(--commit-color); color: var(--gd-on-accent); }
  .gd-ref-source { display: inline-flex; }
  .gd-ref-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .gd-ref-badge:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 1px; }
  .gd-ref-extra { flex: 0 0 auto; border: 1px solid var(--commit-color); border-radius: var(--gd-radius-control); color: var(--commit-color); font-size: 10px; line-height: 1; padding: 3px 6px; white-space: nowrap; cursor: default; }
  .gd-branch-tip { position: fixed; z-index: 60; display: flex; flex-direction: column; align-items: stretch; gap: 2px; min-width: 160px; max-width: 280px; max-height: 200px; overflow-y: auto; background: var(--gd-surface-raised); border: 1px solid var(--gd-border); border-radius: var(--gd-radius-control); padding: 4px; }
  .gd-branch-tip.above { transform: translateY(-100%); }
  .gd-tip-branch { display: flex; align-items: center; gap: 6px; width: 100%; background: transparent; border: 0; border-radius: var(--gd-radius-control); color: var(--gd-accent); font: inherit; font-size: 11px; padding: 4px 6px; cursor: pointer; text-align: left; }
  .gd-tip-branch.remote { color: var(--gd-lane-2); }
  .gd-tip-branch.tag { color: var(--gd-warning); }
  .gd-tip-branch:hover:not(:disabled) { background: var(--gd-surface-hover); }
  .gd-tip-branch:disabled { cursor: default; opacity: .55; }
  .gd-tip-branch:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: -2px; }
  .gd-tip-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gd-tip-current { flex: 0 0 auto; }
  .gd-graph { display: block; }
  .gd-graph-clip { display: block; min-width: 0; height: 28px; overflow: hidden; }
  .gd-subject { min-width: 0; padding: 0 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gd-author { display: flex; flex-direction: column; min-width: 0; padding: 0 12px; color: var(--gd-text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--gd-font-size-small); line-height: 14px; }
  .gd-author small { font: 10px/12px var(--gd-font-code); opacity: .75; }
  .gd-date { min-width: 0; padding: 0 12px; color: var(--gd-text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--gd-font-size-small); }
  .gd-show-graph { position: absolute; right: 7px; top: 4px; font-size: 11px; }
  .gd-state { padding: 20px; color: var(--gd-text-secondary); }
  .gd-more { padding: 8px 12px; color: var(--gd-text-secondary); }
</style>
