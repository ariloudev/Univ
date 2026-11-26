// Binds the mouse on the view's canvas to the view (navigate-with-the-mouse): a drag with the
// primary button turns, a drag with the secondary button or with Shift moves the focus, the
// wheel zooms, a press that hardly moves is a click, and a double click flies. On a touch
// screen, two fingers zoom as they spread and close and move the focus as they move together;
// while two touch, the view neither turns nor clicks.

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

/** What a move of a pinch's finger does: the zoom steps of the fingers' spread, and how far
 * their midpoint moved, pixels. */
interface PinchMove {
  steps: number;
  dx: number;
  dy: number;
}

/** Two fingers as they last were: how far apart, and their midpoint. */
interface Span {
  spread: number;
  x: number;
  y: number;
}

/** The fingers on the view, by pointer id, and how the first two of them last lay. */
class Pinch {
  private readonly fingers = new Map<number, { x: number; y: number }>();
  private span: Span | undefined;

  /** A finger, or the mouse, pressed at `x`, `y`: whether two now touch, a pinch. */
  down(id: number, x: number, y: number): boolean {
    this.fingers.set(id, { x, y });
    this.span = this.measure();
    return this.span !== undefined;
  }

  /** What finger `id` moving to `x`, `y` does; none where it is no finger of a pinch. */
  move(id: number, x: number, y: number): PinchMove | undefined {
    if (!this.fingers.has(id)) return undefined;
    this.fingers.set(id, { x, y });
    const before = this.span;
    this.span = this.measure();
    if (before === undefined || this.span === undefined) return undefined;
    return { steps: pinchSteps(before.spread, this.span.spread), dx: this.span.x - before.x, dy: this.span.y - before.y };
  }

  /** Finger `id` lifted: whether a pinch was under way. */
  up(id: number): boolean {
    const pinching = this.span !== undefined;
    this.fingers.delete(id);
    this.span = this.measure();
    return pinching;
  }

  /** How the first two fingers lie; none unless two touch. */
  private measure(): Span | undefined {
    const [a, b] = [...this.fingers.values()];
    if (a === undefined || b === undefined) return undefined;
    return { spread: Math.hypot(a.x - b.x, a.y - b.y), x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
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
      const pinched = pinch.move(event.pointerId, event.clientX, event.clientY);
      if (pinched !== undefined) {
        // Spreading zooms; moving together moves the focus, as a secondary drag does.
        if (pinched.steps !== 0) gestures.zoom(pinched.steps);
        if (pinched.dx !== 0 || pinched.dy !== 0) gestures.pan(pinched.dx, pinched.dy);
      } else if (press !== undefined) drag(event, press);
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
