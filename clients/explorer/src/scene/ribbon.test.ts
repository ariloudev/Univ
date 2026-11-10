import { describe, expect, it } from "vitest";

import { ribbonOf } from "./ribbon.ts";

describe("a ribbon of segments", () => {
  it("draws each segment as two triangles covering both its sides, from end to end", () => {
    const ribbon = ribbonOf(new Float32Array([0, 0, 0, 1, 0, 0, 5, 5, 5, 6, 6, 6]));
    expect(ribbon.count).toBe(12);
    // Every corner of a segment carries both its ends.
    for (let v = 0; v < 6; v += 1) {
      expect([...ribbon.from.subarray(v * 3, v * 3 + 3)]).toEqual([0, 0, 0]);
      expect([...ribbon.to.subarray(v * 3, v * 3 + 3)]).toEqual([1, 0, 0]);
      expect([...ribbon.from.subarray((v + 6) * 3, (v + 6) * 3 + 3)]).toEqual([5, 5, 5]);
    }
    const corners = [...ribbon.corners.subarray(0, 12)];
    const triangles = [corners.slice(0, 6), corners.slice(6, 12)];
    // Each triangle holds both ends, and the two together all four corners.
    for (const t of triangles) expect(new Set([t[0], t[2], t[4]])).toEqual(new Set([0, 1]));
    const all = new Set([0, 2, 4, 6, 8, 10].map((i) => `${corners[i]},${corners[i + 1]}`));
    expect(all).toEqual(new Set(["0,-1", "0,1", "1,-1", "1,1"]));
    // They share a diagonal, two corners at different ends and on different sides, so that
    // together they cover the quad once: sharing a side would leave half of it bare.
    const [first, second] = triangles.map((t) => [0, 2, 4].map((i) => `${t[i]},${t[i + 1]}`));
    const shared = (first ?? []).filter((c) => (second ?? []).includes(c)).map((c) => c.split(","));
    expect(shared).toHaveLength(2);
    expect(shared[0]?.[0]).not.toBe(shared[1]?.[0]);
    expect(shared[0]?.[1]).not.toBe(shared[1]?.[1]);
  });

  it("gives each corner the neighbour of the end it is at, where the ends have any", () => {
    const offsets = new Float32Array([0, 0, 0, 1, 0, 0]);
    expect(ribbonOf(offsets).neighbours).toBeUndefined();
    const ribbon = ribbonOf(offsets, new Float32Array([1, 2, 3, 4, 5, 6]));
    for (let v = 0; v < 6; v += 1) {
      const end = ribbon.corners[v * 2];
      expect([...(ribbon.neighbours?.subarray(v * 3, v * 3 + 3) ?? [])]).toEqual(end === 0 ? [1, 2, 3] : [4, 5, 6]);
    }
  });

  it("is empty for no segment", () => {
    expect(ribbonOf(new Float32Array()).count).toBe(0);
  });
});
