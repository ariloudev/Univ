import { describe, expect, it } from "vitest";

import { type Gestures, bindMouse } from "./input.ts";

/** A canvas bound to gestures that record what they were told. */
function bound() {
  const canvas = document.createElement("div");
  document.body.append(canvas);
  const seen: string[] = [];
  const gestures: Gestures = {
    turn: (dx, dy) => seen.push(`turn ${dx} ${dy}`),
    pan: (dx, dy) => seen.push(`pan ${dx} ${dy}`),
    zoom: (steps) => seen.push(`zoom ${steps}`),
    click: (x, y) => seen.push(`click ${x} ${y}`),
    doubleClick: (x, y) => seen.push(`double ${x} ${y}`),
    hover: (x, y) => seen.push(`hover ${x} ${y}`),
    leave: () => seen.push("leave"),
  };
  const unbind = bindMouse(canvas, gestures);
  const fire = (type: string, init: PointerEventInit & MouseEventInit) => {
    const event = type === "wheel" ? new WheelEvent(type, { ...init, cancelable: true }) : new MouseEvent(type, init);
    // jsdom has no PointerEvent: a pointer's id and type set on the mouse event.
    for (const key of ["pointerId", "pointerType"] as const) {
      if (init[key] !== undefined) Object.defineProperty(event, key, { value: init[key] });
    }
    canvas.dispatchEvent(event);
  };
  return { seen, fire, unbind };
}

describe("the mouse on the view", () => {
  it("turns the view while the primary button drags", () => {
    const { seen, fire } = bound();
    fire("pointerdown", { clientX: 100, clientY: 100, button: 0 });
    fire("pointermove", { clientX: 110, clientY: 100 });
    fire("pointerup", { clientX: 110, clientY: 100, button: 0 });
    expect(seen).toEqual(["leave", "turn 10 0"]);
  });

  it("calls a press that moves less than 6 pixels a click, and turns nothing", () => {
    const { seen, fire } = bound();
    fire("pointerdown", { clientX: 100, clientY: 100, button: 0 });
    fire("pointermove", { clientX: 103, clientY: 104 });
    fire("pointerup", { clientX: 103, clientY: 104, button: 0 });
    expect(seen).toEqual(["click 103 104"]);
  });

  it("moves the focus while the secondary button, or Shift, drags", () => {
    const { seen, fire } = bound();
    fire("pointerdown", { clientX: 0, clientY: 0, button: 2 });
    fire("pointermove", { clientX: 0, clientY: 20 });
    fire("pointerup", { clientX: 0, clientY: 20, button: 2 });
    fire("pointerdown", { clientX: 0, clientY: 0, button: 0, shiftKey: true });
    fire("pointermove", { clientX: 8, clientY: 0 });
    fire("pointerup", { clientX: 8, clientY: 0, button: 0 });
    expect(seen).toEqual(["leave", "pan 0 20", "leave", "pan 8 0"]);
  });

  it("zooms in a step for each notch scrolled forward", () => {
    const { seen, fire } = bound();
    fire("wheel", { deltaY: -100 } as WheelEventInit);
    fire("wheel", { deltaY: 200 } as WheelEventInit);
    expect(seen).toEqual(["leave", "zoom 1", "leave", "zoom -2"]);
  });

  it("flies on a double click, and stops listening when unbound", () => {
    const { seen, fire, unbind } = bound();
    fire("dblclick", { clientX: 40, clientY: 50 });
    unbind();
    fire("dblclick", { clientX: 40, clientY: 50 });
    expect(seen).toEqual(["double 40 50"]);
  });
});

describe("two fingers on the view", () => {
  const finger = (pointerId: number, clientX: number, clientY = 100) => ({ pointerId, pointerType: "touch", clientX, clientY, button: 0 });

  it("zoom in as they spread and out as they close, by the ratio of their spread", () => {
    const { seen, fire } = bound();
    fire("pointerdown", finger(1, 100));
    fire("pointerdown", finger(2, 200));
    // From 100 pixels apart to 115: in by one step of 1.15.
    fire("pointermove", finger(2, 215));
    fire("pointermove", finger(2, 200));
    const steps = seen.filter((s) => s.startsWith("zoom")).map((s) => Number(s.split(" ")[1]));
    expect(steps).toHaveLength(2);
    // One finger moving pulls their midpoint by half its move: the pinch moves the focus too.
    expect(seen.filter((s) => s.startsWith("pan"))).toEqual(["pan 7.5 0", "pan -7.5 0"]);
    expect(steps[0]).toBeCloseTo(1);
    expect(steps[1]).toBeCloseTo(-1);
  });

  it("neither turn the view nor click, also as they lift", () => {
    const { seen, fire } = bound();
    fire("pointerdown", finger(1, 100));
    fire("pointerdown", finger(2, 102));
    fire("pointermove", finger(1, 90));
    fire("pointerup", finger(2, 102));
    fire("pointermove", finger(1, 60));
    fire("pointerup", finger(1, 60));
    expect(seen.some((s) => s.startsWith("turn") || s.startsWith("click"))).toBe(false);
  });

  it("move the focus as they move together, by the distance their midpoint moved", () => {
    const { seen, fire } = bound();
    fire("pointerdown", finger(1, 100, 100));
    fire("pointerdown", finger(2, 200, 100));
    fire("pointermove", finger(1, 130, 120));
    fire("pointermove", finger(2, 230, 120));
    expect(seen.filter((s) => s.startsWith("pan"))).toEqual(["pan 15 10", "pan 15 10"]);
    // Their spread back where it was: the zooms of the two moves undo each other.
    const zoomed = seen.filter((s) => s.startsWith("zoom")).reduce((sum, s) => sum + Number(s.split(" ")[1]), 0);
    expect(zoomed).toBeCloseTo(0);
  });

  it("leave one finger to turn the view as a drag does", () => {
    const { seen, fire } = bound();
    fire("pointerdown", finger(1, 100));
    fire("pointermove", finger(1, 120));
    fire("pointerup", finger(1, 120));
    expect(seen).toEqual(["leave", "turn 20 0"]);
  });
});
