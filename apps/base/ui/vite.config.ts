import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

const here = (relative: string) => fileURLToPath(new URL(relative, import.meta.url));

/**
 * MapLibre GL JS 6 runs its workers as ES modules loaded from a URL. It can only
 * derive that URL itself when served over http(s); inside the Tauri webview the
 * page comes from a custom scheme. Ship the worker files unmodified next to the
 * bundle so the app can point MapLibre at them (see `setWorkerUrl` in map.ts).
 */
function maplibreWorker(): Plugin {
  const dist = here("./node_modules/maplibre-gl/dist/");
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

function filesBelow(dir: string, prefix = ""): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? filesBelow(path, `${prefix}${name}/`) : [`${prefix}${name}`];
  });
}

/**
 * Map styles and fonts are shared by all platforms and therefore live at the
 * repository root. Bundle them as `map/styles/*` and `map/fonts/*`.
 */
function mapAssets(): Plugin {
  const sources: [string, string, (file: string) => boolean][] = [
    [here("../../../styles/"), "map/styles/", (file) => file.endsWith(".json")],
    [here("../../../assets/map/fonts/"), "map/fonts/", (file) => file.endsWith(".pbf")],
  ];
  return {
    name: "map-assets",
    generateBundle() {
      for (const [dir, target, wanted] of sources) {
        for (const file of filesBelow(dir).filter(wanted)) {
          this.emitFile({ type: "asset", fileName: `${target}${file}`, source: readFileSync(join(dir, file)) });
        }
      }
    },
  };
}

export default defineConfig({
  clearScreen: false,
  plugins: [maplibreWorker(), mapAssets()],
  build: { target: "es2022", outDir: "dist", emptyOutDir: true, chunkSizeWarningLimit: 1500 },
});
