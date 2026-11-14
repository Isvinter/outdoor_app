/** The "Karten" panel: building new regions, catalog location, region list, downloads. Rendering only. */
import { ask, open } from "@tauri-apps/plugin-dialog";
import {
  api,
  isCommandError,
  type Bbox,
  type BuildStatus,
  type RegionInfo,
  type RegionProgress,
  type RegionsOverview,
  type StepProgress,
} from "./api";
import { bytes } from "./format";
import { t, type MessageKey } from "./i18n";
import type { MapView } from "./map";

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

/** Ask before building anything expected to be larger than this. */
const CONFIRM_BYTES = 5e9;
/** Wait this long after the last change of the area before estimating it. */
const PLAN_DELAY_MS = 300;
const BUILD_STEPS = ["osm", "basemap", "outdoor", "dem", "slope", "routing", "manifest"] as const;

export interface RegionsPanelHost {
  /** Called whenever the set of installed regions may have changed. */
  onOverview(overview: RegionsOverview): void;
  showRegion(region: RegionInfo): void;
  /** Shows the online map while an area is chosen; `false` brings back the previous style. */
  useOnlineMap(on: boolean): void;
  showMessage(text: string): void;
  showError(error: unknown): void;
}

function stepName(step: string): string {
  return (BUILD_STEPS as readonly string[]).includes(step)
    ? t(`maps.build.step.${step as (typeof BUILD_STEPS)[number]}`)
    : step;
}

function missingName(code: string): string {
  const key = `maps.build.missing.${code}`;
  return ["platform", "repo", "java_version", "docker_daemon"].includes(code) ? t(key as MessageKey) : code;
}

export class RegionsPanel {
  private overview: RegionsOverview | null = null;
  private readonly progress = new Map<string, RegionProgress>();
  /** `null` until the first check is back; building waits for it. */
  private status: BuildStatus | null = null;
  /** The build in progress; also set before its first event arrives. */
  private building: StepProgress | null = null;
  private selecting = false;
  private area: Bbox | null = null;
  private planTimer: number | undefined;
  private planRequest = 0;
  private estimate: number | null = null;
  /** Output of the last failed build, by region. */
  private readonly failures = new Map<string, string>();

  constructor(
    private readonly view: MapView,
    private readonly host: RegionsPanelHost,
  ) {
    $("catalog-apply").addEventListener("click", () => void this.applySource());
    $<HTMLInputElement>("catalog-source").addEventListener("change", () => void this.applySource());
    $("catalog-choose").addEventListener("click", () => void this.chooseFolder());
    $("build-pick").addEventListener("click", () => this.toggleSelection());
    $("build-name").addEventListener("input", () => this.renderForm());
    $("build-start").addEventListener("click", () => void this.buildNew());
  }

  /** Reads the overview and the build status from the core and renders them. */
  async refresh(): Promise<RegionsOverview | null> {
    // The build check starts programs (docker, java); the list must not wait for it.
    api
      .regionBuildCheck()
      .then((status) => {
        this.status = status;
        if (status.running) this.building = status.running;
        this.render();
      })
      .catch((e) => this.host.showError(e));
    try {
      const overview = await api.regionsOverview();
      this.overview = overview;
      $<HTMLInputElement>("catalog-source").value = overview.source;
      this.render();
      this.host.onOverview(overview);
    } catch (e) {
      this.host.showError(e);
    }
    return this.overview;
  }

  onProgress(progress: RegionProgress): void {
    this.progress.set(progress.region, progress);
    const item = document.querySelector<HTMLElement>(`#region-list li[data-id="${progress.region}"]`);
    const bar = item?.querySelector("progress");
    if (!item || !bar) return;
    bar.max = progress.total;
    bar.value = progress.done;
    const meta = item.querySelector(".meta");
    if (meta) meta.textContent = t("maps.progress", { done: bytes(progress.done), total: bytes(progress.total) });
  }

  onBuildProgress(progress: StepProgress): void {
    this.building = progress;
    // A new region appears in the list once its definition is written.
    if (!this.overview?.regions.some((region) => region.id === progress.region)) void this.refresh();
    else this.render();
  }

  // --- choosing an area -------------------------------------------------------

  private toggleSelection(): void {
    if (this.selecting) this.stopSelection();
    else {
      this.selecting = true;
      this.host.useOnlineMap(true);
      this.view.startAreaSelection((area) => {
        this.area = area;
        this.schedulePlan();
        this.renderForm();
      });
      this.renderForm();
    }
  }

  private stopSelection(): void {
    this.selecting = false;
    this.view.stopAreaSelection();
    this.host.useOnlineMap(false);
    this.area = null;
    this.estimate = null;
    this.planRequest++;
    $("build-estimate").textContent = "";
    this.renderForm();
  }

  private schedulePlan(): void {
    window.clearTimeout(this.planTimer);
    const request = ++this.planRequest;
    this.estimate = null;
    $("build-estimate").textContent = this.area ? t("maps.build.planning") : "";
    const area = this.area;
    if (!area) return;
    this.planTimer = window.setTimeout(() => {
      api
        .regionBuildPlan(area)
        .then((plan) => {
          if (request !== this.planRequest) return;
          this.estimate = plan.estimated_bytes;
          $("build-estimate").textContent = t("maps.build.estimate", {
            width: Math.round(plan.width_km),
            height: Math.round(plan.height_km),
            size: bytes(plan.estimated_bytes),
            extracts: plan.extracts.length,
          });
          this.renderForm();
        })
        .catch((e) => {
          if (request !== this.planRequest) return;
          $("build-estimate").textContent = isCommandError(e)
            ? `${t(`error.${e.code}` as MessageKey) ?? t("error.unknown")} (${e.message})`
            : String(e);
        });
    }, PLAN_DELAY_MS);
  }

  // --- building ---------------------------------------------------------------

  private async buildNew(): Promise<void> {
    const area = this.area;
    const name = $<HTMLInputElement>("build-name").value.trim();
    if (!area || name === "") return;
    if (this.estimate !== null && this.estimate >= CONFIRM_BYTES) {
      const go = await ask(t("maps.build.confirm", { size: bytes(this.estimate) }), { kind: "warning" });
      if (!go) return;
    }
    this.stopSelection();
    $<HTMLInputElement>("build-name").value = "";
    await this.build({ name, bbox: area, id: null, refresh: false }, name);
  }

  private async build(request: Parameters<typeof api.regionBuildStart>[0], name: string): Promise<void> {
    this.building = { region: request.id ?? "", step: "", index: 0, total: 0 };
    if (request.id) this.failures.delete(request.id);
    this.render();
    try {
      await api.regionBuildStart(request);
      const region = this.building?.region;
      this.building = null;
      this.host.showMessage(t("maps.build.installed", { name }));
      await this.refresh();
      const installed = this.overview?.regions.find((r) => r.id === region);
      if (installed) this.host.showRegion(installed);
    } catch (e) {
      const region = this.building?.region ?? "";
      this.building = null;
      if (isCommandError(e) && e.code === "build_cancelled") this.host.showMessage(t("maps.build.cancelled"));
      else {
        if (isCommandError(e) && e.code === "build_failed" && region) this.failures.set(region, e.message);
        this.host.showError(e);
      }
      await this.refresh();
    }
  }

  // --- catalog and list ---------------------------------------------------------

  private async applySource(): Promise<void> {
    try {
      await api.regionsSetSource($<HTMLInputElement>("catalog-source").value);
      await this.refresh();
    } catch (e) {
      this.host.showError(e);
    }
  }

  private async chooseFolder(): Promise<void> {
    const folder = await open({ directory: true, multiple: false });
    if (typeof folder !== "string") return;
    $<HTMLInputElement>("catalog-source").value = folder;
    await this.applySource();
  }

  private async install(region: RegionInfo): Promise<void> {
    region.state = "downloading";
    this.progress.delete(region.id);
    this.render();
    try {
      await api.regionInstall(region.id);
      this.host.showMessage(t("maps.installed", { name: region.name }));
      await this.refresh();
      this.host.showRegion(region);
    } catch (e) {
      if (isCommandError(e) && e.code === "region_cancelled") this.host.showMessage(t("maps.cancelled"));
      else this.host.showError(e);
      await this.refresh();
    }
  }

  private async remove(region: RegionInfo): Promise<void> {
    try {
      await api.regionDelete(region.id);
      this.failures.delete(region.id);
      this.host.showMessage(t("maps.deleted", { name: region.name }));
    } catch (e) {
      this.host.showError(e);
    }
    await this.refresh();
  }

  private button(label: string, run: () => void, className = "", disabled = false): HTMLButtonElement {
    const button = document.createElement("button");
    button.textContent = label;
    button.className = className;
    button.disabled = disabled;
    button.addEventListener("click", run);
    return button;
  }

  private canBuild(): boolean {
    return this.building === null && this.status !== null && this.status.missing.length === 0;
  }

  private renderForm(): void {
    $("build-pick").textContent = t(this.selecting ? "maps.build.pick_stop" : "maps.build.pick");
    $<HTMLButtonElement>("build-pick").disabled = !this.canBuild() && !this.selecting;
    const name = $<HTMLInputElement>("build-name").value.trim();
    $<HTMLButtonElement>("build-start").disabled =
      !this.canBuild() || this.area === null || name === "" || this.estimate === null;
    $("build-hint").textContent =
      this.status && this.status.missing.length > 0
        ? t("maps.build.missing", { list: this.status.missing.map(missingName).join(", ") })
        : this.selecting && this.area === null
          ? t("maps.build.pick_hint")
          : "";
  }

  private render(): void {
    this.renderForm();
    const overview = this.overview;
    if (!overview) return;
    $("catalog-hint").textContent =
      overview.source === ""
        ? t("maps.catalog.none")
        : overview.catalog_error
          ? t("maps.catalog.error", { reason: overview.catalog_error })
          : overview.regions.length === 0
            ? t("maps.empty")
            : "";

    const canBuild = this.canBuild();
    $("region-list").replaceChildren(
      ...overview.regions.map((region) => {
        const item = document.createElement("li");
        item.dataset.id = region.id;
        const name = document.createElement("div");
        name.className = "name";
        name.textContent = region.name;
        const meta = document.createElement("div");
        meta.className = "meta";
        meta.textContent =
          region.state === "not_built"
            ? t("maps.state.not_built")
            : `${bytes(region.size)} · ${t(`maps.state.${region.state}`)}`;
        const buttons = document.createElement("div");
        buttons.className = "row";
        item.append(name, meta);

        const build = this.building?.region === region.id ? this.building : null;
        if (build) {
          meta.textContent =
            build.index === 0
              ? t("maps.build.preparing")
              : t("maps.build.progress", { index: build.index, total: build.total, step: stepName(build.step) });
          const bar = document.createElement("progress");
          bar.max = Math.max(build.total, 1);
          bar.value = Math.max(build.index - 1, 0);
          item.append(bar);
          buttons.append(this.button(t("maps.cancel"), () => void api.regionBuildCancel()));
        } else if (region.state === "downloading") {
          const bar = document.createElement("progress");
          const progress = this.progress.get(region.id);
          bar.max = progress?.total ?? region.size;
          bar.value = progress?.done ?? 0;
          item.append(bar);
          buttons.append(this.button(t("maps.cancel"), () => void api.regionCancel(region.id)));
        } else if (region.state === "not_built") {
          buttons.append(
            this.button(
              t("maps.build.build"),
              () => void this.build({ name: null, bbox: null, id: region.id, refresh: false }, region.name),
              "primary",
              !canBuild,
            ),
            this.button(t("maps.delete"), () => void this.remove(region), "danger"),
          );
        } else if (region.state === "available") {
          buttons.append(this.button(t("maps.download"), () => void this.install(region), "primary"));
        } else {
          if (region.state === "update_available") {
            buttons.append(this.button(t("maps.update"), () => void this.install(region), "primary"));
          }
          buttons.append(this.button(t("maps.show"), () => this.host.showRegion(region)));
          if (region.buildable) {
            buttons.append(
              this.button(
                t("maps.build.again"),
                () => void this.build({ name: null, bbox: null, id: region.id, refresh: true }, region.name),
                "",
                !canBuild,
              ),
            );
          }
          buttons.append(this.button(t("maps.delete"), () => void this.remove(region), "danger"));
        }

        const failure = this.failures.get(region.id);
        if (failure && !build) {
          const details = document.createElement("details");
          const summary = document.createElement("summary");
          summary.textContent = t("maps.build.log");
          const pre = document.createElement("pre");
          pre.textContent = failure;
          details.append(summary, pre);
          item.append(details);
        }
        item.append(buttons);
        return item;
      }),
    );
  }
}
