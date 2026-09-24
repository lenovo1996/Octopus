// SearchController: latest-query-wins guard for history_search (T06).
// Tauri invoke() has no AbortController, so out-of-order responses are
// discarded by token: only the newest begin() token in the current
// generation may commit its results. invalidate() bumps the generation
// when the repo changes or the search is cleared.

export interface SearchToken {
  generation: number;
  seq: number;
}

export class SearchController {
  private generation = 0;
  private seq = 0;
  private latestSeq = 0;

  /** Start a new query; its results commit only while isCurrent(token). */
  begin(): SearchToken {
    this.seq += 1;
    this.latestSeq = this.seq;
    return { generation: this.generation, seq: this.seq };
  }

  isCurrent(token: SearchToken): boolean {
    return token.generation === this.generation && token.seq === this.latestSeq;
  }

  /** Drop every in-flight query (repo switch, close, scope change, clear). */
  invalidate(): void {
    this.generation += 1;
    this.latestSeq = 0;
  }
}
