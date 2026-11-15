/**
 * Developer diagnostics, inactive unless requested through environment
 * variables of the host process (see README):
 *  - OUTDOOR_FPS_PROBE: animate the map, report frame timing, exit.
 *  - OUTDOOR_STARTUP_PROBE: report when the page was ready and the map drawn, exit.
 *  - OUTDOOR_SELFTEST_SPEED=<factor>: replay speed of OUTDOOR_SELFTEST (default 100).
 *  - OUTDOOR_SNAPSHOT=<png file>: save a picture of the map, exit.
 *  - OUTDOOR_SELFTEST_REGIONS=<catalog>: download, show and delete the first
 *    region of a catalog through the real controls, report, exit.
 *  - OUTDOOR_SELFTEST_BUILD=<west>,<south>,<east>,<north>;<name>: choose the
 *    area on the map, build it as a region through the real controls, report,
 *    exit.
 *  - OUTDOOR_SELFTEST_ROUTE=<lat>,<lon>;<lat>,<lon>[;…]: plan a hike through
 *    these points with the real controls, save it, report, exit. Together with
 *    OUTDOOR_SNAPSHOT the picture shows the route.
 *  - OUTDOOR_SELFTEST=<gpx file>: drive the real controls through a recording
 *    with a simulated receiver, report what the page shows, exit.
 */
import { invoke } from "@tauri-apps/api/core";
import { LngLat, LngLatBounds } from "maplibre-gl";
import { api, events, type Bbox } from "./api";
import { t } from "./i18n";
import type { MapView } from "./map";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function until(what: string, condition: () => boolean, timeoutMs = 15_000): Promise<void> {
  const start = performance.now();
  while (!condition()) {
    if (performance.now() - start > timeoutMs) throw new Error(`timed out waiting for ${what}`);
    await sleep(50);
  }
}

/** Things in the sidebar that stick out of their section or lie on top of the next one. */
function layoutProblems(): string[] {
  const problems: string[] = [];
  const blocks = [...document.querySelectorAll<HTMLElement>("#sidebar > *")].filter((el) => el.offsetParent !== null);
  blocks.forEach((block, i) => {
    const box = block.getBoundingClientRect();
    for (const el of block.querySelectorAll<HTMLElement>("*")) {
      const inner = el.getBoundingClientRect();
      if (inner.width === 0 || inner.height === 0) continue;
      // What a closed <details> holds is not shown, whatever its box says.
      if (el.tagName !== "SUMMARY" && el.closest("details:not([open])")) continue;
      const name = el.id || el.className || el.tagName;
      if (inner.right > box.right + 0.5 || inner.left < box.left - 0.5) problems.push(`${name} wider than its section`);
      if (inner.bottom > box.bottom + 0.5) problems.push(`${name} below its section`);
    }
    const next = blocks[i + 1]?.getBoundingClientRect();
    if (next && box.bottom > next.top + 0.5) problems.push(`${block.id || block.tagName} overlaps the next block`);
  });
  return [...new Set(problems)];
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
    await api.locationStartReplay(gpxPath, (await invoke<number | null>("selftest_speed")) ?? 100);
    if (!report.recovery_offered) click("rec-start");
    await until("recording phase", () => !el("rec-pause").hidden);
    const atStart = view.liveLinePointCount();
    await until("live points", () => view.liveLinePointCount() > atStart + 100, 600_000);
    report.marker_on_map = document.querySelector(".position-marker") !== null;
    report.source_status = text("loc-status");

    click("rec-pause");
    await until("paused", () => !el("rec-resume").hidden);
    const atPause = view.liveLinePointCount();
    await sleep(500);
    report.points_added_while_paused = view.liveLinePointCount() - atPause;
    click("rec-resume");
    await until("resumed", () => !el("rec-pause").hidden);
    await until("more live points", () => view.liveLinePointCount() > atPause + 100, 600_000);

    report.live_points = view.liveLinePointCount();
    report.live_stats = text("live-stats");
    click("rec-stop");
    await until("track in list", () => document.querySelectorAll("#track-list li[data-id]").length > tracksBefore);
    await until("track detail", () => !el("track-detail").hidden && text("track-stats") !== "");
    report.phase_after_stop = text("rec-phase");
    report.tracks_listed = document.querySelectorAll("#track-list li[data-id]").length;
    report.first_track = text("track-list").slice(0, 80);
    report.track_stats = text("track-stats");
    report.layout_problems = layoutProblems();
    report.legend_entries = document.querySelectorAll("#legend-list li").length;
    report.attribution_lines = document.querySelectorAll("#about-attribution li").length;
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

/**
 * Chooses `bbox` with real mouse events, as a user would: a drag first (must
 * move the map, not set a corner), two corner clicks, then moving the finished
 * rectangle by dragging inside it and back. Notes what happened in `report`.
 */
async function chooseArea(view: MapView, bbox: Bbox, report: Record<string, unknown>): Promise<void> {
  const map = view.map;
  const canvas = map.getCanvas();
  const mouse = (type: string, x: number, y: number) => {
    const box = canvas.getBoundingClientRect();
    const init = { clientX: box.left + x, clientY: box.top + y, bubbles: true, button: 0, buttons: type === "mouseup" || type === "click" ? 0 : 1 };
    canvas.dispatchEvent(new MouseEvent(type, init));
  };
  const drag = async (from: [number, number], to: [number, number]) => {
    mouse("mousedown", ...from);
    for (let i = 1; i <= 5; i++) {
      mouse("mousemove", from[0] + ((to[0] - from[0]) * i) / 5, from[1] + ((to[1] - from[1]) * i) / 5);
      await sleep(30);
    }
    mouse("mouseup", ...to);
    mouse("click", ...to);
    await sleep(300);
  };
  const click = async ([x, y]: [number, number]) => {
    mouse("mousedown", x, y);
    mouse("mouseup", x, y);
    mouse("click", x, y);
    await sleep(150);
  };
  const point = (lng: number, lat: number): [number, number] => {
    const { x, y } = map.project([lng, lat]);
    return [x, y];
  };
  const [w, s, e, n] = bbox;

  map.jumpTo({ center: [(w + e) / 2, (s + n) / 2] });
  const before = map.getCenter();
  await drag([200, 200], [300, 260]);
  report.drag_moved_map = map.getCenter().distanceTo(before) > 1;
  report.drag_set_corner = view.isDrawingArea() || view.currentArea() !== null;

  map.fitBounds([w, s, e, n], { padding: 60, duration: 0 });
  await sleep(300);
  await click(point(w, n));
  report.after_first_click = text("build-hint");
  await click(point(e, s));
  const drawn = view.currentArea();
  report.drawn_area = drawn?.map((v) => v.toFixed(3));

  const centre = point((w + e) / 2, (s + n) / 2);
  const mapBefore = map.getCenter();
  await drag(centre, [centre[0] + 40, centre[1]]);
  const moved = view.currentArea();
  report.area_moved = drawn !== null && moved !== null && moved[0] > drawn[0] && moved[2] > drawn[2];
  report.map_kept_still = map.getCenter().distanceTo(mapBefore) < 1;
  await drag([centre[0] + 40, centre[1]], centre);
  report.hint_with_area = text("build-hint");
}

function text(id: string): string {
  return document.getElementById(id)?.textContent ?? "";
}

async function buildSelfTest(view: MapView, spec: string): Promise<void> {
  const el = (id: string) => document.getElementById(id) as HTMLElement;
  const text = (id: string) => el(id).textContent ?? "";
  const report: Record<string, unknown> = {};
  const steps: string[] = [];
  let id = "";
  const unlisten = await events.onBuildProgress((progress) => {
    id = progress.region;
    if (progress.index > 0) steps.push(`${progress.index}/${progress.total} ${progress.step}`);
  });
  // The id is made from the name; the list shows the name of the built package.
  const row = () => (id ? document.querySelector<HTMLElement>(`#region-list li[data-id="${id}"]`) : null) ?? undefined;
  try {
    const [box, name] = spec.split(";");
    const bbox = (box ?? "").split(",").map(Number) as Bbox;
    if (bbox.length !== 4 || bbox.some(Number.isNaN) || !name) throw new Error("expected <w>,<s>,<e>,<n>;<name>");
    await until("build status", () => !(el("build-pick") as HTMLButtonElement).disabled || text("build-hint") !== "");
    report.hint_before = text("build-hint");
    el("build-pick").click();
    await chooseArea(view, bbox, report);
    const input = el("build-name") as HTMLInputElement;
    input.value = name;
    input.dispatchEvent(new Event("input"));
    await until("estimate", () => text("build-estimate").startsWith("ca."), 60_000);
    report.estimate = text("build-estimate");
    report.layout_problems = layoutProblems();
    el("build-start").click();
    await until("build started", () => row() !== undefined, 60_000);
    // A real build of a large area takes a long time; a failed one ends the test at once.
    const installed = () => row()?.querySelector(".meta")?.textContent?.includes("installiert") ?? false;
    const ended = () => [t("error.build_failed"), t("maps.build.cancelled")].some((s) => text("message").startsWith(s));
    await until("region built", () => installed() || ended(), 4 * 3600_000);
    if (!installed()) throw new Error("build ended without an installed region");
    report.id = row()?.dataset.id;
    report.meta = row()?.querySelector(".meta")?.textContent;
    report.buttons = [...(row()?.querySelectorAll("button") ?? [])].map((b) => b.textContent);
    report.message = text("message");
    report.layout_problems_after = layoutProblems();
    report.ok = true;
  } catch (e) {
    report.ok = false;
    report.error = String(e);
    report.message = text("message");
    report.hint = text("build-hint");
    report.estimate = text("build-estimate");
  }
  unlisten();
  report.steps_seen = steps;
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
    const clickAt = async (place: LngLat) => {
      const { x, y } = view.map.project(place);
      for (const type of ["mousedown", "mouseup", "click"]) {
        const init = { clientX: box.left + x, clientY: box.top + y, bubbles: true, button: 0 };
        canvas.dispatchEvent(new MouseEvent(type, init));
      }
      await sleep(100);
    };
    // With four or more points: all but the last on foot, the last leg by
    // mountain bike, a stage ending at the second point, then one point undone.
    const legs = places.length >= 4;
    for (const place of legs ? places.slice(0, -1) : places) await clickAt(place);
    if (legs) {
      await until("hiking legs drawn", () => view.routeColors().length === places.length - 2);
      const select = el("route-profile") as HTMLSelectElement;
      select.value = "mtb";
      select.dispatchEvent(new Event("change"));
      const last = places[places.length - 1];
      if (last) await clickAt(last);
      await until("all legs drawn", () => view.routeColors().length === places.length - 1);
      const second = document.querySelectorAll<HTMLElement>(".waypoint-marker")[1];
      second?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await until("waypoint menu", () => document.querySelector(".waypoint-menu") !== null);
      report.menu = [...document.querySelectorAll(".waypoint-menu button")].map((b) => b.textContent);
      document.querySelector<HTMLElement>(".waypoint-menu .stage-toggle")?.click();
      await until("stages shown", () => !el("route-stages").hidden);
      await sleep(300);
      report.leg_colors = view.routeColors();
      report.stage_markers = document.querySelectorAll(".waypoint-marker.stage-end").length;
      report.stages = [...el("route-stages").querySelectorAll("li")].map((row) => row.textContent);
      report.legend = [...el("route-legend").querySelectorAll("span > span")].map((s) => s.parentElement?.textContent);
      report.chart_areas = el("route-chart").querySelectorAll("path").length;
      report.chart_stage_lines = el("route-chart").querySelectorAll("line.stage").length;
      report.stats_with_legs = [...el("route-stats").querySelectorAll("div")].map((row) => row.textContent);
      report.layout_problems_legs = layoutProblems();
      el("route-undo").click();
      await until("last leg undone", () => view.routeColors().length === places.length - 2);
      report.after_undo = { colors: view.routeColors(), waypoints: document.querySelectorAll(".waypoint-marker").length };
    }
    await until("route shown", () => !el("route-result").hidden || text("message").includes("("));
    report.waypoints = document.querySelectorAll(".waypoint-marker").length;
    report.stats = [...el("route-stats").querySelectorAll("div")].map((row) => row.textContent);
    report.chart = el("route-chart").querySelector("path") !== null;
    report.chart_scale = el("route-chart").querySelector(".scale")?.textContent;
    report.note = text("route-note");
    report.name = (el("route-name") as HTMLInputElement).value;
    report.layout_problems = layoutProblems();
    // Save what the panel shows, before the timing below plans another route.
    if (!el("route-result").hidden) {
      el("route-save").click();
      await until("route saved", () => document.querySelectorAll("#route-list li[data-id]").length > saved);
      report.saved = text("route-list");
    }
    // How long the core takes for the same route, without the page around it.
    const request = {
      waypoints: places.map((place) => ({ lat: place.lat, lon: place.lng })),
      legs: places.slice(1).map(() => ({ profile: "hike" as const, options: {} })),
      stage_ends: [] as number[],
    };
    const times: number[] = [];
    for (let i = 0; i < 5; i++) {
      const before = performance.now();
      await api.routePlan(request);
      times.push(Math.round(performance.now() - before));
    }
    report.plan_ms = times;
    report.message = text("message");
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

async function startupProbe(view: MapView): Promise<void> {
  const ready = performance.now();
  const map = view.map;
  await until("map drawn", () => map.loaded() && map.areTilesLoaded(), 30_000).catch(() => undefined);
  await api.probeReport({ page_ready_ms: Math.round(ready), map_drawn_ms: Math.round(performance.now()) });
}

export async function runDiagnostics(view: MapView): Promise<void> {
  if (await invoke<boolean>("startup_probe")) await startupProbe(view);
  const shot = await api.snapshotRequest();
  const route = await invoke<string | null>("selftest_route");
  if (route) await routeSelfTest(view, route, shot === null);
  if (shot) await snapshot(view, shot.view, shot.style);
  const build = await invoke<string | null>("selftest_build");
  if (build) await buildSelfTest(view, build);
  const catalog = await invoke<string | null>("selftest_regions");
  if (catalog) await regionsSelfTest(view, catalog);
  if (await api.probeEnabled()) await fpsProbe(view);
  const gpx = await invoke<string | null>("selftest_gpx");
  if (gpx) await selfTest(view, gpx);
}
