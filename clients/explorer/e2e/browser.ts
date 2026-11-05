// How the end-to-end tests and the first-points bench drive the explorer: Chromium with WebGL2
// in software (SwiftShader), as a machine without a GPU has, at one viewport size.

import type { Page } from "@playwright/test";

import { FIRST_POINTS } from "../src/firstPoints.ts";

export const CHROMIUM_ARGS = ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"];

export const VIEWPORT = { width: 1400, height: 900 };

/** The milliseconds from the page's start to its first points drawn, once they are. */
export async function firstPointsMs(page: Page, timeout = 30_000): Promise<number> {
  await page.waitForFunction((mark) => performance.getEntriesByName(mark).length > 0, FIRST_POINTS, { timeout });
  return page.evaluate((mark) => performance.getEntriesByName(mark)[0]?.startTime ?? Number.NaN, FIRST_POINTS);
}
