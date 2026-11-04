import type { ServerResponse } from "node:http";

import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Univ Server's address and, where given, an API key the dev server adds to every read it
// passes on, so that the key never reaches the browser (`cargo xtask explore` sets both).
const server = process.env["UNIV_SERVER"] ?? "http://127.0.0.1:8790";
const key = process.env["UNIV_KEY"];

/** The problem the explorer's own server answers with when Univ Server does not answer. */
function unreachable(response: ServerResponse) {
  response.writeHead(502, { "content-type": "application/problem+json" });
  response.end(
    JSON.stringify({
      type: "urn:univ-explorer:problem:server-unreachable",
      title: "Univ Server is not answering",
      status: 502,
      address: server,
    }),
  );
}

const listen = {
  host: "127.0.0.1",
  port: 5317,
  strictPort: true,
  proxy: {
    "/v1": {
      target: server,
      changeOrigin: true,
      ...(key === undefined ? {} : { headers: { authorization: `Bearer ${key}` } }),
      configure: (proxy: { on: (event: "error", handler: (...args: unknown[]) => void) => void }) => {
        proxy.on("error", (_error, _request, response) => {
          if (response !== null && typeof response === "object" && "writeHead" in response) {
            unreachable(response as ServerResponse);
          }
        });
      },
    },
  },
};

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: listen,
  preview: listen,
  test: {
    environment: "jsdom",
    setupFiles: ["src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    coverage: { provider: "istanbul", reporter: ["text-summary", "json"], include: ["src/**"] },
  },
});
