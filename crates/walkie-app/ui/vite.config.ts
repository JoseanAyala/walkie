import { resolve } from "node:path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

// One page per Tauri window. The build lands in ../dist, which
// tauri.conf.json embeds into the binary.
export default defineConfig({
  plugins: [svelte()],
  // `@/` is `src/`; parent-relative imports are banned (.oxlintrc.json)
  resolve: { alias: { "@": resolve(import.meta.dirname, "src") } },
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "safari15",
    rollupOptions: {
      input: {
        index: resolve(import.meta.dirname, "index.html"),
        overlay: resolve(import.meta.dirname, "overlay.html"),
      },
    },
  },
  test: { include: ["src/**/*.test.ts"], testTimeout: 5000 },
});
