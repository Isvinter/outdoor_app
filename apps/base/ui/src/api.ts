/** Typed access to the Rust core. No logic here: commands in, events out. */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Activity = "bike" | "mtb" | "hike" | "ski_tour" | "other";
export const ACTIVITIES: Activity[] = ["bike", "mtb", "hike", "ski_tour", "other"];

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
  /** Statistics with elevation from the terrain model, if it covers the track. */
  dem_stats: TrackStats | null;
}

export type SourceStatus =
  | { state: "connecting" }
  | { state: "active" }
  | { state: "finished" }
  | { state: "lost"; reason: string };

export type RegionState = "available" | "installed" | "update_available" | "downloading" | "not_built";

export interface RegionInfo {
  id: string;
  name: string;
  /** West, south, east, north in degrees. */
  bbox: [number, number, number, number];
  size: number;
  version: string;
  installed_version: string | null;
  state: RegionState;
  /** This device has the definition of the region and can build it. */
  buildable: boolean;
  /** The map and routing use the region. */
  enabled?: boolean;
}

export interface RegionsOverview {
  source: string;
  catalog_error: string | null;
  regions: RegionInfo[];
  attribution: string[];
  installed_bounds: [number, number, number, number] | null;
  /** The elevation data in use, one `<id>:<sha256>` per enabled region that has any. */
  dem_regions: string[];
}

export interface RegionProgress {
  region: string;
  done: number;
  total: number;
}

/** West, south, east, north in degrees. */
export type Bbox = [number, number, number, number];

/** A region build started a step; `index` 0 means it is still being prepared. */
export interface StepProgress {
  region: string;
  step: string;
  index: number;
  total: number;
}

export interface BuildStatus {
  /** Codes of what this machine lacks for a build; empty if it can build. */
  missing: string[];
  running: StepProgress | null;
}

export interface BuildPlan {
  width_km: number;
  height_km: number;
  estimated_bytes: number;
  extracts: string[];
}

export interface BuildRequest {
  name: string | null;
  bbox: Bbox | null;
  id: string | null;
  refresh: boolean;
}

export type Profile = "road_bike" | "trekking" | "mtb" | "hike" | "car" | "manual";
export type SearchCategory =
  | "fuel"
  | "groceries"
  | "food"
  | "pharmacy"
  | "lodging"
  | "camping"
  | "parking"
  | "retail"
  | "sights";

export interface SearchHit {
  kind: "place" | "street" | "poi";
  category: SearchCategory | null;
  /** Empty for a POI without a name. */
  name: string;
  town: string | null;
  lat: number;
  lon: number;
  distance_m: number;
}

export interface SearchResults {
  hits: SearchHit[];
  /** Names of the enabled regions that could not be searched. */
  unsearchable: string[];
  /** Nothing lay within the radius, so these are the nearest further away. */
  widened: boolean;
  /** The radius the search used, after its limits. */
  radius_m: number;
}

export interface KnownPosition {
  lat: number;
  lon: number;
  age_s: number;
}

export type RouteStart = "ready" | "no_position" | "other_map";

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

/** How one leg (from waypoint i to i + 1) is to be found. */
export interface LegSpec {
  profile: Profile;
  options: RouteOptions;
}

export interface PlanRequest {
  waypoints: LatLon[];
  /** One less than waypoints. */
  legs: LegSpec[];
  /** Inner waypoints where a stage ends, ascending. */
  stage_ends: number[];
}

export interface ProfilePoint {
  distance_m: number;
  lat: number;
  lon: number;
  ele: number | null;
}

/** A sample of the elevation profile, with the leg it lies on. */
export interface RoutePoint extends ProfilePoint {
  leg: number;
}

export interface LegError {
  code: string;
  message: string;
}

export interface Leg {
  profile: Profile;
  options: RouteOptions;
  /** Empty if the leg failed. */
  line: LatLon[];
  distance_m: number;
  duration_s: number | null;
  ascent_m: number | null;
  descent_m: number | null;
  /** On foot the DIN walking time, otherwise the router's time. */
  time_s: number | null;
  region: string | null;
  max_mtb_scale: number | null;
  error: LegError | null;
}

export interface Stage {
  /** First and last waypoint of the stage. */
  from: number;
  to: number;
  distance_m: number;
  ascent_m: number | null;
  descent_m: number | null;
  time_s: number | null;
  profiles: Profile[];
}

export interface Totals {
  distance_m: number;
  ascent_m: number | null;
  descent_m: number | null;
  time_s: number | null;
  min_ele_m: number | null;
  max_ele_m: number | null;
  /** Legs missing from the totals because they could not be computed. */
  failed_legs: number;
}

export interface RoutePlan {
  format: number;
  request: PlanRequest;
  legs: Leg[];
  elevation: RoutePoint[];
  stages: Stage[];
  total: Totals;
}

export interface RouteRecord {
  id: string;
  name: string;
  /** The profiles of the route joined by "+", e.g. "mtb+hike". */
  profile: string;
  distance_m: number;
  ascent_m: number | null;
  created_at_ms: number;
  updated_at_ms: number;
  /** Counts the changes of the route, starting at 1. */
  rev: number;
}

/** A private IPv4 address of this machine in the home network. */
export interface LocalAddress {
  ip: string;
  interface: string;
  prefix: number;
  /** The address the default route leaves from. */
  default_route: boolean;
}

export interface ShareStatus {
  running: boolean;
  port: number;
  /** Default route first. */
  addresses: LocalAddress[];
  /** The address the URL, QR code and firewall hint are for. */
  selected: string | null;
  url: string | null;
  qr_svg: string | null;
  firewall_hint: string | null;
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
  regionBuildCheck: () => invoke<BuildStatus>("region_build_check"),
  regionBuildPlan: (bbox: Bbox) => invoke<BuildPlan>("region_build_plan", { bbox }),
  regionBuildStart: (request: BuildRequest) => invoke<unknown>("region_build_start", { ...request }),
  regionBuildCancel: () => invoke<void>("region_build_cancel"),

  searchText: (text: string, near: LatLon, radiusM: number, limit: number) =>
    invoke<SearchResults>("search_text", { text, near, radiusM, limit }),
  searchCategory: (category: SearchCategory, near: LatLon, radiusM: number, limit: number) =>
    invoke<SearchResults>("search_category", { category, near, radiusM, limit }),
  routeStartCheck: (position: KnownPosition | null, target: LatLon) =>
    invoke<RouteStart>("route_start_check", { position, target }),
  routeIsUnsaved: (request: PlanRequest) => invoke<boolean>("route_is_unsaved", { request }),

  routeProfiles: () => invoke<ProfileInfo[]>("route_profiles"),
  routePlan: (request: PlanRequest) => invoke<RoutePlan>("route_plan", { request }),
  /** No plan (fewer than two points); "save" still overwrites the loaded route. */
  routeClear: () => invoke<void>("route_clear"),
  /** Starts afresh: the next "save" stores a new route. */
  routeNew: () => invoke<void>("route_new"),
  /** Overwrites the route loaded or saved last; otherwise stores a new one. */
  routeSave: (name: string) => invoke<RouteRecord>("route_save", { name }),
  routeSaveNew: (name: string) => invoke<RouteRecord>("route_save_new", { name }),
  routesList: () => invoke<RouteRecord[]>("routes_list"),
  routeLoad: (id: string) => invoke<RoutePlan>("route_load", { id }),
  routeDelete: (id: string) => invoke<void>("route_delete", { id }),
  routeExportGpx: (name: string, path: string) => invoke<void>("route_export_gpx", { name, path }),

  shareStatus: () => invoke<ShareStatus>("share_status"),
  /** Starts the share (stores a changed port); while it runs, only picks the address shown. */
  shareStart: (port: number, ip: string | null) => invoke<ShareStatus>("share_start", { port, ip }),
  shareStop: () => invoke<void>("share_stop"),

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
  onBuildProgress: (handler: (progress: StepProgress) => void): Promise<UnlistenFn> =>
    listen<StepProgress>("regions:build", (e) => handler(e.payload)),
  onError: (handler: (message: string) => void): Promise<UnlistenFn> =>
    listen<string>("app:error", (e) => handler(e.payload)),
};

export function isCommandError(e: unknown): e is CommandError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}
