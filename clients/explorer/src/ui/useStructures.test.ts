import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { structuresOf } from "../api/structures.ts";
import { APPROXIMATE_COLOUR, CHOSEN_COLOUR, STRUCTURE_COLOUR } from "../scene/kinds.ts";
import type { Renderer } from "../scene/renderer.ts";
import type { StructureDrawing } from "../scene/structures.ts";
import { manifestOf } from "../test/fixtures.ts";
import { structureDetail } from "../test/structures.ts";
import { fakeWorld } from "../test/world.ts";
import { rendered, settled, structureView, useStructures } from "./useStructures.ts";

const drawing = (outline: StructureDrawing["outline"], centre: StructureDrawing["centre"] = [10, 0, 0]): StructureDrawing => ({
  id: "s",
  name: "S",
  centre,
  outline,
  approximate: false,
  size: 5,
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

  it("draws its outline in the structures' colour, fainter where approximate, and in the chosen colour where chosen", () => {
    const outline = { segments: [[10, 1, 0], [10, 2, 0]] as [number, number, number][], markers: [] };
    expect(rendered(drawing(outline)).lines?.colour).toEqual(STRUCTURE_COLOUR);
    expect(rendered({ ...drawing(outline), approximate: true }).lines?.colour).toEqual(APPROXIMATE_COLOUR);
    expect(rendered({ ...drawing(outline), approximate: true }, true).lines?.colour).toEqual(CHOSEN_COLOUR);
  });

  it("draws nothing of a structure with no outline and no centre", () => {
    expect(rendered(drawing({ segments: [], markers: [] }, undefined))).toEqual({ lines: undefined, markers: undefined });
  });

  it("flies to a structure's centre, from two and a half times its size", () => {
    const structures = {
      list: [],
      problem: undefined,
      drawings: new Map([["s", drawing({ segments: [], markers: [] })]]),
      details: new Map(),
    };
    expect(structureView(structures, "s")).toEqual({ centre: [10, 0, 0], distance: 12.5 });
    expect(structureView(structures, "none")).toBeUndefined();
  });
});

describe("reading the structures", () => {
  it("reads at most so many at once, and keeps every read that succeeded", async () => {
    let running = 0;
    let most = 0;
    const results = await settled([1, 2, 3, 4, 5], 2, async (n) => {
      running += 1;
      most = Math.max(most, running);
      await new Promise((r) => setTimeout(r, 1));
      running -= 1;
      if (n === 3) throw new Error("unreadable");
      return n * 10;
    });
    expect(most).toBe(2);
    expect(results.map((r) => r.status)).toEqual(["fulfilled", "fulfilled", "rejected", "fulfilled", "fulfilled"]);
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

describe("the chosen structure", () => {
  it("is drawn in the chosen colour, outline and markers, until another is chosen or none", async () => {
    const records: Record<string, unknown> = {
      "/v1/structures?release=7": {
        release: 7,
        structures: ["a", "b"].map((id) => ({ id, name: id.toUpperCase(), kind: "void", scale_domain: "cosmic_web" })),
        next: null,
      },
      "/v1/structures/a?release=7": structureDetail({ id: "a", name: "A" }),
      "/v1/structures/b?release=7": structureDetail({ id: "b", name: "B" }),
    };
    vi.stubGlobal("fetch", vi.fn((url: string) => Promise.resolve(Response.json(records[url]))));
    const setLines = vi.fn();
    const setMarkers = vi.fn();
    const world = fakeWorld({ renderer: { setLines, setMarkers } as unknown as Renderer });
    const hook = renderHook(({ chosen }: { chosen: string | undefined }) => useStructures(7, world, manifestOf("stars").rotations, chosen), {
      initialProps: { chosen: undefined as string | undefined },
    });
    await waitFor(() => expect(hook.result.current.drawings.size).toBe(2));
    // The last colour each structure was drawn in, and whether its markers were drawn as chosen.
    const colour = (id: string) => setLines.mock.calls.filter((c) => c[0] === `structure:${id}`).at(-1)?.[1]?.colour;
    const marked = (id: string) => setMarkers.mock.calls.filter((c) => c[0] === `structure:${id}`).at(-1)?.[2];
    expect(colour("a")).not.toEqual(CHOSEN_COLOUR);
    hook.rerender({ chosen: "a" });
    expect(colour("a")).toEqual(CHOSEN_COLOUR);
    expect(marked("a")).toBe(true);
    expect(colour("b")).not.toEqual(CHOSEN_COLOUR);
    hook.rerender({ chosen: "b" });
    expect(colour("a")).not.toEqual(CHOSEN_COLOUR);
    expect(marked("a")).toBe(false);
    expect(colour("b")).toEqual(CHOSEN_COLOUR);
    hook.rerender({ chosen: undefined });
    expect(colour("b")).not.toEqual(CHOSEN_COLOUR);
  });
});
