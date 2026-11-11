import { describe, expect, it } from "vitest";

import { structureCss } from "../../scene/kinds.ts";
import type { StructureDrawing } from "../../scene/structures.ts";
import { structureDetail } from "../../test/structures.ts";
import { type DepthRow, banded, byName, depthRows, depthX, distanceText, matching } from "./depth.ts";

const drawing = (id: string, centre: [number, number, number] | undefined) => ({ id, centre }) as unknown as StructureDrawing;
const row = (name: string, distance: number | undefined, kind = "void"): DepthRow => ({ id: name, name, kind, colour: "", known: "observed", distance });

describe("the depth index's rows (see-the-structures)", () => {
  it("place each structure at the distance from Earth to its centre as drawn, in its kind's colour, known as its record says", () => {
    const shown = [
      { id: "bootes", name: "Bootes Void", kind: "void", scale_domain: "cosmic_web" },
      { id: "giant-arc", name: "Giant Arc", kind: "arc", scale_domain: "cosmic_web" },
    ];
    const rows = depthRows(shown, {
      drawings: new Map([["bootes", drawing("bootes", [3, 4, 12])], ["giant-arc", drawing("giant-arc", [0, 0, 2820])]]),
      details: new Map([["giant-arc", structureDetail({ id: "giant-arc", kind: "arc", status: "statistical_claim" })], ["bootes", structureDetail({ id: "bootes", status: "observed" })]]),
    });
    expect(rows).toEqual([
      { id: "bootes", name: "Bootes Void", kind: "void", colour: structureCss("void"), known: "observed", distance: 13 },
      { id: "giant-arc", name: "Giant Arc", kind: "arc", colour: structureCss("arc"), known: "statistical_claim", distance: 2820 },
    ]);
  });

  it("name a structure as the view does, \"(at least)\" after a name whose shape is a lower limit", () => {
    const [rowOf] = depthRows([{ id: "vela", name: "Vela Supercluster", kind: "supercluster", scale_domain: "cosmic_web" }], {
      drawings: new Map([["vela", { ...drawing("vela", [0, 0, 291]), name: "Vela Supercluster (at least)" }]]),
      details: new Map(),
    });
    expect(rowOf?.name).toBe("Vela Supercluster (at least)");
  });

  it("leave a structure with no placed shape unplaced, and one whose record is not read known in no way", () => {
    const [rowOf] = depthRows([{ id: "sa68", name: "SA 68 Supercluster", kind: "supercluster", scale_domain: "cosmic_web" }], {
      drawings: new Map([["sa68", drawing("sa68", undefined)]]),
      details: new Map(),
    });
    expect(rowOf?.distance).toBeUndefined();
    expect(rowOf?.known).toBeUndefined();
  });

  it("read a reconstructed structure as such, and a structure whose record is not read, or whose status the explorer does not know, as known in no way", () => {
    const shown = [
      { id: "ga", name: "Great Attractor", kind: "attractor", scale_domain: "cosmic_web" },
      { id: "x", name: "X", kind: "void", scale_domain: "cosmic_web" },
    ];
    const rows = depthRows(shown, { drawings: new Map(), details: new Map([["ga", structureDetail({ status: "reconstructed" })], ["x", structureDetail({ status: "rumoured" })]]) });
    expect(rows.map((r) => r.known)).toEqual(["reconstructed", undefined]);
  });
});

describe("a structure's distance in words (see-the-structures)", () => {
  it("is in kiloparsecs to three figures below 1 Mpc, whole megaparsecs below 1,000 Mpc, and gigaparsecs to three figures from there", () => {
    expect(distanceText(0.001_853)).toBe("1.85 kpc");
    expect(distanceText(0.0244)).toBe("24.4 kpc");
    expect(distanceText(0.7832)).toBe("783 kpc");
    expect(distanceText(11.4)).toBe("11 Mpc");
    expect(distanceText(226.4)).toBe("226 Mpc");
    expect(distanceText(999.6)).toBe("1,000 Mpc");
    expect(distanceText(1000)).toBe("1 Gpc");
    expect(distanceText(1038)).toBe("1.04 Gpc");
    expect(distanceText(6627)).toBe("6.63 Gpc");
  });

  it("reads \"around us\" for a structure centred on Earth, and \"not placed\" for one with no placed shape", () => {
    expect(distanceText(0)).toBe("around us");
    // A model arm a kiloparsec or two away is not around us.
    expect(distanceText(0.0012)).toBe("1.2 kpc");
    expect(distanceText(undefined)).toBe("not placed");
  });
});

describe("the depth axis (see-the-structures)", () => {
  it("runs logarithmically from 1 kpc to 10 Gpc", () => {
    expect(depthX(0.001, 700)).toBe(0);
    expect(depthX(0.01, 700)).toBeCloseTo(100);
    expect(depthX(1, 700)).toBeCloseTo(300);
    expect(depthX(1000, 700)).toBeCloseTo(600);
    expect(depthX(10_000, 700)).toBe(700);
  });

  it("holds a structure nearer or farther than its ends at its ends", () => {
    expect(depthX(0, 700)).toBe(0);
    expect(depthX(0.0002, 700)).toBe(0);
    expect(depthX(40_000, 700)).toBe(700);
  });
});

describe("the depth index's headings (see-the-structures)", () => {
  it("group the rows within 30 kpc, from 30 kpc to 100 Mpc, from 100 Mpc to 1 Gpc, from 1 to 10 Gpc and not placed, nearest first under each", () => {
    const rows = [row("Far", 5580), row("Unplaced", undefined), row("Near", 36), row("Nearest", 11), row("Middle", 226), row("Beyond", 1038), row("Arm", 0.0041), row("Andromeda", 0.78)];
    expect(banded(rows).map((b) => [b.heading, b.rows.map((r) => r.name)])).toEqual([
      ["Within 30 kpc", ["Arm"]],
      ["30 kpc to 100 Mpc", ["Andromeda", "Nearest", "Near"]],
      ["100 Mpc to 1 Gpc", ["Middle"]],
      ["1 to 10 Gpc", ["Beyond", "Far"]],
      ["Not placed", ["Unplaced"]],
    ]);
  });

  it("leave out a heading that holds no row", () => {
    expect(banded([row("Near", 36)]).map((b) => b.heading)).toEqual(["30 kpc to 100 Mpc"]);
  });

  it("order by name, A to Z, when asked", () => {
    expect(byName([row("Sloan Great Wall", 307), row("Bootes Void", 226), row("Local Void", 36)]).map((r) => r.name)).toEqual(["Bootes Void", "Local Void", "Sloan Great Wall"]);
  });
});

describe("finding a structure in the depth index (see-the-structures)", () => {
  const rows = [row("Bootes Void", 226), row("Huge-LQG", 3927, "quasar_group"), row("Sloan Great Wall", 307, "wall")];

  it("keeps the structures whose name holds the text typed, whatever its case", () => {
    expect(matching(rows, "  sloan ").map((r) => r.name)).toEqual(["Sloan Great Wall"]);
  });

  it("keeps the structures whose kind, in words, holds the text typed", () => {
    expect(matching(rows, "quasar group").map((r) => r.name)).toEqual(["Huge-LQG"]);
  });

  it("keeps every structure for no text, and none where nothing holds it", () => {
    expect(matching(rows, "")).toHaveLength(3);
    expect(matching(rows, "no such thing")).toEqual([]);
  });
});
