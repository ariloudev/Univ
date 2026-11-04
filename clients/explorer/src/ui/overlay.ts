// Keeps the overlay's elements over the places they mark: Earth, the ring's label, the chosen
// object, and the structures' names, which are kept from overlapping.

import type { RefObject } from "react";

import { project } from "../scene/camera.ts";
import { shortDistance } from "../scene/format.ts";
import { ringAnchor } from "../scene/frames.ts";
import { type LabelBox, clearLabels } from "../scene/labels.ts";
import { ORIGIN, type Vec3 } from "../scene/vec.ts";
import type { World } from "./useWorld.ts";

/** Overlay elements kept over the places they mark. */
export interface Overlay {
  earth: RefObject<HTMLDivElement | null>;
  ring: RefObject<HTMLDivElement | null>;
  ringLabel: RefObject<HTMLSpanElement | null>;
  chosen: RefObject<HTMLDivElement | null>;
  /** Each structure's name and the place it names. */
  labels: RefObject<Map<string, { element: HTMLElement; centre: Vec3 }>>;
}

type At = { x: number; y: number; depth: number } | undefined;

function place(element: HTMLElement | null, at: At): void {
  if (element === null) return;
  element.style.display = at === undefined ? "none" : "";
  if (at !== undefined) element.style.transform = `translate(${at.x}px, ${at.y}px)`;
}

function placeLabels(labels: Overlay["labels"]["current"], at: (p: Vec3) => At): void {
  const boxes: LabelBox[] = [];
  for (const [id, { element, centre }] of labels) {
    const on = at(centre);
    if (on !== undefined) boxes.push({ id, x: on.x, y: on.y, width: element.offsetWidth, height: element.offsetHeight, depth: on.depth });
    place(element, on);
  }
  const kept = clearLabels(boxes);
  for (const [id, { element }] of labels) if (!kept.has(id)) element.style.display = "none";
}

/** Places every overlay element for the view `world` shows on a viewport of `size`. */
export function placeOverlay(world: World, overlay: Overlay, size: { width: number; height: number }): void {
  const { controller } = world;
  const at = (p: Vec3): At => project(controller.view, controller.toFrame, p, size.width, size.height);
  place(overlay.earth.current, at(ORIGIN));
  place(overlay.ring.current, at(ringAnchor(controller.toFrame, controller.ringRadius)));
  place(overlay.chosen.current, world.chosen === undefined ? undefined : at(world.chosen));
  placeLabels(overlay.labels.current, at);
  const label = overlay.ringLabel.current;
  const text = shortDistance(controller.ringRadius);
  if (label !== null && label.textContent !== text) label.textContent = text;
}
