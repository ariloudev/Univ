import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Univ Server's address and, where given, an API key the dev server adds to every read it
// passes on, so that the key never reaches the browser (`cargo xtask explore` sets both).
const server = process.env["UNIV_SERVER"] ?? "http://127.0.0.1:8790";
const key = process.env["UNIV_KEY"];
const proxy = {
  "/v1": {
    target: server,
    changeOrigin: true,
    ...(key === undefined ? {} : { headers: { authorization: `Bearer ${key}` } }),
  },
};

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: { __UNIV_SERVER__: JSON.stringify(server) },
  server: { host: "127.0.0.1", port: 5317, strictPort: true, proxy, fs: { allow: [".."] } },
  preview: { host: "127.0.0.1", port: 5317, strictPort: true, proxy },
  test: {
    environment: "jsdom",
    setupFiles: ["src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    coverage: { provider: "istanbul", reporter: ["text-summary", "json"], include: ["src/**"] },
  },
});
