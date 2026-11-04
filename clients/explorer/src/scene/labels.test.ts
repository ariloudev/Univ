import { describe, expect, it } from "vitest";

import { clearLabels, contains } from "./labels.ts";

const box = (id: string, x: number, y: number, depth: number) => ({ id, x, y, width: 80, height: 16, depth });

describe("structure names", () => {
  it("are not drawn over one another: the nearer is kept", () => {
    const kept = clearLabels([box("far", 100, 100, 50), box("near", 130, 105, 10), box("apart", 400, 100, 60)]);
    expect(kept.map((b) => b.id).sort()).toEqual(["apart", "near"]);
  });

  it("hold the points within their box, edges included", () => {
    const b = box("kbc", 100, 100, 1);
    expect(contains(b, 140, 108)).toBe(true);
    expect(contains(b, 141, 100)).toBe(false);
    expect(contains(b, 100, 109)).toBe(false);
  });
});
