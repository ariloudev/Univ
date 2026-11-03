// The reference TypeScript decoder of Univ Server's tiles (decode-the-tile-format).
//
// A tile holds the points of one octree node of a scale domain. The domain's tile manifest
// gives the node's bounds, the quantization step of its level and the source table its
// points index; this module turns the tile's bytes into points in the manifest's frame and
// unit. Univ Server's tile format route describes the same layout as data.
//
// Encoding version 1, every number little-endian, nothing aligned or padded; a tile of
// n points is 48 + 17 n bytes:
//
//   0       4 bytes  magic "UVTL"
//   4       u16      encoding version, 1
//   6       u16      reserved, 0
//   8       u32      point count n
//   12      u32      reserved, 0
//   16      3 f64    the node's minimum corner x, y, z
//   40      f64      the node's edge
//   48      3n u16   each point's quantized x, y, z
//   48+6n   n u64    each point's object id, 1 to 2^53 - 1
//   48+14n  n u16    each point's source: an index into the manifest's sources
//   48+16n  n u8     each point's kind of value: an index into encoding.kinds
//
// A coordinate decodes to the centre of its step: min + (q + 0.5) * step, with step the
// node's edge over encoding.steps_per_edge (65,536), the step its level states. It lies
// within half a step of the position the tile was built from.
//
// Only erasable TypeScript syntax is used, so Node runs this file as it is.

/** Whether a value was measured, estimated, inferred by analysis or reconstructed by a model. */
export type KindOfValue = 'measured' | 'estimated' | 'inferred' | 'reconstructed';

export type Vec3 = [number, number, number];

/** A source whose points, or whose points' sky positions, the tiles hold. */
export interface ManifestSource {
  index: number;
  id: string;
  role: 'distance' | 'position' | 'distance_and_position';
  title: string;
  citation: string;
  /** The credit the source requires, quoted from its terms. */
  credit: string;
}

/** One octree node and its tile. */
export interface ManifestNode {
  address: string;
  level: number;
  bounds: { min: Vec3; max: Vec3 };
  points: number;
  sampled_from: number;
  bytes: number;
  sha256: string;
}

export interface ManifestLevel {
  level: number;
  edge: number;
  step: number;
  fraction: number;
}

export interface ManifestEncoding {
  version: number;
  header_bytes: number;
  point_bytes: number;
  steps_per_edge: number;
  max_tile_bytes: number;
  point_source: string;
  kinds: KindOfValue[];
}

/** A scale domain's tile manifest: the parts a decoder reads, among others it ignores. */
export interface TileManifest {
  format: string;
  manifest_version: number;
  release: string;
  scale_domain: string;
  frame: { name: string; origin: string; axes: string; epoch: string };
  unit: string;
  encoding: ManifestEncoding;
  levels: ManifestLevel[];
  sources: ManifestSource[];
  nodes: ManifestNode[];
}

/** A decoded point, at the centre of its quantization step. */
export interface DecodedPoint {
  /** The position in the manifest's frame (ICRS equatorial Cartesian, barycentric) and unit. */
  x: number;
  y: number;
  z: number;
  /** The object id, exact: below 2^53. */
  id: number;
  /** The source of the point's preferred distance, from the manifest's source table. */
  source: ManifestSource;
  /** The kind of value of that distance. */
  kind: KindOfValue;
}

/** The encoding versions this decoder knows. */
export const VERSIONS: readonly number[] = [1];

const MAGIC = [0x55, 0x56, 0x54, 0x4c]; // "UVTL"
const HEADER_BYTES = 48;
const POINT_BYTES = 17;
const MAX_TILE_BYTES = 1_048_576;
const STEPS = 65_536;
const TWO_32 = 4_294_967_296;
/** The high 32 bits of an object id are below this: ids are below 2^53. */
const ID_HIGH_LIMIT = 0x20_0000;

const fail = (message: string): never => {
  throw new Error(`tile decoder: ${message}`);
};

/**
 * Decodes the tile of `node`, one of `manifest`'s nodes, into its points in the order stored.
 *
 * Throws an `Error` on another magic, an encoding version it does not know (naming it, read
 * before the size), a reserved field that is not zero, a size other than the tile's count or
 * the node's byte size gives, a header whose node is not `node`, an object id outside 1 to
 * 2^53 - 1, or a source or kind of value the manifest does not list.
 */
export function decodeTile(
  bytes: Uint8Array,
  node: ManifestNode,
  manifest: TileManifest,
): DecodedPoint[] {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.length < 6) {
    fail(`the tile is ${bytes.length} bytes, too short for its magic and version`);
  }
  if (MAGIC.some((b, i) => bytes[i] !== b)) {
    fail(`the tile starts with ${[...bytes.subarray(0, 4)].join(' ')}, not the magic UVTL`);
  }
  const version = view.getUint16(4, true);
  if (!VERSIONS.includes(version)) {
    fail(
      `the tile encoding version ${version} is not one this decoder knows (it knows ${VERSIONS.join(', ')})`,
    );
  }
  if (manifest.encoding.version !== version) {
    fail(`the tile is of encoding version ${version}, the manifest of ${manifest.encoding.version}`);
  }
  if (bytes.length > MAX_TILE_BYTES) {
    fail(`the tile is ${bytes.length} bytes, beyond the ${MAX_TILE_BYTES} a tile may hold`);
  }
  if (bytes.length < HEADER_BYTES) {
    fail(`the tile is ${bytes.length} bytes, short of its ${HEADER_BYTES}-byte header`);
  }
  if (view.getUint16(6, true) !== 0 || view.getUint32(12, true) !== 0) {
    fail('a reserved field of the tile header is not zero');
  }
  const n = view.getUint32(8, true);
  const expected = HEADER_BYTES + POINT_BYTES * n;
  if (bytes.length !== expected) {
    fail(`the tile is ${bytes.length} bytes, not the ${expected} its count of ${n} points gives`);
  }
  if (n !== node.points || bytes.length !== node.bytes) {
    fail(
      `node ${node.address} lists ${node.points} points in ${node.bytes} bytes, the tile holds ${n} in ${bytes.length}`,
    );
  }

  const min: Vec3 = [view.getFloat64(16, true), view.getFloat64(24, true), view.getFloat64(32, true)];
  const edge = view.getFloat64(40, true);
  const step = edge / STEPS;
  const level = manifest.levels.find((l) => l.level === node.level);
  const sameNode =
    min.every((c, i) => c === node.bounds.min[i] && c + edge === node.bounds.max[i]) &&
    level !== undefined &&
    level.step === step &&
    manifest.encoding.steps_per_edge === STEPS;
  if (!sameNode) {
    fail(
      `the tile's node, corner ${min.join(', ')} and edge ${edge}, is not node ${node.address} of the manifest`,
    );
  }

  const sources = new Map(manifest.sources.map((s) => [s.index, s]));
  const kinds = manifest.encoding.kinds;
  const ids = HEADER_BYTES + 6 * n;
  const sourceColumn = HEADER_BYTES + 14 * n;
  const kindColumn = HEADER_BYTES + 16 * n;
  const points: DecodedPoint[] = new Array(n);
  for (let k = 0; k < n; k++) {
    const at = HEADER_BYTES + 6 * k;
    const coordinate = (i: number) => min[i] + (view.getUint16(at + 2 * i, true) + 0.5) * step;

    const low = view.getUint32(ids + 8 * k, true);
    const high = view.getUint32(ids + 8 * k + 4, true);
    if (high >= ID_HIGH_LIMIT || (high === 0 && low === 0)) {
      fail(`point ${k} has the object id ${(BigInt(high) << 32n) | BigInt(low)}, outside 1 to 2^53 - 1`);
    }
    const index = view.getUint16(sourceColumn + 2 * k, true);
    const source = sources.get(index) ?? fail(`point ${k} names source ${index}, which the manifest does not list`);
    const code = bytes[kindColumn + k];
    const kind = kinds[code] ?? fail(`point ${k} has the kind of value ${code}, which the manifest does not list`);
    points[k] = {
      x: coordinate(0),
      y: coordinate(1),
      z: coordinate(2),
      id: high * TWO_32 + low,
      source,
      kind,
    };
  }
  return points;
}
