// Keeps the overlay's elements over the places they mark: Earth, the ring's label, the chosen
// object, and the structures' names, which are kept from overlapping. Nothing is measured from
// the page, so that placing them each frame lays nothing out.

import type { RefObject } from "react";

import { project } from "../scene/camera.ts";
import { shortDistance } from "../scene/format.ts";
import { ringAnchor } from "../scene/frames.ts";
import { type LabelBox, clearLabels } from "../scene/labels.ts";
import { ORIGIN, type Vec3 } from "../scene/vec.ts";
import type { World } from "./useWorld.ts";

/** A structure's name to place: its id, the place it names, and its text. */
export interface NameLabel {
  id: string;
  centre: Vec3;
  text: string;
}

/** Overlay elements kept over the places they mark. */
export interface Overlay {
  earth: RefObject<HTMLDivElement | null>;
  ring: RefObject<HTMLDivElement | null>;
  ringLabel: RefObject<HTMLSpanElement | null>;
  chosen: RefObject<HTMLDivElement | null>;
  /** Holds one element for each structure's name, its id in `data-structure`. */
  names: RefObject<HTMLDivElement | null>;
}

type At = { x: number; y: number; depth: number } | undefined;

/** A name's box: the overlay's small type is about 6.6 px a character and 16 px a line. */
export const labelSize = (text: string) => ({ width: text.length * 6.6 + 4, height: 16 });

function show(element: HTMLElement, shown: boolean): void {
  const display = shown ? "" : "none";
  if (element.style.display !== display) element.style.display = display;
}

function place(element: HTMLElement | null, at: At): void {
  if (element === null) return;
  show(element, at !== undefined);
  if (at !== undefined) element.style.transform = `translate(${at.x}px, ${at.y}px)`;
}

/** Places the names in `container`; the boxes of those shown. */
function placeNames(container: HTMLElement | null, names: readonly NameLabel[], at: (p: Vec3) => At): LabelBox[] {
  if (container === null) return [];
  const placed = new Map<string, At>();
  const boxes: LabelBox[] = [];
  for (const name of names) {
    const on = at(name.centre);
    placed.set(name.id, on);
    if (on !== undefined) boxes.push({ id: name.id, x: on.x, y: on.y, depth: on.depth, ...labelSize(name.text) });
  }
  const kept = clearLabels(boxes);
  for (const element of container.children) {
    if (!(element instanceof HTMLElement)) continue;
    const id = element.dataset["structure"] ?? "";
    const on = placed.get(id);
    if (on !== undefined && kept.has(id)) place(element, on);
    else show(element, false);
  }
  return boxes.filter((box) => kept.has(box.id));
}

/** The structure whose name, as last placed, is under `x`, `y`, if any. */
export function nameAt(world: World, x: number, y: number): string | undefined {
  const box = world.namesShown.find((b) => Math.abs(x - b.x) * 2 <= b.width && Math.abs(y - b.y) * 2 <= b.height);
  return box?.id;
}

/** Places every overlay element for the view `world` shows on a viewport of `size`. */
export function placeOverlay(world: World, overlay: Overlay, size: { width: number; height: number }): void {
  const { controller } = world;
  const at = (p: Vec3): At => project(controller.view, controller.toFrame, p, size.width, size.height);
  place(overlay.earth.current, at(ORIGIN));
  place(overlay.ring.current, at(ringAnchor(controller.toFrame, controller.ringRadius)));
  place(overlay.chosen.current, world.chosen === undefined ? undefined : at(world.chosen));
  world.namesShown = placeNames(overlay.names.current, world.names, at);
  const label = overlay.ringLabel.current;
  const text = shortDistance(controller.ringRadius);
  if (label !== null && label.textContent !== text) label.textContent = text;
}
