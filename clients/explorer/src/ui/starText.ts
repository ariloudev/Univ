// What the panel says of a star's colour and type (read-an-object): the spectral types its
// sources publish, its colour indices, and, where no source gives a type, the class its colour
// index suggests for a main-sequence star along the dwarf sequence of Pecaut & Mamajek 2013.

import type { Measurement, ObjectRecord } from "../api/objects.ts";
import { DWARF_SEQUENCE_CITATION } from "../scene/dwarfSequence.ts";
import { figures } from "../scene/format.ts";
import { type StarColourIndex, temperatureOf } from "../scene/starColour.ts";
import { bandsText } from "./text.ts";

/** A spectral type a source publishes, with its source and kind of value. */
export interface PublishedType {
  text: string;
  source: string;
  kind: string;
}

/** Whether `text` names a type: Gaia gives "unknown" where it classified none. */
const isType = (text: string): boolean => text.trim() !== "" && text.trim().toLowerCase() !== "unknown";

/** The spectral types the object's sources publish, as published; "unknown" is none. */
export const publishedTypes = (record: ObjectRecord): PublishedType[] =>
  record.entries.flatMap((e) =>
    e.spectral_type !== null && e.spectral_type !== undefined && isType(e.spectral_type.text)
      ? [{ text: e.spectral_type.text, source: e.source, kind: e.spectral_type.kind_of_value }]
      : [],
  );

/** The colour indices of the object, as measurements. */
export const colourIndices = (record: ObjectRecord): Measurement[] => record.measurements.filter((m) => m.quantity === "colour_index");

/** The rank of a colour index as ingest chooses the one a star is drawn by (build-octree-tiles):
 * a published BP−RP, a published B−V, a derived BP−RP, a derived B−V; none for other bands. */
function rank(m: Measurement): number | undefined {
  const derived = m.derived_from !== null && m.derived_from !== undefined ? 2 : 0;
  if (m.band === "BP-RP") return derived;
  if (m.band === "B-V") return derived + 1;
  return undefined;
}

/** The colour index a star is drawn by: of the best rank, the first its entries give. */
export function drawnColourIndex(indices: readonly Measurement[]): (StarColourIndex & { source: string }) | undefined {
  let best: { at: number; m: Measurement } | undefined;
  for (const m of indices) {
    const at = rank(m);
    if (at !== undefined && (best === undefined || at < best.at)) best = { at, m };
  }
  if (best === undefined) return undefined;
  const bands = best.m.band === "B-V" ? "B-V" : "BP-RP";
  return { index: best.m.native.value, bands, source: best.m.source };
}

/** The class the colour index a star is drawn by suggests, where its sources publish no type. */
export function estimatedClass(types: readonly PublishedType[], indices: readonly Measurement[]): string | undefined {
  if (types.length > 0) return undefined;
  const colour = drawnColourIndex(indices);
  if (colour === undefined) return undefined;
  const { teff, type, clamped } = temperatureOf(colour.index, colour.bands);
  const end = clamped === undefined ? "" : `, at the ${clamped} end of the sequence`;
  return (
    `${type}, about ${figures(teff, 2)} K${end}: estimated from ${bandsText(colour.bands)} ${colour.index} ` +
    `(${colour.source}) as for a main-sequence star, along the dwarf sequence of ${DWARF_SEQUENCE_CITATION}; ` +
    "the colour as observed, not corrected for interstellar reddening, and with no uncertainty given"
  );
}
