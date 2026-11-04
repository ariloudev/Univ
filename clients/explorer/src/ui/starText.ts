// What the panel says of a star's colour and type (read-an-object): the spectral types its
// sources publish, its colour indices, and, where no source gives a type, the class its colour
// index suggests for a main-sequence star along the dwarf sequence of Pecaut & Mamajek 2013.

import type { ColourBands } from "univ-tile-decoder";

import type { Measurement, ObjectRecord } from "../api/objects.ts";
import { DWARF_SEQUENCE_CITATION } from "../scene/dwarfSequence.ts";
import { figures } from "../scene/format.ts";
import { temperatureOf } from "../scene/starColour.ts";

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

/** The bands the dwarf sequence reads, in the order the tiles prefer them. */
const SEQUENCE_BANDS: readonly ColourBands[] = ["BP-RP", "B-V"];

/** The class the object's colour index suggests, where no source publishes a type: estimated
 * from the first index in bands the dwarf sequence reads, BP−RP before B−V. */
export function estimatedClass(record: ObjectRecord): string | undefined {
  if (publishedTypes(record).length > 0) return undefined;
  const indices = colourIndices(record);
  for (const bands of SEQUENCE_BANDS) {
    const m = indices.find((i) => i.band === bands);
    if (m === undefined) continue;
    const { teff, type, clamped } = temperatureOf(m.native.value, bands);
    const end = clamped === undefined ? "" : `, at the ${clamped} end of the sequence`;
    const kelvin = figures(teff, 2);
    return (
      `${type}, about ${kelvin} K${end}: estimated from ${bands.replace("-", "−")} ${m.native.value} ` +
      `(${m.source}) as for a main-sequence star, along the dwarf sequence of ${DWARF_SEQUENCE_CITATION}`
    );
  }
  return undefined;
}
