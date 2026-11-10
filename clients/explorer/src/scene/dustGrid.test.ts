// @vitest-environment node
import { describe, expect, it } from "vitest";

import { type DustVolume, STAR_STEPS, coarsened, columnTo, densityAt, dimmedBy, dustFrame, textureOf } from "./dustGrid.ts";
import { type Mat3, type Vec3, apply, sub } from "./vec.ts";

/** A rotation about `axis` (0, 1 or 2) by `angle` radians. */
function rotation(axis: number, angle: number): Mat3 {
  const [c, s] = [Math.cos(angle), Math.sin(angle)];
  const [i, j] = [(axis + 1) % 3, (axis + 2) % 3];
  const m = [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ].map((row) => [...row]);
  for (const [r, cs] of [
    [i, [c, -s]],
    [j, [s, c]],
  ] as const) {
    const row = m[r] ?? [];
    row[i] = cs[0];
    row[j] = cs[1];
  }
  return m as unknown as Mat3;
}

const close = (a: readonly number[], b: readonly number[]) => a.forEach((v, i) => expect(v).toBeCloseTo(b[i] ?? Number.NaN, 9));

describe("the dust map's grid", () => {
  it("averages blocks of 2 × 2 × 2 cells, X fastest, a cell of no data counting as no dust", () => {
    // 4 cells a side of 10 pc, first centred at −15 pc: each sample its X index, but one NaN.
    const n = 4;
    const samples = new Float32Array(n * n * n).map((_, i) => Math.floor(i / (n * n)));
    samples[0] = Number.NaN;
    const volume = coarsened(samples, { shape: [n, n, n], cell: 10, origin: [-15, -15, -15] });
    expect([volume.n, volume.corner, volume.edge]).toEqual([2, -20, 40]);
    // Blocks along X (fastest) hold X indices 0–1 and 2–3: means 0.5 and 2.5.
    expect(volume.densities[1]).toBeCloseTo(2.5);
    expect(volume.densities[2]).toBeCloseTo(0.5);
    // The first block holds the NaN, counted as 0 among its eight: (0 + 0·3 + 1·4) / 8.
    expect(volume.densities[0]).toBeCloseTo(0.5);
    expect(volume.densities.every((d) => Number.isFinite(d))).toBe(true);
  });

  it("refuses samples that do not fill a cube", () => {
    expect(() => coarsened(new Float32Array(7), { shape: [2, 2, 2], cell: 10, origin: [-5, -5, -5] })).toThrow(/not a cube/);
  });

  it("lies at the galactic position of each place in the view, however the eye is turned", () => {
    const volume: DustVolume = { n: 125, corner: -1250, edge: 2500, densities: new Float32Array(125 ** 3) };
    const toGalactic = rotation(2, 1.1);
    const axes = rotation(0, 0.4);
    const eye: Vec3 = [2e-4, -1e-4, 3e-4];
    const star: Vec3 = [-5e-4, 7e-4, 1e-4];
    const frame = dustFrame(volume, eye, axes, toGalactic);
    const place = apply(axes, sub(star, eye));
    const galactic = apply(toGalactic, star).map((c) => c * 1e6);
    close(textureOf(frame, place), galactic.map((c) => (c + 1250) / 2500));
    // The Sun at the cube's centre.
    close(dustFrame(volume, [0, 0, 0], axes, toGalactic).eye, [0.5, 0.5, 0.5]);
  });

  it("dims a star behind a cloud by the dust between it and the eye, and not one in front of it", () => {
    // A cloud of four cells of 20 pc, galactic X 110–190 pc about the Sun, 0.01 per parsec.
    const volume: DustVolume = { n: 125, corner: -1250, edge: 2500, densities: new Float32Array(125 ** 3) };
    for (let k = 0; k < 125; k += 1)
      for (let j = 0; j < 125; j += 1)
        for (let i = 0; i < 125; i += 1) {
          const x = -1250 + (i + 0.5) * 20;
          if (x > 100 && x < 200) volume.densities[(k * 125 + j) * 125 + i] = 0.01;
        }
    expect(densityAt(volume, [(150 + 1250) / 2500, 0.5, 0.5])).toBeCloseTo(0.01, 9);
    const frame = dustFrame(volume, [0, 0, 0], rotation(2, 0), rotation(2, 0));
    const along = (pc: number): Vec3 => [pc * 1e-6, 0, 0];
    expect(columnTo(volume, frame, along(80))).toBe(0);
    expect(dimmedBy(columnTo(volume, frame, along(80)))).toBe(1);
    // 80 pc of 0.01 per pc, to within a step of the integral (400 pc in its steps).
    const behind = columnTo(volume, frame, along(400));
    const step = (400 / STAR_STEPS) * 0.01;
    expect(Math.abs(behind - 0.8)).toBeLessThanOrEqual(step + 1e-9);
    expect(dimmedBy(behind)).toBeLessThan(0.2);
    // Within the cloud, part of it.
    expect(columnTo(volume, frame, along(150))).toBeGreaterThan(0);
    expect(columnTo(volume, frame, along(150))).toBeLessThan(behind);
    // Away from the cloud, along Y, nothing.
    expect(columnTo(volume, frame, [0, 400e-6, 0])).toBe(0);
  });
});
