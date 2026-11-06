import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { StructureDetail } from "../api/structures.ts";
import { structureDetail } from "../test/structures.ts";
import { fakeWorld } from "../test/world.ts";
import { type Selection, SidePanels } from "./SidePanels.tsx";
import { NOTHING_HIDDEN, hiddenInAddress } from "./hidden.ts";
import type { ShownStructures } from "./useStructures.ts";

const world = fakeWorld();

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

function show(structure: StructureDetail | undefined, hidden = NOTHING_HIDDEN, shown = structures.shown) {
  const selection: Selection = { chosen: { kind: "none" }, structure, approximate: false, inside: undefined, object: none, structureById: none, close: none };
  const choices = { domains: ["cosmic_web"], sources: [], structureKinds: [{ kind: "void", count: 1 }] };
  render(
    <SidePanels
      world={world}
      release={7}
      cosmology="Planck 2018"
      structures={{ ...structures, shown }}
      selection={selection}
      show={{ choices, hidden, onToggle: none }}
    />,
  );
}

describe("the panels beside the view", () => {
  it("hold where the eye is, the structures, any problem reading them, and the search", () => {
    show(undefined);
    expect(screen.getByTestId("distance")).toBeTruthy();
    expect(screen.getByText("Structures (1)")).toBeTruthy();
    expect(screen.getByText("The structures could not be read: 503 Unavailable")).toBeTruthy();
    expect(screen.getByRole("search")).toBeTruthy();
    expect(screen.queryByRole("region", { name: "The chosen structure" })).toBeNull();
  });

  it("hold the checkboxes of what is drawn, and list no structure of a kind turned off", () => {
    show(undefined, hiddenInAddress("?hide=structure:void"), []);
    expect(screen.getByText("Show")).toBeTruthy();
    expect(screen.getByText("Structures (0)")).toBeTruthy();
    expect((screen.getByLabelText("void (1)") as HTMLInputElement).checked).toBe(false);
  });

  it("show the chosen structure's record", () => {
    show(structures.details.get("kbc-void"));
    expect(screen.getByRole("region", { name: "The chosen structure" }).textContent).toContain("KBC void");
  });
});
