// Fetching, checking and decoding one tile (draw-the-catalogues), and the scale domains and
// manifests of a release.

import { type DecodedPoint, type ManifestNode, decodeTile } from "univ-tile-decoder";

import { type ReadOptions, read, readJson } from "../api/client.ts";
import { DomainList, ExplorerManifest } from "../api/schemas.ts";
import { KIND_INDEX } from "../scene/kinds.ts";
import { starBytes } from "../scene/starColour.ts";
import { TRACERS_DOMAIN, tracerBytes } from "../scene/tracers.ts";
import { add, scale } from "../scene/vec.ts";
import { type Extent, type Tile, extentOf, levelOfAddress } from "./lod.ts";
import { shuffled } from "./shuffle.ts";

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
  /** The stars' colours, for a tile of the stars domain. */
  stars?: StarColours | undefined;
  /** Each point's colour as red, green, blue and opacity bytes: a star's, or a tracer's source's;
   * none for the other domains, whose points are drawn by their kind of value. */
  colours?: Uint8Array | undefined;
  ids: Float64Array;
  /** Where the points lie. */
  extent: Extent;
}

/** The scale domain whose points are drawn in their stars' colours. */
const STARS_DOMAIN = "stars";

/** The colours of a tile's stars. */
export interface StarColours {
  /** Each star's colour as red, green, blue and opacity bytes: from its colour index, or grey,
   * half as opaque where its preferred distance is inferred. */
  bytes: Uint8Array;
  /** Each star's colour index in hundredths of a magnitude, as the tile holds it. */
  hundredths: Int16Array;
  /** Each star's colour bands, by the manifest's code: their place in its `colour_bands` plus
   * one, 0 for a star without a colour index. */
  bands: Uint8Array;
  /** How many have a colour index. */
  coloured: number;
}

/** The colours of the stars `points`, of a tile whose manifest names its colour bands `names`. */
function starColoursOf(points: readonly DecodedPoint[], names: readonly string[]): StarColours {
  const stars: StarColours = {
    bytes: new Uint8Array(points.length * 4),
    hundredths: new Int16Array(points.length),
    bands: new Uint8Array(points.length),
    coloured: 0,
  };
  points.forEach((p, i) => {
    // A colour index in bands the manifest does not name is not read: the star is drawn grey.
    const band = p.colour === undefined ? 0 : names.indexOf(p.colour.bands) + 1;
    stars.bytes.set(starBytes(band === 0 ? undefined : p.colour), i * 4);
    if (p.kind === "inferred") stars.bytes[i * 4 + 3] = Math.round((stars.bytes[i * 4 + 3] ?? 0) / 2);
    if (band === 0 || p.colour === undefined) return;
    stars.hundredths[i] = Math.round(p.colour.index * 100);
    stars.bands[i] = band;
    stars.coloured += 1;
  });
  return stars;
}

/** A tile that did not match its manifest's SHA-256, or did not decode. */
export class TileFailed extends Error {}

/** The size of each unit a manifest may be in, in megaparsecs. */
export const MPC_PER: Record<ExplorerManifest["unit"], number> = { pc: 1e-6, Mpc: 1 };
// The manifests name their units by symbol, the API by name: the two agree on their sizes.

/** The tiles of `manifest`'s domain `name`, in megaparsecs. */
export function tilesOf(name: string, manifest: ExplorerManifest): DomainTile[] {
  const toMpc = MPC_PER[manifest.unit];
  const held = cumulativeOf(manifest);
  const split = new Set(manifest.nodes.map((node) => node.address.slice(0, -1)));
  return manifest.nodes.map((node) => {
    const { min, max } = node.bounds;
    const level = levelOfAddress(node.address);
    const above = level === 0 ? 0 : (held[level - 1] ?? 1);
    return {
      id: `${name}/${node.address}`,
      domain: name,
      address: node.address,
      parent: node.address.length > 1 ? `${name}/${node.address.slice(0, -1)}` : undefined,
      centre: scale(add(min, max), 0.5 * toMpc),
      edge: (max[0] - min[0]) * toMpc,
      points: node.points,
      slice: [above, split.has(node.address) ? (held[level] ?? 1) : 1] as const,
      node,
    };
  });
}

/** The share of a domain's points the levels down to each hold, by level: each split tile of a
 * level keeps the level's fraction of the points that reach it, the rest going down. */
function cumulativeOf(manifest: ExplorerManifest): number[] {
  const fractions = new Map(manifest.levels.map((l) => [l.level, l.fraction]));
  let depth = 0;
  for (const node of manifest.nodes) depth = Math.max(depth, levelOfAddress(node.address) + 1);
  const held: number[] = [];
  let left = 1;
  for (let level = 0; level < depth; level += 1) {
    left *= 1 - Math.min(1, Math.max(0, fractions.get(level) ?? 1));
    held.push(1 - left);
  }
  return held;
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
  // Shuffled by the tile's id, so that its first points, however many, are an even sample.
  const points = shuffled(decoded(domain, tile, bytes), tile.id);
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
  // A stars tile from before colour indices were kept lists no bands: its stars keep their
  // kinds' colours rather than all turning grey.
  const names = domain.manifest.encoding.colour_bands;
  const stars = domain.name === STARS_DOMAIN && names !== undefined ? starColoursOf(points, names) : undefined;
  const colours = domain.name === TRACERS_DOMAIN ? tracerBytes(sources) : stars?.bytes;
  return { tile, offsets, kinds, sources, ids, stars, colours, extent: extentOf(offsets, tile.centre) };
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
