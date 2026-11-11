// The tile decoder's fixtures, which the Rust encoder writes: manifests of three scale domains
// and their tiles, each with the points it holds.

import type { ColourBands } from "univ-tile-decoder";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { ExplorerManifest } from "../api/schemas.ts";

const DIR = join(dirname(fileURLToPath(import.meta.url)), "../../../tile-decoder/fixtures");

export const DOMAINS = ["stars", "nearby_galaxies", "cosmic_web"];

/** A fixture's text. */
export const fixtureText = (file: string): string => readFileSync(join(DIR, file), "utf8");

/** The manifest of `domain`, checked as the explorer checks it. */
export const manifestOf = (domain: string): ExplorerManifest =>
  ExplorerManifest.parse(JSON.parse(fixtureText(`${domain}.manifest.json`)));

export interface FixtureTile {
  address: string;
  hex: string;
  points: {
    position: [number, number, number];
    id: number;
    kind: string;
    colour?: [number, ColourBands];
    /** The base-10 logarithm of its age in years before rounding, where it carries one. */
    log_age?: number;
  }[];
}

/** The tiles of `domain`, with the points each holds. */
export const fixtureTiles = (domain: string): FixtureTile[] =>
  (JSON.parse(fixtureText(`${domain}.tiles.json`)) as { tiles: FixtureTile[] }).tiles;

/** The bytes a hexadecimal string spells. */
export const bytesOf = (hex: string): Uint8Array<ArrayBuffer> =>
  Uint8Array.from(hex.match(/../g) ?? [], (b) => Number.parseInt(b, 16));
