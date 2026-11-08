// Keeps the overlay's elements over the places they mark: Earth, the ring's label or the
// distance rings' labels, the chosen
// object, and the structures' names, which are kept from overlapping. Nothing is measured from
// the page, so that placing them each frame lays nothing out.

import type { RefObject } from "react";

import { project } from "../scene/camera.ts";
import { shortDistance } from "../scene/format.ts";
import { ringAnchor } from "../scene/frames.ts";
import { ringLabels } from "../scene/rings.ts";
import { type LabelBox, clearLabels, contains } from "../scene/labels.ts";
import { ORIGIN, type Vec3 } from "../scene/vec.ts";
import type { World } from "./useWorld.ts";

/** A structure's name to place: its id, the place it names, and its text. */
export interface NameLabel {
  id: string;
  centre: Vec3;
  text: string;
  /** How far below its centre the name is drawn, device pixels: clear of a flow symbol's
   * arrows. None for a name drawn on its centre. */
  below?: number | undefined;
}

/** Overlay elements kept over the places they mark. */
export interface Overlay {
  earth: RefObject<HTMLDivElement | null>;
  ring: RefObject<HTMLDivElement | null>;
  ringLabel: RefObject<HTMLSpanElement | null>;
  /** Holds the distance rings' labels, one element each, made as they are needed. */
  ringLabels: RefObject<HTMLDivElement | null>;
  chosen: RefObject<HTMLDivElement | null>;
  /** Holds one element for each structure's name, its id in `data-structure`. */
  names: RefObject<HTMLDivElement | null>;
}

type At = { x: number; y: number; depth: number } | undefined;

/** The structures' names' type: italic Spectral at Tailwind's `text-xs`, measured where a canvas
 * can measure it. */
let measure: ((text: string) => number) | undefined | null;

function measurer(): ((text: string) => number) | null {
  if (measure !== undefined) return measure;
  const context = document.createElement("canvas").getContext("2d");
  if (context === null) return (measure = null);
  // A name measured before Spectral has loaded is measured again once it has.
  void document.fonts?.ready.then(() => {
    widths.clear();
  });
  context.font = `italic 12px ${getComputedStyle(document.documentElement).getPropertyValue("--font-serif").trim() || "serif"}`;
  return (measure = (text) => context.measureText(text).width);
}

const widths = new Map<string, number>();

/** A name's box: its text measured once, or, where nothing can measure it, about 6.6 px a
 * character; 16 px a line. */
export function labelSize(text: string): { width: number; height: number } {
  let width = widths.get(text);
  if (width === undefined) {
    width = (measurer()?.(text) ?? text.length * 6.6) + 4;
    widths.set(text, width);
  }
  return { width, height: 16 };
}

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
/** The gap between a flow symbol's arrows and the name below them, CSS pixels. */
const BELOW_GAP = 2;

function placeNames(container: HTMLElement | null, names: readonly NameLabel[], at: (p: Vec3) => At, ratio: number): LabelBox[] {
  if (container === null) return [];
  const placed = new Map<string, At>();
  const boxes: LabelBox[] = [];
  for (const name of names) {
    const size = labelSize(name.text);
    const centre = at(name.centre);
    // A name drawn below its centre has its box's top there, in CSS pixels.
    const on = centre === undefined || name.below === undefined ? centre : { ...centre, y: centre.y + name.below / ratio + BELOW_GAP + size.height / 2 };
    placed.set(name.id, on);
    if (on !== undefined) boxes.push({ id: name.id, x: on.x, y: on.y, depth: on.depth, ...size });
  }
  const shown = clearLabels(boxes);
  const kept = new Set(shown.map((box) => box.id));
  for (const element of container.children) {
    if (!(element instanceof HTMLElement)) continue;
    const id = element.dataset["structure"] ?? "";
    const on = placed.get(id);
    if (on !== undefined && kept.has(id)) place(element, on);
    else show(element, false);
  }
  return shown;
}

/** The structure whose name, as last placed, is under `x`, `y`, if any: its id and name. */
export function nameAt(world: World, x: number, y: number): NameLabel | undefined {
  const box = world.namesShown.find((b) => contains(b, x, y));
  return box === undefined ? undefined : world.names.find((name) => name.id === box.id);
}

/** Places the labels `labels` in `container`, each to the right of its place, the nearest first
 * and each clear of those placed before it; hides those left over. */
function placeRingLabels(container: HTMLElement | null, labels: readonly { text: string; at: Vec3 }[], at: (p: Vec3) => At): void {
  if (container === null) return;
  const boxes: (LabelBox & { text: string })[] = [];
  labels.forEach((label, i) => {
    const on = at(label.at);
    const size = labelSize(label.text);
    if (on !== undefined) boxes.push({ id: String(i), x: on.x + size.width / 2, y: on.y, depth: on.depth, ...size, text: label.text });
  });
  const shown = clearLabels(boxes) as (LabelBox & { text: string })[];
  while (container.children.length < shown.length) container.append(Object.assign(document.createElement("span"), { className: "absolute top-0 left-0 whitespace-nowrap" }));
  [...container.children].forEach((element, i) => {
    if (!(element instanceof HTMLElement)) return;
    const box = shown[i];
    place(element, box === undefined ? undefined : { x: box.x - box.width / 2, y: box.y - box.height / 2, depth: box.depth });
    if (box !== undefined && element.textContent !== box.text) element.textContent = box.text;
  });
}

/** Places every overlay element for the view `world` shows on a viewport of `size`. */
export function placeOverlay(world: World, overlay: Overlay, size: { width: number; height: number; ratio?: number }): void {
  const { controller } = world;
  const at = (p: Vec3): At => project(controller.view, controller.toFrame, p, size.width, size.height);
  place(overlay.earth.current, at(ORIGIN));
  const { rings } = controller;
  // The distance rings take the place of the equator's ring and its label.
  place(overlay.ring.current, rings === undefined ? at(ringAnchor(controller.toFrame, controller.ringRadius)) : undefined);
  placeRingLabels(overlay.ringLabels.current, rings === undefined ? [] : ringLabels(controller.toFrame, rings), at);
  place(overlay.chosen.current, world.chosen === undefined ? undefined : at(world.chosen));
  world.namesShown = placeNames(overlay.names.current, world.names, at, size.ratio ?? 1);
  const label = overlay.ringLabel.current;
  const text = shortDistance(controller.ringRadius);
  if (label !== null && label.textContent !== text) label.textContent = text;
}
