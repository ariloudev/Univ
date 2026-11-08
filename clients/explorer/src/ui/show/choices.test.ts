import { describe, expect, it } from "vitest";

import { DOMAINS, manifestOf } from "../../test/fixtures.ts";
import { NOTHING_HIDDEN, hiddenInAddress } from "../hidden.ts";
import { choicesOf, summaryOf } from "./choices.ts";

const domains = DOMAINS.map((name) => ({ name, manifest: manifestOf(name) }));
const STRUCTURES = [{ kind: "void" }, { kind: "supercluster" }, { kind: "void" }, { kind: "attractor" }, { kind: "bubble" }, { kind: "wall" }];

describe("what the Show drawer offers", () => {
  it("heads each scale domain's sources with its number of points and its number of sources, each by its short name and title", () => {
    const choices = choicesOf(domains, STRUCTURES);
    expect(choices.domains.map((d) => [d.name, d.points, d.sources.length])).toEqual([
      ["stars", 2, 2],
      ["nearby_galaxies", 9, 3],
      ["cosmic_web", 6, 2],
    ]);
    const [cf4] = choices.domains[1]?.sources ?? [];
    expect(cf4).toEqual({ id: "cosmicflows-4", short: "Cosmicflows-4 groups", title: "Cosmicflows-4 group distances", alsoIn: ["cosmic_web"] });
    expect(choices.sources).toEqual(["researched-star-distances", "hipparcos-2007", "cosmicflows-4", "mcconnachie-2012", "updated-nearby-galaxies", "twomrs"]);
  });

  it("lists the scale domains from the smallest scale up, whatever the order they come in", () => {
    expect(choicesOf([...domains].reverse(), []).domains.map((d) => d.name)).toEqual(["stars", "nearby_galaxies", "cosmic_web"]);
  });

  it("marks a source that places the points of more than one scale domain with the others", () => {
    const choices = choicesOf(domains, []);
    const alsoIn = (domain: number, id: string) => choices.domains[domain]?.sources.find((s) => s.id === id)?.alsoIn;
    expect(alsoIn(1, "cosmicflows-4")).toEqual(["cosmic_web"]);
    expect(alsoIn(2, "cosmicflows-4")).toEqual(["nearby_galaxies"]);
    expect(alsoIn(2, "twomrs")).toEqual([]);
  });

  it("groups the kinds of structure by family, with their counts, any other kind under other", () => {
    expect(choicesOf([], STRUCTURES).families).toEqual([
      { family: "denser", kinds: [{ kind: "supercluster", count: 1 }] },
      { family: "emptier", kinds: [{ kind: "void", count: 2 }] },
      { family: "walls and threads", kinds: [{ kind: "wall", count: 1 }] },
      { family: "flows", kinds: [{ kind: "attractor", count: 1 }] },
      { family: "other", kinds: [{ kind: "bubble", count: 1 }] },
    ]);
  });

  it("sums up how many domains, sources, kinds of value and kinds of structure are on, of how many", () => {
    const choices = choicesOf(domains, STRUCTURES);
    expect(summaryOf(choices, NOTHING_HIDDEN)).toEqual({
      domains: { on: 3, of: 3 },
      sources: { on: 6, of: 6 },
      kinds: { on: 4, of: 4 },
      structures: { on: 5, of: 5 },
    });
    const hidden = hiddenInAddress("?hide=domain:stars,source:twomrs,source:gone,kind:inferred,structure:void,structure:wall");
    expect(summaryOf(choices, hidden)).toEqual({
      domains: { on: 2, of: 3 },
      sources: { on: 5, of: 6 },
      kinds: { on: 3, of: 4 },
      structures: { on: 3, of: 5 },
    });
  });
});
