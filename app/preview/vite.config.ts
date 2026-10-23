/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  root: __dirname,
  plugins: [svelte()],
  server: { port: 1421, strictPort: true, host: "127.0.0.1", fs: { allow: [".."] } },
  build: { target: "es2022" },
});
