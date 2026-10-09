import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

// `make web` builds one self-contained page into src/assets (embedded in the binary).
// `make web-dev`: run `ksef --no-open` first, then open the Vite URL with the same `?t=<token>`.
export default defineConfig({
  plugins: [svelte(), viteSingleFile()],
  build: {
    outDir: "../src/assets",
    emptyOutDir: true,
  },
  server: {
    proxy: {
      "/api": { target: "http://127.0.0.1:4848", changeOrigin: true },
      "/pdf": { target: "http://127.0.0.1:4848", changeOrigin: true },
    },
  },
});
