import { describe, expect, it } from "vitest";

import { DOMAINS, manifestOf } from "../../test/fixtures.ts";
import { type Change, NOTHING_HIDDEN, applied, hiddenInAddress, hideParameter } from "../hidden.ts";
import { allPoints, allStructures, everything, nonePoints, noneStructures, onlyKind, onlySource } from "./actions.ts";
import { anyOff, choicesOf, summaryOf } from "./choices.ts";

const choices = choicesOf(
  DOMAINS.map((name) => ({ name, manifest: manifestOf(name) })),
  [{ kind: "void" }, { kind: "wall" }, { kind: "attractor" }],
);
const after = (search: string, changes: readonly Change[]) => hideParameter(applied(hiddenInAddress(search), changes));

describe("the Show drawer's changes", () => {
  it("turn a scale domain off or on alone, leaving every source's switch as it is", () => {
    expect(after("?hide=source:twomrs", [{ group: "domain", name: "cosmic_web", shown: false }])).toBe("domain:cosmic_web,source:twomrs");
    expect(after("?hide=domain:cosmic_web,source:twomrs", [{ group: "domain", name: "cosmic_web", shown: true }])).toBe("source:twomrs");
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
    expect(anyOff(summaryOf(choices, hidden))).toBe(true);
    expect(hideParameter(applied(hidden, everything(hidden)))).toBeUndefined();
  });

  it("change nothing, and draw nothing again, where every group is already as asked", () => {
    expect(applied(NOTHING_HIDDEN, allPoints(choices))).toBe(NOTHING_HIDDEN);
  });
});
