/**
 * Developer diagnostics, inactive unless requested through environment
 * variables of the host process (see README):
 *  - OUTDOOR_FPS_PROBE: animate the map, report frame timing, exit.
 *  - OUTDOOR_SELFTEST=<gpx file>: drive the real controls through a recording
 *    with a simulated receiver, report what the page shows, exit.
 */
import { invoke } from "@tauri-apps/api/core";
import { api } from "./api";
import type { MapView } from "./map";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function until(what: string, condition: () => boolean, timeoutMs = 15_000): Promise<void> {
  const start = performance.now();
  while (!condition()) {
    if (performance.now() - start > timeoutMs) throw new Error(`timed out waiting for ${what}`);
    await sleep(50);
  }
}

async function fpsProbe(view: MapView): Promise<void> {
  const map = view.map;
  await new Promise<void>((resolve) => (map.loaded() ? resolve() : map.once("idle", () => resolve())));
  const gl = map.getCanvas().getContext("webgl2");
  const info = gl?.getExtension("WEBGL_debug_renderer_info");
  const renderer = gl && info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : "unknown";

  const durationMs = 10_000;
  const frames: number[] = [];
  let last = performance.now();
  const start = last;
  map.easeTo({ zoom: 14.5, bearing: 180, pitch: 45, duration: durationMs, easing: (x: number) => x });
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
  await api.probeReport({
    renderer,
    frames: frames.length,
    fps_avg: (frames.length * 1000) / total,
    frame_ms_median: frames[Math.floor(frames.length / 2)],
    frame_ms_p95: frames[Math.floor(frames.length * 0.95)],
    frame_ms_max: frames[frames.length - 1],
    canvas: `${map.getCanvas().width}x${map.getCanvas().height}`,
  });
}

async function selfTest(view: MapView, gpxPath: string): Promise<void> {
  const el = (id: string) => document.getElementById(id) as HTMLElement;
  const click = (id: string) => el(id).click();
  const text = (id: string) => el(id).textContent ?? "";
  const report: Record<string, unknown> = {};
  try {
    const tracksBefore = document.querySelectorAll("#track-list li[data-id]").length;
    (el("activity") as HTMLSelectElement).value = "bike";
    // If the previous run ended mid-recording, take it up again instead of starting anew.
    report.recovery_offered = !el("recovery").hidden;
    if (report.recovery_offered) {
      click("recovery-continue");
      await until("recording continued", () => !el("rec-pause").hidden && el("recovery").hidden === true);
      report.points_recovered = view.liveLinePointCount();
    }
    await api.locationStartReplay(gpxPath, 100);
    if (!report.recovery_offered) click("rec-start");
    await until("recording phase", () => !el("rec-pause").hidden);
    const atStart = view.liveLinePointCount();
    await until("live points", () => view.liveLinePointCount() > atStart + 100);
    report.marker_on_map = document.querySelector(".position-marker") !== null;
    report.source_status = text("loc-status");

    click("rec-pause");
    await until("paused", () => !el("rec-resume").hidden);
    const atPause = view.liveLinePointCount();
    await sleep(500);
    report.points_added_while_paused = view.liveLinePointCount() - atPause;
    click("rec-resume");
    await until("resumed", () => !el("rec-pause").hidden);
    await until("more live points", () => view.liveLinePointCount() > atPause + 100);

    report.live_points = view.liveLinePointCount();
    report.live_stats = text("live-stats");
    click("rec-stop");
    await until("track in list", () => document.querySelectorAll("#track-list li[data-id]").length > tracksBefore);
    await until("track detail", () => !el("track-detail").hidden && text("track-stats") !== "");
    report.phase_after_stop = text("rec-phase");
    report.tracks_listed = document.querySelectorAll("#track-list li[data-id]").length;
    report.first_track = text("track-list").slice(0, 80);
    report.track_stats = text("track-stats");
    report.message = text("message");
    report.ok = true;
  } catch (e) {
    report.ok = false;
    report.error = String(e);
    report.message = text("message");
  }
  await api.probeReport(report);
}

export async function runDiagnostics(view: MapView): Promise<void> {
  if (await api.probeEnabled()) await fpsProbe(view);
  const gpx = await invoke<string | null>("selftest_gpx");
  if (gpx) await selfTest(view, gpx);
}
