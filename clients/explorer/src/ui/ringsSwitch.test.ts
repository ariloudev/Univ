import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { ringsInAddress, useRings } from "./ringsSwitch.ts";

afterEach(() => history.replaceState(null, "", "/"));

describe("the distance rings' switch", () => {
  it("is off unless the address turns the rings on", () => {
    expect(ringsInAddress("")).toBe(false);
    expect(ringsInAddress("?rings=off")).toBe(false);
    expect(ringsInAddress("?grid=on&rings=on")).toBe(true);
  });

  it("is kept on in the address, beside what else it holds, and leaves it once off again", () => {
    history.replaceState(null, "", "/?grid=on");
    const { result } = renderHook(() => useRings());
    expect(result.current[0]).toBe(false);
    act(() => result.current[1](true));
    expect(location.search).toBe("?grid=on&rings=on");
    act(() => result.current[1](false));
    expect(location.search).toBe("?grid=on");
  });

  it("starts on where the address turns the rings on, as after a reload", () => {
    history.replaceState(null, "", "/?rings=on");
    const { result } = renderHook(() => useRings());
    expect(result.current[0]).toBe(true);
  });
});
