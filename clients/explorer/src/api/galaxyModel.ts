// The Galaxy model of a release as the explorer reads it (read-the-galaxy-model): each
// population's code, name, display colour and form of density, which gives its counterparts,
// and the papers the definition cites. Read once for each release.

import { z } from "zod";

import { type ReadOptions, cacheOf, kept, read } from "./client.ts";

const Population = z.looseObject({
  code: z.number().int().min(1).max(255),
  id: z.string(),
  name: z.string(),
  density: z.looseObject({ form: z.string() }),
  colour: z.looseObject({
    srgb: z.tuple([z.number().int(), z.number().int(), z.number().int()]),
    b_minus_v: z.looseObject({ value: z.number() }).nullable().optional(),
    srgb_from: z.looseObject({ from: z.string() }),
  }),
});

const Paper = z.looseObject({ id: z.string(), reference: z.string(), doi: z.string().nullable().optional() });

const GalaxyModel = z.looseObject({
  release: z.number(),
  domain: z.string(),
  definition: z.looseObject({ populations: z.array(Population), papers: z.array(Paper) }),
  followed: z.array(z.string()),
});
export type GalaxyModel = z.infer<typeof GalaxyModel>;
export type ModelPaper = z.infer<typeof Paper>;

const models = cacheOf<number, GalaxyModel | undefined>();

/** The Galaxy model of `release`; none where the release holds none. */
export function galaxyModel(release: number, options: ReadOptions = {}): Promise<GalaxyModel | undefined> {
  return kept(models, release, async () => {
    const response = await read("/v1/galaxy-model", { ...options, release, accept: [404] });
    if (response.status === 404) return undefined;
    return GalaxyModel.parse(await response.json());
  });
}
