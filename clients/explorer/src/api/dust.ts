// The dust map's grid as Univ Server serves it (draw-the-dust-map, fetch-a-volume-grid): its
// header, checked for what the explorer draws, and its mean density, read once a release and
// kept.

import { z } from "zod";

import { type DustVolume, coarsened } from "../scene/dustGrid.ts";
import { type ReadOptions, cacheOf, kept, read, readJson } from "./client.ts";

/** The dust map's grid id, its source's. */
export const DUST_GRID = "edenhofer-2024-dust";

const Header = z.object({
  id: z.string(),
  source: z.object({ title: z.string(), citation: z.string(), credit: z.string() }),
  frame: z.literal("galactic"),
  unit: z.literal("parsec"),
  axes: z.tuple([z.literal("x"), z.literal("y"), z.literal("z")]),
  shape: z.tuple([z.number().int().positive(), z.number().int().positive(), z.number().int().positive()]),
  origin: z.tuple([z.number(), z.number(), z.number()]),
  cell: z.number().positive(),
  fields: z.array(z.object({ field: z.string(), unit: z.string(), kind: z.string() })).min(1),
});

/** The dust map ready to draw, with what its key says of it. */
export interface DustMap {
  volume: DustVolume;
  title: string;
  citation: string;
  credit: string;
  /** The unit of its density, as the header names it. */
  unit: string;
  /** The kind of value of its density: reconstructed. */
  kind: string;
}

const maps = cacheOf<number, DustMap>();

/** The dust map of `release`, fetched the first time it is asked for and kept. */
export const dustMapOf = (release: number, options: ReadOptions = {}): Promise<DustMap> =>
  kept(maps, release, () => fetchDustMap(release, options));

async function fetchDustMap(release: number, options: ReadOptions): Promise<DustMap> {
  const at = { ...options, release };
  const header = await readJson(`/v1/grids/${DUST_GRID}`, Header, at);
  const [field] = header.fields;
  if (field === undefined) throw new Error("its header names no field");
  const body = await (await read(`/v1/grids/${DUST_GRID}/fields/0`, at)).arrayBuffer();
  const view = new DataView(body);
  const samples = new Float32Array(body.byteLength / 4);
  for (let i = 0; i < samples.length; i += 1) samples[i] = view.getFloat32(i * 4, true);
  const volume = coarsened(samples, header);
  const { title, citation, credit } = header.source;
  return { volume, title, citation, credit, unit: field.unit, kind: field.kind };
}
