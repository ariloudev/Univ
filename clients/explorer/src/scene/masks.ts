// The groups turned off as bits (choose-what-is-drawn): one mask of kinds of value, one of the
// Galaxy tracers' kinds of object, and for each scale domain one mask of its sources by their
// index in its tile manifest. The shader, the pick and the key read the same masks, so that what
// is drawn, picked and counted agree.

import type { KindOfValue } from "univ-tile-decoder";

import type { Hidden } from "../ui/hidden.ts";
import { KIND_INDEX } from "./kinds.ts";
import { isTracerKind, objectIndex } from "./tracers.ts";

/** The bits a source mask holds: a source of a higher index in its manifest cannot be turned
 * off, and is drawn whatever is (the shader tests the same bound). */
export const MASK_BITS = 32;

/** What is turned off, as bits. */
export interface Masks {
  domains: ReadonlySet<string>;
  /** Bit `KIND_INDEX[kind]` set for each kind of value turned off. */
  kinds: number;
  /** Bit `objectIndex(kind)` set for each kind of object turned off; never bit 0, a tracer of no
   * kind, which no switch turns off. */
  objects: number;
  /** For each domain, bit `index` set for each of its sources turned off. */
  sources: ReadonlyMap<string, number>;
}

/** Nothing turned off. */
export const NO_MASKS: Masks = { domains: new Set(), kinds: 0, objects: 0, sources: new Map() };

/** A domain and the sources of its manifest, by index and id. */
export interface MaskedDomain {
  name: string;
  manifest: { sources: readonly { index: number; id: string }[] };
}

/** The groups of `hidden` that points belong to, as masks over `domains`' sources. */
export function masksOf(hidden: Pick<Hidden, "domain" | "source" | "kind" | "object">, domains: readonly MaskedDomain[]): Masks {
  let kinds = 0;
  for (const kind of hidden.kind) if (Object.hasOwn(KIND_INDEX, kind)) kinds |= 1 << KIND_INDEX[kind as KindOfValue];
  let objects = 0;
  for (const kind of hidden.object) if (isTracerKind(kind)) objects |= 1 << objectIndex(kind);
  const sources = new Map<string, number>();
  for (const { name, manifest } of domains) {
    let bits = 0;
    for (const { index, id } of manifest.sources) if (index < MASK_BITS && hidden.source.has(id)) bits |= 1 << index;
    sources.set(name, bits >>> 0);
  }
  return { domains: hidden.domain, kinds, objects, sources };
}

/** Whether a point of `domain`, of the source, kind of value and object index given, is drawn. */
export function isPointShown(masks: Masks, domain: string, source: number, kind: number, object = 0): boolean {
  if (masks.domains.has(domain) || ((masks.kinds >>> kind) & 1) === 1 || ((masks.objects >>> object) & 1) === 1) return false;
  return source >= MASK_BITS || (((masks.sources.get(domain) ?? 0) >>> source) & 1) === 0;
}

/** Which points of a batch are drawn: "none" where its domain is turned off, "all" where nothing
 * of it is, or else by index. */
export type Shown = "all" | "none" | ((i: number) => boolean);

/** Which points of a batch of `domain`, of `sources`, `kinds` and `objects` (object indices,
 * none for a batch outside the Galaxy tracers) by index, are drawn. */
export function shownIn(
  masks: Masks,
  domain: string,
  sources: ArrayLike<number>,
  kinds: ArrayLike<number>,
  objects?: ArrayLike<number>,
): Shown {
  if (masks.domains.has(domain)) return "none";
  const sourceBits = masks.sources.get(domain) ?? 0;
  const kindBits = masks.kinds;
  const objectBits = objects === undefined ? 0 : masks.objects;
  if (kindBits === 0 && sourceBits === 0 && objectBits === 0) return "all";
  return (i) => {
    const source = sources[i] ?? 0;
    return (
      ((kindBits >>> (kinds[i] ?? 0)) & 1) === 0 &&
      ((objectBits >>> (objects?.[i] ?? 0)) & 1) === 0 &&
      (source >= MASK_BITS || ((sourceBits >>> source) & 1) === 0)
    );
  };
}
