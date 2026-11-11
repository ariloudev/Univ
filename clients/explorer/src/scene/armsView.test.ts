import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { manifestOf } from "../test/fixtures.ts";
import { ARMS_FOCUS_KPC, ARMS_REACH_KPC, GALACTIC_CENTRE_KPC, armsView } from "./armsView.ts";
import { eyeAxes, eyeOf, project } from "./camera.ts";
import { type Vec3, apply, unapply } from "./vec.ts";

// The galactic rotation a tile manifest gives (the decoder's fixture of the stars domain).
const TO_GALACTIC = manifestOf("stars").rotations.to_galactic;

/** The ICRS point, Mpc, at heliocentric galactic `x`, `y`, `z` kpc. */
const galactic = (x: number, y: number, z = 0): Vec3 => unapply(TO_GALACTIC, [x * 1e-3, y * 1e-3, z * 1e-3]);

/** The point `r` kpc from the Galactic Centre at Galactocentric azimuth `beta` degrees (0 toward
 * the Sun, increasing toward l = 90°), in the plane 5.5 pc below the Sun, as the model arms are. */
const disk = (r: number, beta: number): Vec3 => {
  const b = (beta * Math.PI) / 180;
  return galactic(GALACTIC_CENTRE_KPC - r * Math.cos(b), r * Math.sin(b), -0.0055);
};

describe("the arms view (see-the-galaxys-arms-face-on)", () => {
  it("looks from the north Galactic pole toward the south one, the focus 4.075 kpc toward l = 0°, b = 0°", () => {
    const view = armsView(TO_GALACTIC, 16 / 9);
    expect(ARMS_FOCUS_KPC).toBe(4.075);
    const focus = apply(TO_GALACTIC, view.focus).map((c) => c * 1e3);
    expect(focus[0]).toBeCloseTo(4.075, 9);
    expect(focus[1]).toBeCloseTo(0, 9);
    expect(focus[2]).toBeCloseTo(0, 9);
    // The eye's backward axis, in galactic axes, is the north Galactic pole.
    const back = apply(TO_GALACTIC, unapply(eyeAxes(view, TO_GALACTIC), [0, 0, 1]));
    expect(back[2]).toBeCloseTo(1, 12);
    const eye = apply(TO_GALACTIC, eyeOf(view, TO_GALACTIC));
    expect(eye[2]).toBeGreaterThan(0);
  });

  it("is defined at the pole: every axis of the eye finite and of unit length", () => {
    const axes = eyeAxes(armsView(TO_GALACTIC, 1), TO_GALACTIC);
    for (const row of axes) {
      expect(row.every(Number.isFinite)).toBe(true);
      expect(Math.hypot(...row)).toBeCloseTo(1, 12);
    }
  });

  it("puts the Sun above the Galactic Centre and l = 90° to its right, so that the Galaxy turns clockwise", () => {
    const [w, h] = [1600, 900];
    const view = armsView(TO_GALACTIC, w / h);
    const sun = project(view, TO_GALACTIC, [0, 0, 0], w, h);
    const centre = project(view, TO_GALACTIC, galactic(GALACTIC_CENTRE_KPC, 0), w, h);
    const ahead = project(view, TO_GALACTIC, galactic(0, 1), w, h);
    if (sun === undefined || centre === undefined || ahead === undefined) throw new Error("not in view");
    expect(sun.x).toBeCloseTo(centre.x, 6);
    expect(sun.y).toBeLessThan(centre.y);
    // The Sun moves toward l = 90°, to the right as it lies above the centre: clockwise.
    expect(ahead.x).toBeGreaterThan(sun.x);
    expect(ahead.y).toBeCloseTo(sun.y, 6);
  });

  it("holds every point within 16 kpc of the Galactic Centre in view, on a wide view and on a tall one", () => {
    expect(ARMS_REACH_KPC).toBeCloseTo(20.075);
    for (const [w, h] of [
      [1600, 900],
      [390, 844],
    ] as const) {
      const view = armsView(TO_GALACTIC, w / h);
      for (let beta = 0; beta < 360; beta += 5) {
        for (const r of [3, 8, 12, 16]) {
          const at = project(view, TO_GALACTIC, disk(r, beta), w, h);
          if (at === undefined) throw new Error(`behind the eye at ${r} kpc, ${beta}°`);
          expect(at.x).toBeGreaterThanOrEqual(0);
          expect(at.x).toBeLessThanOrEqual(w);
          expect(at.y).toBeGreaterThanOrEqual(0);
          expect(at.y).toBeLessThanOrEqual(h);
        }
      }
      // And not so far that the disk is lost: the far edge reaches past four fifths of the
      // narrower half.
      const edge = project(view, TO_GALACTIC, disk(16, 180), w, h);
      const sun = project(view, TO_GALACTIC, [0, 0, 0], w, h);
      if (edge === undefined || sun === undefined) throw new Error("not in view");
      const half = Math.min(w, h) / 2;
      expect(Math.hypot(edge.x - w / 2, edge.y - h / 2)).toBeGreaterThan(0.8 * half);
    }
  });
});

describe("the arms view's numbers, as ingest and the structure files give them", () => {
  const repo = join(dirname(fileURLToPath(import.meta.url)), "../../../..");
  const read = (path: string) => readFileSync(join(repo, path), "utf8");

  it("reaches as far as ingest lets an extrapolation run from the Galactic Centre", () => {
    const band = /pub const RADIUS_BAND_KPC: \(f64, f64\) = \(([\d.]+), ([\d.]+)\);/.exec(read("crates/univ-ingest/src/structures/spiral.rs"));
    expect(Number(band?.[2])).toBe(ARMS_REACH_KPC - ARMS_FOCUS_KPC);
  });

  it("places the Galactic Centre at the R0 every arm file cites", () => {
    const files = readdirSync(join(repo, "structures")).filter((f) => f.startsWith("reid-2019-"));
    expect(files).toHaveLength(14);
    for (const file of files) expect(/r0:\s*\{ value: ([\d.]+), unit: kiloparsec/.exec(read(`structures/${file}`))?.[1]).toBe(String(GALACTIC_CENTRE_KPC));
  });
});
