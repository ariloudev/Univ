// @vitest-environment node
import { describe, expect, it } from "vitest";

import { answer, fakeHttp } from "../test/http.ts";
import { DUST_GRID, dustMapOf } from "./dust.ts";

/** A header of a 4-cell cube of 10 pc, as Univ Ingest publishes the dust map's. */
const HEADER = {
  format: "univ-volume-grid",
  id: DUST_GRID,
  release: 13,
  source: { id: DUST_GRID, title: "Edenhofer et al. 2024 3D dust map", citation: "Edenhofer G. et al. 2024, A&A 685, A82", credit: "CC BY 4.0" },
  frame: "galactic",
  unit: "parsec",
  axes: ["x", "y", "z"],
  shape: [4, 4, 4],
  origin: [-15, -15, -15],
  cell: 10,
  fields: [{ column: "MEAN", field: "density", unit: "extinction_zgr23_per_parsec", kind: "reconstructed" }],
  resampling: "…",
  no_data: "NaN: …",
};

/** Little-endian 32-bit floats, each its index. */
function samples(n: number): Response {
  const view = new DataView(new ArrayBuffer(n * 4));
  for (let i = 0; i < n; i += 1) view.setFloat32(i * 4, i, true);
  return new Response(view.buffer);
}

describe("the dust map", () => {
  it("is fetched once a release, its header and its mean density, and kept", async () => {
    const http = fakeHttp(answer(200, HEADER), samples(64));
    const map = await dustMapOf(13, { http });
    expect(http.urls).toEqual([`/v1/grids/${DUST_GRID}?release=13`, `/v1/grids/${DUST_GRID}/fields/0?release=13`]);
    expect(map.volume.n).toBe(2);
    expect(map.unit).toBe("extinction_zgr23_per_parsec");
    expect(map.kind).toBe("reconstructed");
    expect(map.title).toBe(HEADER.source.title);
    expect(await dustMapOf(13, { http })).toBe(map);
    expect(http.urls).toHaveLength(2);
  });

  it("fails on a grid it cannot draw, and is fetched again when next asked for", async () => {
    const http = fakeHttp(answer(200, { ...HEADER, frame: "supergalactic" }), answer(200, HEADER), samples(7));
    await expect(dustMapOf(14, { http })).rejects.toThrow();
    await expect(dustMapOf(14, { http })).rejects.toThrow(/not a cube/);
    expect(http.urls).toHaveLength(3);
  });
});
