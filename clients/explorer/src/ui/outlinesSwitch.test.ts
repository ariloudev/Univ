import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { outlinesInAddress, useOutlines } from "./outlinesSwitch.ts";

afterEach(() => history.replaceState(null, "", "/"));

describe("the structures' outlines' switch", () => {
  it("is on unless the address turns the outlines off", () => {
    expect(outlinesInAddress("")).toBe(true);
    expect(outlinesInAddress("?outlines=on")).toBe(true);
    expect(outlinesInAddress("?grid=on&outlines=off")).toBe(false);
  });

  it("is kept off in the address, beside what else it holds, and leaves it once on again", () => {
    history.replaceState(null, "", "/?rings=on");
    const { result } = renderHook(() => useOutlines());
    expect(result.current[0]).toBe(true);
    act(() => result.current[1](false));
    expect(location.search).toBe("?rings=on&outlines=off");
    act(() => result.current[1](true));
    expect(location.search).toBe("?rings=on");
  });

  it("starts off where the address turns the outlines off, as after a reload", () => {
    history.replaceState(null, "", "/?outlines=off");
    const { result } = renderHook(() => useOutlines());
    expect(result.current[0]).toBe(false);
  });
});
