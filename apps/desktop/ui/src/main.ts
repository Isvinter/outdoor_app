import { invoke } from "@tauri-apps/api/core";
import { Map as MapLibreMap, setWorkerUrl } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";

function log(level: "info" | "error", message: string): void {
  void invoke("ui_log", { level, message });
}
window.addEventListener("error", (e) => log("error", `${e.message} (${e.filename}:${e.lineno})`));
window.addEventListener("unhandledrejection", (e) => log("error", `unhandled: ${String(e.reason)}`));
log("info", `ui started, ${navigator.userAgent}`);

// The worker files are copied next to the bundle by vite.config.ts.
setWorkerUrl(new URL("maplibre/maplibre-gl-worker.mjs", document.baseURI).href);

const map = new MapLibreMap({
  container: "map",
  style: "https://tiles.openfreemap.org/styles/liberty",
  center: [11.57, 47.68],
  zoom: 12,
});

/** Animates the map for a few seconds and reports frame timing to the host. */
async function runFpsProbe(): Promise<void> {
  const gl = map.getCanvas().getContext("webgl2");
  const info = gl?.getExtension("WEBGL_debug_renderer_info");
  const renderer = gl && info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : "unknown";

  const durationMs = 10_000;
  const frames: number[] = [];
  let last = performance.now();
  const start = last;
  map.easeTo({ zoom: 14.5, bearing: 180, pitch: 45, duration: durationMs, easing: (t: number) => t });
  await new Promise<void>((resolve) => {
    const tick = (now: number) => {
      frames.push(now - last);
      last = now;
      if (now - start < durationMs) requestAnimationFrame(tick);
      else resolve();
    };
    requestAnimationFrame(tick);
  });
  frames.sort((a, b) => a - b);
  const total = frames.reduce((a, b) => a + b, 0);
  await invoke("probe_report", {
    report: {
      renderer,
      frames: frames.length,
      fps_avg: (frames.length * 1000) / total,
      frame_ms_median: frames[Math.floor(frames.length / 2)],
      frame_ms_p95: frames[Math.floor(frames.length * 0.95)],
      frame_ms_max: frames[frames.length - 1],
      device_pixel_ratio: window.devicePixelRatio,
      canvas: `${map.getCanvas().width}x${map.getCanvas().height}`,
    },
  });
}

map.on("error", (e) => log("error", `map: ${e.error?.message ?? String(e)}`));
map.once("load", () => log("info", "map loaded"));
map.once("idle", async () => {
  log("info", "map idle");
  if (await invoke<boolean>("probe_enabled")) await runFpsProbe();
});
