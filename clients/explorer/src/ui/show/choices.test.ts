import { describe, expect, it } from "vitest";

import { STRUCTURE_FAMILIES } from "../../scene/kinds.ts";
import { DOMAINS, manifestOf } from "../../test/fixtures.ts";
import { NOTHING_HIDDEN, hiddenInAddress } from "../hidden.ts";
import { anyOff, choicesOf, summaryOf } from "./choices.ts";

const domains = DOMAINS.map((name) => ({ name, manifest: manifestOf(name) }));
const STRUCTURES = [{ kind: "void" }, { kind: "supercluster" }, { kind: "void" }, { kind: "attractor" }, { kind: "bubble" }, { kind: "wall" }];

describe("what the Show drawer offers", () => {
  it("gives each scale domain its column with its number of points, from the smallest scale up whatever the order they come in", () => {
    const choices = choicesOf([...domains].reverse(), []);
    // The fixtures' radii: stars 210 pc, nearby galaxies 8.8 Mpc, cosmic web 168 Mpc.
    expect(choices.domains).toEqual([
      { name: "stars", points: 2 },
      { name: "nearby_galaxies", points: 9 },
      { name: "cosmic_web", points: 6 },
    ]);
    expect(choices.domainNames).toEqual(["stars", "nearby_galaxies", "cosmic_web"]);
  });

  it("lists each source once by its short name and title, with every domain whose points it places, grouped by the smallest", () => {
    const choices = choicesOf([...domains].reverse(), []);
    expect(choices.sources.map((s) => [s.id, s.domains])).toEqual([
      ["researched-star-distances", ["stars"]],
      ["hipparcos-2007", ["stars"]],
      ["cosmicflows-4", ["nearby_galaxies", "cosmic_web"]],
      ["mcconnachie-2012", ["nearby_galaxies"]],
      ["updated-nearby-galaxies", ["nearby_galaxies"]],
      ["twomrs", ["cosmic_web"]],
    ]);
    expect(choices.sources[2]).toMatchObject({ short: "Cosmicflows-4 groups", title: "Cosmicflows-4 group distances" });
    expect(choices.sourceIds).toHaveLength(6);
  });

  it("groups the kinds of structure by family, with their counts, any other kind under other", () => {
    const choices = choicesOf([], STRUCTURES);
    expect(choices.families).toEqual([
      { family: "denser", kinds: [{ kind: "supercluster", count: 1 }] },
      { family: "emptier", kinds: [{ kind: "void", count: 2 }] },
      { family: "walls and threads", kinds: [{ kind: "wall", count: 1 }] },
      { family: "flows", kinds: [{ kind: "attractor", count: 1 }] },
      { family: "other", kinds: [{ kind: "bubble", count: 1 }] },
    ]);
    expect(choices.structureKinds).toEqual(["supercluster", "void", "wall", "attractor", "bubble"]);
  });

  it("keeps each kind of structure's family with its colour, every kind with a colour in one family", () => {
    const kinds = STRUCTURE_FAMILIES.flatMap((f) => f.kinds);
    expect(kinds).toHaveLength(15);
    expect(new Set(kinds).size).toBe(15);
    expect(STRUCTURE_FAMILIES.map((f) => f.family)).toEqual(["denser", "emptier", "walls and threads", "distant arcs and groups", "flows", "the Galaxy's arms"]);
    expect(STRUCTURE_FAMILIES.at(-1)?.kinds).toEqual(["spiral_arm", "spiral_arm_extrapolation"]);
  });

  it("sums up how many domains, sources, kinds of value and kinds of structure are on, of how many this release offers", () => {
    const choices = choicesOf(domains, STRUCTURES);
    const all = summaryOf(choices, NOTHING_HIDDEN);
    expect(all).toEqual({ domains: { on: 3, of: 3 }, sources: { on: 6, of: 6 }, kinds: { on: 4, of: 4 }, objects: { on: 0, of: 0 }, structures: { on: 5, of: 5 } });
    expect(anyOff(all)).toBe(false);
    const hidden = hiddenInAddress("?hide=domain:stars,source:twomrs,source:gone,kind:inferred,structure:void,structure:wall");
    expect(summaryOf(choices, hidden)).toEqual({ domains: { on: 2, of: 3 }, sources: { on: 5, of: 6 }, kinds: { on: 3, of: 4 }, objects: { on: 0, of: 0 }, structures: { on: 3, of: 5 } });
    // A name this release does not offer turns nothing it offers off.
    expect(anyOff(summaryOf(choices, hiddenInAddress("?hide=source:gone,structure:ring")))).toBe(false);
  });
});
