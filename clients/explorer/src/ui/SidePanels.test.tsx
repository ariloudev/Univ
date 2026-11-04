import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Controller } from "../scene/controller.ts";
import { SidePanels } from "./SidePanels.tsx";
import type { Structures } from "./useStructures.ts";
import type { World } from "./useWorld.ts";

const world: World = {
  controller: new Controller(
    { draw: () => undefined, setLines: () => undefined },
    { size: () => ({ width: 800, height: 600, ratio: 1 }), now: () => 0, reducedMotion: () => true },
  ),
  renderer: {} as World["renderer"],
  catalogue: undefined,
  domains: new Map(),
  rotations: undefined,
  pickable: [],
  chosen: undefined,
};

const structures: Structures = {
  list: [{ id: "kbc-void", name: "KBC void", kind: "void", scale_domain: "cosmic_web" }],
  details: new Map([
    [
      "kbc-void",
      { id: "kbc-void", name: "KBC void", kind: "void", scale_domain: "cosmic_web", centre: null, bounds: null, shape: null, record: {} },
    ],
  ]),
  drawings: new Map(),
  problem: "The structures could not be read: 503 Unavailable",
};

const none = () => undefined;

function show(structureId: string | undefined) {
  render(
    <SidePanels
      world={world}
      release={7}
      cosmology="Planck 2018"
      structures={structures}
      chosen={{ kind: "none" }}
      structureId={structureId}
      onChooseObject={none}
      onCloseObject={none}
      onChooseStructure={none}
      onCloseStructure={none}
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

  it("show the chosen structure's record", () => {
    show("kbc-void");
    expect(screen.getByRole("region", { name: "The chosen structure" }).textContent).toContain("KBC void");
  });
});
