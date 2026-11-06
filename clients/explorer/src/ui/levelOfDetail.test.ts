import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { levelOfDetailInAddress, useLevelOfDetail } from "./levelOfDetail.ts";

afterEach(() => history.replaceState(null, "", "/"));

describe("level of detail", () => {
  it("is on unless the address turns it off", () => {
    expect(levelOfDetailInAddress("")).toBe(true);
    expect(levelOfDetailInAddress("?lod=on")).toBe(true);
    expect(levelOfDetailInAddress("?object=7&lod=off")).toBe(false);
  });

  it("is kept off in the address, beside what else it holds, and leaves it once on again", () => {
    history.replaceState(null, "", "/?object=7");
    const { result } = renderHook(() => useLevelOfDetail());
    expect(result.current[0]).toBe(true);
    expect(location.search).toBe("?object=7");
    act(() => result.current[1](false));
    expect(result.current[0]).toBe(false);
    expect(location.search).toBe("?object=7&lod=off");
    act(() => result.current[1](true));
    expect(location.search).toBe("?object=7");
  });

  it("starts off where the address turns it off, as after a reload", () => {
    history.replaceState(null, "", "/?lod=off");
    const { result } = renderHook(() => useLevelOfDetail());
    expect(result.current[0]).toBe(false);
  });
});
