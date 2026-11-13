/**
 * The "Route planen" panel: profile, settings, waypoints, result, saved routes.
 * Input and rendering only; routes and their numbers come from the core.
 */
import { save } from "@tauri-apps/plugin-dialog";
import {
  api,
  type LatLon,
  type Profile,
  type ProfileInfo,
  type RouteOptionKey,
  type RouteOptions,
  type RoutePlan,
  type RouteRecord,
} from "./api";
import { distance, elevation, hours, renderRows } from "./format";
import { t, type MessageKey } from "./i18n";
import type { MapView } from "./map";
import { renderProfileChart } from "./profile-chart";

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

/** Profiles in the order they are offered. */
const ORDER: Profile[] = ["hike", "mtb", "trekking", "road_bike", "car", "manual"];
const BIKES: Profile[] = ["mtb", "trekking", "road_bike"];

export interface RoutePanelHost {
  showMessage(text: string): void;
  showError(error: unknown): void;
}

export class RoutePanel {
  private profiles: ProfileInfo[] = [];
  private waypoints: LatLon[] = [];
  private options: RouteOptions = {};
  private picking = false;
  private plan: RoutePlan | null = null;
  /** Counts requests so that a slow answer cannot replace a newer one. */
  private request = 0;

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
      this.renderOptions();
      this.renderControls();
      void this.compute(false);
    });
    $("route-edit").addEventListener("click", () => this.setPicking(!this.picking));
    $("route-undo").addEventListener("click", () => this.setWaypoints(this.waypoints.slice(0, -1)));
    $("route-clear").addEventListener("click", () => {
      this.setPicking(false);
      delete $<HTMLInputElement>("route-name").dataset.auto;
      this.setWaypoints([]);
    });
    $("route-save").addEventListener("click", () => void this.save());
    $("route-export").addEventListener("click", () => void this.export());
    this.view.onClick((at) => {
      if (this.picking) this.setWaypoints([...this.waypoints, at]);
    });
    this.renderOptions();
    this.renderControls();
    this.renderPlan();
    await this.refreshList();
  }

  private profile(): Profile {
    return $<HTMLSelectElement>("route-profile").value as Profile;
  }

  private setPicking(picking: boolean): void {
    this.picking = picking;
    this.view.setPicking(picking);
    this.renderControls();
  }

  private setWaypoints(waypoints: LatLon[]): void {
    this.waypoints = waypoints;
    this.renderWaypoints();
    this.renderControls();
    void this.compute(false);
  }

  private renderWaypoints(): void {
    this.view.setWaypoints(this.waypoints, {
      moved: (index, to) => this.setWaypoints(this.waypoints.map((p, i) => (i === index ? to : p))),
      removed: (index) => this.setWaypoints(this.waypoints.filter((_, i) => i !== index)),
    });
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
          void this.compute(false);
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
      const plan = await api.routePlan({ waypoints: this.waypoints, profile: this.profile(), options: this.options });
      if (request !== this.request) return;
      this.plan = plan;
      this.host.showMessage("");
    } catch (e) {
      if (request !== this.request) return;
      this.plan = null;
      this.host.showError(e);
    }
    this.renderPlan(fit);
  }

  private renderPlan(fit = false): void {
    const plan = this.plan;
    this.view.showRoute(plan?.route.line ?? [], fit);
    this.view.showProfileCursor(null);
    $("route-result").hidden = plan === null;
    if (!plan) return;
    const { route } = plan;
    const rows: [MessageKey, string][] = [["stats.distance", distance(route.distance_m)]];
    if (route.duration_s !== null) {
      rows.push([route.profile === "hike" ? "route.time.walk" : "route.time.ride", hours(route.duration_s)]);
    }
    if (plan.ascent_m !== null && plan.descent_m !== null) {
      rows.push(["stats.ascent", elevation(plan.ascent_m)], ["stats.descent", elevation(plan.descent_m)]);
      rows.push(["stats.elevation", `${elevation(plan.min_ele_m)} / ${elevation(plan.max_ele_m)}`]);
    }
    if (plan.hiking_time_s !== null) rows.push(["route.time.din", hours(plan.hiking_time_s)]);
    renderRows($("route-stats"), rows);
    renderProfileChart($("route-chart"), plan.elevation, (point) => this.view.showProfileCursor(point));
    const notes: string[] = [];
    if (plan.ascent_m === null) notes.push(t("route.note.no_elevation"));
    if (plan.max_mtb_scale !== null && BIKES.includes(route.profile)) {
      notes.push(t("route.note.mtb_scale", { scale: plan.max_mtb_scale }));
    }
    $("route-note").textContent = notes.join(" ");
    const name = $<HTMLInputElement>("route-name");
    name.placeholder = t("route.name");
    if (name.dataset.auto !== "no") name.value = this.defaultName(plan);
    name.oninput = () => (name.dataset.auto = "no");
  }

  private defaultName(plan: RoutePlan): string {
    return t("route.default_name", {
      profile: t(`route.profile.${plan.route.profile}`),
      distance: distance(plan.route.distance_m),
    });
  }

  private routeName(): string {
    const name = $<HTMLInputElement>("route-name").value.trim();
    return name === "" && this.plan ? this.defaultName(this.plan) : name;
  }

  private async save(): Promise<void> {
    try {
      await api.routeSave(this.routeName());
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
      this.waypoints = plan.route.waypoints;
      this.options = Object.fromEntries(
        Object.entries(plan.options).filter(([, value]) => value !== null),
      ) as Partial<Record<RouteOptionKey, number>>;
      $<HTMLSelectElement>("route-profile").value = plan.route.profile;
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
        meta.textContent = `${distance(record.distance_m)}${ascent}`;
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
