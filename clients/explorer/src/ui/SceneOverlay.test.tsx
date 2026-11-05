import { act, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { CHOSEN_CSS, STRUCTURE_CSS } from "../scene/kinds.ts";
import { fakeWorld } from "../test/world.ts";
import type { Overlay } from "./overlay.ts";
import { HoverLabel, StructureNames } from "./SceneOverlay.tsx";

describe("the label under the pointer", () => {
  it("names a structure and says a click reads it, and is gone when nothing is under the pointer", () => {
    const world = fakeWorld();
    render(<HoverLabel world={world} />);
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
