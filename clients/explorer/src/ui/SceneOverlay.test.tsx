import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { CHOSEN_CSS, STRUCTURE_CSS } from "../scene/kinds.ts";
import { fakeWorld } from "../test/world.ts";
import { HIP_32754 } from "../test/records.ts";
import type { Overlay } from "./overlay.ts";
import { HoverLabel, StructureNames } from "./SceneOverlay.tsx";

describe("the label under the pointer", () => {
  it("names a structure and says a click reads it, and is gone when nothing is under the pointer", () => {
    const world = fakeWorld();
    render(<HoverLabel world={world} release={7} />);
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => world.hover.set({ x: 10, y: 20, name: { id: "kbc", centre: [0, 0, 0], text: "KBC void" } }));
    expect(screen.getByRole("tooltip").textContent).toBe("KBC void. Click to read the structure.");
    act(() => world.hover.set(undefined));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});

describe("the structures' names", () => {
  it("writes the chosen structure's name in the chosen colour, and the others in the structures' own", () => {
    const names = [
      { id: "kbc", centre: [0, 0, 0] as [number, number, number], text: "KBC void" },
      { id: "shapley", centre: [1, 0, 0] as [number, number, number], text: "Shapley Supercluster" },
    ];
    const overlay = { names: { current: null } } as unknown as Overlay;
    const { container, rerender } = render(<StructureNames names={names} overlay={overlay} chosen="shapley" />);
    const colour = (id: string) => (container.querySelector(`[data-structure="${id}"]`) as HTMLElement).style.color;
    const css = (text: string) => {
      const probe = document.createElement("span");
      probe.style.color = text;
      return probe.style.color;
    };
    expect(colour("shapley")).toBe(css(CHOSEN_CSS));
    expect(colour("kbc")).toBe(css(STRUCTURE_CSS));
    rerender(<StructureNames names={names} overlay={overlay} />);
    expect(colour("shapley")).toBe(css(STRUCTURE_CSS));
  });
});

describe("the label of a point the pointer rests on", () => {
  it("names the object from its record once the pointer has rested on it for 0.3 seconds, in place of its id", async () => {
    vi.useFakeTimers();
    const fetch = vi.fn((..._args: unknown[]) => Promise.resolve(Response.json(HIP_32754)));
    vi.stubGlobal("fetch", fetch);
    try {
      const world = fakeWorld();
      render(<HoverLabel world={world} release={12} />);
      const point = { id: 17, domain: "stars", place: [0, 0, 0] as const, distanceMpc: 3.4683907e-5, source: "bailer-jones-2021", kind: "estimated" };
      act(() => world.hover.set({ x: 0, y: 0, point }));
      expect(screen.getByRole("tooltip").textContent).toMatch(/^17 · stars · /);
      // Before the rest is over, nothing is read.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(299);
      });
      expect(fetch).not.toHaveBeenCalled();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1);
      });
      expect(fetch.mock.calls[0]?.[0]).toBe("/v1/objects/17?release=12");
      expect(screen.getByRole("tooltip").textContent).toMatch(/^HIP 32754 · stars · /);
      // Another point is named by its id until the pointer has rested on it too.
      act(() => world.hover.set({ x: 0, y: 0, point: { ...point, id: 18 } }));
      expect(screen.getByRole("tooltip").textContent).toMatch(/^18 · /);
    } finally {
      vi.useRealTimers();
      vi.unstubAllGlobals();
    }
  });

  it("reads nothing for a point the pointer passes over", async () => {
    vi.useFakeTimers();
    const fetch = vi.fn((..._args: unknown[]) => Promise.resolve(Response.json(HIP_32754)));
    vi.stubGlobal("fetch", fetch);
    try {
      const world = fakeWorld();
      render(<HoverLabel world={world} release={12} />);
      const point = { id: 17, domain: "stars", place: [0, 0, 0] as const, distanceMpc: 1e-5, source: "gcns", kind: "estimated" };
      act(() => world.hover.set({ x: 0, y: 0, point }));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(200);
      });
      act(() => world.hover.set(undefined));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(500);
      });
      expect(fetch).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
      vi.unstubAllGlobals();
    }
  });
});
