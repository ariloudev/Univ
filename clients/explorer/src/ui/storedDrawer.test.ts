import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { DRAWER_KEY, storeDrawer, storedDrawer, useStoredDrawer } from "./storedDrawer.ts";

const refused = (): Storage => {
  throw new DOMException("The operation is insecure.", "SecurityError");
};

afterEach(() => {
  localStorage.clear();
});

describe("the drawer kept in local storage (frame-the-view-as-an-atlas)", () => {
  it("opens the drawer kept", () => {
    localStorage.setItem(DRAWER_KEY, "show");
    const { result } = renderHook(() => useStoredDrawer());
    expect(result.current[0]).toBe("show");
  });

  it("opens no drawer where none is kept", () => {
    const { result } = renderHook(() => useStoredDrawer());
    expect(result.current[0]).toBeUndefined();
  });

  it("opens no drawer where the one kept is not one the rail opens", () => {
    localStorage.setItem(DRAWER_KEY, "layers");
    expect(storedDrawer(() => localStorage)).toBeUndefined();
  });

  it("keeps each drawer opened, and that none is once it closes", () => {
    const { result } = renderHook(() => useStoredDrawer());
    act(() => {
      result.current[1]("structures");
    });
    expect(result.current[0]).toBe("structures");
    expect(localStorage.getItem(DRAWER_KEY)).toBe("structures");
    act(() => {
      result.current[1](undefined);
    });
    expect(result.current[0]).toBeUndefined();
    expect(localStorage.getItem(DRAWER_KEY)).toBeNull();
  });

  it("opens no drawer, and keeps none without failing, where the browser refuses local storage", () => {
    expect(storedDrawer(refused)).toBeUndefined();
    expect(() => {
      storeDrawer(refused, "view");
    }).not.toThrow();
  });
});
