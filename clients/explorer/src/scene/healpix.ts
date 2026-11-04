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

/** A cell's edges, from corner to corner across it, south-east, north-east, north-west, south-west. */
const EDGES: readonly (readonly [number, number, number, number])[] = [
  [0, 0, 1, 0],
  [1, 0, 1, 1],
  [1, 1, 0, 1],
  [0, 1, 0, 0],
];

/** A point as a key that two cells meeting there agree on, to well within a cell. */
const keyOf = (v: Vec3) => v.map((c) => Math.round(c * 1e9)).join(",");

/** The edges of `cells` of `order` that no two of them share, which outline the region they
 * cover; each a line of `steps` pieces along the cell's own edge. */
export function boundary(order: number, cells: readonly number[], steps = 3): Vec3[][] {
  const edges = new Map<string, Vec3[] | undefined>();
  for (const cell of cells) {
    for (const [u0, v0, u1, v1] of EDGES) {
      const ends = [keyOf(across(order, cell, u0, v0)), keyOf(across(order, cell, u1, v1))].sort().join("|");
      // An edge met twice lies between two cells of the region, inside it.
      if (edges.has(ends)) {
        edges.set(ends, undefined);
        continue;
      }
      const line = Array.from({ length: steps + 1 }, (_, i) => across(order, cell, u0 + ((u1 - u0) * i) / steps, v0 + ((v1 - v0) * i) / steps));
      edges.set(ends, line);
    }
  }
  return [...edges.values()].filter((line) => line !== undefined);
}
