import { afterEach, describe, expect, it, vi } from "vitest";

import { structuresOf } from "../api/structures.ts";
import type { StructureDrawing } from "../scene/structures.ts";
import { rendered, structureView } from "./useStructures.ts";

const drawing = (outline: StructureDrawing["outline"], centre: StructureDrawing["centre"] = [10, 0, 0]): StructureDrawing => ({
  id: "s",
  name: "S",
  centre,
  outline,
  approximate: false,
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("drawing a structure", () => {
  it("draws its segments as lines and its markers as points, about its centre", () => {
    const { lines, markers } = rendered(drawing({ segments: [[10, 1, 0], [10, 2, 0]], markers: [[11, 0, 0]] }));
    expect([...(lines?.offsets ?? [])]).toEqual([0, 1, 0, 0, 2, 0]);
    expect(lines?.centre).toEqual([10, 0, 0]);
    expect([...(markers?.offsets ?? [])]).toEqual([1, 0, 0]);
  });

  it("draws nothing of a structure with no outline and no centre", () => {
    expect(rendered(drawing({ segments: [], markers: [] }, undefined))).toEqual({ lines: undefined, markers: undefined });
  });

  it("flies to a structure's centre, from two and a half times its size", () => {
    const structures = {
      list: [],
      problem: undefined,
      drawings: new Map([["s", drawing({ segments: [], markers: [] })]]),
      details: new Map([["s", { bounds: { min: [0, 0, 0], max: [3, 4, 0] } } as never]]),
    };
    expect(structureView(structures, "s")).toEqual({ centre: [10, 0, 0], distance: 12.5 });
    expect(structureView(structures, "none")).toBeUndefined();
  });
});

describe("the list of structures", () => {
  it("is read page after page, each in the release chosen", async () => {
    const pages = [
      { release: 7, structures: [{ id: "a", name: "A", kind: "void", scale_domain: "cosmic_web" }], next: "c1" },
      { release: 7, structures: [{ id: "b", name: "B", kind: "wall", scale_domain: "cosmic_web" }], next: null },
    ];
    const fetch = vi.fn((..._args: unknown[]) => Promise.resolve(Response.json(pages.shift())));
    vi.stubGlobal("fetch", fetch);
    const list = await structuresOf(7);
    expect(list.map((s) => s.id)).toEqual(["a", "b"]);
    expect(fetch.mock.calls.map((c) => c[0])).toEqual(["/v1/structures?release=7", "/v1/structures?cursor=c1&release=7"]);
  });
});
