import { renderHook, act } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ageInAddress, logAgeOf, useAgeCut } from "./age.ts";

describe("the age of the Galaxy's tracers drawn", () => {
  it("is read from the page's address, any age where it names none offered", () => {
    expect(ageInAddress("?age=30")).toBe(30);
    expect(ageInAddress("?hide=domain:stars&age=10")).toBe(10);
    for (const search of ["", "?age=31", "?age=", "?age=thirty"]) expect(ageInAddress(search)).toBeUndefined();
  });

  it("cuts at the logarithm of the age in years", () => {
    expect(logAgeOf(30)).toBeCloseTo(7.477, 3);
    expect(logAgeOf(undefined)).toBeUndefined();
  });

  it("is kept in the page's address, and reloading the page gives the same age", () => {
    history.replaceState(null, "", "/?hide=domain:stars");
    const { result } = renderHook(() => useAgeCut());
    expect(result.current[0]).toBeUndefined();
    act(() => result.current[1](30));
    expect(new URLSearchParams(location.search).get("age")).toBe("30");
    expect(new URLSearchParams(location.search).get("hide")).toBe("domain:stars");
    // Reloaded: the same address gives the same age.
    expect(renderHook(() => useAgeCut()).result.current[0]).toBe(30);
    act(() => result.current[1](undefined));
    expect(new URLSearchParams(location.search).has("age")).toBe(false);
  });
});
