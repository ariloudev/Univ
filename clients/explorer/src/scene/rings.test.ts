import { describe, expect, it } from "vitest";

import { type View, focalPixels, project, unitsPerPixel } from "./camera.ts";
import { FADE, type Lines } from "./drawing.ts";
import { type RingSet, ringLabels, ringLines, ringSet, ringSpacing } from "./rings.ts";
import { scaleBar } from "./scale.ts";
import { IDENTITY, type Mat3, type Vec3, add, apply } from "./vec.ts";

const RAD = Math.PI / 180;

/** Seen from `distance` megaparsecs, looking at `focus` from above the plane at 40 degrees. */
const view = (focus: Vec3, distance: number, yaw = 0.3): View => ({ focus, distance, yaw, pitch: 40 * RAD });

/** A rotation about the x axis by `degrees`, standing in for other axes. */
const tilted = (degrees: number): Mat3 => {
  const c = Math.cos(degrees * RAD);
  const s = Math.sin(degrees * RAD);
  return [
    [1, 0, 0],
    [0, c, s],
    [0, -s, c],
  ];
};

/** `value`, which the test needs to be there. */
function must<T>(value: T | undefined): T {
  if (value === undefined) throw new Error("missing");
  return value;
}

/** The rings for `v` in the ICRS's own axes on an 800-pixel viewport. */
const rings = (v: View, toFrame: Mat3 = IDENTITY) => must(ringSet(v, toFrame, "galactic", 800));

/** The vertices of `lines`, in the axes `toFrame` turns the ICRS into. */
function vertices(lines: Lines, toFrame: Mat3): Vec3[] {
  const out: Vec3[] = [];
  for (let i = 0; i < lines.offsets.length; i += 3) {
    const p = add(lines.centre, [lines.offsets[i] ?? 0, lines.offsets[i + 1] ?? 0, lines.offsets[i + 2] ?? 0]);
    out.push(apply(toFrame, p));
  }
  return out;
}

describe("the distance rings", () => {
  it("are spaced at the scale bar's length, at each multiple within three times the eye's distance of the focus, and none past four", () => {
    const v = view([0, 0, 0], 100);
    const set = rings(v);
    expect(set.step).toBeCloseTo(must(scaleBar(unitsPerPixel(v, 800))).parsecs / 1e6, 12);
    expect(set.first).toBe(1);
    expect((set.last + 1) * set.step).toBeGreaterThan(300);
    expect(set.last * set.step).toBeLessThanOrEqual(400 + 1e-9);
    // A focus 1,000 Mpc out in the plane, seen from 100: rings from 700 to 1,300 at least, and
    // within 600 to 1,400.
    const far = rings(view([1000, 0, 0], 100));
    expect(far.first * far.step).toBeLessThanOrEqual(700 + 1e-9);
    expect(far.first * far.step).toBeGreaterThanOrEqual(600 - 1e-9);
    expect((far.last + 1) * far.step).toBeGreaterThan(1300);
    expect(far.last * far.step).toBeLessThanOrEqual(1400 + 1e-9);
    // Measured in the plane: a focus straight above the barycentre is as near as Earth.
    expect(rings(view([0, 0, 500], 100)).first).toBe(1);
  });

  it("keep the set drawn while it still reaches three times the eye's distance and runs no farther than four", () => {
    const at = view([1000, 0, 0], 100);
    const set = rings(at);
    // A small zoom out, within the same step: the set drawn still serves.
    const out = { ...at, distance: 104 };
    expect(ringSet(out, IDENTITY, "galactic", 800, set)).toBe(set);
    // Far enough that it no longer reaches: a new set.
    const moved = { ...at, focus: [1200, 0, 0] as const };
    expect(ringSet(moved, IDENTITY, "galactic", 800, set)).not.toBe(set);
  });

  it("lie in the axes' equator, each at its distance, with a spoke every 30 degrees", () => {
    for (const toFrame of [IDENTITY, tilted(62.9)]) {
      const set = ringSet(view([0, 0, 0], 50), toFrame, "galactic", 800);
      if (set === undefined) throw new Error("no rings");
      const lines = ringLines(toFrame, set);
      const radii = new Set<number>();
      for (const [x, y, z] of vertices(lines, toFrame)) {
        expect(Math.abs(z)).toBeLessThan(1e-4);
        radii.add(Math.round((Math.hypot(x, y) / set.step) * 1e3) / 1e3);
      }
      const rings = set.last - set.first + 1;
      for (let k = set.first; k <= set.last; k += 1) expect(radii.has(k)).toBe(true);
      // 192 segments a ring, 12 spokes.
      expect(lines.offsets.length / 6).toBe(rings * 192 + 12);
    }
  });

  it("are arcs near a focus far from the barycentre, drawn finely and from a centre among them", () => {
    const toFrame = tilted(30);
    const set = ringSet(view([5000, 3000, 0], 2), toFrame, "galactic", 800);
    if (set === undefined) throw new Error("no rings");
    expect(set.sector).toBeDefined();
    const lines = ringLines(toFrame, set);
    // Offsets small beside the distance from the barycentre, so float32 keeps them.
    for (const o of lines.offsets) expect(Math.abs(o)).toBeLessThan(50);
    // Each arc's segments stay on its circle: a chord's middle within a millionth of the step.
    const points = vertices(lines, toFrame);
    for (let i = 0; i + 1 < Math.min(points.length, 400); i += 2) {
      const [a, b] = [points[i] ?? [0, 0, 0], points[i + 1] ?? [0, 0, 0]];
      const mid = Math.hypot((a[0] + b[0]) / 2, (a[1] + b[1]) / 2);
      expect(Math.abs(mid - Math.hypot(a[0], a[1]))).toBeLessThan(set.step * 1e-3);
    }
    // Its labels lie on the line through the arcs' middle.
    for (const label of ringLabels(toFrame, set)) {
      const [x, y] = apply(toFrame, label.at);
      expect(Math.atan2(y, x) / RAD).toBeCloseTo(set.sector?.lon ?? 0, 6);
    }
  });

  it("label each ring with its distance where it crosses the spoke of zero longitude", () => {
    const set = ringSet(view([0, 0, 0], 100), IDENTITY, "galactic", 800);
    if (set === undefined) throw new Error("no rings");
    const labels = ringLabels(IDENTITY, set);
    expect(labels).toHaveLength(set.last - set.first + 1);
    expect(labels[0]?.at[1]).toBeCloseTo(0, 9);
    expect(labels[0]?.at[0]).toBeCloseTo(set.step, 9);
    expect(labels[0]?.text).toMatch(/^[\d.,]+ (pc|kpc|Mpc|Gpc)$/);
  });

  it("label rings side by side apart, to the step's last figure, far from the barycentre", () => {
    // About 1,000 Mpc out, seen from 50: rings every 5 Mpc read 1,005 and 1,010 Mpc, not both 1.01 Gpc.
    const set = rings(view([1000, 0, 0], 50));
    const texts = ringLabels(IDENTITY, set).map((l) => l.text);
    expect(new Set(texts).size).toBe(texts.length);
    for (const text of texts) expect(text).toMatch(/^[\d,]+(\.\d+)? (pc|kpc|Mpc|Gpc)$/);
  });

  it("are whole rings, labelled on the spoke of zero longitude, where arcs would go round", () => {
    // Just past four times the eye's distance: arcs half 128 degrees wide would go round.
    const set = rings(view([0, 410, 0], 100));
    expect(set.sector).toBeUndefined();
    const [label] = ringLabels(IDENTITY, set);
    expect(label?.at[1]).toBeCloseTo(0, 9);
    expect(ringLines(IDENTITY, set).offsets.length / 6).toBe((set.last - set.first + 1) * 192 + 12);
  });

  it("write each distance in the largest unit it reaches one of, to the step's last figure", () => {
    const set: RingSet = { axes: "galactic", step: 0.2, first: 4, last: 6, sector: undefined, key: "" };
    expect(ringLabels(IDENTITY, set).map((l) => l.text)).toEqual(["800 kpc", "1.0 Mpc", "1.2 Mpc"]);
    expect(ringLabels(IDENTITY, { ...set, first: 71, last: 71 })[0]?.text).toBe("14.2 Mpc");
    expect(ringLabels(IDENTITY, { ...set, step: 5, first: 199, last: 202 }).map((l) => l.text)).toEqual(["995 Mpc", "1.000 Gpc", "1.005 Gpc", "1.010 Gpc"]);
  });

  it("lie as far apart on the view as their step from above, and closer toward the horizon", () => {
    const at = (v: View) => (p: Vec3) => project(v, IDENTITY, p, 800, 800);
    // From straight above, 100 Mpc off: a step of 10 Mpc is a tenth of the focal length.
    const above: View = { focus: [0, 0, 0], distance: 100, yaw: 0, pitch: 89.99 * RAD };
    expect(ringSpacing(at(above), [50, 0, 0], 10)).toBeCloseTo((focalPixels(800) * 10) / 100, 0);
    // Seen 5 degrees above the plane, the far rings crowd and the near ones do not.
    const grazing = view([0, 0, 0], 10, 0);
    grazing.pitch = 5 * RAD;
    const near = ringSpacing(at(grazing), [-5, 0, 0], 0.5);
    const far = ringSpacing(at(grazing), [-25, 0, 0], 0.5);
    expect(far).toBeLessThan(near / 5);
    // Behind the eye nothing is seen: no crowding to fade for.
    expect(ringSpacing(at(grazing), [20, 0, 0], 0.5)).toBe(Infinity);
  });

  it("carry at each ring's end the vector out to the next ring, by which they fade as they crowd, and none on the spokes", () => {
    const set = rings(view([0, 0, 0], 100));
    const lines = ringLines(IDENTITY, set);
    const ends = vertices(lines, IDENTITY);
    const next = (i: number): Vec3 => [lines.neighbours?.[i * 3] ?? NaN, lines.neighbours?.[i * 3 + 1] ?? NaN, lines.neighbours?.[i * 3 + 2] ?? NaN];
    const ringEnds = (set.last - set.first + 1) * 192 * 2;
    expect(lines.neighbours).toHaveLength(lines.offsets.length);
    for (let i = 0; i < ringEnds; i += 97) {
      const [x = 0, y = 0] = ends[i] ?? [];
      const [nx, ny, nz] = next(i);
      // Out from the barycentre, in the plane, a step long.
      expect(Math.hypot(nx, ny) / set.step).toBeCloseTo(1, 6);
      expect(nz).toBe(0);
      expect((nx * y - ny * x) / (set.step * Math.hypot(x, y))).toBeCloseTo(0, 6);
    }
    for (let i = ringEnds; i < ends.length; i += 1) expect(next(i)).toEqual([0, 0, 0]);
    expect(FADE).toEqual({ gone: 2, full: 6 });
  });

  it("stay as they are while the view turns, and change as the eye moves, zooms past a step or changes axes", () => {
    const key = (v: View, toFrame: Mat3 = IDENTITY, axes: "galactic" | "equatorial" = "galactic") => ringSet(v, toFrame, axes, 800)?.key;
    const at = view([0, 0, 0], 100);
    expect(key({ ...at, yaw: 2 })).toBe(key(at));
    expect(key({ ...at, focus: [400, 0, 0] })).not.toBe(key(at));
    expect(key({ ...at, distance: 30 })).not.toBe(key(at));
    expect(key(at, tilted(10), "equatorial")).not.toBe(key(at));
    // A small move near a far focus keeps the arcs.
    expect(key(view([5000, 3000, 0], 2))).toBe(key(view([5000.01, 3000, 0], 2)));
  });
});
