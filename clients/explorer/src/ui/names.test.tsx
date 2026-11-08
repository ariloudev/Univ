import { act, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { structureDetail } from "../test/structures.ts";
import { fakeWorld } from "../test/world.ts";
import { ObjectPanel } from "./ObjectPanel.tsx";
import type { Overlay } from "./overlay.ts";
import { HoverLabel, StructureNames } from "./SceneOverlay.tsx";
import { StructureList, StructurePanel } from "./StructuresPanel.tsx";

const none = () => undefined;

/** Whether `element` or anything about it is set in italic type. */
const italic = (element: Element | null): boolean => element?.closest(".italic") !== null && element !== null;

describe("names, as atlases name areas and points", () => {
  it("names structures in italic type, in the view, in their list and in their panel", () => {
    const overlay = { names: { current: null } } as unknown as Overlay;
    render(
      <>
        <StructureNames names={[{ id: "kbc", centre: [0, 0, 0], text: "KBC void" }]} overlay={overlay} />
        <StructureList list={[{ id: "shapley", name: "Shapley Supercluster", kind: "supercluster", scale_domain: "cosmic_web" }]} onChoose={none} />
        <StructurePanel detail={structureDetail({ id: "laniakea", name: "Laniakea" })} approximate={false} inside={undefined} onClose={none} />
      </>,
    );
    expect(italic(screen.getByText("KBC void"))).toBe(true);
    expect(italic(screen.getByText("Shapley Supercluster"))).toBe(true);
    expect(italic(screen.getByRole("heading", { name: /Laniakea/ }))).toBe(true);
  });

  it("names a structure in italic in the hover label, and an object in upright type there and in its panel", () => {
    const world = fakeWorld();
    render(<HoverLabel world={world} release={7} />);
    act(() => world.hover.set({ x: 0, y: 0, name: { id: "kbc", centre: [0, 0, 0], text: "KBC void" } }));
    expect(italic(screen.getByText("KBC void"))).toBe(true);
    act(() => world.hover.set({ x: 0, y: 0, point: { id: 17, domain: "stars", place: [0, 0, 0], distanceMpc: 1e-5, source: "hipparcos-2007", kind: "measured" } }));
    expect(italic(screen.getByRole("tooltip"))).toBe(false);
    expect(screen.getByRole("tooltip").querySelector(".italic")).toBeNull();
    render(<ObjectPanel chosen={{ kind: "loading", id: 17 }} onChoose={none} onClose={none} />);
    const heading = screen.getByRole("heading", { name: "Object" });
    expect(italic(heading)).toBe(false);
    expect(heading.className).toContain("font-serif");
  });
});
