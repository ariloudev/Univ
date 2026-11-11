// The reference decoder against tiles the Rust encoder wrote (fixtures/), each point's
// position before quantization, object id, source and kind of value beside it.
// univ-ingest's test tile_decoder_fixtures writes and checks the fixtures.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

import { VERSIONS, decodeTile } from './decode.ts';
import type { ColourBands, KindOfValue, ManifestNode, TileManifest, Vec3 } from './decode.ts';

interface Expected {
  id: number;
  position: Vec3;
  source: string;
  kind: KindOfValue;
  /** The colour index before rounding and its bands; null where the point has none. */
  colour?: [number, ColourBands] | null;
  /** The base-10 logarithm of its age in years before rounding, where it carries one. */
  log_age?: number;
}

interface FixtureTile {
  address: string;
  hex: string;
  points: Expected[];
}

interface Fixture {
  name: string;
  manifest: TileManifest;
  tiles: FixtureTile[];
}

/** The fixtures of a directory: the current encoding's in fixtures/, versions 1's and 2's kept
 * in v1/ and v2/. */
const fixturesIn = (path: string): Fixture[] => {
  const dir = new URL(path, import.meta.url);
  const read = (file: string): unknown => JSON.parse(readFileSync(new URL(file, dir), 'utf8'));
  return readdirSync(dir)
    .filter((f) => f.endsWith('.manifest.json'))
    .sort()
    .map((f) => {
      const name = f.slice(0, -'.manifest.json'.length);
      const tiles = read(`${name}.tiles.json`) as { tiles: FixtureTile[] };
      return { name, manifest: read(f) as TileManifest, tiles: tiles.tiles };
    });
};

const fixtures = fixturesIn('./fixtures/');
const version1 = fixturesIn('./fixtures/v1/');
const version2 = fixturesIn('./fixtures/v2/');

const bytesOf = (hex: string): Uint8Array => Uint8Array.from(Buffer.from(hex, 'hex'));

const nodeOf = (manifest: TileManifest, address: string): ManifestNode => {
  const node = manifest.nodes.find((n) => n.address === address);
  assert.ok(node, `the manifest lists node ${address}`);
  return node;
};

/** Every tile of `fixtures`, with its manifest and node, each named with `prefix`. */
const tilesOf = (of: Fixture[], prefix = '') =>
  of.flatMap((f) =>
    f.tiles.map((t) => ({ fixture: `${prefix}${f.name}`, manifest: f.manifest, node: nodeOf(f.manifest, t.address), tile: t })),
  );

/** Every tile of every fixture of the current encoding. */
const everyTile = tilesOf(fixtures);

test('the fixtures hold every domain of the fixture release, a simulated one, and every node', () => {
  assert.deepEqual(
    fixtures.map((f) => f.name),
    // The fixture release holds no tracer source, so its Galaxy tracers domain has no node.
    ['cosmic_web', 'galaxy_tracers', 'nearby_galaxies', 'simulated', 'stars'],
  );
  for (const f of fixtures) {
    assert.deepEqual(
      f.tiles.map((t) => t.address),
      f.manifest.nodes.map((n) => n.address),
      `${f.name}: a tile for each node`,
    );
  }
  const kinds = new Set(everyTile.flatMap((t) => t.tile.points.map((p) => p.kind)));
  assert.deepEqual([...kinds].sort(), ['estimated', 'inferred', 'measured', 'reconstructed']);
});

test('AC-282: each decoded position lies within the stated quantization step of the original', () => {
  let checked = 0;
  for (const { fixture, manifest, node, tile } of everyTile) {
    const level = manifest.levels.find((l) => l.level === node.level);
    assert.ok(level, `${fixture}: level ${node.level} is listed`);
    const decoded = decodeTile(bytesOf(tile.hex), node, manifest);
    assert.equal(decoded.length, tile.points.length);
    decoded.forEach((p, k) => {
      const original = tile.points[k].position;
      [p.x, p.y, p.z].forEach((c, i) => {
        const off = Math.abs(c - original[i]);
        assert.ok(off <= level.step, `${fixture} ${tile.address} point ${k} axis ${i}: ${off} beyond the step ${level.step}`);
        // The encoding promises half a step: each coordinate decodes to the centre of its step.
        assert.ok(off <= (level.step / 2) * (1 + 1e-9), `${fixture} ${tile.address} point ${k}: beyond half a step`);
      });
      checked++;
    });
  }
  assert.ok(checked >= 70, `${checked} points checked`);
});

test("AC-283: each decoded point gives its object id, its source and its kind of value", () => {
  for (const { fixture, manifest, node, tile } of everyTile) {
    const decoded = decodeTile(bytesOf(tile.hex), node, manifest);
    decoded.forEach((p, k) => {
      const expected = tile.points[k];
      assert.equal(p.id, expected.id, `${fixture} ${tile.address} point ${k}: id`);
      assert.equal(p.source.id, expected.source, `${fixture} ${tile.address} point ${k}: source`);
      assert.ok(p.source.credit.length > 0, 'the source carries its credit');
      assert.equal(p.kind, expected.kind, `${fixture} ${tile.address} point ${k}: kind of value`);
    });
  }
});

test('the decoder decodes tiles of encoding versions 1, 2 and 3', () => {
  assert.deepEqual(VERSIONS, [1, 2, 3]);
  assert.ok(fixtures.every((f) => f.manifest.encoding.version === 3));
  assert.deepEqual(version1.map((f) => [f.name, f.manifest.encoding.version]), [['simulated', 1]]);
  assert.deepEqual(version2.map((f) => [f.name, f.manifest.encoding.version]), [['simulated', 2]]);
  let checked = 0;
  for (const f of version1) {
    for (const tile of f.tiles) {
      const decoded = decodeTile(bytesOf(tile.hex), nodeOf(f.manifest, tile.address), f.manifest);
      decoded.forEach((p, k) => {
        assert.equal(p.id, tile.points[k].id);
        assert.equal(p.colour, undefined, 'a version 1 point has no colour');
        checked += 1;
      });
    }
  }
  assert.ok(checked > 0);
});

/** Every tile of the version 2 fixtures, kept as they were written. */
const everyVersion2Tile = tilesOf(version2, 'v2/');

test('each decoded point of a version 2 or 3 tile gives its colour index and bands, or that it has none', () => {
  let coloured = 0;
  let none = 0;
  for (const { fixture, manifest, node, tile } of [...everyTile, ...everyVersion2Tile]) {
    decodeTile(bytesOf(tile.hex), node, manifest).forEach((p, k) => {
      const expected = tile.points[k].colour ?? null;
      const at = `${fixture} ${tile.address} point ${k}`;
      if (expected === null) {
        assert.equal(p.colour, undefined, `${at}: no colour`);
        none += 1;
      } else {
        assert.ok(p.colour, `${at}: a colour`);
        assert.ok(Math.abs(p.colour.index - expected[0]) <= 0.005 + 1e-12, `${at}: within 0.005 mag`);
        assert.equal(p.colour.bands, expected[1], `${at}: bands`);
        coloured += 1;
      }
    });
  }
  assert.ok(coloured > 0 && none > 0, `${coloured} coloured, ${none} not`);
  const bands = new Set(everyTile.flatMap((t) => t.tile.points.flatMap((p) => (p.colour ? [p.colour[1]] : []))));
  assert.deepEqual([...bands].sort(), ['B-V', 'BP-RP']);
});

test('each decoded point of a version 3 tile gives the logarithm of its age, or that it has none', () => {
  let aged = 0;
  for (const { fixture, manifest, node, tile } of everyTile) {
    assert.equal(manifest.encoding.log_age_code, 3);
    decodeTile(bytesOf(tile.hex), node, manifest).forEach((p, k) => {
      const expected = tile.points[k].log_age;
      const at = `${fixture} ${tile.address} point ${k}`;
      if (expected === undefined) {
        assert.equal(p.logAge, undefined, `${at}: no age`);
      } else {
        assert.equal(p.colour, undefined, `${at}: an age in place of a colour`);
        assert.ok(p.logAge !== undefined && Math.abs(p.logAge - expected) <= 0.005 + 1e-12, `${at}: within 0.005`);
        aged += 1;
      }
    });
  }
  assert.ok(aged > 0, `${aged} points with an age`);
  // A version 2 point never has an age.
  for (const { manifest, node, tile } of everyVersion2Tile) {
    assert.ok(decodeTile(bytesOf(tile.hex), node, manifest).every((p) => p.logAge === undefined));
  }
});

test('a version 3 manifest naming another log age code is refused, so that no colour reads as an age', () => {
  const t = everyTile.find((t) => t.tile.points.some((p) => p.colour !== null && p.colour !== undefined));
  assert.ok(t);
  const manifest = { ...t.manifest, encoding: { ...t.manifest.encoding, log_age_code: 1 } };
  assert.throws(() => decodeTile(bytesOf(t.tile.hex), t.node, manifest), /log age code 1, not the 3/);
});

test('the age code is refused in a version 2 tile', () => {
  const t = everyTile.find((t) => t.tile.points.some((p) => p.log_age !== undefined));
  assert.ok(t);
  const bytes = bytesOf(t.tile.hex);
  new DataView(bytes.buffer).setUint16(4, 2, true);
  const { log_age_code: _, ...encoding } = t.manifest.encoding;
  const manifest = { ...t.manifest, encoding: { ...encoding, version: 2 } };
  assert.throws(() => decodeTile(bytes, t.node, manifest), /value code 3, which the manifest does not list/);
});

/** The first fixture tile with points, as bytes to alter. */
const sample = () => {
  const t = everyTile.find((t) => t.tile.points.length > 1);
  assert.ok(t);
  return { ...t, bytes: bytesOf(t.tile.hex) };
};

test('AC-284: a tile of an encoding version the decoder does not know fails naming the version', () => {
  for (const version of [0, 4, 7, 65_535]) {
    const { bytes, node, manifest } = sample();
    new DataView(bytes.buffer).setUint16(4, version, true);
    assert.throws(
      () => decodeTile(bytes, node, manifest),
      (e: unknown) => e instanceof Error && new RegExp(`\\bversion ${version}\\b`).test(e.message),
    );
    // At any size: the version is read before the size.
    assert.throws(() => decodeTile(bytes.subarray(0, 6), node, manifest), new RegExp(`version ${version}\\b`));
  }
});

test('a tile that is not what its manifest lists is refused', () => {
  const cases: [string, (b: Uint8Array) => Uint8Array, RegExp][] = [
    ['another magic', (b) => (b.set([0x58], 0), b), /magic UVTL/],
    ['a reserved field set', (b) => (b.set([1], 6), b), /reserved/],
    ['a byte short', (b) => b.subarray(0, b.length - 1), /not the \d+ its count/],
    ['an object id of 0', (b) => (b.fill(0, 48 + 6 * pointsOf(b), 48 + 6 * pointsOf(b) + 8), b), /object id 0/],
    ['an object id of 2^53', (b) => (b.set([0, 0, 0, 0, 0, 0, 0x20, 0], 48 + 6 * pointsOf(b)), b), /outside 1 to 2\^53/],
    ['an unknown kind', (b) => (b.set([4], 48 + 16 * pointsOf(b)), b), /kind of value 4/],
    ['an unknown value code', (b) => (b.set([4], 48 + 19 * pointsOf(b)), b), /value code 4/],
    [
      'a value with no code',
      (b) => (new DataView(b.buffer).setInt16(48 + 17 * pointsOf(b), 100, true), b.set([0], 48 + 19 * pointsOf(b)), b),
      /value code 0/,
    ],
    ['an unlisted source', (b) => (b.set([0xff, 0xff], 48 + 14 * pointsOf(b)), b), /source 65535/],
    ['another node', (b) => (new DataView(b.buffer).setFloat64(16, 1e9, true), b), /is not node/],
  ];
  for (const [what, alter, message] of cases) {
    const { bytes, node, manifest } = sample();
    assert.throws(() => decodeTile(alter(bytes), node, manifest), message, what);
  }
  const { bytes, manifest } = sample();
  const other = manifest.nodes.find((n) => n.bytes !== bytes.length) ?? { ...manifest.nodes[0], points: -1 };
  assert.throws(() => decodeTile(bytes, other, manifest), /lists .* points/);
});

function pointsOf(b: Uint8Array): number {
  return new DataView(b.buffer, b.byteOffset).getUint32(8, true);
}
