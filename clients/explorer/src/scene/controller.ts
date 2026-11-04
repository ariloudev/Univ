// The view and what moves it (navigate-with-the-mouse): turning, zooming, moving the focus,
// flights, the chosen axes, and one drawing for each animation frame that needs one.

import type { Rotations } from "../api/schemas.ts";
import { type View, eyeOf } from "./camera.ts";
import { type Sight, sightOf } from "./frustum.ts";
import { FLIGHT_MS, between, pan, turn, zoom } from "./controls.ts";
import type { Drawer } from "./drawing.ts";
import { type Axes, EQUATORIAL, type Frame, equatorRing, frameOf, ringRadius } from "./frames.ts";
import { type Mat3, ORIGIN, type Vec3 } from "./vec.ts";

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
  /** The ICRS's own axes until the release's rotations arrive and galactic is chosen. */
  axes: Axes = "equatorial";
  frame: Frame = EQUATORIAL;
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

  get toFrame(): Mat3 {
    return this.frame.toFrame;
  }

  /** What the eye sees on the canvas, in CSS pixels. */
  get sight(): Sight {
    const { width, height } = this.around.size();
    return sightOf(this.view, this.toFrame, width, height);
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

  /** Moves the view at once, ending any flight. */
  private steer(view: View): void {
    this.flight = undefined;
    this.changed(view);
  }

  /** The rotations of the release, from its manifests; the view turns to galactic axes. */
  setRotations(rotations: Rotations): void {
    this.rotations = rotations;
    this.setAxes("galactic");
  }

  /** Whether `axes` can be shown: a rotated frame needs the release's rotations. */
  canShow(axes: Axes): boolean {
    return frameOf(axes, this.rotations) !== undefined;
  }

  setAxes(axes: Axes): void {
    const frame = frameOf(axes, this.rotations);
    if (frame === undefined) return;
    this.axes = axes;
    this.frame = frame;
    this.ring = 0;
    this.changed(this.view);
  }

  turn(dx: number, dy: number): void {
    this.steer(turn(this.view, dx, dy));
  }

  pan(dx: number, dy: number): void {
    this.steer(pan(this.view, this.toFrame, dx, dy, this.around.size().height));
  }

  zoom(steps: number): void {
    this.steer(zoom(this.view, steps));
  }

  /** Flies to `focus`, seen from `distance`, or moves there at once for reduced motion. */
  flyTo(focus: Vec3, distance: number): void {
    const to = { ...this.view, focus, distance };
    if (this.around.reducedMotion()) {
      this.steer(to);
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

  /** Advances any flight and draws if anything changed since the last frame; whether it drew. */
  tick(): boolean {
    if (this.flight !== undefined) {
      const t = Math.min(1, (this.around.now() - this.flight.start) / FLIGHT_MS);
      const view = between(this.flight.from, this.flight.to, t);
      if (t >= 1) this.flight = undefined;
      this.changed(view);
    }
    if (!this.dirty) return false;
    this.dirty = false;
    const radius = ringRadius(this.view.distance);
    if (radius !== this.ring) {
      this.ring = radius;
      this.drawer.setLines("equator", equatorRing(this.toFrame, radius));
    }
    const { width, height, ratio } = this.around.size();
    this.drawer.draw(this.view, this.toFrame, Math.round(width * ratio), Math.round(height * ratio));
    return true;
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
