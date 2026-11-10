import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { insideInAddress, useStructureChoice } from "./inside.ts";

afterEach(() => history.replaceState(null, "", "/"));

describe("the cull to the chosen structure", () => {
  it("is named in the address by the structure it culls to", () => {
    expect(insideInAddress("")).toBeUndefined();
    expect(insideInAddress("?inside=")).toBeUndefined();
    expect(insideInAddress("?object=7&inside=local-void")).toBe("local-void");
  });

  it("is kept in the address beside what else it holds, and leaves it once off", () => {
    history.replaceState(null, "", "/?lod=off");
    const { result } = renderHook(() => useStructureChoice());
    act(() => result.current.choose("local-void"));
    expect(location.search).toBe("?lod=off");
    act(() => result.current.setInside(true));
    expect(location.search).toBe("?lod=off&inside=local-void");
    act(() => result.current.setInside(false));
    expect(location.search).toBe("?lod=off");
  });

  it("stays on when another structure is chosen, for that structure", () => {
    const { result } = renderHook(() => useStructureChoice());
    act(() => result.current.choose("local-void"));
    act(() => result.current.setInside(true));
    act(() => result.current.choose("huge-lqg"));
    expect(result.current.structure).toBe("huge-lqg");
    expect(result.current.inside).toBe(true);
    expect(location.search).toBe("?inside=huge-lqg");
  });

  it("is turned off when the structure's panel is closed", () => {
    const { result } = renderHook(() => useStructureChoice());
    act(() => result.current.choose("local-void"));
    act(() => result.current.setInside(true));
    act(() => result.current.close());
    expect(result.current.structure).toBeUndefined();
    expect(result.current.inside).toBe(false);
    expect(location.search).toBe("");
  });

  it("chooses the structure and culls to it where the address names it, as after a reload", () => {
    history.replaceState(null, "", "/?inside=bahcall-soneira-void");
    const { result } = renderHook(() => useStructureChoice());
    expect(result.current.structure).toBe("bahcall-soneira-void");
    expect(result.current.inside).toBe(true);
    expect(location.search).toBe("?inside=bahcall-soneira-void");
  });
});
