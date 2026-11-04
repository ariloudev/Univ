// Structure records for tests, typed as Univ Server gives them.

import type { StructureDetail } from "../api/structures.ts";

/** A placed shape. */
export const shapeOf = (shape: NonNullable<StructureDetail["shape"]>) => shape;

/** A void at the origin, 2 Mpc across, `overrides` set. */
export function structureDetail(overrides: Partial<StructureDetail> = {}): StructureDetail {
  return {
    id: "s",
    name: "S",
    kind: "void",
    scale_domain: "cosmic_web",
    centre: { x: 0, y: 0, z: 0, unit: "megaparsec" },
    bounds: { min: [-1, -1, -1], max: [1, 1, 1] },
    shape: null,
    record: {},
    ...overrides,
  };
}
