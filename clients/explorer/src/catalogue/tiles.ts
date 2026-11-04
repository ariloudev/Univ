// Fetching, checking and decoding one tile (draw-the-catalogues), and the scale domains and
// manifests of a release.

import { decodeTile, type TileManifest } from "univ-tile-decoder";
import { z } from "zod";

import { type ReadOptions, read, readJson } from "../api/client.ts";
import type { Vec3 } from "../scene/vec.ts";
import type { Tile } from "./lod.ts";

/** A domain of the release, its manifest, its unit in megaparsecs and its tiles. */
export interface Domain {
  name: string;
  manifest: TileManifest;
  toMpc: number;
  tiles: Tile[];
}

/** The points of one tile, ready to draw: offsets from its centre in megaparsecs. */
export interface Batch {
  tile: Tile;
  centre: Vec3;
  offsets: Float32Array;
  /** Each point's index in the manifest's kinds of value. */
  kinds: Uint8Array;
  /** Each point's index in the manifest's sources. */
  sources: Uint16Array;
  ids: Float64Array;
}

/** A tile that did not match its manifest's SHA-256, or did not decode. */
export class TileFailed extends Error {}

const DomainList = z.object({ domains: z.array(z.object({ name: z.string() })) });

/** The size of a manifest's unit in megaparsecs. */
export function megaparsecsPer(unit: string): number {
  const sizes: Record<string, number> = { pc: 1e-6, parsec: 1e-6, kpc: 1e-3, kiloparsec: 1e-3, Mpc: 1, megaparsec: 1 };
  const size = sizes[unit];
  if (size === undefined) throw new Error(`a tile manifest in ${unit}, which the explorer cannot place`);
  return size;
}

/** The tiles of `manifest`'s domain, in megaparsecs. */
export function tilesOf(name: string, manifest: TileManifest, toMpc: number): Tile[] {
  return manifest.nodes.map((node) => ({
    id: `${name}/${node.address}`,
    domain: name,
    address: node.address,
    parent: node.address.length > 1 ? `${name}/${node.address.slice(0, -1)}` : undefined,
    centre: [0, 1, 2].map((i) => ((node.bounds.min[i] ?? 0) + (node.bounds.max[i] ?? 0)) * 0.5 * toMpc) as unknown as Vec3,
    edge: ((node.bounds.max[0] ?? 0) - (node.bounds.min[0] ?? 0)) * toMpc,
    points: node.points,
  }));
}

/** The scale domains of `release` with their manifests. */
export async function domainsOf(release: number, options: ReadOptions = {}): Promise<Domain[]> {
  const list = await readJson("/v1/scale-domains", DomainList, { ...options, release });
  return Promise.all(
    list.domains.map(async ({ name }) => {
      const response = await read(`/v1/scale-domains/${encodeURIComponent(name)}/manifest`, { ...options, release });
      const manifest = (await response.json()) as TileManifest;
      const toMpc = megaparsecsPer(manifest.unit);
      return { name, manifest, toMpc, tiles: tilesOf(name, manifest, toMpc) };
    }),
  );
}

async function sha256(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** The points of `bytes`, the tile of `tile` in `domain`, checked against the manifest. */
export async function batchOf(domain: Domain, tile: Tile, bytes: Uint8Array<ArrayBuffer>): Promise<Batch> {
  const node = domain.manifest.nodes.find((n) => n.address === tile.address);
  if (node === undefined) throw new TileFailed(`${tile.id} is not in its manifest`);
  if ((await sha256(bytes)) !== node.sha256) throw new TileFailed(`${tile.id} does not match its SHA-256`);
  let points;
  try {
    points = decodeTile(bytes, node, domain.manifest);
  } catch (error) {
    throw new TileFailed(`${tile.id}: ${error instanceof Error ? error.message : String(error)}`);
  }
  const offsets = new Float32Array(points.length * 3);
  const kinds = new Uint8Array(points.length);
  const sources = new Uint16Array(points.length);
  const ids = new Float64Array(points.length);
  const kindIndex = new Map(domain.manifest.encoding.kinds.map((k, i) => [k, i]));
  points.forEach((p, i) => {
    offsets[i * 3] = p.x * domain.toMpc - tile.centre[0];
    offsets[i * 3 + 1] = p.y * domain.toMpc - tile.centre[1];
    offsets[i * 3 + 2] = p.z * domain.toMpc - tile.centre[2];
    kinds[i] = kindIndex.get(p.kind) ?? 0;
    sources[i] = p.source.index;
    ids[i] = p.id;
  });
  return { tile, centre: tile.centre, offsets, kinds, sources, ids };
}

/** Fetches, checks and decodes the tile of `tile` in `domain` of `release`. */
export async function fetchBatch(
  domain: Domain,
  tile: Tile,
  release: number,
  options: ReadOptions = {},
): Promise<Batch> {
  const path = `/v1/scale-domains/${encodeURIComponent(domain.name)}/tiles/${encodeURIComponent(tile.address)}`;
  const response = await read(path, { ...options, release });
  return batchOf(domain, tile, new Uint8Array(await response.arrayBuffer()));
}
