import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

/**
 * MapLibre GL JS 6 runs its workers as ES modules loaded from a URL. It can only
 * derive that URL itself when served over http(s); inside the Tauri webview the
 * page comes from a custom scheme. Ship the worker files unmodified next to the
 * bundle so the app can point MapLibre at them (see `setWorkerUrl` in main.ts).
 */
function maplibreWorker(): Plugin {
  const dist = fileURLToPath(new URL("./node_modules/maplibre-gl/dist/", import.meta.url));
  return {
    name: "maplibre-worker",
    generateBundle() {
      for (const file of ["maplibre-gl-worker.mjs", "maplibre-gl-shared.mjs"]) {
        this.emitFile({
          type: "asset",
          fileName: `maplibre/${file}`,
          source: readFileSync(join(dist, file)),
        });
      }
    },
  };
}

export default defineConfig({
  clearScreen: false,
  plugins: [maplibreWorker()],
  build: { target: "es2022", outDir: "dist", emptyOutDir: true, chunkSizeWarningLimit: 1500 },
});
