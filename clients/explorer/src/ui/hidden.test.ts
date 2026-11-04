import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { NOTHING_HIDDEN, hiddenInAddress, hideParameter, toggled, useHidden } from "./hidden.ts";

afterEach(() => history.replaceState(null, "", "/"));

describe("the groups turned off", () => {
  it("are none by default, so everything is drawn", () => {
    expect(hiddenInAddress("")).toEqual(NOTHING_HIDDEN);
    expect(hideParameter(NOTHING_HIDDEN)).toBeUndefined();
  });

  it("go through the address and back, each group by its name", () => {
    let hidden = toggled(NOTHING_HIDDEN, "domain", "cosmic_web", false);
    hidden = toggled(hidden, "source", "sdss-dr17-galaxies", false);
    hidden = toggled(hidden, "kind", "reconstructed", false);
    hidden = toggled(hidden, "structure", "void", false);
    const value = hideParameter(hidden) ?? "";
    expect(value).toBe("domain:cosmic_web,source:sdss-dr17-galaxies,kind:reconstructed,structure:void");
    expect(hiddenInAddress(`?hide=${encodeURIComponent(value)}`)).toEqual(hidden);
    const shown = toggled(hidden, "structure", "void", true);
    expect(shown.structure.size).toBe(0);
    expect(shown.domain).toBe(hidden.domain);
  });

  it("leave out entries that name no group or no kind of value", () => {
    const hidden = hiddenInAddress("?hide=planet:mars,kind:imagined,domain:,source:twomrs");
    expect(hideParameter(hidden)).toBe("source:twomrs");
  });

  it("are kept in the address with its other parameters", () => {
    history.replaceState(null, "", "/?object=852881&as=Betelgeuse&hide=kind:inferred");
    const { result } = renderHook(() => useHidden());
    expect(result.current[0].kind.has("inferred")).toBe(true);
    act(() => result.current[1]("domain", "stars", false));
    const params = new URLSearchParams(location.search);
    expect(params.get("object")).toBe("852881");
    expect(params.get("as")).toBe("Betelgeuse");
    expect(params.get("hide")).toBe("domain:stars,kind:inferred");
    act(() => result.current[1]("domain", "stars", true));
    act(() => result.current[1]("kind", "inferred", true));
    expect(new URLSearchParams(location.search).has("hide")).toBe(false);
  });
});
