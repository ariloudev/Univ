// The view and what moves it (navigate-with-the-mouse): turning, zooming, moving the focus,
// flights, the chosen axes, and one drawing for each animation frame that needs one.

import type { View } from "./camera.ts";
import { FLIGHT_MS, arriving, between, pan, turn, zoom } from "./controls.ts";
import { type Axes, type Rotations, equatorRing, frameOf, ringRadius } from "./frames.ts";
import type { Lines } from "./renderer.ts";
import { IDENTITY, type Mat3, ORIGIN, type Vec3 } from "./vec.ts";

/** What the controller draws on. */
export interface Drawer {
  draw: (view: View, toFrame: Mat3, width: number, height: number) => void;
  setLines: (name: string, lines: Lines | undefined) => void;
}

export interface Surroundings {
  /** The canvas's size in CSS pixels and the device's pixels per CSS pixel. */
  size: () => { width: number; height: number; ratio: number };
  now: () => number;
  /** Whether the person's system asks for reduced motion. */
  reducedMotion: () => boolean;
}

/** The view the explorer opens on: Earth at the focus, the nearby universe around it. */
const OPENING: View = { focus: ORIGIN, distance: 400, yaw: 0.9, pitch: 0.35 };

interface Flight {
  from: View;
  to: View;
  start: number;
}

export class Controller {
  view: View = OPENING;
  axes: Axes = "galactic";
  toFrame: Mat3 = IDENTITY;
  basis = "";
  private rotations: Rotations | undefined;
  private flight: Flight | undefined;
  private dirty = true;
  private ring = 0;
  private readonly listeners = new Set<() => void>();
  private readonly drawer: Drawer;
  private readonly around: Surroundings;

  constructor(drawer: Drawer, around: Surroundings) {
    this.drawer = drawer;
    this.around = around;
  }

  /** Called whenever the view or the axes change. */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed(view: View): void {
    this.view = view;
    this.dirty = true;
    for (const listener of this.listeners) listener();
  }

  /** The rotations of the release, from its manifests. */
  setRotations(rotations: Rotations): void {
    this.rotations = rotations;
    this.setAxes(this.axes);
  }

  setAxes(axes: Axes): void {
    this.axes = axes;
    if (this.rotations !== undefined) {
      const frame = frameOf(axes, this.rotations);
      this.toFrame = frame.toFrame;
      this.basis = frame.basis;
    }
    this.ring = 0;
    this.changed(this.view);
  }

  turn(dx: number, dy: number): void {
    this.flight = undefined;
    this.changed(turn(this.view, dx, dy));
  }

  pan(dx: number, dy: number): void {
    this.flight = undefined;
    this.changed(pan(this.view, this.toFrame, dx, dy, this.around.size().height));
  }

  zoom(steps: number): void {
    this.flight = undefined;
    this.changed(zoom(this.view, steps));
  }

  /** Flies to `focus`, seen from `distance`, or moves there at once for reduced motion. */
  flyTo(focus: Vec3, distance: number): void {
    const to = arriving(this.view, focus, distance);
    if (this.around.reducedMotion()) {
      this.flight = undefined;
      this.changed(to);
      return;
    }
    this.flight = { from: this.view, to, start: this.around.now() };
    this.dirty = true;
  }

  /** Makes Earth the focus again, keeping the distance. */
  returnToEarth(): void {
    this.flyTo(ORIGIN, this.view.distance);
  }

  /** Whether a flight is under way. */
  get flying(): boolean {
    return this.flight !== undefined;
  }

  /** Advances any flight and draws, if anything changed since the last frame. */
  frame(): void {
    if (this.flight !== undefined) {
      const t = Math.min(1, (this.around.now() - this.flight.start) / FLIGHT_MS);
      const view = between(this.flight.from, this.flight.to, t);
      if (t >= 1) this.flight = undefined;
      this.changed(view);
    }
    if (!this.dirty) return;
    this.dirty = false;
    const radius = ringRadius(this.view.distance);
    if (radius !== this.ring) {
      this.ring = radius;
      this.drawer.setLines("equator", equatorRing(this.toFrame, radius));
    }
    const { width, height, ratio } = this.around.size();
    this.drawer.draw(this.view, this.toFrame, Math.round(width * ratio), Math.round(height * ratio));
  }

  /** Asks for a drawing on the next frame, as when new points arrive. */
  invalidate(): void {
    this.dirty = true;
  }

  /** The radius of the equator ring drawn, megaparsecs. */
  get ringRadius(): number {
    return ringRadius(this.view.distance);
  }
}
