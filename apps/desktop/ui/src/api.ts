/** Typed access to the Rust core. No logic here: commands in, events out. */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Activity = "bike" | "mtb" | "hike" | "ski_tour";
export const ACTIVITIES: Activity[] = ["bike", "mtb", "hike", "ski_tour"];

export interface Fix {
  lat: number;
  lon: number;
  ele: number | null;
  time_ms: number;
  h_accuracy_m: number | null;
  speed_mps: number | null;
  course_deg: number | null;
}

export interface TrackStats {
  distance_m: number;
  ascent_m: number;
  descent_m: number;
  max_speed_mps: number;
  avg_moving_speed_mps: number;
  avg_total_speed_mps: number;
  total_time_s: number;
  moving_time_s: number;
  min_ele_m: number | null;
  max_ele_m: number | null;
  points_total: number;
  points_used: number;
}

export type Line = [number, number][][];

export interface RecordingStatus {
  phase: "idle" | "recording" | "paused";
  track_id: string | null;
  activity: Activity | null;
  stats: TrackStats | null;
  appended: { segment: number; lon: number; lat: number }[];
}

export interface TrackRecord {
  id: string;
  name: string | null;
  activity: Activity;
  state: "recording" | "paused" | "finished";
  started_at_ms: number | null;
  ended_at_ms: number | null;
  stats: TrackStats | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface RecordingState {
  status: RecordingStatus;
  line: Line;
  interrupted: TrackRecord | null;
}

export interface TrackDetail {
  record: TrackRecord;
  line: Line;
}

export type SourceStatus =
  | { state: "connecting" }
  | { state: "active" }
  | { state: "finished" }
  | { state: "lost"; reason: string };

export type RegionState = "available" | "installed" | "update_available" | "downloading";

export interface RegionInfo {
  id: string;
  name: string;
  /** West, south, east, north in degrees. */
  bbox: [number, number, number, number];
  size: number;
  version: string;
  installed_version: string | null;
  state: RegionState;
}

export interface RegionsOverview {
  source: string;
  catalog_error: string | null;
  regions: RegionInfo[];
  attribution: string[];
  installed_bounds: [number, number, number, number] | null;
}

export interface RegionProgress {
  region: string;
  done: number;
  total: number;
}

export type Profile = "road_bike" | "trekking" | "mtb" | "hike" | "car" | "manual";
export type RouteOptionKey = "use_hills" | "use_roads" | "avoid_bad_surfaces" | "max_hiking_difficulty";

export interface LatLon {
  lat: number;
  lon: number;
}

export interface OptionSpec {
  key: RouteOptionKey;
  min: number;
  max: number;
  default: number;
  step: number;
}

export interface ProfileInfo {
  profile: Profile;
  options: OptionSpec[];
  /** False if this build cannot compute the profile. */
  available: boolean;
}

export type RouteOptions = Partial<Record<RouteOptionKey, number | null>>;

export interface RouteRequest {
  waypoints: LatLon[];
  profile: Profile;
  options: RouteOptions;
}

export interface ProfilePoint {
  distance_m: number;
  lat: number;
  lon: number;
  ele: number | null;
}

export interface RoutePlan {
  route: {
    profile: Profile;
    waypoints: LatLon[];
    line: LatLon[];
    distance_m: number;
    duration_s: number | null;
  };
  options: RouteOptions;
  elevation: ProfilePoint[];
  ascent_m: number | null;
  descent_m: number | null;
  min_ele_m: number | null;
  max_ele_m: number | null;
  hiking_time_s: number | null;
  region: string | null;
  max_mtb_scale: number | null;
}

export interface RouteRecord {
  id: string;
  name: string;
  profile: Profile;
  distance_m: number;
  ascent_m: number | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface SnapshotRequest {
  path: string;
  /** Longitude, latitude and zoom to show, if given. */
  view: [number, number, number] | null;
  /** Value of the style selector to switch to, if given. */
  style: string | null;
}

export interface CommandError {
  code: string;
  message: string;
}

export const api = {
  recordingStart: (activity: Activity) => invoke<RecordingStatus>("recording_start", { activity }),
  recordingPause: () => invoke<RecordingStatus>("recording_pause"),
  recordingResume: () => invoke<RecordingStatus>("recording_resume"),
  recordingStop: () => invoke<TrackRecord | null>("recording_stop"),
  recordingState: () => invoke<RecordingState>("recording_state"),
  recordingContinueInterrupted: () => invoke<RecordingState>("recording_continue_interrupted"),
  recordingFinishInterrupted: () => invoke<TrackRecord | null>("recording_finish_interrupted"),

  tracksList: () => invoke<TrackRecord[]>("tracks_list"),
  trackDetail: (id: string) => invoke<TrackDetail>("track_detail", { id }),
  trackRename: (id: string, name: string) => invoke<void>("track_rename", { id, name }),
  trackDelete: (id: string) => invoke<void>("track_delete", { id }),
  trackImportGpx: (path: string, activity: Activity | null) =>
    invoke<TrackRecord[]>("track_import_gpx", { path, activity }),
  trackExportGpx: (id: string, path: string) => invoke<void>("track_export_gpx", { id, path }),

  locationStartReplay: (path: string, speed: number) => invoke<void>("location_start_replay", { path, speed }),
  locationStartGpsd: () => invoke<void>("location_start_gpsd", { address: null }),
  locationStop: () => invoke<void>("location_stop"),

  regionsOverview: () => invoke<RegionsOverview>("regions_overview"),
  regionsSetSource: (source: string) => invoke<void>("regions_set_source", { source }),
  regionInstall: (id: string) => invoke<unknown>("region_install", { id }),
  regionCancel: (id: string) => invoke<void>("region_cancel", { id }),
  regionDelete: (id: string) => invoke<void>("region_delete", { id }),

  routeProfiles: () => invoke<ProfileInfo[]>("route_profiles"),
  routePlan: (request: RouteRequest) => invoke<RoutePlan>("route_plan", { request }),
  routeClear: () => invoke<void>("route_clear"),
  routeSave: (name: string) => invoke<RouteRecord>("route_save", { name }),
  routesList: () => invoke<RouteRecord[]>("routes_list"),
  routeLoad: (id: string) => invoke<RoutePlan>("route_load", { id }),
  routeDelete: (id: string) => invoke<void>("route_delete", { id }),
  routeExportGpx: (name: string, path: string) => invoke<void>("route_export_gpx", { name, path }),

  snapshotRequest: () => invoke<SnapshotRequest | null>("snapshot_request"),
  snapshotSave: (png: number[]) => invoke<void>("snapshot_save", { png }),
  probeEnabled: () => invoke<boolean>("probe_enabled"),
  probeReport: (report: unknown) => invoke<void>("probe_report", { report }),
  log: (level: "info" | "error", message: string) => void invoke("ui_log", { level, message }),
};

export const events = {
  onFix: (handler: (fix: Fix) => void): Promise<UnlistenFn> =>
    listen<Fix>("location:fix", (e) => handler(e.payload)),
  onRecording: (handler: (status: RecordingStatus) => void): Promise<UnlistenFn> =>
    listen<RecordingStatus>("recording:update", (e) => handler(e.payload)),
  onSourceStatus: (handler: (status: SourceStatus) => void): Promise<UnlistenFn> =>
    listen<SourceStatus>("location:status", (e) => handler(e.payload)),
  onRegionProgress: (handler: (progress: RegionProgress) => void): Promise<UnlistenFn> =>
    listen<RegionProgress>("regions:progress", (e) => handler(e.payload)),
  onError: (handler: (message: string) => void): Promise<UnlistenFn> =>
    listen<string>("app:error", (e) => handler(e.payload)),
};

export function isCommandError(e: unknown): e is CommandError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}
