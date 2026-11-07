import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import type { StructureDetail } from "../api/structures.ts";
import { structureDetail } from "../test/structures.ts";
import { fakeWorld } from "../test/world.ts";
import { Atlas, type Selection } from "./Atlas.tsx";
import { NOTHING_HIDDEN, hiddenInAddress } from "./hidden.ts";
import type { ChosenObject } from "./useChosen.ts";
import type { ShownStructures } from "./useStructures.ts";

const KBC = { id: "kbc-void", name: "KBC void", kind: "void", scale_domain: "cosmic_web" };

const structures: ShownStructures = {
  list: [KBC],
  shown: [KBC],
  names: [],
  details: new Map([["kbc-void", structureDetail({ id: "kbc-void", name: "KBC void", centre: null, bounds: null })]]),
  drawings: new Map(),
  problem: "The structures could not be read: 503 Unavailable",
};

const none = () => undefined;

interface Shown {
  structure?: StructureDetail | undefined;
  chosen?: ChosenObject;
  hidden?: typeof NOTHING_HIDDEN;
  shown?: typeof structures.shown;
}

function selectionOf({ structure, chosen = { kind: "none" } }: Shown): Selection {
  return { chosen, structure, approximate: false, inside: undefined, object: none, structureById: none, close: none };
}

function atlas(world: ReturnType<typeof fakeWorld>, shown: Shown) {
  const choices = { domains: ["cosmic_web"], sources: [], structureKinds: [{ kind: "void", count: 1 }] };
  return (
    <Atlas
      world={world}
      release={7}
      cosmology="Planck 2018"
      bar={{ start: <p>Drawing release 7</p>, end: <p>Level of detail</p> }}
      structures={{ ...structures, shown: shown.shown ?? structures.shown }}
      selection={selectionOf(shown)}
      show={{ choices, hidden: shown.hidden ?? NOTHING_HIDDEN, onToggle: none }}
    >
      <canvas />
    </Atlas>
  );
}

function show(shown: Shown = {}, world = fakeWorld()) {
  const view = render(atlas(world, shown));
  return { world, rerender: (next: Shown) => view.rerender(atlas(world, next)) };
}

const STAR: ChosenObject = { kind: "loading", id: 17 };

describe("the explorer laid out as an atlas", () => {
  it("holds the release, the eye's distance with the axes, the search and the switch in its top bar", () => {
    show();
    const bar = screen.getByRole("banner");
    expect(within(bar).getByText("Drawing release 7")).toBeTruthy();
    expect(within(bar).getByTestId("distance")).toBeTruthy();
    expect(within(bar).getByRole("combobox")).toBeTruthy();
    expect(within(bar).getByRole("search")).toBeTruthy();
    expect(within(bar).getByText("Level of detail")).toBeTruthy();
  });

  it("opens each drawer from the rail and closes it again with the same button", async () => {
    show();
    const rail = screen.getByRole("navigation", { name: "Panels" });
    const structuresButton = within(rail).getByRole("button", { name: "Structures" });
    expect(structuresButton.getAttribute("aria-expanded")).toBe("false");
    await userEvent.click(structuresButton);
    expect(structuresButton.getAttribute("aria-expanded")).toBe("true");
    const drawer = screen.getByRole("complementary", { name: "Structures" });
    expect(within(drawer).getByText("Structures (1)")).toBeTruthy();
    expect(within(drawer).getByText("The structures could not be read: 503 Unavailable")).toBeTruthy();
    // One drawer at a time: another replaces it.
    await userEvent.click(within(rail).getByRole("button", { name: "Show" }));
    expect(screen.queryByRole("complementary", { name: "Structures" })).toBeNull();
    expect(screen.getByRole("complementary", { name: "Show" })).toBeTruthy();
    await userEvent.click(within(rail).getByRole("button", { name: "Show" }));
    expect(screen.queryByRole("complementary", { name: "Show" })).toBeNull();
    await userEvent.click(within(rail).getByRole("button", { name: "View" }));
    expect(within(screen.getByRole("complementary", { name: "The view" })).getByTestId("frame")).toBeTruthy();
  });

  it("puts the drawer in the row beside the view, which narrows, and over the view below 1,024 pixels", async () => {
    show();
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    const drawer = screen.getByRole("complementary", { name: "Show" });
    const view = screen.getByRole("region", { name: "The view" });
    expect(drawer.parentElement).toBe(view.parentElement);
    expect(drawer.className).toContain("shrink-0");
    expect(view.className).toContain("flex-1");
    expect(drawer.className).toContain("max-lg:absolute");
  });

  it("lists no structure of a kind turned off, and holds the checkboxes of what is drawn", async () => {
    show({ hidden: hiddenInAddress("?hide=structure:void"), shown: [] });
    await userEvent.click(screen.getByRole("button", { name: "Structures" }));
    expect(screen.getByText("Structures (0)")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    expect((screen.getByLabelText("void (1)") as HTMLInputElement).checked).toBe(false);
  });

  it("returns to Earth from the rail", async () => {
    const { world } = show();
    act(() => {
      world.controller.flyTo([30, 0, 0], 2);
    });
    await userEvent.click(screen.getByRole("button", { name: "Return to Earth" }));
    expect(world.controller.view.focus).toEqual([0, 0, 0]);
  });

  it("shows the record in the right margin only while something is chosen", () => {
    const { rerender } = show();
    expect(screen.queryByRole("complementary", { name: "The record" })).toBeNull();
    rerender({ structure: structures.details.get("kbc-void") });
    const record = screen.getByRole("complementary", { name: "The record" });
    expect(within(record).getByRole("region", { name: "The chosen structure" }).textContent).toContain("KBC void");
    rerender({ chosen: STAR });
    expect(within(screen.getByRole("complementary", { name: "The record" })).getByRole("region", { name: "The chosen object" })).toBeTruthy();
  });

  it("folds the record to the window's edge when asked, and opens it again for the next thing chosen", async () => {
    const { rerender } = show({ chosen: STAR });
    await userEvent.click(screen.getByRole("button", { name: "Fold the record" }));
    expect(screen.queryByRole("complementary", { name: "The record" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Open the record" }));
    expect(screen.getByRole("complementary", { name: "The record" })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Fold the record" }));
    rerender({ chosen: { kind: "loading", id: 18 } });
    expect(screen.getByRole("complementary", { name: "The record" })).toBeTruthy();
  });

  it("marks the view's edges with their coordinates, and holds the scale, the key and the credits in its bottom bar", () => {
    show();
    const frame = screen.getByTestId("atlas-frame");
    const names = [...frame.querySelectorAll("[data-edge]")].map((e) => e.getAttribute("data-coordinate"));
    expect(names).toEqual(["right ascension", "right ascension", "declination", "declination"]);
    expect(frame.textContent).toMatch(/\dh/);
    // Drawn inside the view, over its canvas, taking none of its width or height.
    expect(frame.parentElement).toBe(screen.getByRole("region", { name: "The view" }));
    expect(frame.getAttribute("class")).toContain("absolute inset-0");
    expect(frame.getAttribute("class")).toContain("pointer-events-none");
    const bottom = screen.getByRole("contentinfo");
    expect(within(bottom).getByTestId("scale").textContent).toMatch(/Scale at the focus: [\d,.]+ (pc|kpc|Mpc)/);
  });

  it("holds the key and the credits in its bottom bar once the catalogue is read", () => {
    const catalogue = { kinds: [5, 0, 0, 0], stars: { coloured: 3, grey: 1 }, failed: 0, unread: 0, credits: () => [] };
    show({}, fakeWorld({ catalogue: catalogue as unknown as ReturnType<typeof fakeWorld>["catalogue"] }));
    const bottom = screen.getByRole("contentinfo");
    expect(within(bottom).getByText("Key")).toBeTruthy();
    expect(within(bottom).getByText("Credits")).toBeTruthy();
    expect(within(bottom).getByTestId("key").textContent).toContain("measured: 5 points");
  });

  it("moves the ticks and the scale with the view", () => {
    const { world } = show();
    const ticks = () => screen.getByTestId("atlas-frame").textContent;
    const scale = () => screen.getByTestId("scale").textContent;
    const [before, scaleBefore] = [ticks(), scale()];
    act(() => {
      world.controller.turn(200, 80);
      world.controller.zoom(-6);
      world.frame.set(1);
    });
    expect(ticks()).not.toBe(before);
    expect(scale()).not.toBe(scaleBefore);
  });
});
