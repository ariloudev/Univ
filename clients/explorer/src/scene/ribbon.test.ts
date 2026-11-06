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
  });

  it("is empty for no segment", () => {
    expect(ribbonOf(new Float32Array()).count).toBe(0);
  });
});
