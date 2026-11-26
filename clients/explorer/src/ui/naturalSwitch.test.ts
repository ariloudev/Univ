import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { useNaturalColours } from "./naturalSwitch.ts";

afterEach(() => history.replaceState(null, "", "/"));

describe("the switch of the tracers' natural colours", () => {
  it("is on unless the address turns it off, and the address keeps it off while it is off", () => {
    history.replaceState(null, "", "/?object=7");
    const { result } = renderHook(() => useNaturalColours());
    expect(result.current[0]).toBe(true);
    expect(location.search).toBe("?object=7");
    act(() => result.current[1](false));
    expect(location.search).toBe("?object=7&natural=off");
    act(() => result.current[1](true));
    expect(location.search).toBe("?object=7");
  });

  it("starts off where the address turns it off, as after a reload", () => {
    history.replaceState(null, "", "/?natural=off");
    const { result } = renderHook(() => useNaturalColours());
    expect(result.current[0]).toBe(false);
  });
});
