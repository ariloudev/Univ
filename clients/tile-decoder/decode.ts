// The reference TypeScript decoder of Univ Server's tiles (decode-the-tile-format).
//
// A tile holds the points of one octree node of a scale domain. The domain's tile manifest
// gives the node's bounds, the quantization step of its level and the source table its
// points index; this module turns the tile's bytes into points in the manifest's frame and
// unit. Univ Server's tile format route describes the same layout as data.
//
// Encoding version 1, every number little-endian, nothing aligned or padded; a tile of
// n points is 48 + 17 n bytes (version 2 adds two columns after these, below):
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
// Version 2 has the version field 2, and a tile of n points is 48 + 20 n bytes:
//
//   48+17n  n i16    each point's colour index in hundredths of a magnitude; -32768 for none
//   48+19n  n u8     its colour bands: 0 for none, else 1 + an index into encoding.colour_bands
//
// A colour index decodes to its value over 100, within 0.005 magnitudes of the value it was
// built from. A point with -32768 and a code other than 0, or the other way round, is refused.
//
// Version 3 has the version field 3 and the same 48 + 20 n bytes. Its last two columns hold a
// point's value and the code of what it is: 1 + an index into encoding.colour_bands for a
// colour index, as in version 2, or encoding.log_age_code (3) for the base-10 logarithm of an
// age in years, in hundredths, within 0.005 of the value it was built from. The age code is
// refused in a version 2 tile.
//
// Version 4 has the version field 4, and a tile of n points is 48 + 21 n bytes: version 3's
// columns, then
//
//   48+20n  n u8     each point's kind of object: 0 for none, else 1 + an index into
//                    encoding.object_kinds
//
// Only the Galaxy tracers' tiles take version 4; a code beyond object_kinds is refused.
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
  /** The colour bands of version 2, coded by their index plus one; absent before version 2. */
  colour_bands?: ColourBands[];
  /** The code of a point whose value is the base-10 logarithm of its age in years; absent
   * before version 3. */
  log_age_code?: number;
  /** The kinds of object of version 4, coded by their index plus one; absent before version 4. */
  object_kinds?: ObjectKind[];
}

/** What a Galaxy tracer is, as its source names it. */
export type ObjectKind =
  | 'galaxy_group'
  | 'open_cluster'
  | 'globular_cluster'
  | 'classical_cepheid'
  | 'rr_lyrae_star'
  | 'star_forming_region'
  | 'luminous_hot_star';

/** The two bands a colour index lies between. */
export type ColourBands = 'BP-RP' | 'B-V';

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
  /** Its colour index in magnitudes and the bands it lies between; none in a version 1 tile. */
  colour?: { index: number; bands: ColourBands } | undefined;
  /** The base-10 logarithm of its age in years, where it carries one in place of a colour
   * index; none before version 3. */
  logAge?: number | undefined;
  /** Its object's kind, from version 4; none for one body or in an older tile. */
  objectKind?: ObjectKind | undefined;
}

const MAGIC = [0x55, 0x56, 0x54, 0x4c]; // "UVTL"
const HEADER_BYTES = 48;
/** The bytes a point of version 1 takes; version 2's columns follow version 1's. */
const V1_POINT_BYTES = 17;
/** The bytes of a version 2 or 3 point: version 1's, then a value and its code. */
const V3_POINT_BYTES = 20;
/** The bytes a point takes, by encoding version: the versions this decoder knows. */
const POINT_BYTES: Readonly<Record<number, number>> = {
  1: V1_POINT_BYTES,
  2: V3_POINT_BYTES,
  3: V3_POINT_BYTES,
  4: V3_POINT_BYTES + 1,
};

/** The encoding versions this decoder knows. */
export const VERSIONS: readonly number[] = Object.keys(POINT_BYTES).map(Number);
/** The value a version 2 or 3 point holds when it has none. */
const NO_COLOUR = -32_768;
/** The code of a version 3 point whose value is the base-10 logarithm of its age in years. */
const LOG_AGE_CODE = 3;
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
 * 2^53 - 1, or a source, kind of value or, from version 4, kind of object the manifest does not
 * list.
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
  const pointBytes =
    POINT_BYTES[version] ??
    fail(`the tile encoding version ${version} is not one this decoder knows (it knows ${VERSIONS.join(', ')})`);
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
  const expected = HEADER_BYTES + pointBytes * n;
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
  // Version 2's columns follow version 1's.
  const colourColumn = HEADER_BYTES + V1_POINT_BYTES * n;
  const bandsColumn = colourColumn + 2 * n;
  const bandNames = manifest.encoding.colour_bands ?? [];
  // Version 3 codes a log age apart from the colour bands, as LOG_AGE_CODE, which its manifest
  // repeats: one that names another code is refused, so that no colour is read as an age.
  const ageCode = version >= 3 ? LOG_AGE_CODE : undefined;
  if (ageCode !== undefined && manifest.encoding.log_age_code !== ageCode) {
    fail(`the manifest gives the log age code ${manifest.encoding.log_age_code}, not the ${ageCode} of version ${version}`);
  }
  // Version 4's kind of object follows version 3's columns.
  const objectColumn = HEADER_BYTES + V3_POINT_BYTES * n;
  const objectKinds = manifest.encoding.object_kinds ?? [];
  if (version >= 4 && objectKinds.length === 0) {
    fail(`the manifest of a version ${version} tile lists no kinds of object`);
  }
  const points: DecodedPoint[] = new Array(n);
  for (let k = 0; k < n; k++) {
    const at = HEADER_BYTES + 6 * k;
    const coordinate = (i: 0 | 1 | 2) => min[i] + (view.getUint16(at + 2 * i, true) + 0.5) * step;

    const low = view.getUint32(ids + 8 * k, true);
    const high = view.getUint32(ids + 8 * k + 4, true);
    if (high >= ID_HIGH_LIMIT || (high === 0 && low === 0)) {
      fail(`point ${k} has the object id ${(BigInt(high) << 32n) | BigInt(low)}, outside 1 to 2^53 - 1`);
    }
    const index = view.getUint16(sourceColumn + 2 * k, true);
    const source = sources.get(index) ?? fail(`point ${k} names source ${index}, which the manifest does not list`);
    const code = view.getUint8(kindColumn + k);
    const kind = kinds[code] ?? fail(`point ${k} has the kind of value ${code}, which the manifest does not list`);
    let colour: DecodedPoint['colour'];
    let logAge: DecodedPoint['logAge'];
    if (version >= 2) {
      const held = view.getInt16(colourColumn + 2 * k, true);
      // Within the tile: its length was checked against its count above.
      const band = bytes[bandsColumn + k] ?? 0;
      if ((held === NO_COLOUR) !== (band === 0)) {
        fail(`point ${k} has the value code ${band}, which its value contradicts`);
      }
      if (band === ageCode) {
        logAge = held / 100;
      } else if (band !== 0) {
        const bands =
          bandNames[band - 1] ?? fail(`point ${k} has the value code ${band}, which the manifest does not list`);
        colour = { index: held / 100, bands };
      }
    }
    let objectKind: DecodedPoint['objectKind'];
    if (version >= 4) {
      const object = bytes[objectColumn + k] ?? 0;
      if (object !== 0) {
        objectKind =
          objectKinds[object - 1] ??
          fail(`point ${k} has the kind of object code ${object}, which the manifest does not list`);
      }
    }
    // Every point has the same properties, colour, age and kind of object among them, so they
    // share one shape.
    points[k] = {
      x: coordinate(0),
      y: coordinate(1),
      z: coordinate(2),
      id: high * TWO_32 + low,
      source,
      kind,
      colour,
      logAge,
      objectKind,
    };
  }
  return points;
}
