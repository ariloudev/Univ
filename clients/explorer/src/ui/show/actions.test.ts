import { describe, expect, it } from "vitest";

import { DOMAINS, manifestOf } from "../../test/fixtures.ts";
import { NOTHING_HIDDEN, applied, hiddenInAddress, hideParameter } from "../hidden.ts";
import { allPoints, allStructures, anyOff, domainOff, domainOn, domainState, everything, nonePoints, noneStructures, onlyKind, onlySource } from "./actions.ts";
import { choicesOf } from "./choices.ts";

const choices = choicesOf(
  DOMAINS.map((name) => ({ name, manifest: manifestOf(name) })),
  [{ kind: "void" }, { kind: "wall" }, { kind: "attractor" }],
);
const [stars, nearby, web] = choices.domains;
if (stars === undefined || nearby === undefined || web === undefined) throw new Error("the fixtures hold three domains");
const after = (search: string, changes: Parameters<typeof applied>[1]) => hideParameter(applied(hiddenInAddress(search), changes));

describe("the Show drawer's changes", () => {
  it("show a scale domain mixed while it is on and some but not all of its sources are off", () => {
    expect(domainState(nearby, NOTHING_HIDDEN)).toBe("on");
    expect(domainState(nearby, hiddenInAddress("?hide=source:cosmicflows-4"))).toBe("mixed");
    expect(domainState(nearby, hiddenInAddress("?hide=source:cosmicflows-4,source:mcconnachie-2012,source:updated-nearby-galaxies"))).toBe("on");
    expect(domainState(nearby, hiddenInAddress("?hide=domain:nearby_galaxies,source:cosmicflows-4"))).toBe("off");
  });

  it("turn a scale domain on, from off or from mixed, with every source under it", () => {
    expect(after("?hide=domain:nearby_galaxies,source:cosmicflows-4,source:twomrs", domainOn(nearby))).toBe("source:twomrs");
    expect(after("?hide=source:mcconnachie-2012", domainOn(nearby))).toBeUndefined();
  });

  it("turn a scale domain off, its sources' switches as they are", () => {
    expect(after("?hide=source:mcconnachie-2012", domainOff(nearby))).toBe("domain:nearby_galaxies,source:mcconnachie-2012");
  });

  it("turn only a source on, with each scale domain whose points it places, and every other source off", () => {
    expect(after("?hide=domain:cosmic_web,domain:stars", onlySource("cosmicflows-4", choices))).toBe(
      "domain:stars,source:hipparcos-2007,source:mcconnachie-2012,source:researched-star-distances,source:twomrs,source:updated-nearby-galaxies",
    );
  });

  it("turn only a kind of structure on, and every other kind off", () => {
    expect(after("?hide=structure:void", onlyKind("void", choices))).toBe("structure:attractor,structure:wall");
  });

  it("turn every group of the points on, or every scale domain off", () => {
    expect(after("?hide=domain:stars,source:twomrs,kind:inferred,structure:void", allPoints(choices))).toBe("structure:void");
    expect(after("?hide=source:twomrs", nonePoints(choices))).toBe("domain:cosmic_web,domain:nearby_galaxies,domain:stars,source:twomrs");
  });

  it("turn every kind of structure on, or off", () => {
    expect(after("?hide=structure:void,kind:inferred", allStructures(choices))).toBe("kind:inferred");
    expect(after("", noneStructures(choices))).toBe("structure:attractor,structure:void,structure:wall");
  });

  it("turn every group that is off on at once", () => {
    const hidden = hiddenInAddress("?hide=domain:stars,source:twomrs,kind:inferred,structure:void,source:gone");
    expect(anyOff(hidden)).toBe(true);
    const all = applied(hidden, everything(hidden));
    expect(anyOff(all)).toBe(false);
    expect(anyOff(NOTHING_HIDDEN)).toBe(false);
  });
});
