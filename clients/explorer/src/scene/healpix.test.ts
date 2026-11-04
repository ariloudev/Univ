import { describe, expect, it } from "vitest";

import { boundary, centreOf, corners } from "./healpix.ts";
import type { Vec3 } from "./vec.ts";

/** The unit vector at longitude `lon` and latitude `lat`, radians. */
const at = (lon: number, lat: number): Vec3 => [Math.cos(lat) * Math.cos(lon), Math.cos(lat) * Math.sin(lon), Math.sin(lat)];

const near = (a: Vec3, b: Vec3) => {
  for (let i = 0; i < 3; i += 1) expect(a[i]).toBeCloseTo(b[i] ?? 0, 9);
};

// Longitudes and latitudes, radians, as cdshealpix 0.10.1 gives them (nested::center and
// nested::vertices), the crate Univ Ingest makes sky regions with.
const REFERENCE: [number, number, [number, number], [number, number][]][] = [
  [0, 0, [0.7853981634, 0.7297276562], [[0.7853981634, 0], [1.5707963268, 0.7297276562], [0.7853981634, 1.5707963268], [0, 0.7297276562]]],
  [0, 4, [0, 0], [[0, -0.7297276562], [0.7853981634, 0], [0, 0.7297276562], [5.4977871438, 0]]],
  [0, 11, [5.4977871438, -0.7297276562], [[5.4977871438, -1.5707963268], [0, -0.7297276562], [5.4977871438, 0], [4.7123889804, -0.7297276562]]],
  [1, 0, [0.7853981634, 0.3398369095], [[0.7853981634, 0], [1.1780972451, 0.3398369095], [0.7853981634, 0.7297276562], [0.3926990817, 0.3398369095]]],
  [1, 1, [1.1780972451, 0.7297276562], [[1.1780972451, 0.3398369095], [1.5707963268, 0.7297276562], [1.5707963268, 1.1596584645], [0.7853981634, 0.7297276562]]],
  [1, 29, [5.1050880621, 0], [[5.1050880621, -0.3398369095], [5.4977871438, 0], [5.1050880621, 0.3398369095], [4.7123889804, 0]]],
  [6, 12345, [5.4855152975, 0.1253278312], [[5.4855152975, 0.1148355603], [5.4977871438, 0.1253278312], [5.4855152975, 0.1358339908], [5.4732434512, 0.1253278312]]],
];

describe("a HEALPix cell", () => {
  it.each(REFERENCE)("of order %i, cell %i, has the centre and corners cdshealpix gives it", (order, cell, centre, vertices) => {
    near(centreOf(order, cell), at(...centre));
    corners(order, cell).forEach((corner, i) => {
      const [lon = 0, lat = 0] = vertices[i] ?? [];
      near(corner, at(lon, lat));
    });
  });
});

describe("the boundary of cells", () => {
  it("of one cell is its four edges, each from corner to corner", () => {
    const edges = boundary(1, [0], 3);
    expect(edges).toHaveLength(4);
    const [s, e] = corners(1, 0);
    near(edges[0]?.[0] ?? [0, 0, 0], s);
    near(edges[0]?.[3] ?? [0, 0, 0], e);
  });

  it("of two neighbours is six edges, the one they share inside the region", () => {
    // Cells 0 and 1 of order 1 share an edge within base cell 0.
    expect(boundary(1, [0, 1])).toHaveLength(6);
    // Base cells 0 and 4 share an edge across the faces of the projection.
    expect(boundary(0, [0, 4])).toHaveLength(6);
  });

  it("of every cell of the sphere is nothing", () => {
    expect(boundary(0, Array.from({ length: 12 }, (_, i) => i))).toEqual([]);
    expect(boundary(2, Array.from({ length: 192 }, (_, i) => i))).toEqual([]);
  });
});
