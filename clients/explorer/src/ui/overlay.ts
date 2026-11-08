// Keeps the overlay's elements over the places they mark: Earth, the ring's label or the
// distance rings' labels, the chosen
// object, and the structures' names, which are kept from overlapping. Nothing is measured from
// the page, so that placing them each frame lays nothing out.

import type { RefObject } from "react";

import { project } from "../scene/camera.ts";
import { shortDistance } from "../scene/format.ts";
import { ringAnchor } from "../scene/frames.ts";
import type { RingLabel } from "../scene/rings.ts";
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

/** The labels' types, at Tailwind's `text-xs`: the structures' names in italic Spectral, the
 * rings' distances in the page's sans. */
type LabelType = "name" | "ring";
const FONTS: Record<LabelType, [style: string, variable: string, fallback: string]> = { name: ["italic ", "--font-serif", "serif"], ring: ["", "--font-sans", "sans-serif"] };

/** Each type's measure, where a canvas can measure it. */
const measures = new Map<LabelType, ((text: string) => number) | null>();

function measurer(type: LabelType): ((text: string) => number) | null {
  const made = measures.get(type);
  if (made !== undefined) return made;
  const context = document.createElement("canvas").getContext("2d");
  if (context === null) {
    measures.set(type, null);
    return null;
  }
  // A label measured before its font has loaded is measured again once it has.
  void document.fonts?.ready.then(() => {
    widths.clear();
  });
  const [style, variable, fallback] = FONTS[type];
  context.font = `${style}12px ${getComputedStyle(document.documentElement).getPropertyValue(variable).trim() || fallback}`;
  const measure = (text: string) => context.measureText(text).width;
  measures.set(type, measure);
  return measure;
}

const widths = new Map<string, number>();

/** A label's box: its text measured once in its type, or, where nothing can measure it, about
 * 6.6 px a character; 16 px a line. */
export function labelSize(text: string, type: LabelType = "name"): { width: number; height: number } {
  const key = `${type}\u0000${text}`;
  let width = widths.get(key);
  if (width === undefined) {
    width = (measurer(type)?.(text) ?? text.length * 6.6) + 4;
    widths.set(key, width);
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

/** Places the labels `labels` in `container`, each to the right of its place, those on the view
 * only, the nearest first and each clear of those placed before it; hides those left over. */
function placeRingLabels(container: HTMLElement | null, labels: readonly RingLabel[], at: (p: Vec3) => At, size: { width: number; height: number }, names: readonly LabelBox[]): void {
  if (container === null) return;
  const boxes: (LabelBox & { text: string })[] = [];
  labels.forEach((label, i) => {
    const on = at(label.at);
    const box = labelSize(label.text, "ring");
    if (on !== undefined && on.x >= -box.width && on.x <= size.width && on.y >= 0 && on.y <= size.height) {
      boxes.push({ id: String(i), x: on.x + box.width / 2, y: on.y, depth: on.depth, ...box, text: label.text });
    }
  });
  // Clear of the structures' names too, which are placed first.
  const shown = clearLabels(boxes, names);
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
  // The distance rings take the place of the equator's ring and its label.
  place(overlay.ring.current, controller.ringsOn ? undefined : at(ringAnchor(controller.toFrame, controller.ringRadius)));
  place(overlay.chosen.current, world.chosen === undefined ? undefined : at(world.chosen));
  world.namesShown = placeNames(overlay.names.current, world.names, at, size.ratio ?? 1);
  placeRingLabels(overlay.ringLabels.current, controller.ringLabels, at, size, world.namesShown);
  const label = overlay.ringLabel.current;
  const text = shortDistance(controller.ringRadius);
  if (label !== null && label.textContent !== text) label.textContent = text;
}
