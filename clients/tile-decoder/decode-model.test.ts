// The reference decoder against the Galaxy model's version 5 tiles the Rust encoder wrote
// (fixtures/model/), each model point's position before quantization, population and whether
// real objects cover it beside it. univ-ingest's test tile_decoder_fixtures writes and checks
// the fixtures.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { decodeModelTile, decodeTile } from './decode.ts';
import type { ManifestNode, TileManifest, Vec3 } from './decode.ts';

interface ExpectedModel {
  position: Vec3;
  population: number;
  covered: boolean;
}

interface FixtureTile {
  address: string;
  hex: string;
  points: ExpectedModel[];
}

const read = (file: string): unknown => JSON.parse(readFileSync(new URL(`./fixtures/model/${file}`, import.meta.url), 'utf8'));
const manifest = read('simulated_model.manifest.json') as TileManifest;
const tiles = (read('simulated_model.tiles.json') as { tiles: FixtureTile[] }).tiles;
const objects = read('../stars.manifest.json') as TileManifest;
const objectTiles = (read('../stars.tiles.json') as { tiles: FixtureTile[] }).tiles;

const bytesOf = (hex: string): Uint8Array => Uint8Array.from(Buffer.from(hex, 'hex'));

const nodeOf = (of: TileManifest, address: string): ManifestNode => {
  const node = of.nodes.find((n) => n.address === address);
  assert.ok(node, `the manifest lists node ${address}`);
  return node;
};

test('each model point of a version 5 tile gives its position, its population and whether it is covered', () => {
  assert.equal(manifest.encoding.version, 5);
  assert.equal(manifest.scale_domain, 'galaxy_model');
  assert.ok(tiles.length > 1, 'several nodes');
  const populations = new Set<string>();
  let covered = 0;
  let checked = 0;
  for (const tile of tiles) {
    const node = nodeOf(manifest, tile.address);
    const level = manifest.levels.find((l) => l.level === node.level);
    assert.ok(level);
    const decoded = decodeModelTile(bytesOf(tile.hex), node, manifest);
    assert.equal(decoded.length, tile.points.length);
    decoded.forEach((p, k) => {
      const expected = tile.points[k];
      const at = `${tile.address} point ${k}`;
      [p.x, p.y, p.z].forEach((c, i) => {
        assert.ok(Math.abs(c - expected.position[i]) <= (level.step / 2) * (1 + 1e-9), `${at} axis ${i}`);
      });
      assert.equal(p.population.code, expected.population, `${at}: population`);
      assert.equal(p.covered, expected.covered, `${at}: covered`);
      populations.add(p.population.id);
      if (p.covered) covered++;
      checked++;
    });
  }
  assert.equal(checked, manifest.points);
  assert.equal(populations.size, 6, 'every population, named by its code');
  assert.ok(covered > 0 && covered < checked);
});

test('each decoder refuses the tiles of the other', () => {
  const tile = tiles[0];
  assert.throws(() => decodeTile(bytesOf(tile.hex), nodeOf(manifest, tile.address), manifest), /decodeModelTile/);
  const object = objectTiles[0];
  assert.throws(() => decodeModelTile(bytesOf(object.hex), nodeOf(objects, object.address), objects), /decodeTile/);
});

test('a version 5 population or flag the manifest does not define is refused', () => {
  const tile = tiles.find((t) => t.points.length > 0)!;
  const node = nodeOf(manifest, tile.address);
  const n = tile.points.length;
  const population = bytesOf(tile.hex);
  population[48 + 6 * n] = 7;
  assert.throws(() => decodeModelTile(population, node, manifest), /population 7, which the manifest does not list/);
  const flags = bytesOf(tile.hex);
  flags[48 + 7 * n] = 2;
  assert.throws(() => decodeModelTile(flags, node, manifest), /flags 2; only bit 0, covered/);
});
