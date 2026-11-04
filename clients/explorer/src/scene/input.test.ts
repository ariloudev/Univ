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
  };
  const unbind = bindMouse(canvas, gestures);
  const fire = (type: string, init: PointerEventInit & MouseEventInit) => {
    const event = type === "wheel" ? new WheelEvent(type, { ...init, cancelable: true }) : new MouseEvent(type, init);
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
    expect(seen).toEqual(["turn 10 0"]);
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
    expect(seen).toEqual(["pan 0 20", "pan 8 0"]);
  });

  it("zooms in a step for each notch scrolled forward", () => {
    const { seen, fire } = bound();
    fire("wheel", { deltaY: -100 } as WheelEventInit);
    fire("wheel", { deltaY: 200 } as WheelEventInit);
    expect(seen).toEqual(["zoom 1", "zoom -2"]);
  });

  it("flies on a double click, and stops listening when unbound", () => {
    const { seen, fire, unbind } = bound();
    fire("dblclick", { clientX: 40, clientY: 50 });
    unbind();
    fire("dblclick", { clientX: 40, clientY: 50 });
    expect(seen).toEqual(["double 40 50"]);
  });
});
