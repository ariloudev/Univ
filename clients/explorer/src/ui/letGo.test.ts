import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { LET_GO_AFTER_MS, clickChooser, isChosen, targetOf } from "./letGo.ts";
import type { NameLabel } from "./overlay.ts";
import type { ScenePoint } from "./scenePoint.ts";

const point = (id: number) => ({ point: { id } as ScenePoint });
const name = (id: string) => ({ name: { id } as NameLabel });

describe("what is chosen already (read-an-object, see-the-structures)", () => {
  it("is the chosen object's point, and no other point", () => {
    const chosen = { object: 42, structure: undefined };
    expect(isChosen(targetOf(point(42)), chosen)).toBe(true);
    expect(isChosen(targetOf(point(43)), chosen)).toBe(false);
  });

  it("is the chosen structure, by its name in the view or in the list, and no other", () => {
    const chosen = { object: undefined, structure: "st-local-void" };
    expect(isChosen(targetOf(name("st-local-void")), chosen)).toBe(true);
    expect(isChosen({ structure: "st-local-void" }, chosen)).toBe(true);
    expect(isChosen(targetOf(name("st-kbc-void")), chosen)).toBe(false);
  });

  it("is nothing while nothing is chosen", () => {
    const chosen = { object: undefined, structure: undefined };
    expect(isChosen(targetOf(point(42)), chosen)).toBe(false);
    expect(isChosen(targetOf(name("st-local-void")), chosen)).toBe(false);
  });

  it("does not take a point for a structure's name, nor a name for a point", () => {
    expect(isChosen(targetOf(point(42)), { object: undefined, structure: "42" })).toBe(false);
    expect(isChosen(targetOf(name("42")), { object: 42, structure: undefined })).toBe(false);
  });
});

describe("a click in the view on what is chosen already (read-an-object, see-the-structures)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  const chooser = (chosen: string) => {
    const done: string[] = [];
    const clicks = clickChooser<string>({
      isChosen: (found) => found === chosen,
      letGo: () => done.push("let go"),
      choose: (found, double) => done.push(`${double ? "double-chose" : "chose"} ${found}`),
    });
    return { clicks, done };
  };

  it("lets it go once a double-click's time has passed, and chooses nothing", () => {
    const { clicks, done } = chooser("the chosen point");
    clicks.click("the chosen point");
    expect(done).toEqual([]);
    vi.advanceTimersByTime(LET_GO_AFTER_MS);
    expect(done).toEqual(["let go"]);
  });

  it("chooses anything else at once", () => {
    const { clicks, done } = chooser("the chosen point");
    clicks.click("another point");
    expect(done).toEqual(["chose another point"]);
    vi.advanceTimersByTime(LET_GO_AFTER_MS);
    expect(done).toEqual(["chose another point"]);
  });

  it("lets nothing go where a double-click follows, which keeps it chosen", () => {
    const { clicks, done } = chooser("the chosen point");
    clicks.click("the chosen point");
    clicks.click("the chosen point");
    clicks.doubleClick("the chosen point");
    vi.advanceTimersByTime(LET_GO_AFTER_MS);
    expect(done).toEqual(["double-chose the chosen point"]);
  });

  it("lets nothing go once cancelled, as when the view is unbound", () => {
    const { clicks, done } = chooser("the chosen point");
    clicks.click("the chosen point");
    clicks.cancel();
    vi.advanceTimersByTime(LET_GO_AFTER_MS);
    expect(done).toEqual([]);
  });
});
