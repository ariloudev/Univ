import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { StructureDetail } from "../api/structures.ts";
import { structureDetail } from "../test/structures.ts";
import { fakeWorld } from "../test/world.ts";
import { Atlas, AtlasPage, type Selection } from "./Atlas.tsx";
import type { DrawerName } from "./Rail.tsx";
import { NOTHING_HIDDEN, hiddenInAddress } from "./hidden.ts";
import { useOutlines } from "./outlinesSwitch.ts";
import { choicesOf } from "./show/choices.ts";
import type { ChosenObject } from "./useChosen.ts";
import type { ShownStructures } from "./useStructures.ts";

const PLANCK_2018 = { name: "Planck 2018", hubble_constant: 67.66, omega_matter: 0.3111 };

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
  inside?: Selection["inside"];
}

function selectionOf({ structure, chosen = { kind: "none" }, inside }: Shown): Selection {
  return { chosen, structure, approximate: false, inside, object: none, structureById: none, close: none };
}

/** The page and the view of `shown`, the page keeping the top bar's place and the drawer, as the
 * explorer's does. */
function Page({ world, shown }: { world: ReturnType<typeof fakeWorld>; shown: Shown }) {
  const [topBar, setTopBar] = useState<HTMLElement | null>(null);
  const [drawer, setDrawer] = useState<DrawerName>();
  const choices = choicesOf([], [{ kind: "void" }]);
  const outlines = useOutlines();
  return (
    <AtlasPage start={<p>Drawing release 7</p>} end={<p>Level of detail</p>} slot={setTopBar}>
      <Atlas
        world={world}
        release={7}
        cosmology={PLANCK_2018}
        place={{ topBar, drawer, onDrawer: setDrawer }}
        structures={{ ...structures, shown: shown.shown ?? structures.shown }}
        selection={selectionOf(shown)}
        show={{ choices, hidden: shown.hidden ?? NOTHING_HIDDEN, onApply: none }}
        outlines={outlines}
      >
        <canvas />
      </Atlas>
    </AtlasPage>
  );
}

const atlas = (world: ReturnType<typeof fakeWorld>, shown: Shown) => <Page world={world} shown={shown} />;

function show(shown: Shown = {}, world = fakeWorld()) {
  const view = render(atlas(world, shown));
  return { world, rerender: (next: Shown) => view.rerender(atlas(world, next)) };
}

const STAR: ChosenObject = { kind: "loading", id: 17 };

afterEach(() => history.replaceState(null, "", "/"));

describe("the explorer laid out as an atlas", () => {
  it("holds the release, the eye's distance with the axes, the search and the switch in its top bar", () => {
    show();
    const bar = screen.getByRole("banner");
    expect(within(bar).getByText("Drawing release 7")).toBeTruthy();
    expect(within(bar).getByTestId("distance")).toBeTruthy();
    expect(within(bar).getByRole("combobox")).toBeTruthy();
    expect(within(bar).getByRole("search")).toBeTruthy();
    expect(within(bar).getByText("Level of detail")).toBeTruthy();
    // The grid's switch sits beside the axes.
    const axes = within(bar).getByRole("combobox").closest("label");
    expect(axes?.nextElementSibling?.textContent).toBe("Grid");
    expect(within(bar).getByRole("switch", { name: "Grid" })).toBeTruthy();
    // The distance rings' switch beside the grid's.
    expect(within(bar).getByRole("switch", { name: "Grid" }).closest("label")?.nextElementSibling?.textContent).toBe("Rings");
  });

  it("offers the structures' outlines' switch beside the rings', on, and keeps it off in the address", async () => {
    show();
    const bar = screen.getByRole("banner");
    expect(within(bar).getByRole("switch", { name: "Rings" }).closest("label")?.nextElementSibling?.textContent).toBe("Outlines");
    const toggle = within(bar).getByRole("switch", { name: "Outlines" }) as HTMLInputElement;
    expect(toggle.checked).toBe(true);
    await userEvent.click(toggle);
    expect(toggle.checked).toBe(false);
    expect(location.search).toBe("?outlines=off");
    await userEvent.click(toggle);
    expect(location.search).toBe("");
  });

  it("draws the distance rings in place of the equator's ring while their switch is on, and keeps them on in the address", async () => {
    const { world } = show();
    const toggle = screen.getByRole("switch", { name: "Rings" });
    expect((toggle as HTMLInputElement).checked).toBe(false);
    await userEvent.click(toggle);
    expect(location.search).toBe("?rings=on");
    act(() => {
      world.controller.tick();
    });
    expect(world.controller.rings?.step).toBeGreaterThan(0);
    await userEvent.click(toggle);
    expect(location.search).toBe("");
    act(() => {
      world.controller.tick();
    });
    expect(world.controller.rings).toBeUndefined();
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
    // And the bottom bar says so while the drawer is closed.
    expect(within(screen.getByRole("contentinfo")).getByText("Structures not read").getAttribute("title")).toBe(
      "The structures could not be read: 503 Unavailable",
    );
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

  it("lists no structure of a kind turned off, and holds the switches of what is drawn", async () => {
    show({ hidden: hiddenInAddress("?hide=structure:void"), shown: [] });
    await userEvent.click(screen.getByRole("button", { name: "Structures" }));
    expect(screen.getByText("Structures (0)")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    expect(screen.getByRole("button", { name: "void 1 structure" }).getAttribute("aria-pressed")).toBe("false");
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

  it("offers \"Only what lies inside\" in the top bar beside the level-of-detail switch while a structure drawn as a volume is chosen, and not in the record", async () => {
    const set = vi.fn();
    const { rerender } = show({ structure: structures.details.get("kbc-void") });
    expect(screen.queryByRole("checkbox", { name: "Only what lies inside" })).toBeNull();
    rerender({ structure: structures.details.get("kbc-void"), inside: { on: false, approximate: false, set } });
    const box = within(screen.getByRole("banner")).getByRole<HTMLInputElement>("checkbox", { name: "Only what lies inside" });
    expect(box.checked).toBe(false);
    // The last of the view's parts of the top bar, so beside the switch the page puts after them.
    const lod = within(screen.getByRole("banner")).getByText("Level of detail");
    expect(box.compareDocumentPosition(lod) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(screen.getByRole("banner")).getByRole("search").compareDocumentPosition(box) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(screen.getByRole("complementary", { name: "The record" })).queryByRole("checkbox")).toBeNull();
    await userEvent.click(box);
    expect(set).toHaveBeenCalledWith(true);
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

  it("draws the grid across the view while its switch is on, and keeps it on in the address", async () => {
    show();
    const grid = () => screen.queryByTestId("grid");
    const toggle = screen.getByRole("switch", { name: "Grid" });
    expect((toggle as HTMLInputElement).checked).toBe(false);
    expect(grid()).toBeNull();
    await userEvent.click(toggle);
    expect(location.search).toBe("?grid=on");
    const lines = screen.getByTestId("grid-lines");
    const equator = screen.getByTestId("grid-equator");
    expect(lines.getAttribute("d")).toMatch(/^M[\d.-]+ [\d.-]+L/);
    // Inside the frame, taking no pointer events, under the neat line and the ticks.
    const frame = screen.getByTestId("atlas-frame");
    expect(frame.contains(grid())).toBe(true);
    expect(frame.getAttribute("class")).toContain("pointer-events-none");
    expect(frame.firstElementChild).toBe(grid());
    // Thinner and fainter than the neat line, the equator heavier than the rest.
    const neat = frame.querySelector("rect");
    const width = (e: Element | null) => Number(e?.getAttribute("stroke-width"));
    const opacity = (e: Element | null) => Number(e?.getAttribute("stroke-opacity") ?? 1);
    expect(width(lines)).toBeLessThan(width(equator));
    expect(width(equator)).toBeLessThan(width(neat));
    expect(opacity(lines)).toBeLessThan(opacity(equator));
    expect(opacity(equator)).toBeLessThan(opacity(neat));
    await userEvent.click(toggle);
    expect(grid()).toBeNull();
    expect(location.search).toBe("");
  });

  it("opens with the grid on where the address turns it on", () => {
    history.replaceState(null, "", "/?grid=on");
    show();
    expect((screen.getByRole("switch", { name: "Grid" }) as HTMLInputElement).checked).toBe(true);
    expect(screen.getByTestId("grid")).toBeTruthy();
  });

  it("turns the grid with the view, and keeps it still while the eye zooms", () => {
    history.replaceState(null, "", "/?grid=on");
    const { world } = show();
    const lines = () => screen.getByTestId("grid-lines").getAttribute("d");
    const before = lines();
    act(() => {
      world.controller.zoom(-6);
      world.frame.set(1);
    });
    expect(lines()).toBe(before);
    act(() => {
      world.controller.turn(200, 80);
      world.frame.set(2);
    });
    expect(lines()).not.toBe(before);
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
