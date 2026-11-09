import { describe, expect, it } from "vitest";

import { isChosen } from "./letGo.ts";
import type { NameLabel } from "./overlay.ts";
import type { ScenePoint } from "./scenePoint.ts";

const point = (id: number) => ({ point: { id } as ScenePoint });
const name = (id: string) => ({ name: { id } as NameLabel });

describe("a click on what is chosen already (read-an-object, see-the-structures)", () => {
  it("is on the chosen object's point, and on no other point", () => {
    const chosen = { object: 42, structure: undefined };
    expect(isChosen(point(42), chosen)).toBe(true);
    expect(isChosen(point(43), chosen)).toBe(false);
  });

  it("is on the chosen structure's name, and on no other name", () => {
    const chosen = { object: undefined, structure: "st-local-void" };
    expect(isChosen(name("st-local-void"), chosen)).toBe(true);
    expect(isChosen(name("st-kbc-void"), chosen)).toBe(false);
  });

  it("is on nothing while nothing is chosen", () => {
    const chosen = { object: undefined, structure: undefined };
    expect(isChosen(point(42), chosen)).toBe(false);
    expect(isChosen(name("st-local-void"), chosen)).toBe(false);
  });

  it("does not take a point for a structure's name, nor a name for a point", () => {
    expect(isChosen(point(42), { object: undefined, structure: "42" })).toBe(false);
    expect(isChosen(name("42"), { object: 42, structure: undefined })).toBe(false);
  });
});
