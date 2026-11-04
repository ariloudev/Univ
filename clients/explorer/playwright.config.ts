import { defineConfig, devices } from "@playwright/test";

import { CHROMIUM_ARGS, VIEWPORT } from "./e2e/browser.ts";

// End-to-end tests of the explorer against a running Univ Server: `cargo xtask integration`
// starts one on the fixture state and gives its address in UNIV_SERVER. The explorer's dev server
// runs beside it on a port of its own, so that a running `cargo xtask explore` is left alone.
const PORT = 5318;

export default defineConfig({
  testDir: "e2e",
  reporter: "list",
  timeout: 60_000,
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: "retain-on-failure",
    ...devices["Desktop Chrome"],
    viewport: VIEWPORT,
    launchOptions: { args: CHROMIUM_ARGS },
  },
  webServer: {
    command: `pnpm exec vite --port ${PORT}`,
    url: `http://127.0.0.1:${PORT}/`,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
