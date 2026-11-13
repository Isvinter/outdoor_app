/**
 * Developer diagnostics, inactive unless requested through environment
 * variables of the host process (see README):
 *  - OUTDOOR_FPS_PROBE: animate the map, report frame timing, exit.
 *  - OUTDOOR_SNAPSHOT=<png file>: save a picture of the map, exit.
 *  - OUTDOOR_SELFTEST_REGIONS=<catalog>: download, show and delete the first
 *    region of a catalog through the real controls, report, exit.
 *  - OUTDOOR_SELFTEST_ROUTE=<lat>,<lon>;<lat>,<lon>[;…]: plan a hike through
 *    these points with the real controls, save it, report, exit. Together with
 *    OUTDOOR_SNAPSHOT the picture shows the route.
 *  - OUTDOOR_SELFTEST=<gpx file>: drive the real controls through a recording
 *    with a simulated receiver, report what the page shows, exit.
 */
import { invoke } from "@tauri-apps/api/core";
import { LngLat, LngLatBounds } from "maplibre-gl";
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

async function regionsSelfTest(view: MapView, catalog: string): Promise<void> {
  const el = (id: string) => document.getElementById(id) as HTMLElement;
  const firstRegion = () => document.querySelector<HTMLElement>("#region-list li");
  const meta = () => firstRegion()?.querySelector(".meta")?.textContent ?? "";
  const press = (label: string) =>
    [...(firstRegion()?.querySelectorAll("button") ?? [])].find((b) => b.textContent === label)?.click();
  // While a style is being replaced there is none.
  const sources = () => Object.keys(view.map.getStyle()?.sources ?? {});
  const report: Record<string, unknown> = {};
  try {
    (el("catalog-source") as HTMLInputElement).value = catalog;
    el("catalog-apply").click();
    await until("catalog listed", () => firstRegion() !== null);
    report.offered = `${firstRegion()?.querySelector(".name")?.textContent} – ${meta()}`;
    report.sources_before = sources();
    press("Herunterladen");
    await until("region installed", () => meta().includes("installiert"), 120_000);
    await until("offline sources", () => sources().includes("basemap"));
    report.installed = meta();
    report.sources_installed = sources();
    report.message_installed = el("message").textContent;
    press("Löschen");
    await until("region deleted", () => meta().includes("nicht geladen"));
    await until("sources gone", () => !sources().includes("basemap"));
    report.after_delete = meta();
    report.ok = true;
  } catch (e) {
    report.ok = false;
    report.error = String(e);
    report.message = el("message").textContent;
    report.hint = el("catalog-hint").textContent;
  }
  await api.probeReport(report);
}

async function routeSelfTest(view: MapView, spec: string, exit: boolean): Promise<void> {
  const el = (id: string) => document.getElementById(id) as HTMLElement;
  const text = (id: string) => el(id).textContent ?? "";
  const report: Record<string, unknown> = {};
  try {
    const points = spec.split(";").map((pair) => pair.split(",").map(Number));
    const saved = document.querySelectorAll("#route-list li[data-id]").length;
    report.profiles = [...el("route-profile").querySelectorAll("option")].map((o) => o.textContent);
    el("route-edit").click();
    const places = points.map(([lat, lon]) => new LngLat(lon ?? 0, lat ?? 0));
    const first = places[0];
    if (!first) throw new Error("no waypoints given");
    view.map.fitBounds(
      places.reduce((bounds, place) => bounds.extend(place), new LngLatBounds(first, first)),
      { padding: 80, duration: 0 },
    );
    await sleep(300);
    // Real mouse events on the map, as a user would cause them.
    const canvas = view.map.getCanvas();
    const box = canvas.getBoundingClientRect();
    for (const place of places) {
      const { x, y } = view.map.project(place);
      for (const type of ["mousedown", "mouseup", "click"]) {
        const init = { clientX: box.left + x, clientY: box.top + y, bubbles: true, button: 0 };
        canvas.dispatchEvent(new MouseEvent(type, init));
      }
      await sleep(100);
    }
    await until("route shown", () => !el("route-result").hidden || text("message").includes("("));
    report.waypoints = document.querySelectorAll(".waypoint-marker").length;
    report.stats = [...el("route-stats").querySelectorAll("div")].map((row) => row.textContent);
    report.chart = el("route-chart").querySelector("path") !== null;
    report.chart_scale = el("route-chart").querySelector(".scale")?.textContent;
    report.note = text("route-note");
    report.name = (el("route-name") as HTMLInputElement).value;
    report.message = text("message");
    if (!el("route-result").hidden) {
      el("route-save").click();
      await until("route saved", () => document.querySelectorAll("#route-list li[data-id]").length > saved);
      report.saved = text("route-list");
    }
    el("route-edit").click();
    report.ok = !el("route-result").hidden;
  } catch (e) {
    report.ok = false;
    report.error = String(e);
    report.message = text("message");
  }
  if (exit) await api.probeReport(report);
  else api.log("info", JSON.stringify(report));
}

/** Saves a picture of the map once everything in view is drawn (OUTDOOR_SNAPSHOT). */
async function snapshot(view: MapView, target: [number, number, number] | null, style: string | null): Promise<void> {
  const map = view.map;
  if (style) {
    const select = document.getElementById("map-style") as HTMLSelectElement;
    select.value = style;
    select.dispatchEvent(new Event("change"));
    await sleep(1000);
  }
  if (target) map.jumpTo({ center: [target[0], target[1]], zoom: target[2] });
  await sleep(500);
  await until("map drawn", () => map.loaded() && map.areTilesLoaded(), 30_000).catch(() => undefined);
  await sleep(1500);
  // The drawing buffer is only readable while a frame is being presented.
  const blob = await new Promise<Blob | null>((resolve) => {
    map.once("render", () => map.getCanvas().toBlob(resolve, "image/png"));
    map.triggerRepaint();
  });
  await api.snapshotSave(blob ? Array.from(new Uint8Array(await blob.arrayBuffer())) : []);
}

export async function runDiagnostics(view: MapView): Promise<void> {
  const shot = await api.snapshotRequest();
  const route = await invoke<string | null>("selftest_route");
  if (route) await routeSelfTest(view, route, shot === null);
  if (shot) await snapshot(view, shot.view, shot.style);
  const catalog = await invoke<string | null>("selftest_regions");
  if (catalog) await regionsSelfTest(view, catalog);
  if (await api.probeEnabled()) await fpsProbe(view);
  const gpx = await invoke<string | null>("selftest_gpx");
  if (gpx) await selfTest(view, gpx);
}
