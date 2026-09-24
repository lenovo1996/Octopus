import { describe, expect, it } from "vitest";
import { layoutGraph, windowRows, type GraphRow } from "../../src/lib/graph/layout";

const linear: GraphRow[] = [
  { oid: "c", parents: ["b"] },
  { oid: "b", parents: ["a"] },
  { oid: "a", parents: [] }
];

describe("graph lane layout (T05)", () => {
  it("keeps a linear history on one lane", () => {
    const { rows, laneCount } = layoutGraph(linear, new Map());
    expect(laneCount).toBe(1);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 0]);
    expect(rows[2].edges).toEqual([]);
    expect(rows[0].incoming).toBe(false);
    expect(rows[0].rails).toEqual([]);
    expect(rows[0].edges).toEqual([{ fromLane: 0, toLane: 0, parentOid: "b" }]);
    expect(rows[2].incoming).toBe(true);
    expect(rows[2].rails).toEqual([]);
  });

  it("forks and joins merge legs on distinct lanes", () => {
    // m merges b1 (first parent) and b2 (second parent), both from base.
    const rows: GraphRow[] = [
      { oid: "m", parents: ["b1", "b2"] },
      { oid: "b2", parents: ["base"] },
      { oid: "b1", parents: ["base"] },
      { oid: "base", parents: [] }
    ];
    const laid = layoutGraph(rows, new Map());
    expect(laid.rows[0].lane).toBe(0);
    const fork = laid.rows[0].edges.find((e) => e.parentOid === "b2");
    expect(fork).toBeDefined();
    expect(fork?.toLane).not.toBe(0);
    // Every edge endpoint lane is within the lane count.
    for (const row of laid.rows) {
      for (const edge of row.edges) {
        expect(edge.fromLane).toBeLessThan(laid.laneCount);
        expect(edge.toLane).toBeLessThan(laid.laneCount);
      }
    }
    // Base row is reachable on a live lane (join visible).
    expect(laid.rows[3].lane).toBeLessThan(laid.laneCount);
  });

  it("carries lanes across page boundaries without renumbering", () => {
    const page1: GraphRow[] = [
      { oid: "m", parents: ["b1", "b2"] },
      { oid: "b2", parents: ["base"] }
    ];
    const first = layoutGraph(page1, new Map());
    const page2: GraphRow[] = [
      { oid: "b1", parents: ["base"] },
      { oid: "base", parents: [] }
    ];
    const second = layoutGraph(page2, first.carry);
    const lanes1 = new Map(first.rows.map((r) => [r.oid, r.lane]));
    for (const row of second.rows) {
      const before = first.carry.get(row.oid) ?? lanes1.get(row.oid);
      if (before !== undefined) expect(row.lane).toBe(before);
    }
  });

  it("handles branch fan-out with bounded lanes", () => {
    const rows: GraphRow[] = [
      { oid: "tip", parents: ["a", "b", "c"] },
      { oid: "a", parents: ["root"] },
      { oid: "b", parents: ["root"] },
      { oid: "c", parents: ["root"] },
      { oid: "root", parents: [] }
    ];
    const laid = layoutGraph(rows, new Map());
    expect(laid.laneCount).toBeLessThanOrEqual(4);
  });

  it("reserves carried lanes before allocating an unrelated tip", () => {
    const incoming = new Map([["parent", 0], ["side", 2]]);
    const result = layoutGraph([
      { oid: "new-tip", parents: ["new-parent", "parent"] },
      { oid: "parent", parents: [] },
      { oid: "side", parents: [] },
      { oid: "new-parent", parents: [] }
    ], incoming);
    expect(result.rows.map((row) => row.lane)).toEqual([1, 0, 2, 1]);
    expect(result.rows[0].rails).toEqual([0, 2]);
    expect(result.carry.size).toBe(0);
    expect(incoming).toEqual(new Map([["parent", 0], ["side", 2]]));
  });

  it("gives exactly the same graph when split at any page boundary", () => {
    const topology = [
      { oid: "merge", parents: ["main", "side", "other"] },
      { oid: "side", parents: ["side-parent"] },
      { oid: "new-tip", parents: ["root"] },
      { oid: "main", parents: ["root"] },
      { oid: "other", parents: ["root"] },
      { oid: "side-parent", parents: ["root"] },
      { oid: "root", parents: [] }
    ];
    const whole = layoutGraph(topology, new Map());
    for (let cut = 1; cut < topology.length; cut++) {
      const first = layoutGraph(topology.slice(0, cut), new Map());
      const second = layoutGraph(topology.slice(cut), first.carry);
      expect([...first.rows, ...second.rows]).toEqual(whole.rows);
      expect(second.carry.size).toBe(0);
    }
  });

  it("connects every parent and every adjacent row boundary without phantom rails", () => {
    // Deterministic DAG with interleaved tips, merges and disconnected roots.
    const topology: GraphRow[] = Array.from({ length: 120 }, (_, i) => ({
      oid: String(i),
      parents: i % 11 === 0 ? [] : [...new Set([i + 1, i + 3, i + 9])]
        .filter((p, j) => p < 120 && (j === 0 || i % (j + 2) === 0)).map(String)
    }));
    const { rows, carry } = layoutGraph(topology, new Map());
    for (const [i, row] of rows.entries()) {
      expect(row.edges.map((edge) => edge.parentOid)).toEqual(topology[i].parents);
      expect(row.rails).not.toContain(row.lane);
      if (i > 0) {
        const previous = rows[i - 1];
        const bottom = new Set([...previous.rails, ...previous.edges.map((edge) => edge.toLane)]);
        const top = new Set([...row.rails, ...(row.incoming ? [row.lane] : [])]);
        expect(top).toEqual(bottom);
      }
      for (const edge of row.edges) {
        const parentIndex = topology.findIndex((r) => r.oid === edge.parentOid);
        expect(rows[parentIndex].lane).toBe(edge.toLane);
        for (let j = i + 1; j < parentIndex; j++) expect(rows[j].rails).toContain(edge.toLane);
      }
    }
    expect(carry.size).toBe(0);
  });
});

describe("virtual window (T05)", () => {
  it("renders only viewport plus overscan, never 10k nodes", () => {
    const rows = Array.from({ length: 10_000 }, (_, i) => i);
    const { start, end } = windowRows(rows, 5000, 600, 32, 5);
    expect(end - start).toBeLessThan(60);
    expect(start).toBeGreaterThan(0);
  });

  it("clamps to list bounds", () => {
    const rows = [1, 2, 3];
    expect(windowRows(rows, 0, 600, 32, 5)).toEqual({ start: 0, end: 3 });
    expect(windowRows(rows, 10_000, 600, 32, 5).end).toBe(3);
  });
});
