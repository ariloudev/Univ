// Three-vectors and 3×3 rotations in double precision, row-major as the tile manifests give
// them: `apply(m, v)` is the matrix times the column vector.

export type Vec3 = readonly [number, number, number];
export type Mat3 = readonly [Vec3, Vec3, Vec3];

export const ORIGIN: Vec3 = [0, 0, 0];
export const IDENTITY: Mat3 = [
  [1, 0, 0],
  [0, 1, 0],
  [0, 0, 1],
];

export const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const scale = (a: Vec3, s: number): Vec3 => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a: Vec3, b: Vec3): number => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const norm = (a: Vec3): number => Math.hypot(a[0], a[1], a[2]);

export function cross(a: Vec3, b: Vec3): Vec3 {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}

export function normalize(a: Vec3): Vec3 {
  const n = norm(a);
  return n === 0 ? a : scale(a, 1 / n);
}

/** `m` times the column vector `v`. */
export function apply(m: Mat3, v: Vec3): Vec3 {
  return [dot(m[0], v), dot(m[1], v), dot(m[2], v)];
}

/** The transpose of `m`: the inverse of a rotation. */
export function transpose(m: Mat3): Mat3 {
  return [
    [m[0][0], m[1][0], m[2][0]],
    [m[0][1], m[1][1], m[2][1]],
    [m[0][2], m[1][2], m[2][2]],
  ];
}

/** The vector whose image under the rotation `m` is `v`: `m`'s axes back to the ICRS. */
export const unapply = (m: Mat3, v: Vec3): Vec3 => apply(transpose(m), v);

/** `a` times `b`. */
export function multiply(a: Mat3, b: Mat3): Mat3 {
  const t = transpose(b);
  return [apply(t, a[0]), apply(t, a[1]), apply(t, a[2])];
}
