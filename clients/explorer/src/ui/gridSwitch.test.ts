import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { gridInAddress, useGrid } from "./gridSwitch.ts";

afterEach(() => history.replaceState(null, "", "/"));

describe("the grid's switch", () => {
  it("is off unless the address turns it on", () => {
    expect(gridInAddress("")).toBe(false);
    expect(gridInAddress("?grid=off")).toBe(false);
    expect(gridInAddress("?object=7&grid=on")).toBe(true);
  });

  it("is kept on in the address, beside what else it holds, and leaves it once off again", () => {
    history.replaceState(null, "", "/?object=7&lod=off");
    const { result } = renderHook(() => useGrid());
    expect(result.current[0]).toBe(false);
    expect(location.search).toBe("?object=7&lod=off");
    act(() => result.current[1](true));
    expect(result.current[0]).toBe(true);
    expect(location.search).toBe("?object=7&lod=off&grid=on");
    act(() => result.current[1](false));
    expect(location.search).toBe("?object=7&lod=off");
  });

  it("starts on where the address turns it on, as after a reload", () => {
    history.replaceState(null, "", "/?grid=on");
    const { result } = renderHook(() => useGrid());
    expect(result.current[0]).toBe(true);
  });
});
