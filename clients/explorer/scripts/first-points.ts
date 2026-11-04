// Measures the explorer's first points (draw-the-catalogues, quality attribute
// explorer.first_points.latency): opens the explorer 10 times and reads, from each load, the
// milliseconds from the page's start to the first points drawn; prints each and the 95th
// percentile against the 3-second bound, and exits 1 over it.
//
// It serves the explorer itself, through Vite's dev server on its own port with the proxy of
// vite.config.ts, as `cargo xtask explore` does: `cargo xtask bench first-points` gives it
// Univ Server's address and a key in UNIV_SERVER and UNIV_KEY.

import { chromium } from "@playwright/test";
import { createServer } from "vite";

const LOADS = 10;
const BOUND_MS = 3000;
const PORT = 5318;

/** The milliseconds from the start of a fresh load of `address` to its first points drawn. */
async function load(browser: import("@playwright/test").Browser, address: string): Promise<number> {
  // A new context each time: nothing cached from the load before.
  const context = await browser.newContext({ viewport: { width: 1400, height: 900 } });
  const page = await context.newPage();
  await page.goto(address);
  await page.waitForFunction(() => performance.getEntriesByName("univ:first-points").length > 0, null, { timeout: 60_000 });
  const ms = await page.evaluate(() => performance.getEntriesByName("univ:first-points")[0]?.startTime ?? Number.NaN);
  await context.close();
  return ms;
}

/** The `q` quantile of sorted `times`, by the nearest rank. */
const rank = (times: number[], q: number): number => times[Math.max(1, Math.ceil(q * times.length)) - 1] ?? Number.NaN;

const server = await createServer({ server: { port: PORT, strictPort: true } });
await server.listen();
const browser = await chromium.launch({ args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const times: number[] = [];
try {
  for (let n = 1; n <= LOADS; n += 1) {
    const ms = await load(browser, `http://127.0.0.1:${PORT}/`);
    times.push(ms);
    console.log(`load ${n}: first points after ${ms.toFixed(0)} ms`);
  }
} finally {
  await browser.close();
  await server.close();
}
times.sort((a, b) => a - b);
const p95 = rank(times, 0.95);
const met = p95 <= BOUND_MS;
console.log(
  `first points over ${LOADS} loads: p50 ${rank(times, 0.5).toFixed(0)} ms, p95 ${p95.toFixed(0)} ms, ` +
    `max ${rank(times, 1).toFixed(0)} ms: ${met ? "within" : "over"} the ${BOUND_MS} ms bound`,
);
process.exit(met ? 0 : 1);
