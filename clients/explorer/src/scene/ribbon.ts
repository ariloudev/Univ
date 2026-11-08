// Lines as ribbons: each segment two triangles, widened across the view in the vertex shader,
// since WebGL draws lines one pixel wide wherever it is asked for more (see-the-structures:
// the chosen structure's outline is thicker).

/** A segment's six corners, two triangles: which end each is at, 0 or 1, and on which side. */
const CORNERS = [0, -1, 0, 1, 1, 1, 1, 1, 0, 1, 1, -1];

/** The ribbon of the segments `offsets` holds, end to end in pairs: for each of their six
 * corners, the segment's ends and the corner's place on it. */
export interface Ribbon {
  from: Float32Array;
  to: Float32Array;
  corners: Float32Array;
  /** Where the segments' ends have neighbours (drawing.ts, `Lines`), each corner's end's. */
  neighbours: Float32Array | undefined;
  count: number;
}

/** The ribbon of the segments whose ends `offsets` holds in pairs, three numbers each, with each
 * end's neighbour from `neighbours` where there are any. */
export function ribbonOf(offsets: Float32Array, neighbours?: Float32Array): Ribbon {
  const segments = Math.floor(offsets.length / 6);
  const count = segments * 6;
  const from = new Float32Array(count * 3);
  const to = new Float32Array(count * 3);
  const corners = new Float32Array(count * 2);
  const near = neighbours === undefined ? undefined : new Float32Array(count * 3);
  for (let s = 0; s < segments; s += 1) {
    const a = offsets.subarray(s * 6, s * 6 + 3);
    const b = offsets.subarray(s * 6 + 3, s * 6 + 6);
    for (let c = 0; c < 6; c += 1) {
      const v = s * 6 + c;
      from.set(a, v * 3);
      to.set(b, v * 3);
      corners[v * 2] = CORNERS[c * 2] ?? 0;
      corners[v * 2 + 1] = CORNERS[c * 2 + 1] ?? 0;
      const end = (s * 2 + (CORNERS[c * 2] ?? 0)) * 3;
      near?.set(neighbours?.subarray(end, end + 3) ?? [], v * 3);
    }
  }
  return { from, to, corners, neighbours: near, count };
}
