// Pure commit-graph lane layout (no DOM, no Git).
// Lanes carry across page boundaries via the caller-held LaneCarry map.

export interface GraphRow {
  oid: string;
  parents: string[];
}

export interface GraphEdge {
  fromLane: number;
  toLane: number;
  /** Parent oid this edge points at (for merge-leg styling/tests). */
  parentOid: string;
}

export interface LaidRow {
  oid: string;
  lane: number;
  edges: GraphEdge[];
  /** A child above connects to this node; tips must not grow a rail upward. */
  incoming: boolean;
  /** Reservations passing through this row without touching its node. */
  rails: number[];
}

/** Opaque lane reservations surviving into the next page. */
export type LaneCarry = Map<string, number>;

export interface LayoutResult {
  rows: LaidRow[];
  laneCount: number;
  carry: LaneCarry;
}

function firstFree(lanes: (string | null)[]): number {
  const i = lanes.indexOf(null);
  if (i >= 0) return i;
  lanes.push(null);
  return lanes.length - 1;
}

/** Assign lanes to topo-ordered rows (children before parents).
 * Every edge starts at the node and ends at the next row's upper boundary.
 * Passing rails and the node's incoming half-rail are separate, so neither
 * tips nor roots acquire phantom connections. Reservations survive paging.
 */
export function layoutGraph(rows: GraphRow[], incoming: LaneCarry): LayoutResult {
  const lanes: (string | null)[] = [];
  const reserved = new Map(incoming);
  for (const [oid, lane] of incoming) {
    while (lanes.length <= lane) lanes.push(null);
    lanes[lane] = oid;
  }
  const out: LaidRow[] = [];
  let laneCount = lanes.length;

  for (const row of rows) {
    const existing = reserved.get(row.oid);
    const lane = existing ?? firstFree(lanes);
    const rails: number[] = [];
    lanes.forEach((oid, i) => {
      if (oid !== null && i !== lane) rails.push(i);
    });
    reserved.delete(row.oid);
    lanes[lane] = null;
    const edges: GraphEdge[] = [];

    row.parents.forEach((parent, index) => {
      let parentLane = reserved.get(parent);
      if (parentLane === undefined) {
        parentLane = index === 0 ? lane : firstFree(lanes);
        lanes[parentLane] = parent;
        reserved.set(parent, parentLane);
      }
      edges.push({ fromLane: lane, toLane: parentLane, parentOid: parent });
    });

    laneCount = Math.max(laneCount, lanes.length, lane + 1);
    out.push({ oid: row.oid, lane, incoming: existing !== undefined, edges, rails });
    // Reuse empty trailing lanes without moving any still-live reservation.
    while (lanes.length > 0 && lanes[lanes.length - 1] === null) lanes.pop();
  }

  return { rows: out, laneCount, carry: new Map(reserved) };
}

/** Visible window over rows for virtualized rendering (rowHeight px each). */
export function windowRows<T>(rows: T[], scrollTop: number, viewportHeight: number, rowHeight: number, overscan: number): { start: number; end: number } {
  const start = Math.min(rows.length, Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const end = Math.min(rows.length, Math.max(start, Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan));
  return { start, end };
}
