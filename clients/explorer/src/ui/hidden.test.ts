import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { NOTHING_HIDDEN, applied, hiddenInAddress, hideParameter, useHidden } from "./hidden.ts";

afterEach(() => history.replaceState(null, "", "/"));

describe("the groups turned off", () => {
  it("are none by default, so everything is drawn", () => {
    expect(hiddenInAddress("")).toEqual(NOTHING_HIDDEN);
    expect(hideParameter(NOTHING_HIDDEN)).toBeUndefined();
  });

  it("go through the address and back, each group by its name", () => {
    const hidden = applied(NOTHING_HIDDEN, [
      { group: "domain", name: "cosmic_web", shown: false },
      { group: "source", name: "sdss-dr17-galaxies", shown: false },
      { group: "kind", name: "reconstructed", shown: false },
      { group: "structure", name: "void", shown: false },
    ]);
    const value = hideParameter(hidden) ?? "";
    expect(value).toBe("domain:cosmic_web,source:sdss-dr17-galaxies,kind:reconstructed,structure:void");
    expect(hiddenInAddress(`?hide=${encodeURIComponent(value)}`)).toEqual(hidden);
    const shown = applied(hidden, [{ group: "structure", name: "void", shown: true }]);
    expect(shown.structure.size).toBe(0);
    expect(shown.domain).toBe(hidden.domain);
  });

  it("make several changes at once, in order, keeping each group no change turns on or off", () => {
    const hidden = hiddenInAddress("?hide=source:twomrs,kind:inferred");
    const next = applied(hidden, [
      { group: "source", name: "twomrs", shown: true },
      { group: "source", name: "gama-dr4", shown: false },
      { group: "source", name: "gama-dr4", shown: true },
      { group: "domain", name: "stars", shown: false },
      { group: "kind", name: "measured", shown: true },
    ]);
    expect([...next.source]).toEqual([]);
    expect([...next.domain]).toEqual(["stars"]);
    expect(next.kind).toBe(hidden.kind);
    expect(next.structure).toBe(hidden.structure);
    expect(applied(hidden, [])).toBe(hidden);
    expect(applied(hidden, [{ group: "kind", name: "inferred", shown: false }])).toBe(hidden);
  });

  it("leave out entries that name no group or no kind of value", () => {
    const hidden = hiddenInAddress("?hide=planet:mars,kind:imagined,domain:,source:twomrs");
    expect(hideParameter(hidden)).toBe("source:twomrs");
  });

  it("are kept in the address with its other parameters", () => {
    history.replaceState(null, "", "/?object=852881&as=Betelgeuse&hide=kind:inferred");
    const { result } = renderHook(() => useHidden());
    expect(result.current[0].kind.has("inferred")).toBe(true);
    act(() => result.current[1]([{ group: "domain", name: "stars", shown: false }]));
    const params = new URLSearchParams(location.search);
    expect(params.get("object")).toBe("852881");
    expect(params.get("as")).toBe("Betelgeuse");
    expect(params.get("hide")).toBe("domain:stars,kind:inferred");
    act(() =>
      result.current[1]([
        { group: "domain", name: "stars", shown: true },
        { group: "kind", name: "inferred", shown: true },
      ]),
    );
    expect(new URLSearchParams(location.search).has("hide")).toBe(false);
  });

  it("turn off a kind of object of the Galaxy's tracers through the address, and leave out a name that is none", () => {
    const hidden = hiddenInAddress("?hide=object:rr_lyrae_star,object:classical_cepheid,object:galaxy,object:body");
    expect([...hidden.object].sort()).toEqual(["classical_cepheid", "rr_lyrae_star"]);
    expect(hideParameter(hidden)).toBe("object:classical_cepheid,object:rr_lyrae_star");
    expect(hiddenInAddress(`?hide=${encodeURIComponent(hideParameter(hidden) ?? "")}`)).toEqual(hidden);
  });
});
