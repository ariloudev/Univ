// HEALPix cells of the nested scheme (Górski et al. 2005, ApJ 622, 759), as Univ Ingest makes
// sky regions with the CDS cdshealpix crate: a cell's corners on the unit sphere, and the
// boundary of a set of cells, which outlines the region they cover (see-the-structures).

import type { Vec3 } from "./vec.ts";

/** Each base cell's ring of its southern corner, and its longitude in eighths of a turn. */
const JRLL = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
const JPLL = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

/** The unit vector at `(x, y)` ∈ [0, 1]² on base cell `face`, x toward its eastern corner and
 * y toward its western, by the HEALPix projection. */
function onFace(face: number, x: number, y: number): Vec3 {
  const jr = (JRLL[face] ?? 0) - x - y;
  let nr = 1;
  let z = ((2 - jr) * 2) / 3;
  if (jr < 1) {
    nr = jr;
    z = 1 - (nr * nr) / 3;
  } else if (jr > 3) {
    nr = 4 - jr;
    z = (nr * nr) / 3 - 1;
  }
  const t = ((((JPLL[face] ?? 0) * nr + x - y) % 8) + 8) % 8;
  const phi = nr < 1e-15 ? 0 : ((Math.PI / 4) * t) / nr;
  const s = Math.sqrt(Math.max(0, 1 - z * z));
  return [s * Math.cos(phi), s * Math.sin(phi), z];
}

/** Where nested cell `cell` of `order` lies: its base cell, and its column and row there. */
function located(order: number, cell: number): { face: number; ix: number; iy: number } {
  const per = 4 ** order;
  let rest = cell % per;
  let ix = 0;
  let iy = 0;
  // The nested index interleaves the column's bits (even) with the row's (odd).
  for (let bit = 0; rest > 0; bit += 1) {
    ix += (rest % 2) * 2 ** bit;
    iy += (Math.floor(rest / 2) % 2) * 2 ** bit;
    rest = Math.floor(rest / 4);
  }
  return { face: Math.floor(cell / per), ix, iy };
}

/** The point at `(u, v)` ∈ [0, 1]² across cell `cell` of `order`. */
function across(order: number, cell: number, u: number, v: number): Vec3 {
  const { face, ix, iy } = located(order, cell);
  const side = 2 ** order;
  return onFace(face, (ix + u) / side, (iy + v) / side);
}

/** The corners of cell `cell` of `order`: south, east, north and west, as cdshealpix gives them. */
export function corners(order: number, cell: number): [Vec3, Vec3, Vec3, Vec3] {
  return [across(order, cell, 0, 0), across(order, cell, 1, 0), across(order, cell, 1, 1), across(order, cell, 0, 1)];
}

/** The centre of cell `cell` of `order`. */
export const centreOf = (order: number, cell: number): Vec3 => across(order, cell, 0.5, 0.5);

/** A cell's corners across it, south, east, north and west; its edges run from each to the next. */
const CORNERS: readonly (readonly [number, number])[] = [
  [0, 0],
  [1, 0],
  [1, 1],
  [0, 1],
];

/** A point as a key that two cells meeting there agree on, to well within a cell. */
const keyOf = (v: Vec3) => v.map((c) => Math.round(c * 1e9)).join(",");

/** The key of the edge between the points keyed `a` and `b`, either way round. */
const edgeKey = (a: string, b: string) => (a < b ? `${a}|${b}` : `${b}|${a}`);

/** The edges of `cells` of `order` that no two of them share, which outline the region they
 * cover; each a line of `steps` pieces along the cell's own edge. */
export function boundary(order: number, cells: readonly number[], steps = 3): Vec3[][] {
  const side = 2 ** order;
  const placed = cells.map((cell) => {
    const { face, ix, iy } = located(order, cell);
    const point = (u: number, v: number) => onFace(face, (ix + u) / side, (iy + v) / side);
    const keys = CORNERS.map(([u, v]) => keyOf(point(u, v)));
    const edges = keys.map((key, i) => edgeKey(key, keys[(i + 1) % 4] ?? key));
    return { point, edges };
  });
  // An edge met twice lies between two cells of the region, inside it.
  const met = new Map<string, number>();
  for (const { edges } of placed) for (const edge of edges) met.set(edge, (met.get(edge) ?? 0) + 1);
  return placed.flatMap(({ point, edges }) =>
    edges.flatMap((edge, i) => {
      if (met.get(edge) !== 1) return [];
      const [u0, v0] = CORNERS[i] ?? [0, 0];
      const [u1, v1] = CORNERS[(i + 1) % 4] ?? [0, 0];
      return [Array.from({ length: steps + 1 }, (_, j) => point(u0 + ((u1 - u0) * j) / steps, v0 + ((v1 - v0) * j) / steps))];
    }),
  );
}
