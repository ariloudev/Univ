// The changes the Show drawer's controls make together (choose-what-is-drawn): only one source or
// kind of structure, All and None, and everything at once.

import { KINDS } from "../../scene/kinds.ts";
import { type Change, type Group, GROUPS, type Hidden } from "../hidden.ts";
import type { ShowChoices } from "./choices.ts";

/** Each of `names` in `group` turned on, or off. */
export const every = (group: Group, names: readonly string[], shown: boolean): Change[] => names.map((name) => ({ group, name, shown }));

/** Only the source `id`: it and each domain whose points it places on, every other source off. */
export const onlySource = (id: string, choices: ShowChoices): Change[] => [
  ...every("domain", choices.sources.find((s) => s.id === id)?.domains ?? [], true),
  ...choices.sourceIds.map((name) => ({ group: "source" as const, name, shown: name === id })),
];

/** Only the kind of structure `kind`: it on, every other kind off. */
export const onlyKind = (kind: string, choices: ShowChoices): Change[] =>
  choices.structureKinds.map((name) => ({ group: "structure", name, shown: name === kind }));

/** Every domain, source, kind of value and kind of object on. */
export const allPoints = (choices: ShowChoices): Change[] => [
  ...every("domain", choices.domainNames, true),
  ...every("source", choices.sourceIds, true),
  ...every("kind", KINDS, true),
  ...every("object", choices.objectKinds, true),
];

/** Every domain off, so no point is drawn, the sources' switches as they are. */
export const nonePoints = (choices: ShowChoices): Change[] => every("domain", choices.domainNames, false);

/** Every kind of structure on, or off. */
export const allStructures = (choices: ShowChoices): Change[] => every("structure", choices.structureKinds, true);
export const noneStructures = (choices: ShowChoices): Change[] => every("structure", choices.structureKinds, false);

/** Every group turned off now, on. */
export const everything = (hidden: Hidden): Change[] => GROUPS.flatMap((group) => every(group, [...hidden[group]], true));
