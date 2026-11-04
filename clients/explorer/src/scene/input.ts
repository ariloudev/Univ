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

/** Listens on `canvas` until the returned function is called. */
export function bindMouse(canvas: HTMLElement, gestures: Gestures): () => void {
  let press: { x: number; y: number; moved: number; panning: boolean } | undefined;
  const local = (event: MouseEvent) => {
    const box = canvas.getBoundingClientRect();
    return { x: event.clientX - box.left, y: event.clientY - box.top };
  };
  const down = (event: PointerEvent) => {
    press = { x: event.clientX, y: event.clientY, moved: 0, panning: event.button === 2 || event.shiftKey };
    canvas.setPointerCapture?.(event.pointerId);
  };
  const move = (event: PointerEvent) => {
    if (press === undefined) {
      const at = local(event);
      gestures.hover(at.x, at.y);
      return;
    }
    const dx = event.clientX - press.x;
    const dy = event.clientY - press.y;
    press.moved += Math.hypot(dx, dy);
    press.x = event.clientX;
    press.y = event.clientY;
    if (isClick(press.moved, 0)) return;
    if (press.panning) gestures.pan(dx, dy);
    else gestures.turn(dx, dy);
  };
  const up = (event: PointerEvent) => {
    if (press !== undefined && isClick(press.moved, 0) && event.button === 0) {
      const at = local(event);
      gestures.click(at.x, at.y);
    }
    press = undefined;
  };
  const wheel = (event: WheelEvent) => {
    event.preventDefault();
    gestures.zoom(-wheelSteps(event.deltaY, event.deltaMode));
  };
  const double = (event: MouseEvent) => {
    const at = local(event);
    gestures.doubleClick(at.x, at.y);
  };
  const menu = (event: Event) => {
    event.preventDefault();
  };
  const listeners: [string, EventListener][] = [
    ["pointerdown", down as EventListener],
    ["pointermove", move as EventListener],
    ["pointerup", up as EventListener],
    ["wheel", wheel as EventListener],
    ["dblclick", double as EventListener],
    ["contextmenu", menu],
  ];
  for (const [type, listener] of listeners) canvas.addEventListener(type, listener, { passive: false });
  return () => {
    for (const [type, listener] of listeners) canvas.removeEventListener(type, listener);
  };
}
