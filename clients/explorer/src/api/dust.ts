// The dust map's grid as Univ Server serves it (draw-the-dust-map, fetch-a-volume-grid): its
// header, checked for what the explorer draws, and its mean density, read once a release and
// kept.

import { z } from "zod";

import { type DustVolume, coarsened } from "../scene/dustGrid.ts";
import { type ReadOptions, cacheOf, kept, read, readJson } from "./client.ts";

/** The dust map's grid id, its source's. */
export const DUST_GRID = "edenhofer-2024-dust";

const Field = z.object({ field: z.string(), unit: z.string(), kind: z.string() });

const Header = z.object({
  source: z.object({ title: z.string(), citation: z.string(), credit: z.string() }),
  frame: z.literal("galactic"),
  unit: z.literal("parsec"),
  axes: z.tuple([z.literal("x"), z.literal("y"), z.literal("z")]),
  shape: z.tuple([z.number().int().positive(), z.number().int().positive(), z.number().int().positive()]),
  origin: z.tuple([z.number(), z.number(), z.number()]),
  cell: z.number().positive(),
  fields: z.tuple([Field]).rest(Field),
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

/** The dust map of `release`, fetched the first time it is asked for and kept while it is the
 * release drawn. */
export const dustMapOf = (release: number, options: ReadOptions = {}): Promise<DustMap> =>
  kept(maps, release, () => fetchDustMap(release, options), 1);

async function fetchDustMap(release: number, options: ReadOptions): Promise<DustMap> {
  const at = { ...options, release };
  const header = await readJson(`/v1/grids/${DUST_GRID}`, Header, at);
  const [field] = header.fields;
  const body = await (await read(`/v1/grids/${DUST_GRID}/fields/0`, at)).arrayBuffer();
  if (body.byteLength % 4 !== 0) throw new Error(`${body.byteLength} bytes are not 32-bit floats`);
  // The samples are little-endian, as every browser's floats are.
  const volume = coarsened(new Float32Array(body), header);
  const { title, citation, credit } = header.source;
  return { volume, title, citation, credit, unit: field.unit, kind: field.kind };
}
