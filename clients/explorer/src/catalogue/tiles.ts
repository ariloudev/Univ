// Fetching, checking and decoding one tile (draw-the-catalogues), and the scale domains and
// manifests of a release.

import { type DecodedPoint, type ManifestNode, decodeModelTile, decodeTile } from "univ-tile-decoder";

import { type ReadOptions, read, readJson } from "../api/client.ts";
import { type GalaxyModel, type ModelPaper, galaxyModel } from "../api/galaxyModel.ts";
import { DomainList, ExplorerManifest } from "../api/schemas.ts";
import { KIND_INDEX } from "../scene/kinds.ts";
import { COVERED_BY_STARS, COVERED_BY_TRACERS, MODEL_DOMAIN, MODEL_VERSION, NOT_COVERED } from "../scene/model.ts";
import { starBytes } from "../scene/starColour.ts";
import { TRACERS_DOMAIN, objectIndex, tracerBytes } from "../scene/tracers.ts";
import { add, scale } from "../scene/vec.ts";
import { type Extent, type Tile, extentOf, levelOfAddress } from "./lod.ts";
import { shuffled } from "./shuffle.ts";

/** A tile of a domain, with the manifest's node for it. */
export interface DomainTile extends Tile {
  node: ManifestNode;
}

/** A domain of the release, its manifest, its unit in megaparsecs and its tiles; for the Galaxy
 * model's, how its populations look. */
export interface Domain {
  name: string;
  manifest: ExplorerManifest;
  toMpc: number;
  tiles: DomainTile[];
  model?: ModelLook | undefined;
}

/** How the Galaxy model's points look, by their population's code: each population's display
 * colour as RGBA bytes, and what covers a covered point of it; and the papers its definition
 * cites, which the credits list. */
export interface ModelLook {
  colours: Uint8Array;
  cover: Uint8Array;
  /** Each population, in the definition's order, for the key. */
  populations: readonly { code: number; name: string; srgb: readonly [number, number, number] }[];
  papers: readonly ModelPaper[];
}

/** How `model`'s points look: each population in its display colour, opaque, its light given by
 * the drawing; covered by young tracers for a population of the arms, by stars for any other. */
export function lookOf(model: GalaxyModel): ModelLook {
  const colours = new Uint8Array(256 * 4);
  const cover = new Uint8Array(256);
  for (const p of model.definition.populations) {
    colours.set([...p.colour.srgb, 255], p.code * 4);
    cover[p.code] = p.density.form === "arms" ? COVERED_BY_TRACERS : COVERED_BY_STARS;
  }
  const populations = model.definition.populations.map((p) => ({ code: p.code, name: p.name, srgb: p.colour.srgb }));
  return { colours, cover, populations, papers: model.definition.papers };
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
  /** Each point's colour as red, green, blue and opacity bytes: a star's, or a tracer's kind of
   * object's; none for the other domains, whose points are drawn by their kind of value. */
  colours?: Uint8Array | undefined;
  /** Each point's object index (tracers.ts's `objectIndex`), 0 for none, for a tile of the
   * Galaxy tracers domain; none for any other tile. */
  objects?: Uint8Array | undefined;
  /** Each point's base-10 logarithm of its age in years, NaN where it carries none, for a tile
   * of the Galaxy tracers domain with any age; none for any other tile. */
  ages?: Float32Array | undefined;
  /** For a tile of the Galaxy model, what covers each model point (model.ts's `COVERED_BY_…`,
   * or `NOT_COVERED`): its points are drawn as light, and are no object to name or choose. */
  covered?: Uint8Array | undefined;
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

/** The scale domains of `release` with their manifests; the Galaxy model's with how its points
 * look, and left out where the release holds no definition for it. */
export async function domainsOf(release: number, options: ReadOptions = {}): Promise<Domain[]> {
  const list = await readJson("/v1/scale-domains", DomainList, { ...options, release });
  const model = list.domains.some((d) => d.name === MODEL_DOMAIN) ? await galaxyModel(release, options) : undefined;
  const names = list.domains.map((d) => d.name).filter((name) => name !== MODEL_DOMAIN || model !== undefined);
  return Promise.all(
    names.map(async (name) => {
      const path = `/v1/scale-domains/${encodeURIComponent(name)}/manifest`;
      const manifest = await readJson(path, ExplorerManifest, { ...options, release });
      const look = name === MODEL_DOMAIN && model !== undefined ? lookOf(model) : undefined;
      return { name, manifest, toMpc: MPC_PER[manifest.unit], tiles: tilesOf(name, manifest), model: look };
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

/** The offsets of `points`, in `domain`'s unit, from the centre of `tile`, in megaparsecs. */
function offsetsOf(points: readonly { x: number; y: number; z: number }[], domain: Domain, tile: DomainTile): Float32Array {
  const offsets = new Float32Array(points.length * 3);
  const [cx, cy, cz] = tile.centre;
  points.forEach((p, i) => {
    offsets[i * 3] = p.x * domain.toMpc - cx;
    offsets[i * 3 + 1] = p.y * domain.toMpc - cy;
    offsets[i * 3 + 2] = p.z * domain.toMpc - cz;
  });
  return offsets;
}

/** The model points of `bytes`, a version 5 tile of the Galaxy model, looking as `look` gives:
 * shuffled as any tile's, each in its population's colour and with what covers it; of no kind,
 * source or id. */
function modelBatchOf(domain: Domain, tile: DomainTile, bytes: Uint8Array, look: ModelLook): Batch {
  let decoded;
  try {
    decoded = decodeModelTile(bytes, tile.node, domain.manifest);
  } catch (error) {
    throw new TileFailed(`${tile.id}: ${error instanceof Error ? error.message : String(error)}`);
  }
  const points = shuffled(decoded, tile.id);
  const offsets = offsetsOf(points, domain, tile);
  const colours = new Uint8Array(points.length * 4);
  const covered = new Uint8Array(points.length);
  points.forEach((p, i) => {
    const code = p.population.code;
    colours.set(look.colours.subarray(code * 4, code * 4 + 4), i * 4);
    covered[i] = p.covered ? (look.cover[code] ?? NOT_COVERED) : NOT_COVERED;
  });
  const n = points.length;
  const none = { kinds: new Uint8Array(n), sources: new Uint16Array(n), ids: new Float64Array(n) };
  return { tile, offsets, ...none, colours, covered, extent: extentOf(offsets, tile.centre) };
}

/** The points of `bytes`, the tile of `tile` in `domain`, checked against the manifest. */
export async function batchOf(domain: Domain, tile: DomainTile, bytes: Uint8Array<ArrayBuffer>): Promise<Batch> {
  if ((await sha256(bytes)) !== tile.node.sha256) throw new TileFailed(`${tile.id} does not match its SHA-256`);
  if (domain.manifest.encoding.version === MODEL_VERSION && domain.model !== undefined) return modelBatchOf(domain, tile, bytes, domain.model);
  // Shuffled by the tile's id, so that its first points, however many, are an even sample.
  const points = shuffled(decoded(domain, tile, bytes), tile.id);
  const offsets = offsetsOf(points, domain, tile);
  const kinds = new Uint8Array(points.length);
  const sources = new Uint16Array(points.length);
  const ids = new Float64Array(points.length);
  points.forEach((p, i) => {
    kinds[i] = KIND_INDEX[p.kind];
    sources[i] = p.source.index;
    ids[i] = p.id;
  });
  // A stars tile from before colour indices were kept lists no bands: its stars keep their
  // kinds' colours rather than all turning grey.
  const names = domain.manifest.encoding.colour_bands;
  const stars = domain.name === STARS_DOMAIN && names !== undefined ? starColoursOf(points, names) : undefined;
  const tracers = domain.name === TRACERS_DOMAIN;
  const objects = tracers ? Uint8Array.from(points, (p) => objectIndex(p.objectKind)) : undefined;
  const colours = objects === undefined ? stars?.bytes : tracerBytes(objects);
  // A tracers tile with no age carries none, so that no age choice acts on it.
  const ages = tracers && points.some((p) => p.logAge !== undefined) ? Float32Array.from(points, (p) => p.logAge ?? Number.NaN) : undefined;
  return { tile, offsets, kinds, sources, ids, stars, colours, objects, ages, extent: extentOf(offsets, tile.centre) };
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
