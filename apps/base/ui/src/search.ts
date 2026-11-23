/**
 * The search at the top of the sidebar: text or a category, a list of hits, a
 * card for the chosen one. What is found and in which order comes from the core.
 */
import { api, type SearchCategory, type SearchHit, type SearchResults } from "./api";
import { distance } from "./format";
import { t } from "./i18n";
import type { MapView } from "./map";

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

const MIN_CHARS = 2;
const LIMIT = 30;
const TYPING_DELAY_MS = 200;
/** The quick choices, in the order they are offered. */
const CATEGORIES: SearchCategory[] = ["lodging", "food", "groceries", "pharmacy", "fuel", "camping", "parking", "sights"];

export interface SearchPanelHost {
  routeHere(hit: SearchHit): void;
  showError(error: unknown): void;
}

export class SearchPanel {
  /** Counts the searches so that a slow answer cannot replace a newer one. */
  private request = 0;
  private timer: number | undefined;
  private selected: SearchHit | null = null;

  constructor(
    private readonly view: MapView,
    private readonly host: SearchPanelHost,
  ) {}

  start(): void {
    const input = $<HTMLInputElement>("search-input");
    input.placeholder = t("search.placeholder");
    input.addEventListener("input", () => this.typed(input.value));
    input.addEventListener("keydown", (e) => {
      if (e.key !== "Escape") return;
      input.value = "";
      this.typed("");
    });
    $("search-categories").replaceChildren(
      ...CATEGORIES.map((category) => {
        const button = document.createElement("button");
        button.type = "button";
        button.dataset.category = category;
        button.textContent = t(`search.category.${category}`);
        button.addEventListener("click", () => void this.chooseCategory(category));
        return button;
      }),
    );
    $("search-route").addEventListener("click", () => {
      if (this.selected) this.host.routeHere(this.selected);
    });
  }

  private typed(text: string): void {
    window.clearTimeout(this.timer);
    this.clearSelection();
    this.markCategory(null);
    const query = text.trim();
    $("search-categories").hidden = query.length > 0;
    if (query.length < MIN_CHARS) {
      this.request++;
      this.renderResults(null);
      return;
    }
    this.timer = window.setTimeout(() => {
      void this.run(() => api.searchText(query, this.view.centre(), LIMIT));
    }, TYPING_DELAY_MS);
  }

  private async chooseCategory(category: SearchCategory): Promise<void> {
    window.clearTimeout(this.timer);
    this.clearSelection();
    this.markCategory(category);
    await this.run(() => api.searchCategory(category, this.view.centre(), LIMIT));
  }

  private async run(search: () => Promise<SearchResults>): Promise<void> {
    const request = ++this.request;
    try {
      const results = await search();
      if (request === this.request) this.renderResults(results);
    } catch (e) {
      if (request === this.request) this.host.showError(e);
    }
  }

  private markCategory(category: SearchCategory | null): void {
    for (const button of $("search-categories").querySelectorAll<HTMLElement>("button")) {
      button.setAttribute("aria-pressed", String(button.dataset.category === category));
    }
  }

  private renderResults(results: SearchResults | null): void {
    const list = $("search-results");
    list.replaceChildren(...(results?.hits ?? []).map((hit) => this.row(hit)));
    list.hidden = list.childElementCount === 0;
    const notes = (results?.unsearchable ?? []).map((region) => t("search.unsearchable", { region }));
    if (results && results.hits.length === 0) notes.unshift(t("search.none"));
    const note = $("search-notes");
    note.textContent = notes.join(" ");
    note.hidden = notes.length === 0;
  }

  private row(hit: SearchHit): HTMLElement {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "search-hit";
    const title = document.createElement("span");
    title.textContent = hitTitle(hit);
    const meta = document.createElement("small");
    meta.textContent = hitDetails(hit);
    button.append(title, meta);
    button.addEventListener("click", () => this.select(hit));
    item.append(button);
    return item;
  }

  private select(hit: SearchHit): void {
    this.selected = hit;
    const at = { lat: hit.lat, lon: hit.lon };
    // The map would otherwise jump back with the next position.
    $<HTMLInputElement>("follow").checked = false;
    this.view.followPosition = false;
    this.view.showSearchPin(at);
    this.view.flyToResult(at);
    this.renderResults(null);
    $("search-categories").hidden = true;
    $("search-card-title").textContent = hitTitle(hit);
    $("search-card-details").textContent = hitDetails(hit);
    $("search-card").hidden = false;
  }

  private clearSelection(): void {
    this.selected = null;
    this.view.showSearchPin(null);
    $("search-card").hidden = true;
  }
}

function hitTitle(hit: SearchHit): string {
  if (hit.name) return hit.name;
  return hit.category ? t(`search.category.${hit.category}`) : t(`search.kind.${hit.kind}`);
}

function hitDetails(hit: SearchHit): string {
  const kind = hit.category ? t(`search.category.${hit.category}`) : t(`search.kind.${hit.kind}`);
  return [kind, hit.town, distance(hit.distance_m)].filter((part) => part).join(" · ");
}
