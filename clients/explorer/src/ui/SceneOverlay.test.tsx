import { act, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { fakeWorld } from "../test/world.ts";
import { HoverLabel } from "./SceneOverlay.tsx";

describe("the label under the pointer", () => {
  it("names a structure and says a click reads it, and is gone when nothing is under the pointer", () => {
    const world = fakeWorld();
    render(<HoverLabel world={world} />);
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => world.hover.set({ x: 10, y: 20, name: { id: "kbc", centre: [0, 0, 0], text: "KBC void" } }));
    expect(screen.getByRole("tooltip").textContent).toBe("KBC void. Click to read the structure.");
    act(() => world.hover.set(undefined));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
