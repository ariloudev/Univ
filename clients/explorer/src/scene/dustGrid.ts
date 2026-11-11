// The dust map's grid made ready to draw (draw-the-dust-map): the mean dust density of the
// Edenhofer et al. 2024 map, as Univ Server serves it, averaged into cells twice as wide so that
// it fits a 3D texture of a few megabytes, laid out X fastest as WebGL takes it, and placed in
// the view by the release's galactic rotation. A cell that marks no data (NaN) counts as no dust,
// so it adds nothing to the volume.

import { type Mat3, type Vec3, add, apply, multiply, norm, scale, sub, transpose } from "./vec.ts";

/** Parsecs in a megaparsec, the view's unit. */
const PC_PER_MPC = 1e6;

/** The grid as the explorer draws it: `n` cells a side of a cube from `corner` (galactic
 * parsecs, each axis) `edge` parsecs wide, the densities X fastest. */
export interface DustVolume {
  n: number;
  corner: number;
  edge: number;
  /** Mean dust density per cell, in the grid's unit, X fastest, then Y, then Z. */
  densities: Float32Array;
}

/** A cubic grid as its header gives it: cells along X, Y and Z (array order X slowest, Z
 * fastest), the cell in parsecs and the centre of its first cell. */
export interface GridShape {
  shape: readonly [number, number, number];
  cell: number;
  origin: readonly [number, number, number];
}

/** The side of the cube `samples` fills, checked: an even number of cells, centred alike on
 * each axis. */
function sideOf(samples: Float32Array, { shape, origin }: GridShape): number {
  const [side, ny, nz] = shape;
  if (side !== ny || ny !== nz || samples.length !== side ** 3) throw new Error(`not a cube of ${side ** 3} samples: ${samples.length}`);
  if (side % 2 !== 0) throw new Error(`a cube of ${side} cells a side, which does not halve`);
  if (origin[0] !== origin[1] || origin[1] !== origin[2]) throw new Error("not a cube centred alike on each axis");
  return side;
}

/** `samples`, a cubic grid of `shape` with an even side, averaged over blocks of 2 × 2 × 2
 * cells, no data as no dust, and laid out X fastest. */
export function coarsened(samples: Float32Array, grid: GridShape): DustVolume {
  const side = sideOf(samples, grid);
  const n = side / 2;
  const densities = new Float32Array(n * n * n);
  // Sample i is cell (x, y, z), Z fastest; its block's place has X fastest.
  samples.forEach((value, i) => {
    if (Number.isNaN(value)) return;
    const at = ((i % side) >> 1) * n * n + ((Math.floor(i / side) % side) >> 1) * n + (Math.floor(i / side / side) >> 1);
    densities[at] = (densities[at] ?? 0) + value / 8;
  });
  return { n, corner: grid.origin[0] - grid.cell / 2, edge: side * grid.cell, densities };
}

/** The density at the cell holding the texture coordinate `t` (each in 0–1), 0 outside. */
export function densityAt(volume: DustVolume, t: Vec3): number {
  const [i, j, k] = t.map((c) => Math.floor(c * volume.n));
  const inside = [i, j, k].every((c) => c !== undefined && c >= 0 && c < volume.n);
  return inside ? (volume.densities[((k ?? 0) * volume.n + (j ?? 0)) * volume.n + (i ?? 0)] ?? 0) : 0;
}

/** How the view's eye-relative positions map to the volume's texture coordinates: the eye's
 * coordinate, and the matrix that turns a position in the eye's axes (megaparsecs) into a step
 * in them. `axes` turns the ICRS into the eye's axes, `toGalactic` the ICRS into galactic. */
export interface DustFrame {
  eye: Vec3;
  axes: Mat3;
}

/** The dust frame of an eye at `eye` (ICRS megaparsecs) looking along `axes`. */
export function dustFrame(volume: DustVolume, eye: Vec3, axes: Mat3, toGalactic: Mat3): DustFrame {
  const k = PC_PER_MPC / volume.edge;
  const corner: Vec3 = [volume.corner, volume.corner, volume.corner];
  const [x, y, z] = multiply(toGalactic, transpose(axes));
  return {
    eye: scale(sub(scale(apply(toGalactic, eye), PC_PER_MPC), corner), 1 / volume.edge),
    axes: [scale(x, k), scale(y, k), scale(z, k)],
  };
}

/** The texture coordinate of `place`, a position in the eye's axes relative to the eye. */
export const textureOf = (frame: DustFrame, place: Vec3): Vec3 => add(frame.eye, apply(frame.axes, place));

/** The most steps of the integral from the eye to a star, as the point shader takes them: one a
 * cell the segment crosses, up to this. */
export const STAR_STEPS = 32;

/** The steps of an integral over `fraction` of the segment `d` (texture coordinates) through a
 * grid of `n` cells a side: about one a cell crossed, at least one, at most `most`. */
const stepsOf = (fraction: number, d: Vec3, n: number, most: number): number =>
  Math.min(most, Math.max(1, Math.ceil(fraction * norm(d) * n)));

/** The part of the segment from the eye to `place` inside the grid, as fractions of it; none
 * where it misses the grid. */
function clipped(frame: DustFrame, place: Vec3): [number, number] | undefined {
  const d = apply(frame.axes, place);
  let [t0, t1] = [0, 1];
  for (let k = 0; k < 3; k += 1) {
    const [o, v] = [frame.eye[k] ?? 0, d[k] ?? 0];
    const step = Math.abs(v) < 1e-9 ? 1e-9 : v;
    const [a, b] = [-o / step, (1 - o) / step];
    t0 = Math.max(t0, Math.min(a, b));
    t1 = Math.min(t1, Math.max(a, b));
  }
  return t1 > t0 ? [t0, t1] : undefined;
}

/** The dust column from the eye to `place` (eye axes, megaparsecs), in the grid's unit times
 * parsecs: the point shader's integral on the CPU, over the part of the segment inside the grid,
 * each step sampling the cell it falls in. */
export function columnTo(volume: DustVolume, frame: DustFrame, place: Vec3): number {
  const inside = clipped(frame, place);
  if (inside === undefined) return 0;
  const [t0, t1] = inside;
  const steps = stepsOf(t1 - t0, apply(frame.axes, place), volume.n, STAR_STEPS);
  let column = 0;
  for (let s = 0; s < steps; s += 1) {
    column += densityAt(volume, textureOf(frame, scale(place, t0 + ((t1 - t0) * (s + 0.5)) / steps)));
  }
  return (column * (t1 - t0) * norm(place) * PC_PER_MPC) / steps;
}

/** How strongly a dust column dims the stars behind it, for display: a star is drawn
 * `exp(-DIMMING × column)` as bright, the column in the map's extinction per parsec times parsecs.
 * Chosen to show where the dust lies, not a photometric extinction. */
export const DIMMING = 2.5;

/** How bright a star is drawn behind a dust `column`. */
export const dimmedBy = (column: number): number => Math.exp(-DIMMING * column);
