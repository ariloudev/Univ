// The view and what moves it (navigate-with-the-mouse): turning, zooming, moving the focus,
// flights, the chosen axes, and one drawing for each animation frame that needs one.

import type { Rotations } from "../api/schemas.ts";
import { armsView } from "./armsView.ts";
import { type View, eyeOf } from "./camera.ts";
import { type Sight, sightOf } from "./frustum.ts";
import { FLIGHT_MS, between, pan, turn, viewingDistance, zoom } from "./controls.ts";
import type { Drawer } from "./drawing.ts";
import { type Axes, EQUATORIAL, type Frame, equatorRing, frameOf, ringRadius } from "./frames.ts";
import { type RingLabel, type RingSet, ringLabels, ringLines, ringSet } from "./rings.ts";
import { type Mat3, ORIGIN, type Vec3, norm } from "./vec.ts";

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
  /** Whether the distance rings are drawn in place of the equator's ring, and the set drawn. */
  private ringsSwitched = false;
  private drawnRings: RingSet | undefined;
  private drawnLabels: readonly RingLabel[] = [];
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

  /** The canvas's size, CSS pixels. */
  get size(): { width: number; height: number } {
    return this.around.size();
  }

  /** What the eye sees on the canvas, in CSS pixels. */
  get sight(): Sight {
    const { width, height } = this.size;
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
    // The rings in the old axes go, though the new ones may be none.
    if (this.drawnRings !== undefined) this.drawer.setLines("distance-rings", undefined);
    this.drawnRings = undefined;
    this.changed(this.view);
  }

  /** Whether the distance rings are on: drawn where any is in reach, and the equator's ring not. */
  get ringsOn(): boolean {
    return this.ringsSwitched;
  }

  /** Draws the distance rings in place of the equator's ring, or the ring again. */
  setRings(on: boolean): void {
    if (on === this.ringsSwitched) return;
    this.ringsSwitched = on;
    this.ring = 0;
    this.drawnRings = undefined;
    this.drawer.setLines(on ? "equator" : "distance-rings", undefined);
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
    this.flyToView({ ...this.view, focus, distance });
  }

  /** Flies to the arms view (see-the-galaxys-arms-face-on) in galactic axes, or moves there at
   * once for reduced motion; whether the release's rotations allow it. */
  armsView(): boolean {
    const frame = frameOf("galactic", this.rotations);
    if (frame === undefined) return false;
    if (this.axes !== "galactic") this.setAxes("galactic");
    const { width, height } = this.around.size();
    this.flyToView(armsView(frame.toFrame, height > 0 ? width / height : 1));
    return true;
  }

  /** Flies to `to`, or moves there at once for reduced motion. */
  private flyToView(to: View): void {
    // Already there, or on the way: a flight begun again would only start over.
    const heading = this.flight?.to ?? this.view;
    const same = heading.distance === to.distance && heading.yaw === to.yaw && heading.pitch === to.pitch;
    if (same && heading.focus.every((c, i) => c === to.focus[i])) return;
    if (this.around.reducedMotion()) {
      this.steer(to);
      return;
    }
    this.flight = { from: this.view, to, start: this.around.now() };
    this.dirty = true;
  }

  /** Makes `focus` the focus, keeping the distance (the one a flight under way ends at): the
   * view then turns about it. */
  centreOn(focus: Vec3): void {
    this.flyTo(focus, (this.flight?.to ?? this.view).distance);
  }

  /** Flies close to `place`, making it the focus. */
  approach(place: Vec3): void {
    this.flyTo(place, viewingDistance(norm(place)));
  }

  /** Makes Earth the focus again, keeping the distance. */
  returnToEarth(): void {
    this.centreOn(ORIGIN);
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
    const { width, height, ratio } = this.around.size();
    if (this.ringsSwitched) this.drawRings(height);
    else this.drawRing();
    this.drawer.draw(this.view, this.toFrame, Math.round(width * ratio), Math.round(height * ratio), ratio);
    return true;
  }

  /** The equator's ring, at the power of ten nearest the eye's distance. */
  private drawRing(): void {
    const radius = ringRadius(this.view.distance);
    if (radius === this.ring) return;
    this.ring = radius;
    this.drawer.setLines("equator", equatorRing(this.toFrame, radius));
  }

  /** The distance rings for a canvas `height` CSS pixels tall, drawn again only when they change. */
  private drawRings(height: number): void {
    const set = ringSet(this.view, this.toFrame, this.axes, height, this.drawnRings);
    if (set === this.drawnRings) return;
    this.drawnRings = set;
    this.drawnLabels = set === undefined ? [] : ringLabels(this.toFrame, set);
    this.drawer.setLines("distance-rings", set === undefined ? undefined : ringLines(this.toFrame, set));
  }

  /** The distance rings drawn, while they are on. */
  get rings(): RingSet | undefined {
    return this.drawnRings;
  }

  /** The labels of the distance rings drawn; none while they are off. */
  get ringLabels(): readonly RingLabel[] {
    return this.drawnRings === undefined ? [] : this.drawnLabels;
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
