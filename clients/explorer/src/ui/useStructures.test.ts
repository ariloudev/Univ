import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { structuresOf } from "../api/structures.ts";
import type { Renderer } from "../scene/renderer.ts";
import type { StructureDrawing } from "../scene/structures.ts";
import { manifestOf } from "../test/fixtures.ts";
import { fakeWorld } from "../test/world.ts";
import { structureDetail } from "../test/structures.ts";
import { SYMBOL_REACH, hiddenOutlines, namesOf, rendered, settled, structureView, useStructures } from "./useStructures.ts";

const drawing = (outline: StructureDrawing["outline"], centre: StructureDrawing["centre"] = [10, 0, 0]): StructureDrawing => ({
  id: "s",
  name: "S",
  kind: "void",
  centre,
  outline,
  approximate: false,
  size: 5,
  symbol: undefined,
});

/** No kind of structure turned off, the same set on every render. */
const NO_KINDS: ReadonlySet<string> = new Set();

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

  it("draws an attractor's or a repeller's symbol about its centre, beside its marker", () => {
    const symbol: [number, number, number][] = [[1, 0, 0], [2, 0, 0]];
    const { lines, markers } = rendered({ ...drawing({ segments: [], markers: [[10, 0, 0]] }), kind: "attractor", symbol });
    expect(lines?.centre).toEqual([10, 0, 0]);
    expect([...(lines?.offsets ?? [])]).toEqual([]);
    expect([...(lines?.symbol?.offsets ?? [])]).toEqual([1, 0, 0, 2, 0, 0]);
    expect([...(markers?.offsets ?? [])]).toEqual([0, 0, 0]);
  });

  it("names an attractor or a repeller below its symbol, and any other structure on its centre", () => {
    const plain = drawing({ segments: [], markers: [[10, 0, 0]] });
    const flow = { ...plain, id: "f", kind: "repeller", symbol: [[1, 0, 0] as [number, number, number]] };
    const structures = { list: [], details: new Map(), drawings: new Map([["s", plain], ["f", flow]]), problem: undefined };
    const names = namesOf(structures, new Set(["s", "f"]));
    expect(names.find((n) => n.id === "s")?.below).toBeUndefined();
    expect(names.find((n) => n.id === "f")?.below).toBe(SYMBOL_REACH);
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
  it("is the one the renderer draws as chosen, until another is chosen or none", () => {
    const choose = vi.fn();
    const world = fakeWorld({ renderer: { setLines: vi.fn(), setMarkers: vi.fn(), choose, hideNames: vi.fn() } as unknown as Renderer });
    vi.stubGlobal("fetch", vi.fn(() => new Promise(() => undefined)));
    const hook = renderHook(({ chosen }: { chosen: string | undefined }) => useStructures(7, world, manifestOf("stars").rotations, chosen, NO_KINDS), {
      initialProps: { chosen: "kbc-void" as string | undefined },
    });
    expect(choose).toHaveBeenLastCalledWith("structure:kbc-void");
    hook.rerender({ chosen: "shapley-supercluster" });
    expect(choose).toHaveBeenLastCalledWith("structure:shapley-supercluster");
    hook.rerender({ chosen: undefined });
    expect(choose).toHaveBeenLastCalledWith(undefined);
  });

  it("asks for a frame once the structures read are rendered, so that their names are placed", async () => {
    vi.stubGlobal("fetch", vi.fn((url: string) => Promise.resolve(Response.json(url.startsWith("/v1/structures?") ? { release: 7, structures: [], next: null } : {}))));
    const world = fakeWorld();
    const invalidate = vi.spyOn(world.controller, "invalidate");
    const hook = renderHook(() => useStructures(7, world, manifestOf("stars").rotations, undefined, NO_KINDS));
    const before = invalidate.mock.calls.length;
    await waitFor(() => expect(hook.result.current.list).toEqual([]));
    await waitFor(() => expect(invalidate.mock.calls.length).toBeGreaterThan(before));
  });

  it("draws and names no structure of a kind turned off, and draws it again once turned on", async () => {
    const detail = structureDetail({ id: "kbc-void", name: "KBC void" });
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(
          Response.json(
            url.startsWith("/v1/structures?")
              ? { release: 7, structures: [{ id: "kbc-void", name: "KBC void", kind: "void", scale_domain: "cosmic_web" }], next: null }
              : detail,
          ),
        ),
      ),
    );
    const hideNames = vi.fn();
    const world = fakeWorld({ renderer: { setLines: vi.fn(), setMarkers: vi.fn(), choose: vi.fn(), hideNames } as unknown as Renderer });
    const rotations = manifestOf("stars").rotations;
    const voids: ReadonlySet<string> = new Set(["void"]);
    const none: ReadonlySet<string> = new Set();
    const hook = renderHook(({ hidden }: { hidden: ReadonlySet<string> }) => useStructures(7, world, rotations, undefined, hidden), {
      initialProps: { hidden: none },
    });
    await waitFor(() => expect(hook.result.current.names.map((n) => n.id)).toEqual(["kbc-void"]));
    expect(world.names).toBe(hook.result.current.names);
    hook.rerender({ hidden: voids });
    expect(hook.result.current.names).toEqual([]);
    expect(hook.result.current.shown).toEqual([]);
    expect(world.names).toEqual([]);
    expect(hideNames).toHaveBeenLastCalledWith(new Set(["structure:kbc-void"]));
    hook.rerender({ hidden: none });
    expect(world.names.map((n) => n.id)).toEqual(["kbc-void"]);
    expect(hideNames).toHaveBeenLastCalledWith(new Set());
  });

  it("draws no outline but the chosen one's while the outlines are off, naming every structure all the same", async () => {
    const details = { "kbc-void": structureDetail({ id: "kbc-void", name: "KBC void" }), "vela": structureDetail({ id: "vela", name: "Vela" }) };
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(
          Response.json(
            url.startsWith("/v1/structures?")
              ? {
                  release: 7,
                  structures: [
                    { id: "kbc-void", name: "KBC void", kind: "void", scale_domain: "cosmic_web" },
                    { id: "vela", name: "Vela", kind: "supercluster", scale_domain: "cosmic_web" },
                  ],
                  next: null,
                }
              : details[url.includes("vela") ? "vela" : "kbc-void"],
          ),
        ),
      ),
    );
    const hideNames = vi.fn();
    const world = fakeWorld({ renderer: { setLines: vi.fn(), setMarkers: vi.fn(), choose: vi.fn(), hideNames } as unknown as Renderer });
    const none: ReadonlySet<string> = new Set();
    const hook = renderHook(({ chosen, outlines }: { chosen: string | undefined; outlines: boolean }) => useStructures(7, world, manifestOf("stars").rotations, chosen, none, outlines), {
      initialProps: { chosen: undefined as string | undefined, outlines: true },
    });
    await waitFor(() => expect(hook.result.current.names.map((n) => n.id).sort()).toEqual(["kbc-void", "vela"]));
    expect(hideNames).toHaveBeenLastCalledWith(new Set());
    hook.rerender({ chosen: undefined, outlines: false });
    expect(hideNames).toHaveBeenLastCalledWith(new Set(["structure:kbc-void", "structure:vela"]));
    expect(world.names.map((n) => n.id).sort()).toEqual(["kbc-void", "vela"]);
    hook.rerender({ chosen: "vela", outlines: false });
    expect(hideNames).toHaveBeenLastCalledWith(new Set(["structure:kbc-void"]));
    hook.rerender({ chosen: "vela", outlines: true });
    expect(hideNames).toHaveBeenLastCalledWith(new Set());
  });

  it("hides only structures' outlines, by their own names, whatever else the renderer draws", () => {
    // The equator and the distance rings are drawn under names of their own, never a structure's.
    expect(hiddenOutlines(["kbc-void", "vela", "wall"], new Set(["kbc-void", "vela"]), "vela", false)).toEqual(new Set(["structure:kbc-void", "structure:wall"]));
    expect(hiddenOutlines(["kbc-void", "wall"], new Set(["kbc-void"]), "wall", true)).toEqual(new Set(["structure:wall"]));
  });
});
