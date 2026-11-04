// Binds the mouse on the view's canvas to the view (navigate-with-the-mouse): a drag with the
// primary button turns, a drag with the secondary button or with Shift moves the focus, the
// wheel zooms, a press that hardly moves is a click, and a double click flies.

import { isClick, wheelSteps } from "./controls.ts";

export interface Gestures {
  turn: (dx: number, dy: number) => void;
  pan: (dx: number, dy: number) => void;
  zoom: (steps: number) => void;
  click: (x: number, y: number) => void;
  doubleClick: (x: number, y: number) => void;
  hover: (x: number, y: number) => void;
}

interface Press {
  x: number;
  y: number;
  /** The pixels the pointer has travelled since the press. */
  travelled: number;
  panning: boolean;
}

/** Listens on `canvas` until the returned function is called. */
export function bindMouse(canvas: HTMLElement, gestures: Gestures): () => void {
  let press: Press | undefined;
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
    if (held.panning) gestures.pan(dx, dy);
    else gestures.turn(dx, dy);
  };
  canvas.addEventListener(
    "pointerdown",
    (event) => {
      press = { x: event.clientX, y: event.clientY, travelled: 0, panning: event.button === 2 || event.shiftKey };
      canvas.setPointerCapture?.(event.pointerId);
    },
    options,
  );
  canvas.addEventListener(
    "pointermove",
    (event) => {
      if (press !== undefined) drag(event, press);
      else gestures.hover(local(event).x, local(event).y);
    },
    options,
  );
  canvas.addEventListener(
    "pointerup",
    (event) => {
      if (press !== undefined && isClick(press.travelled) && event.button === 0) gestures.click(local(event).x, local(event).y);
      end();
    },
    options,
  );
  // A drag the browser takes over ends without a pointerup.
  canvas.addEventListener("pointercancel", end, options);
  canvas.addEventListener("lostpointercapture", end, options);
  canvas.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      gestures.zoom(-wheelSteps(event.deltaY, event.deltaMode));
    },
    { ...options, passive: false },
  );
  canvas.addEventListener("dblclick", (event) => gestures.doubleClick(local(event).x, local(event).y), options);
  canvas.addEventListener("contextmenu", (event) => event.preventDefault(), options);
  return () => listening.abort();
}
