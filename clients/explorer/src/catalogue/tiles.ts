// Fetching, checking and decoding one tile (draw-the-catalogues), and the scale domains and
// manifests of a release.

import { type ManifestNode, decodeTile } from "univ-tile-decoder";

import { type ReadOptions, read, readJson } from "../api/client.ts";
import { DomainList, ExplorerManifest } from "../api/schemas.ts";
import { KIND_INDEX } from "../scene/kinds.ts";
import { add, scale } from "../scene/vec.ts";
import { type Extent, type Tile, extentOf } from "./lod.ts";

/** A tile of a domain, with the manifest's node for it. */
export interface DomainTile extends Tile {
  node: ManifestNode;
}

/** A domain of the release, its manifest, its unit in megaparsecs and its tiles. */
export interface Domain {
  name: string;
  manifest: ExplorerManifest;
  toMpc: number;
  tiles: DomainTile[];
}

/** The points of one tile, ready to draw: offsets from its centre in megaparsecs. */
export interface Batch {
  tile: DomainTile;
  offsets: Float32Array;
  /** Each point's kind of value, as its index in `KINDS`. */
  kinds: Uint8Array;
  /** Each point's index in the manifest's sources. */
  sources: Uint16Array;
  ids: Float64Array;
  /** Where the points lie. */
  extent: Extent;
}

/** A tile that did not match its manifest's SHA-256, or did not decode. */
export class TileFailed extends Error {}

/** The size of each unit a manifest may be in, in megaparsecs. */
export const MPC_PER: Record<ExplorerManifest["unit"], number> = { pc: 1e-6, Mpc: 1 };
// The manifests name their units by symbol, the API by name: the two agree on their sizes.

/** The tiles of `manifest`'s domain `name`, in megaparsecs. */
export function tilesOf(name: string, manifest: ExplorerManifest): DomainTile[] {
  const toMpc = MPC_PER[manifest.unit];
  return manifest.nodes.map((node) => {
    const { min, max } = node.bounds;
    return {
      id: `${name}/${node.address}`,
      domain: name,
      address: node.address,
      parent: node.address.length > 1 ? `${name}/${node.address.slice(0, -1)}` : undefined,
      centre: scale(add(min, max), 0.5 * toMpc),
      edge: (max[0] - min[0]) * toMpc,
      points: node.points,
      node,
    };
  });
}

/** The scale domains of `release` with their manifests. */
export async function domainsOf(release: number, options: ReadOptions = {}): Promise<Domain[]> {
  const list = await readJson("/v1/scale-domains", DomainList, { ...options, release });
  return Promise.all(
    list.domains.map(async ({ name }) => {
      const path = `/v1/scale-domains/${encodeURIComponent(name)}/manifest`;
      const manifest = await readJson(path, ExplorerManifest, { ...options, release });
      return { name, manifest, toMpc: MPC_PER[manifest.unit], tiles: tilesOf(name, manifest) };
    }),
  );
}

async function sha256(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function decoded(domain: Domain, tile: DomainTile, bytes: Uint8Array) {
  try {
    return decodeTile(bytes, tile.node, domain.manifest);
  } catch (error) {
    throw new TileFailed(`${tile.id}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

/** The points of `bytes`, the tile of `tile` in `domain`, checked against the manifest. */
export async function batchOf(domain: Domain, tile: DomainTile, bytes: Uint8Array<ArrayBuffer>): Promise<Batch> {
  if ((await sha256(bytes)) !== tile.node.sha256) throw new TileFailed(`${tile.id} does not match its SHA-256`);
  const points = decoded(domain, tile, bytes);
  const offsets = new Float32Array(points.length * 3);
  const kinds = new Uint8Array(points.length);
  const sources = new Uint16Array(points.length);
  const ids = new Float64Array(points.length);
  const [cx, cy, cz] = tile.centre;
  points.forEach((p, i) => {
    offsets[i * 3] = p.x * domain.toMpc - cx;
    offsets[i * 3 + 1] = p.y * domain.toMpc - cy;
    offsets[i * 3 + 2] = p.z * domain.toMpc - cz;
    kinds[i] = KIND_INDEX[p.kind];
    sources[i] = p.source.index;
    ids[i] = p.id;
  });
  return { tile, offsets, kinds, sources, ids, extent: extentOf(offsets, tile.centre) };
}

/** Fetches, checks and decodes the tile of `tile` in `domain` of `release`. */
export async function fetchBatch(
  domain: Domain,
  tile: DomainTile,
  release: number,
  options: ReadOptions = {},
): Promise<Batch> {
  const path = `/v1/scale-domains/${encodeURIComponent(domain.name)}/tiles/${encodeURIComponent(tile.address)}`;
  const response = await read(path, { ...options, release });
  return batchOf(domain, tile, new Uint8Array(await response.arrayBuffer()));
}
