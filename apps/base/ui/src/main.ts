/** Wires the page to the core. Rendering and input handling only. */
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  ACTIVITIES,
  api,
  events,
  isCommandError,
  type Activity,
  type RecordingStatus,
  type RegionsOverview,
  type SourceStatus,
  type TrackRecord,
} from "./api";
import { dateTime, distance, elevation, renderRows, renderStats, statsRows } from "./format";
import { t, translateDocument, type MessageKey } from "./i18n";
import { MapView, type BaseStyle } from "./map";
import { RegionsPanel } from "./regions";
import { RoutePanel } from "./route";
import { SearchPanel } from "./search";
import { SharePanel } from "./share";
import { renderLegend } from "./legend";
import { runDiagnostics } from "./diagnostics";
import "./style.css";

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

const view = new MapView("map");
let tracks: TrackRecord[] = [];
let selectedTrack: string | null = null;

// --- messages ---------------------------------------------------------------

function showMessage(text: string, kind: "info" | "error" = "info"): void {
  const el = $("message");
  el.textContent = text;
  el.className = kind;
}

function showError(e: unknown): void {
  if (isCommandError(e)) {
    const key = `error.${e.code}` as MessageKey;
    const known = key in errorKeys;
    showMessage(`${known ? t(key) : t("error.unknown")} (${e.message})`, "error");
    api.log("error", `${e.code}: ${e.message}`);
  } else {
    showMessage(`${t("error.unknown")} (${String(e)})`, "error");
    api.log("error", String(e));
  }
}

const errorKeys: Partial<Record<MessageKey, true>> = {
  "error.storage": true,
  "error.gpx": true,
  "error.track": true,
  "error.location": true,
  "error.no_recording": true,
  "error.recording_active": true,
  "error.nothing_interrupted": true,
  "error.activity_required": true,
  "error.io": true,
  "error.no_catalog": true,
  "error.region_io": true,
  "error.build_prerequisites": true,
  "error.build_area": true,
  "error.build_no_extract": true,
  "error.build_index": true,
  "error.build_failed": true,
  "error.build_cancelled": true,
  "error.build_busy": true,
  "error.build_unknown_region": true,
  "error.build_io": true,
  "error.region_archive": true,
  "error.region_manifest": true,
  "error.region_download": true,
  "error.region_checksum": true,
  "error.region_unknown": true,
  "error.region_not_installed": true,
  "error.region_busy": true,
  "error.region_template": true,
  "error.route_too_few_waypoints": true,
  "error.route_not_found": true,
  "error.routing_unavailable": true,
  "error.routing": true,
  "error.route_plan": true,
  "error.no_route_planned": true,
  "error.share": true,
};

/** Runs a user action and reports failures in the message line. */
function action(run: () => Promise<void>): () => void {
  return () => {
    showMessage("");
    run().catch(showError);
  };
}

// --- recording --------------------------------------------------------------

function selectedActivity(): Activity {
  return $<HTMLSelectElement>("activity").value as Activity;
}

function renderRecording(status: RecordingStatus): void {
  const { phase } = status;
  $("rec-start").hidden = phase !== "idle";
  $("rec-pause").hidden = phase !== "recording";
  $("rec-resume").hidden = phase !== "paused";
  $("rec-stop").hidden = phase === "idle";
  $<HTMLSelectElement>("activity").disabled = phase !== "idle";
  if (status.activity) $<HTMLSelectElement>("activity").value = status.activity;
  $("rec-phase").textContent = t(`recording.phase.${phase}`);
  renderStats($("live-stats"), status.stats);
}

function renderRecovery(interrupted: TrackRecord | null): void {
  $("recovery").hidden = interrupted === null;
  if (interrupted) {
    $("recovery-text").textContent = t("recovery.text", { date: dateTime(interrupted.created_at_ms) });
  }
}

async function loadRecordingState(): Promise<void> {
  const state = await api.recordingState();
  view.setLiveLine(state.line);
  renderRecording(state.status);
  renderRecovery(state.interrupted);
}

// --- tracks -----------------------------------------------------------------

function trackTitle(track: TrackRecord): string {
  return track.name ?? t("tracks.unnamed", { date: dateTime(track.started_at_ms ?? track.created_at_ms) });
}

function renderTrackList(): void {
  const list = $("track-list");
  if (tracks.length === 0) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = t("tracks.empty");
    list.replaceChildren(empty);
    return;
  }
  list.replaceChildren(
    ...tracks.map((track) => {
      const item = document.createElement("li");
      item.dataset.id = track.id;
      item.classList.toggle("selected", track.id === selectedTrack);
      const title = document.createElement("span");
      title.textContent = trackTitle(track);
      const meta = document.createElement("span");
      meta.className = "meta";
      meta.textContent = `${t(`activity.${track.activity}`)} · ${track.stats ? distance(track.stats.distance_m) : "–"}`;
      item.append(title, meta);
      item.addEventListener("click", action(() => selectTrack(track.id)));
      return item;
    }),
  );
}

async function refreshTracks(): Promise<void> {
  tracks = (await api.tracksList()).filter((track) => track.state === "finished");
  if (selectedTrack && !tracks.some((track) => track.id === selectedTrack)) await selectTrack(null);
  renderTrackList();
}

async function selectTrack(id: string | null): Promise<void> {
  selectedTrack = id;
  renderTrackList();
  $("track-detail").hidden = id === null;
  if (id === null) {
    view.showTrack([]);
    return;
  }
  const detail = await api.trackDetail(id);
  $<HTMLInputElement>("track-name").value = trackTitle(detail.record);
  const rows = detail.record.stats ? statsRows(detail.record.stats) : [];
  const dem = detail.dem_stats;
  if (dem) {
    rows.push(
      ["stats.ascent_dem", elevation(dem.ascent_m)],
      ["stats.descent_dem", elevation(dem.descent_m)],
      ["stats.elevation_dem", `${elevation(dem.min_ele_m)} / ${elevation(dem.max_ele_m)}`],
    );
  }
  renderRows($("track-stats"), rows);
  $("track-dem-hint").textContent = dem ? t("tracks.dem_hint") : "";
  view.showTrack(detail.line);
}

// --- location ---------------------------------------------------------------

function renderSourceStatus(status: SourceStatus | null): void {
  route.setSourceStatus(status);
  const text =
    status === null
      ? t("location.status.off")
      : status.state === "lost"
        ? t("location.status.lost", { reason: status.reason })
        : t(`location.status.${status.state}`);
  $("loc-status").textContent = text;
  // Stopping only makes sense while a source is running (or retrying).
  $<HTMLButtonElement>("loc-stop").disabled = status === null || status.state === "finished";
}

const gpxFilter = () => [{ name: t("dialog.gpx"), extensions: ["gpx"] }];

// --- maps -------------------------------------------------------------------

const STYLE_KEY = "outdoor.map-style";
let overview: RegionsOverview | null = null;

function selectedStyle(): BaseStyle {
  const value = $<HTMLSelectElement>("map-style").value;
  return value === "online" || value === "winter" ? value : "topo";
}

/** Applies the chosen base map; the offline style depends on what is installed. */
function applyStyle(): void {
  const base = selectedStyle();
  localStorage.setItem(STYLE_KEY, base);
  $("slope-controls").hidden = base !== "winter";
  renderLegend($("legend-list"), base);
  view.setBaseStyle(base, overview).catch(showError);
  if (base !== "online" && overview && overview.installed_bounds === null) showMessage(t("maps.no_region"));
}

/** The map style chosen before the area selection switched to the online map. */
let styleBeforeSelection: BaseStyle | null = null;

const regions = new RegionsPanel(view, {
  onOverview: (next) => {
    const changed =
      JSON.stringify(next.installed_bounds) !== JSON.stringify(overview?.installed_bounds) ||
      JSON.stringify(next.dem_regions) !== JSON.stringify(overview?.dem_regions);
    const first = overview === null;
    overview = next;
    $("about-attribution").replaceChildren(
      ...next.attribution.map((line) => {
        const item = document.createElement("li");
        item.textContent = line;
        return item;
      }),
    );
    // Reload the offline style when regions were added or removed.
    if (first || (changed && selectedStyle() !== "online")) applyStyle();
  },
  showRegion: (region) => view.showBounds(region.bbox),
  // Outside the installed regions the offline styles show nothing to choose from.
  useOnlineMap: (on) => {
    const select = $<HTMLSelectElement>("map-style");
    if (on && styleBeforeSelection === null) {
      styleBeforeSelection = selectedStyle();
      if (styleBeforeSelection === "online") return;
      select.value = "online";
      applyStyle();
    } else if (!on && styleBeforeSelection !== null) {
      const previous = styleBeforeSelection;
      styleBeforeSelection = null;
      if (select.value === "online" && previous !== "online") {
        select.value = previous;
        applyStyle();
      }
    }
  },
  showMessage: (text) => showMessage(text),
  showError,
});

const route = new RoutePanel(view, {
  showMessage: (text) => showMessage(text),
  showError,
  routeHereDone: () => search.routeDone(),
});
const search = new SearchPanel(view, {
  routeHere: (hit) => void route.routeHere({ lat: hit.lat, lon: hit.lon }),
  selectionCleared: () => route.cancelRouteHere(),
  showError,
});
const share = new SharePanel({ showMessage: (text) => showMessage(text), showError });

// --- wiring -----------------------------------------------------------------

function fillSelect(select: HTMLSelectElement, options: [string, string][]): void {
  select.replaceChildren(
    ...options.map(([value, label]) => {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = label;
      return option;
    }),
  );
}

async function start(): Promise<void> {
  translateDocument();
  fillSelect(
    $<HTMLSelectElement>("activity"),
    ACTIVITIES.map((a) => [a, t(`activity.${a}`)]),
  );
  fillSelect(
    $<HTMLSelectElement>("loc-speed"),
    ["1", "5", "20", "100"].map((s) => [s, `${s}×`]),
  );
  $<HTMLSelectElement>("loc-speed").value = "20";
  renderSourceStatus(null);
  fillSelect($<HTMLSelectElement>("map-style"), [
    ["topo", t("maps.style.topo")],
    ["winter", t("maps.style.winter")],
    ["online", t("maps.style.online")],
  ]);
  const savedStyle = localStorage.getItem(STYLE_KEY);
  $<HTMLSelectElement>("map-style").value = savedStyle === "online" || savedStyle === "winter" ? savedStyle : "topo";
  $<HTMLInputElement>("slope-opacity").addEventListener("input", (e) => {
    view.setSlopeOpacity(Number((e.target as HTMLInputElement).value) / 100);
  });
  $<HTMLSelectElement>("map-style").addEventListener("change", applyStyle);

  window.addEventListener("error", (e) => api.log("error", `${e.message} (${e.filename}:${e.lineno})`));
  window.addEventListener("unhandledrejection", (e) => api.log("error", `unhandled: ${String(e.reason)}`));
  view.map.on("error", (e) => api.log("error", `map: ${e.error?.message ?? String(e)}`));

  await events.onFix((fix) => {
    view.showPosition(fix);
    route.setPosition(fix);
  });
  await events.onRecording((status) => {
    view.appendLive(status.appended);
    renderRecording(status);
  });
  await events.onSourceStatus(renderSourceStatus);
  await events.onError((message) => showMessage(message, "error"));
  await events.onRegionProgress((progress) => regions.onProgress(progress));
  await events.onBuildProgress((progress) => regions.onBuildProgress(progress));

  $("rec-start").addEventListener(
    "click",
    action(async () => {
      view.setLiveLine([]);
      renderRecording(await api.recordingStart(selectedActivity()));
      // Starting anew finishes an interrupted recording; it is a track now.
      renderRecovery(null);
      await refreshTracks();
    }),
  );
  $("rec-pause").addEventListener("click", action(async () => renderRecording(await api.recordingPause())));
  $("rec-resume").addEventListener("click", action(async () => renderRecording(await api.recordingResume())));
  $("rec-stop").addEventListener(
    "click",
    action(async () => {
      const record = await api.recordingStop();
      view.setLiveLine([]);
      await refreshTracks();
      if (record) {
        showMessage(t("recording.saved"));
        await selectTrack(record.id);
      } else {
        showMessage(t("recording.discarded"));
      }
    }),
  );

  $("recovery-continue").addEventListener(
    "click",
    action(async () => {
      const state = await api.recordingContinueInterrupted();
      view.setLiveLine(state.line);
      renderRecording(state.status);
      renderRecovery(null);
    }),
  );
  $("recovery-finish").addEventListener(
    "click",
    action(async () => {
      const record = await api.recordingFinishInterrupted();
      renderRecovery(null);
      await refreshTracks();
      if (record) await selectTrack(record.id);
    }),
  );

  $("loc-replay").addEventListener(
    "click",
    action(async () => {
      const path = await open({ multiple: false, filters: gpxFilter() });
      if (typeof path !== "string") return;
      await api.locationStartReplay(path, Number($<HTMLSelectElement>("loc-speed").value));
    }),
  );
  $("loc-gpsd").addEventListener("click", action(() => api.locationStartGpsd()));
  $("loc-stop").addEventListener(
    "click",
    action(async () => {
      await api.locationStop();
      renderSourceStatus(null);
    }),
  );
  $<HTMLInputElement>("follow").addEventListener("change", (e) => {
    view.followPosition = (e.target as HTMLInputElement).checked;
  });

  $("track-import").addEventListener(
    "click",
    action(async () => {
      const path = await open({ multiple: false, filters: gpxFilter() });
      if (typeof path !== "string") return;
      let imported: TrackRecord[];
      try {
        imported = await api.trackImportGpx(path, null);
        showMessage(t("tracks.imported", { count: imported.length }));
      } catch (e) {
        if (!isCommandError(e) || e.code !== "activity_required") throw e;
        const activity = selectedActivity();
        imported = await api.trackImportGpx(path, activity);
        showMessage(t("tracks.imported_as", { activity: t(`activity.${activity}`) }));
      }
      await refreshTracks();
      if (imported[0]) await selectTrack(imported[0].id);
    }),
  );
  $("track-export").addEventListener(
    "click",
    action(async () => {
      if (!selectedTrack) return;
      const name = $<HTMLInputElement>("track-name").value.replace(/[^\p{L}\p{N} ._-]/gu, "_");
      const path = await save({ defaultPath: `${name}.gpx`, filters: gpxFilter() });
      if (!path) return;
      await api.trackExportGpx(selectedTrack, path);
      showMessage(t("tracks.exported", { path }));
    }),
  );
  $("track-delete").addEventListener(
    "click",
    action(async () => {
      if (!selectedTrack) return;
      await api.trackDelete(selectedTrack);
      await selectTrack(null);
      await refreshTracks();
    }),
  );
  $<HTMLInputElement>("track-name").addEventListener(
    "change",
    action(async () => {
      const name = $<HTMLInputElement>("track-name").value.trim();
      if (!selectedTrack || name === "") return;
      await api.trackRename(selectedTrack, name);
      await refreshTracks();
    }),
  );

  const initial = await regions.refresh();
  if (initial?.installed_bounds) view.showBounds(initial.installed_bounds);
  await route.start();
  search.start();
  await share.refresh();
  await loadRecordingState();
  await refreshTracks();
  await runDiagnostics(view, route);
}

start().catch(showError);
