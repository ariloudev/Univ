// Binds the mouse on the view's canvas to the view (navigate-with-the-mouse): a drag with the
// primary button turns, a drag with the secondary button or with Shift moves the focus, the
// wheel zooms, a press that hardly moves is a click, and a double click flies. On a touch
// screen, a pinch with two fingers zooms; while two touch, the view neither turns nor clicks.

import { isClick, pinchSteps, wheelSteps } from "./controls.ts";

export interface Gestures {
  turn: (dx: number, dy: number) => void;
  pan: (dx: number, dy: number) => void;
  zoom: (steps: number) => void;
  click: (x: number, y: number) => void;
  doubleClick: (x: number, y: number) => void;
  hover: (x: number, y: number) => void;
  /** The pointer left the view, or began to move it: nothing is under it to name. */
  leave: () => void;
}

interface Press {
  x: number;
  y: number;
  /** The pixels the pointer has travelled since the press. */
  travelled: number;
  panning: boolean;
}

/** The fingers on the view, by pointer id, and how far apart two of them were last. */
class Pinch {
  private readonly fingers = new Map<number, { x: number; y: number }>();
  private spread: number | undefined;

  /** A finger, or the mouse, pressed at `x`, `y`: whether two now touch, a pinch. */
  down(id: number, x: number, y: number): boolean {
    this.fingers.set(id, { x, y });
    this.spread = this.measure();
    return this.spread !== undefined;
  }

  /** The zoom steps of finger `id` moving to `x`, `y`; none where it is no finger of a pinch. */
  move(id: number, x: number, y: number): number | undefined {
    if (!this.fingers.has(id)) return undefined;
    this.fingers.set(id, { x, y });
    const before = this.spread;
    this.spread = this.measure();
    return before === undefined || this.spread === undefined ? undefined : pinchSteps(before, this.spread);
  }

  /** Finger `id` lifted: whether a pinch was under way. */
  up(id: number): boolean {
    const pinching = this.spread !== undefined;
    this.fingers.delete(id);
    this.spread = this.measure();
    return pinching;
  }

  /** How far apart the first two fingers are; none unless two touch. */
  private measure(): number | undefined {
    const [a, b] = [...this.fingers.values()];
    return a === undefined || b === undefined ? undefined : Math.hypot(a.x - b.x, a.y - b.y);
  }
}

/** Listens on `canvas` until the returned function is called. */
export function bindMouse(canvas: HTMLElement, gestures: Gestures): () => void {
  let press: Press | undefined;
  const pinch = new Pinch();
  const listening = new AbortController();
  const options = { signal: listening.signal };
  const local = (event: MouseEvent) => {
    const box = canvas.getBoundingClientRect();
    return { x: event.clientX - box.left, y: event.clientY - box.top };
  };
  const end = () => {
    press = undefined;
  };
  const drag = (event: MouseEvent, held: Press) => {
    const dx = event.clientX - held.x;
    const dy = event.clientY - held.y;
    held.travelled += Math.hypot(dx, dy);
    held.x = event.clientX;
    held.y = event.clientY;
    if (isClick(held.travelled)) return;
    gestures.leave();
    if (held.panning) gestures.pan(dx, dy);
    else gestures.turn(dx, dy);
  };
  canvas.addEventListener(
    "pointerdown",
    (event) => {
      canvas.setPointerCapture?.(event.pointerId);
      // A second finger makes a pinch: the press of the first is no drag nor click.
      if (event.pointerType === "touch" && pinch.down(event.pointerId, event.clientX, event.clientY)) {
        end();
        gestures.leave();
        return;
      }
      press = { x: event.clientX, y: event.clientY, travelled: 0, panning: event.button === 2 || event.shiftKey };
    },
    options,
  );
  canvas.addEventListener(
    "pointermove",
    (event) => {
      const steps = pinch.move(event.pointerId, event.clientX, event.clientY);
      if (steps !== undefined) gestures.zoom(steps);
      else if (press !== undefined) drag(event, press);
      else gestures.hover(local(event).x, local(event).y);
    },
    options,
  );
  canvas.addEventListener(
    "pointerup",
    (event) => {
      // Lifting a finger of a pinch is no click, and leaves the other finger still.
      if (pinch.up(event.pointerId)) return;
      if (press !== undefined && isClick(press.travelled) && event.button === 0) gestures.click(local(event).x, local(event).y);
      end();
    },
    options,
  );
  // A drag the browser takes over ends without a pointerup.
  canvas.addEventListener(
    "pointercancel",
    (event) => {
      pinch.up(event.pointerId);
      end();
    },
    options,
  );
  canvas.addEventListener("pointerleave", () => gestures.leave(), options);
  canvas.addEventListener("lostpointercapture", end, options);
  canvas.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      gestures.leave();
      gestures.zoom(-wheelSteps(event.deltaY, event.deltaMode));
    },
    { ...options, passive: false },
  );
  canvas.addEventListener("dblclick", (event) => gestures.doubleClick(local(event).x, local(event).y), options);
  canvas.addEventListener("contextmenu", (event) => event.preventDefault(), options);
  return () => listening.abort();
}
