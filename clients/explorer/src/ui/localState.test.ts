import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { readLocal, useLocalState, writeLocal } from "./localState.ts";

const refused = (): Storage => {
  throw new DOMException("The operation is insecure.", "SecurityError");
};

afterEach(() => {
  localStorage.clear();
});

describe("a value kept in local storage (frame-the-view-as-an-atlas, see-the-structures)", () => {
  it("is read back as written, and is gone once written as none", () => {
    writeLocal("k", "A to Z");
    expect(readLocal("k")).toBe("A to Z");
    writeLocal("k", undefined);
    expect(readLocal("k")).toBeUndefined();
    expect(localStorage.getItem("k")).toBeNull();
  });

  it("is none, and writing it does not fail, where the browser refuses local storage", () => {
    expect(readLocal("k", refused)).toBeUndefined();
    expect(() => {
      writeLocal("k", "v", refused);
    }).not.toThrow();
  });

  it("opens as kept, and keeps each value set as its writer gives it", () => {
    localStorage.setItem("order", "name");
    const read = (kept: string | undefined) => kept ?? "distance";
    const write = (order: string) => (order === "distance" ? undefined : order);
    const { result } = renderHook(() => useLocalState("order", read, write));
    expect(result.current[0]).toBe("name");
    act(() => {
      result.current[1]("distance");
    });
    expect(result.current[0]).toBe("distance");
    expect(localStorage.getItem("order")).toBeNull();
  });
});
