/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed port and no clearing of the terminal.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: "127.0.0.1" },
  envPrefix: ["VITE_"],
  build: {
    target: "es2022",
    sourcemap: false,
    // No inline scripts: keeps the strict CSP (`script-src 'self'`) valid.
    assetsInlineLimit: 0,
    // The video exporter (mediabunny) is one ~520 kB chunk, loaded on first export.
    chunkSizeWarningLimit: 600,
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
