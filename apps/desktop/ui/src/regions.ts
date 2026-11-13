/** The "Karten" panel: catalog location, region list, downloads. Rendering only. */
import { open } from "@tauri-apps/plugin-dialog";
import { api, isCommandError, type RegionInfo, type RegionProgress, type RegionsOverview } from "./api";
import { bytes } from "./format";
import { t } from "./i18n";

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

export interface RegionsPanelHost {
  /** Called whenever the set of installed regions may have changed. */
  onOverview(overview: RegionsOverview): void;
  showRegion(region: RegionInfo): void;
  showMessage(text: string): void;
  showError(error: unknown): void;
}

export class RegionsPanel {
  private overview: RegionsOverview | null = null;
  private readonly progress = new Map<string, RegionProgress>();

  constructor(private readonly host: RegionsPanelHost) {
    $("catalog-apply").addEventListener("click", () => void this.applySource());
    $<HTMLInputElement>("catalog-source").addEventListener("change", () => void this.applySource());
    $("catalog-choose").addEventListener("click", () => void this.chooseFolder());
  }

  /** Reads the overview from the core and renders it. */
  async refresh(): Promise<RegionsOverview | null> {
    try {
      this.overview = await api.regionsOverview();
      $<HTMLInputElement>("catalog-source").value = this.overview.source;
      this.render();
      this.host.onOverview(this.overview);
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
      this.host.showMessage(t("maps.deleted", { name: region.name }));
    } catch (e) {
      this.host.showError(e);
    }
    await this.refresh();
  }

  private button(label: string, run: () => void, className = ""): HTMLButtonElement {
    const button = document.createElement("button");
    button.textContent = label;
    button.className = className;
    button.addEventListener("click", run);
    return button;
  }

  private render(): void {
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

    $("region-list").replaceChildren(
      ...overview.regions.map((region) => {
        const item = document.createElement("li");
        item.dataset.id = region.id;
        const name = document.createElement("div");
        name.className = "name";
        name.textContent = region.name;
        const meta = document.createElement("div");
        meta.className = "meta";
        meta.textContent = `${bytes(region.size)} · ${t(`maps.state.${region.state}`)}`;
        const buttons = document.createElement("div");
        buttons.className = "row";
        item.append(name, meta);

        if (region.state === "downloading") {
          const bar = document.createElement("progress");
          const progress = this.progress.get(region.id);
          bar.max = progress?.total ?? region.size;
          bar.value = progress?.done ?? 0;
          item.append(bar);
          buttons.append(this.button(t("maps.cancel"), () => void api.regionCancel(region.id)));
        } else if (region.state === "available") {
          buttons.append(this.button(t("maps.download"), () => void this.install(region), "primary"));
        } else {
          if (region.state === "update_available") {
            buttons.append(this.button(t("maps.update"), () => void this.install(region), "primary"));
          }
          buttons.append(
            this.button(t("maps.show"), () => this.host.showRegion(region)),
            this.button(t("maps.delete"), () => void this.remove(region), "danger"),
          );
        }
        item.append(buttons);
        return item;
      }),
    );
  }
}
