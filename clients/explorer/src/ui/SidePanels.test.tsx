import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { StructureDetail } from "../api/structures.ts";
import { structureDetail } from "../test/structures.ts";
import { fakeWorld } from "../test/world.ts";
import { type Selection, SidePanels } from "./SidePanels.tsx";
import type { Structures } from "./useStructures.ts";

const world = fakeWorld();

const structures: Structures = {
  list: [{ id: "kbc-void", name: "KBC void", kind: "void", scale_domain: "cosmic_web" }],
  details: new Map([["kbc-void", structureDetail({ id: "kbc-void", name: "KBC void", centre: null, bounds: null })]]),
  drawings: new Map(),
  problem: "The structures could not be read: 503 Unavailable",
};

const none = () => undefined;

function show(structure: StructureDetail | undefined) {
  const selection: Selection = { chosen: { kind: "none" }, structure, approximate: false, object: none, structureById: none, close: none };
  render(<SidePanels world={world} release={7} cosmology="Planck 2018" structures={structures} selection={selection} />);
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

  it("show the chosen structure's record", () => {
    show(structures.details.get("kbc-void"));
    expect(screen.getByRole("region", { name: "The chosen structure" }).textContent).toContain("KBC void");
  });
});
