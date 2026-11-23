/**
 * The "Route planen" panel: profile, settings, waypoints, result, saved routes.
 * Input and rendering only; routes and their numbers come from the core.
 *
 * A route is a list of waypoints with one leg between each two of them; every
 * leg has its own profile and settings. The form shows and changes the last
 * leg, and new legs take what the form shows.
 */
import { save } from "@tauri-apps/plugin-dialog";
import {
  api,
  type Fix,
  type KnownPosition,
  type LatLon,
  type LegSpec,
  type Profile,
  type ProfileInfo,
  type RouteOptionKey,
  type RouteOptions,
  type RoutePlan,
  type RouteRecord,
  type SourceStatus,
} from "./api";
import { distance, elevation, hours, renderRows } from "./format";
import { t, type MessageKey } from "./i18n";
import type { MapView } from "./map";
import { renderProfileChart } from "./profile-chart";
import { FAILED_COLOR, PROFILE_COLORS } from "./profile-colors";

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

/** Profiles in the order they are offered. */
const ORDER: Profile[] = ["hike", "mtb", "trekking", "road_bike", "car", "manual"];
const BIKES: Profile[] = ["mtb", "trekking", "road_bike"];
/** The profile "Route hierher" uses: the last one chosen, never the manual one. */
const LAST_PROFILE_KEY = "outdoor.last-route-profile";
/** How long "Route hierher" waits for a first position. */
const POSITION_WAIT_MS = 30_000;
/** Errors of a leg that a straight line can stand in for, and what they tell the user. */
const STRAIGHT_TEXTS: Record<string, MessageKey> = {
  routing_unavailable: "route.straight.no_data",
  route_not_found: "route.straight.no_route",
};

export interface RoutePanelHost {
  showMessage(text: string): void;
  showError(error: unknown): void;
  /** A route to the search result is on the map. */
  routeHereDone(): void;
}

/** Where "Route hierher" stands while it needs something from the user. */
type RouteHereStep = "idle" | "choosing" | "waiting" | "tap";

export class RoutePanel {
  private profiles: ProfileInfo[] = [];
  private waypoints: LatLon[] = [];
  /** Leg i runs from waypoint i to i + 1. */
  private legs: LegSpec[] = [];
  /** Inner waypoints where a stage ends, ascending. */
  private stageEnds: number[] = [];
  /** Settings shown in the form: those of the last leg and of new legs. */
  private options: RouteOptions = {};
  private picking = false;
  private plan: RoutePlan | null = null;
  /** Counts requests so that a slow answer cannot replace a newer one. */
  private request = 0;
  /** The last position, and when it arrived: a replay has old time stamps. */
  private position: { lat: number; lon: number; receivedMs: number } | null = null;
  private source: SourceStatus | null = null;
  private hereStep: RouteHereStep = "idle";
  private hereTarget: LatLon | null = null;
  /** Counts the runs of "Route hierher": an answer of an older one is dropped. */
  private hereRun = 0;
  private hereTimer: number | undefined;

  constructor(
    private readonly view: MapView,
    private readonly host: RoutePanelHost,
  ) {}

  async start(): Promise<void> {
    this.profiles = await api.routeProfiles();
    const select = $<HTMLSelectElement>("route-profile");
    select.replaceChildren(
      ...ORDER.flatMap((id) => {
        const info = this.profiles.find((p) => p.profile === id);
        if (!info) return [];
        const option = document.createElement("option");
        option.value = id;
        const name = t(`route.profile.${id}`);
        option.textContent = info.available ? name : t("route.profile.not_built", { name });
        option.disabled = !info.available;
        return [option];
      }),
    );
    select.value = this.profiles.find((p) => p.profile === "hike")?.available ? "hike" : "manual";
    select.addEventListener("change", () => {
      this.options = {};
      if (this.profile() !== "manual") localStorage.setItem(LAST_PROFILE_KEY, this.profile());
      this.updateLastLeg();
      this.renderOptions();
      this.renderControls();
    });
    $("route-straight-button").addEventListener("click", () => this.connectStraight());
    $("route-edit").addEventListener("click", () => this.setPicking(!this.picking));
    $("route-undo").addEventListener("click", () => this.removeWaypoint(this.waypoints.length - 1));
    $("route-clear").addEventListener("click", () => {
      this.setPicking(false);
      delete $<HTMLInputElement>("route-name").dataset.auto;
      this.waypoints = [];
      this.legs = [];
      this.stageEnds = [];
      void api.routeNew().catch(this.host.showError);
      this.changed();
    });
    $("route-save").addEventListener("click", () => void this.save(false));
    $("route-save-new").addEventListener("click", () => void this.save(true));
    $("route-export").addEventListener("click", () => void this.export());
    this.view.onClick((at) => {
      if (this.hereStep === "tap") this.tappedStart(at);
      else if (this.picking) this.addWaypoint(at);
    });
    this.renderOptions();
    this.renderControls();
    this.renderPlan();
    await this.refreshList();
  }

  // --- "Route hierher" -------------------------------------------------------

  /** A new position; "Route hierher" may be waiting for it. */
  setPosition(fix: Fix): void {
    this.position = { lat: fix.lat, lon: fix.lon, receivedMs: Date.now() };
    if (this.hereStep === "waiting") void this.startFromPosition(this.hereRun);
  }

  /** The position source changed; without one there is nothing to wait for. */
  setSourceStatus(status: SourceStatus | null): void {
    this.source = status;
    if (this.hereStep === "waiting" && !this.sourceRunning()) this.askForStart(false);
  }

  private sourceRunning(): boolean {
    return this.source?.state === "connecting" || this.source?.state === "active";
  }

  private knownPosition(): KnownPosition | null {
    if (!this.position) return null;
    const { lat, lon, receivedMs } = this.position;
    return { lat, lon, age_s: (Date.now() - receivedMs) / 1000 };
  }

  /** Starts a route to `target`, asking first if a route is open. */
  async routeHere(target: LatLon): Promise<void> {
    this.cancelRouteHere();
    this.hereTarget = target;
    const run = this.hereRun;
    if (this.waypoints.length === 0) {
      await this.startFromPosition(run);
      return;
    }
    let unsaved = true;
    try {
      unsaved = await api.routeIsUnsaved({ waypoints: this.waypoints, legs: this.legs, stage_ends: this.stageEnds });
    } catch (e) {
      this.host.showError(e);
    }
    if (run !== this.hereRun) return;
    this.hereStep = "choosing";
    this.renderHere(unsaved ? t("route.here.unsaved") : t("route.here.title"), [
      ["new", "route.here.new", true, () => void this.startFromPosition(run)],
      ["append", "route.here.append", false, () => this.appendTarget()],
      ["cancel", "route.here.cancel", false, () => this.cancelRouteHere()],
    ]);
  }

  /** Ends "Route hierher" where it stands; later answers and positions do nothing. */
  cancelRouteHere(): void {
    this.hereRun++;
    window.clearTimeout(this.hereTimer);
    if (this.hereStep === "tap") this.view.setPicking(this.picking);
    this.hereStep = "idle";
    this.hereTarget = null;
    $("route-here").hidden = true;
  }

  private async startFromPosition(run: number): Promise<void> {
    const target = this.hereTarget;
    if (!target) return;
    const position = this.knownPosition();
    let start;
    try {
      start = await api.routeStartCheck(position, target);
    } catch (e) {
      if (run === this.hereRun) this.host.showError(e);
      return;
    }
    // The user chose the start meanwhile: a position no longer decides it.
    if (run !== this.hereRun || this.hereStep === "tap") return;
    if (start === "ready" && position) await this.replaceRoute({ lat: position.lat, lon: position.lon }, target);
    else if (start === "other_map") this.askForStart(true);
    else if (this.sourceRunning()) this.waitForPosition(run);
    else this.askForStart(false);
  }

  private waitForPosition(run: number): void {
    if (this.hereStep === "waiting") return;
    this.hereStep = "waiting";
    this.hereTimer = window.setTimeout(() => {
      if (run === this.hereRun) this.askForStart(false);
    }, POSITION_WAIT_MS);
    this.renderHere(t("route.here.waiting"), [
      ["choose-start", "route.here.choose_start", true, () => this.askForStart(false)],
      ["cancel", "route.here.cancel", false, () => this.cancelRouteHere()],
    ]);
  }

  private askForStart(otherMap: boolean): void {
    window.clearTimeout(this.hereTimer);
    this.setPicking(false);
    this.hereStep = "tap";
    this.view.setPicking(true);
    this.renderHere(t(otherMap ? "route.here.other_map" : "route.here.tap"), [
      ["cancel", "route.here.cancel", false, () => this.cancelRouteHere()],
    ]);
  }

  private tappedStart(at: LatLon): void {
    const target = this.hereTarget;
    if (target) void this.replaceRoute(at, target);
  }

  private async replaceRoute(start: LatLon, target: LatLon): Promise<void> {
    const run = this.hereRun;
    this.useRouteHereProfile();
    const leg = this.formLeg();
    this.finishRouteHere();
    this.setPicking(false);
    delete $<HTMLInputElement>("route-name").dataset.auto;
    this.waypoints = [start, target];
    this.legs = [leg];
    this.stageEnds = [];
    // First, so that it cannot forget the plan computed below.
    await api.routeNew().catch(this.host.showError);
    // Another change came in while waiting; it is computed by itself.
    if (run + 1 === this.hereRun) this.changed(true);
  }

  private appendTarget(): void {
    const target = this.hereTarget;
    if (!target) return;
    this.useRouteHereProfile();
    this.finishRouteHere();
    this.legs = [...this.legs, this.formLeg()];
    this.waypoints = [...this.waypoints, target];
    this.changed(true);
  }

  private finishRouteHere(): void {
    this.cancelRouteHere();
    this.host.routeHereDone();
  }

  /** Shows the last profile in the form. No `change`: the open route stays as it is. */
  private useRouteHereProfile(): void {
    const available = (id: string | null) =>
      id !== null && id !== "manual" && this.profiles.some((p) => p.profile === id && p.available);
    const last = localStorage.getItem(LAST_PROFILE_KEY);
    const profile = available(last) ? last : available("hike") ? "hike" : null;
    if (profile === null || profile === this.profile()) return;
    $<HTMLSelectElement>("route-profile").value = profile;
    this.options = {};
    this.renderOptions();
    this.renderControls();
  }

  private renderHere(text: string, actions: [string, MessageKey, boolean, () => void][]): void {
    $("route-here-text").textContent = text;
    $("route-here-actions").replaceChildren(
      ...actions.map(([action, label, primary, run]) => {
        const button = document.createElement("button");
        button.type = "button";
        button.dataset.action = action;
        button.className = primary ? "primary" : "";
        button.textContent = t(label);
        button.addEventListener("click", run);
        return button;
      }),
    );
    $("route-here").hidden = false;
  }

  /** The first leg that failed in a way a straight line can stand in for. */
  private straightLeg(): number {
    return this.plan?.legs.findIndex((leg) => leg.error !== null && leg.error.code in STRAIGHT_TEXTS) ?? -1;
  }

  private connectStraight(): void {
    const index = this.straightLeg();
    if (index < 0 || index >= this.legs.length) return;
    this.legs = this.legs.map((leg, i) => (i === index ? { profile: "manual", options: {} } : leg));
    this.changed();
  }

  private renderStraight(): void {
    const code = this.plan?.legs[this.straightLeg()]?.error?.code;
    $("route-straight").hidden = code === undefined;
    if (code !== undefined) $("route-straight-text").textContent = t(STRAIGHT_TEXTS[code] as MessageKey);
  }

  private profile(): Profile {
    return $<HTMLSelectElement>("route-profile").value as Profile;
  }

  private setPicking(picking: boolean): void {
    this.picking = picking;
    this.view.setPicking(picking);
    this.renderControls();
  }

  /** What a new leg gets: the profile and settings shown in the form. */
  private formLeg(): LegSpec {
    return { profile: this.profile(), options: { ...this.options } };
  }

  /** The form changed: the last leg follows it. */
  private updateLastLeg(): void {
    if (this.legs.length === 0) return;
    this.legs = [...this.legs.slice(0, -1), this.formLeg()];
    void this.compute(false);
  }

  private addWaypoint(at: LatLon): void {
    if (this.waypoints.length > 0) this.legs = [...this.legs, this.formLeg()];
    this.waypoints = [...this.waypoints, at];
    this.changed();
  }

  private moveWaypoint(index: number, to: LatLon): void {
    this.waypoints = this.waypoints.map((p, i) => (i === index ? to : p));
    this.changed();
  }

  /**
   * Removes a waypoint. Its neighbours are joined by the leg before it (the
   * first point takes its leg along); a stage ending there ends no more.
   */
  private removeWaypoint(index: number): void {
    if (index < 0 || index >= this.waypoints.length) return;
    this.waypoints = this.waypoints.filter((_, i) => i !== index);
    const leg = index === 0 ? 0 : index - 1 === this.legs.length - 1 ? index - 1 : index;
    this.legs = this.legs.filter((_, i) => i !== leg);
    this.stageEnds = this.stageEnds.filter((end) => end !== index).map((end) => (end > index ? end - 1 : end));
    this.changed();
  }

  /** Marks or unmarks an inner waypoint as the end of a stage. */
  private toggleStageEnd(index: number): void {
    if (index <= 0 || index >= this.waypoints.length - 1) return;
    this.stageEnds = this.stageEnds.includes(index)
      ? this.stageEnds.filter((end) => end !== index)
      : [...this.stageEnds, index].sort((a, b) => a - b);
    this.changed();
  }

  private changed(fit = false): void {
    // Only inner waypoints can end a stage.
    this.stageEnds = this.stageEnds.filter((end) => end > 0 && end < this.waypoints.length - 1);
    this.renderWaypoints();
    this.renderControls();
    void this.compute(fit);
  }

  private renderWaypoints(): void {
    this.view.setWaypoints(
      this.waypoints,
      this.stageEnds,
      {
        moved: (index, to) => this.moveWaypoint(index, to),
        removed: (index) => this.removeWaypoint(index),
        stageToggled: (index) => this.toggleStageEnd(index),
      },
      { stageEnd: t("route.waypoint.stage_end"), remove: t("route.waypoint.remove") },
    );
  }

  private renderControls(): void {
    $("route-edit").textContent = t(this.picking ? "route.edit.stop" : "route.edit.start");
    $<HTMLButtonElement>("route-undo").disabled = this.waypoints.length === 0;
    $<HTMLButtonElement>("route-clear").disabled = this.waypoints.length === 0;
    $("route-hint").textContent = this.picking
      ? t("route.hint.picking")
      : this.profile() === "manual"
        ? t("route.hint.manual")
        : this.waypoints.length === 0
          ? t("route.hint.idle")
          : "";
  }

  /** One slider per setting of the chosen profile, as the core describes them. */
  private renderOptions(): void {
    const specs = this.profiles.find((p) => p.profile === this.profile())?.options ?? [];
    $("route-options").replaceChildren(
      ...specs.map((spec) => {
        const row = document.createElement("label");
        row.className = "field";
        const label = document.createElement("span");
        label.textContent = t(`route.option.${spec.key}`);
        const slider = document.createElement("input");
        slider.type = "range";
        slider.min = String(spec.min);
        slider.max = String(spec.max);
        slider.step = String(spec.step);
        slider.value = String(this.options[spec.key] ?? spec.default);
        slider.dataset.key = spec.key;
        const value = document.createElement("output");
        const show = () => {
          const n = Number(slider.value);
          value.textContent = spec.key === "max_hiking_difficulty" ? `T${n}` : `${Math.round(n * 100)} %`;
        };
        show();
        slider.addEventListener("input", show);
        slider.addEventListener("change", () => {
          this.options = { ...this.options, [spec.key]: Number(slider.value) };
          this.updateLastLeg();
        });
        row.append(label, value, slider);
        return row;
      }),
    );
  }

  /** Asks the core for the route through the waypoints and shows it. */
  private async compute(fit: boolean): Promise<void> {
    const request = ++this.request;
    if (this.waypoints.length < 2) {
      this.plan = null;
      this.renderPlan();
      await api.routeClear().catch(this.host.showError);
      return;
    }
    this.host.showMessage(t("route.computing"));
    try {
      const plan = await api.routePlan({ waypoints: this.waypoints, legs: this.legs, stage_ends: this.stageEnds });
      if (request !== this.request) return;
      this.plan = plan;
      // Legs a straight line can stand in for have their own message with the button.
      const failed = plan.legs.findIndex((leg) => leg.error !== null && !(leg.error.code in STRAIGHT_TEXTS));
      const error = plan.legs[failed]?.error;
      if (error) {
        this.host.showError({ code: error.code, message: t("route.leg_failed", { n: failed + 1, reason: error.message }) });
      } else this.host.showMessage("");
    } catch (e) {
      if (request !== this.request) return;
      this.plan = null;
      this.host.showError(e);
    }
    this.renderPlan(fit);
  }

  private renderPlan(fit = false): void {
    const plan = this.plan;
    this.view.showRoute(
      (plan?.legs ?? []).map((leg, i) =>
        leg.error
          ? { line: [this.waypoints[i], this.waypoints[i + 1]].filter((p) => p !== undefined), color: FAILED_COLOR, failed: true }
          : { line: leg.line, color: PROFILE_COLORS[leg.profile], failed: false },
      ),
      fit,
    );
    this.view.showProfileCursor(null);
    this.renderStraight();
    $("route-result").hidden = plan === null;
    if (!plan) return;
    const { total } = plan;
    const rows: [MessageKey, string][] = [["stats.distance", distance(total.distance_m)]];
    if (total.ascent_m !== null && total.descent_m !== null) {
      rows.push(["stats.ascent", elevation(total.ascent_m)], ["stats.descent", elevation(total.descent_m)]);
      rows.push(["stats.elevation", `${elevation(total.min_ele_m)} / ${elevation(total.max_ele_m)}`]);
    }
    if (total.time_s !== null) rows.push(["route.time", hours(total.time_s)]);
    renderRows($("route-stats"), rows);
    // Where each stage ends along the profile: the last sample before its end point.
    const stageEnds = plan.request.stage_ends.map((end) =>
      plan.elevation.reduce((at, p) => (p.leg < end ? Math.max(at, p.distance_m) : at), 0),
    );
    renderProfileChart($("route-chart"), plan.elevation, (point) => this.view.showProfileCursor(point), {
      colorOf: (point) => PROFILE_COLORS[plan.legs[point.leg]?.profile ?? "manual"],
      stageEnds,
    });
    renderLegend($("route-legend"), plan.legs.map((leg) => leg.profile));
    renderStages($("route-stages"), plan);
    const notes: string[] = [];
    if (total.failed_legs > 0) notes.push(t("route.note.failed_legs", { n: total.failed_legs }));
    if (total.ascent_m === null) notes.push(t("route.note.no_elevation"));
    const scale = plan.legs.find((leg) => BIKES.includes(leg.profile) && leg.max_mtb_scale !== null)?.max_mtb_scale;
    if (scale !== undefined && scale !== null) notes.push(t("route.note.mtb_scale", { scale }));
    $("route-note").textContent = notes.join(" ");
    const name = $<HTMLInputElement>("route-name");
    name.placeholder = t("route.name");
    if (name.dataset.auto !== "no") name.value = this.defaultName(plan);
    name.oninput = () => (name.dataset.auto = "no");
  }

  private defaultName(plan: RoutePlan): string {
    return t("route.default_name", {
      profile: profileNames(plan.legs.map((leg) => leg.profile)),
      distance: distance(plan.total.distance_m),
    });
  }

  private routeName(): string {
    const name = $<HTMLInputElement>("route-name").value.trim();
    return name === "" && this.plan ? this.defaultName(this.plan) : name;
  }

  /** Overwrites the route loaded or saved last, or stores a new one. */
  private async save(asNew: boolean): Promise<void> {
    try {
      const name = this.routeName();
      await (asNew ? api.routeSaveNew(name) : api.routeSave(name));
      this.host.showMessage(t("route.saved"));
      await this.refreshList();
    } catch (e) {
      this.host.showError(e);
    }
  }

  private async export(): Promise<void> {
    try {
      const name = this.routeName();
      const file = name.replace(/[^\p{L}\p{N} ._-]/gu, "_");
      const path = await save({ defaultPath: `${file}.gpx`, filters: [{ name: t("dialog.gpx"), extensions: ["gpx"] }] });
      if (!path) return;
      await api.routeExportGpx(name, path);
      this.host.showMessage(t("tracks.exported", { path }));
    } catch (e) {
      this.host.showError(e);
    }
  }

  /** Shows a saved route with its waypoints and settings, ready to be changed. */
  private async load(record: RouteRecord): Promise<void> {
    try {
      const plan = await api.routeLoad(record.id);
      this.request++;
      this.plan = plan;
      this.waypoints = plan.request.waypoints;
      this.legs = plan.request.legs;
      this.stageEnds = plan.request.stage_ends;
      const last = this.legs[this.legs.length - 1];
      this.options = Object.fromEntries(
        Object.entries(last?.options ?? {}).filter(([, value]) => value !== null),
      ) as Partial<Record<RouteOptionKey, number>>;
      if (last) $<HTMLSelectElement>("route-profile").value = last.profile;
      const name = $<HTMLInputElement>("route-name");
      name.value = record.name;
      name.dataset.auto = "no";
      this.setPicking(false);
      this.renderOptions();
      this.renderWaypoints();
      this.renderControls();
      this.renderPlan(true);
    } catch (e) {
      this.host.showError(e);
    }
  }

  private async refreshList(): Promise<void> {
    let records: RouteRecord[] = [];
    try {
      records = await api.routesList();
    } catch (e) {
      this.host.showError(e);
    }
    const list = $("route-list");
    if (records.length === 0) {
      const empty = document.createElement("li");
      empty.className = "empty";
      empty.textContent = t("route.empty");
      list.replaceChildren(empty);
      return;
    }
    list.replaceChildren(
      ...records.map((record) => {
        const item = document.createElement("li");
        item.dataset.id = record.id;
        const name = document.createElement("span");
        name.className = "name";
        name.textContent = record.name;
        const meta = document.createElement("span");
        meta.className = "meta";
        const ascent = record.ascent_m === null ? "" : ` · ↑ ${elevation(record.ascent_m)}`;
        const profiles = profileNames(record.profile.split("+") as Profile[]);
        meta.textContent = `${profiles} · ${distance(record.distance_m)}${ascent}`;
        const remove = document.createElement("button");
        remove.textContent = "✕";
        remove.title = t("route.delete");
        remove.addEventListener("click", (e) => {
          e.stopPropagation();
          api.routeDelete(record.id).then(() => this.refreshList(), this.host.showError);
        });
        item.append(name, meta, remove);
        item.addEventListener("click", () => void this.load(record));
        return item;
      }),
    );
  }
}

/** "MTB + Wandern": the names of the profiles, each once, in order. */
function profileNames(profiles: Profile[]): string {
  return [...new Set(profiles)].map((profile) => t(`route.profile.${profile}`)).join(" + ");
}

function swatch(color: string): HTMLElement {
  const dot = document.createElement("span");
  dot.className = "swatch";
  dot.style.background = color;
  return dot;
}

/** One entry per profile that occurs in the route, in order. */
function renderLegend(target: HTMLElement, profiles: Profile[]): void {
  target.replaceChildren(
    ...[...new Set(profiles)].map((profile) => {
      const entry = document.createElement("span");
      entry.append(swatch(PROFILE_COLORS[profile]), t(`route.profile.${profile}`));
      return entry;
    }),
  );
}

/** One row per stage, only when there are several. */
function renderStages(target: HTMLElement, plan: RoutePlan): void {
  target.hidden = plan.stages.length < 2;
  if (target.hidden) {
    target.replaceChildren();
    return;
  }
  target.replaceChildren(
    ...plan.stages.map((stage, i) => {
      const row = document.createElement("li");
      const name = document.createElement("span");
      name.className = "name";
      name.append(...stage.profiles.map((profile) => swatch(PROFILE_COLORS[profile])), t("route.stage", { n: i + 1 }));
      const parts = [distance(stage.distance_m)];
      if (stage.ascent_m !== null && stage.descent_m !== null) {
        parts.push(`↑ ${elevation(stage.ascent_m)} ↓ ${elevation(stage.descent_m)}`);
      }
      if (stage.time_s !== null) parts.push(hours(stage.time_s));
      const meta = document.createElement("span");
      meta.className = "meta";
      meta.textContent = parts.join(" · ");
      row.append(name, meta);
      return row;
    }),
  );
}
