/**
 * The search at the top of the sidebar: text or a category, a list of hits and
 * their dots on the map, a card for the chosen one. What is found and in which
 * order comes from the core.
 */
import { api, type LatLon, type SearchCategory, type SearchHit, type SearchResults } from "./api";
import { distance } from "./format";
import { t } from "./i18n";
import type { MapCamera, MapView } from "./map";

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

const MIN_CHARS = 2;
const LIMIT = 30;
const TYPING_DELAY_MS = 200;
/** The map waits this long after an answer while typing, so it does not jump with every letter. */
const FIT_DELAY_MS = 500;
/** The quick choices, in the order they are offered. */
const CATEGORIES: SearchCategory[] = ["lodging", "food", "groceries", "pharmacy", "fuel", "camping", "parking", "sights"];

export interface SearchPanelHost {
  routeHere(hit: SearchHit): void;
  /** The chosen hit is gone: a route to it is no longer being set up. */
  selectionCleared(): void;
  showError(error: unknown): void;
}

/** The area a search is about. */
interface Anchor {
  centre: LatLon;
  radiusM: number;
}

export class SearchPanel {
  /** Counts the searches so that a slow answer cannot replace a newer one. */
  private request = 0;
  private timer: number | undefined;
  private fitTimer: number | undefined;
  private selected: SearchHit | null = null;
  private category: SearchCategory | null = null;
  private results: SearchResults | null = null;
  /** Kept while the app moves the map, so that fitting it does not shift the search. */
  private anchor: Anchor | null = null;
  /** Where the map is heading, or has arrived, to show all hits. */
  private overview: MapCamera | null = null;
  /** Where "Zurück" takes the map: the view before the first hit was opened. */
  private returnTo: MapCamera | null = null;

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
    $("search-back").addEventListener("click", () => this.back());
    this.view.onSearchHitClick((index) => {
      const hit = this.results?.hits[index];
      if (hit) this.select(hit);
    });
    this.view.onUserMove(() => {
      window.clearTimeout(this.fitTimer);
      this.anchor = null;
      this.overview = null;
    });
  }

  private typed(text: string): void {
    window.clearTimeout(this.timer);
    window.clearTimeout(this.fitTimer);
    this.clearSelection();
    this.markCategory(null);
    const query = text.trim();
    $("search-categories").hidden = query.length > 0;
    if (query.length < MIN_CHARS) {
      this.request++;
      this.anchor = null;
      this.overview = null;
      this.renderResults(null);
      return;
    }
    this.timer = window.setTimeout(() => {
      void this.run((at) => api.searchText(query, at.centre, at.radiusM, LIMIT), FIT_DELAY_MS);
    }, TYPING_DELAY_MS);
  }

  private async chooseCategory(category: SearchCategory): Promise<void> {
    window.clearTimeout(this.timer);
    window.clearTimeout(this.fitTimer);
    this.clearSelection();
    this.markCategory(category);
    await this.run((at) => api.searchCategory(category, at.centre, at.radiusM, LIMIT), 0);
  }

  private async run(search: (at: Anchor) => Promise<SearchResults>, fitDelayMs: number): Promise<void> {
    const request = ++this.request;
    this.anchor ??= { centre: this.view.centre(), radiusM: this.view.searchRadiusM() };
    try {
      const results = await search(this.anchor);
      if (request !== this.request) return;
      this.renderResults(results);
      this.overview = null;
      if (fitDelayMs === 0) this.fit(results);
      else this.fitTimer = window.setTimeout(() => this.fit(results), fitDelayMs);
    } catch (e) {
      if (request !== this.request) return;
      this.renderResults(null);
      this.host.showError(e);
    }
  }

  /** Brings the hits around the search into view; a far place alone does not pull the map out. */
  private fit(results: SearchResults): void {
    const inside = results.hits.filter((hit) => hit.distance_m <= results.radius_m);
    this.stopFollowing();
    this.overview = this.view.fitSearchHits(inside.length > 0 ? inside : results.hits);
  }

  /** The map would otherwise jump back with the next position. */
  private stopFollowing(): void {
    $<HTMLInputElement>("follow").checked = false;
    this.view.followPosition = false;
  }

  private markCategory(category: SearchCategory | null): void {
    this.category = category;
    for (const button of $("search-categories").querySelectorAll<HTMLElement>("button")) {
      button.setAttribute("aria-pressed", String(button.dataset.category === category));
    }
  }

  private renderResults(results: SearchResults | null): void {
    this.results = results;
    this.view.showSearchHits(results?.hits ?? []);
    const list = $("search-results");
    list.replaceChildren(...(results?.hits ?? []).map((hit) => this.row(hit)));
    list.hidden = list.childElementCount === 0;
    const notes = (results?.unsearchable ?? []).map((region) => t("search.unsearchable", { region }));
    if (results?.widened) notes.unshift(t("search.widened"));
    if (results && results.hits.length === 0) notes.unshift(t(this.category ? "search.none_nearby" : "search.none"));
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
    // A search still under way must not open its list over the card.
    window.clearTimeout(this.timer);
    window.clearTimeout(this.fitTimer);
    this.request++;
    this.host.selectionCleared();
    this.returnTo ??= this.overview ?? this.view.camera();
    this.selected = hit;
    const at = { lat: hit.lat, lon: hit.lon };
    this.stopFollowing();
    this.view.showSearchPin(at);
    this.view.flyToResult(at);
    $("search-results").hidden = true;
    $("search-notes").hidden = true;
    $("search-categories").hidden = true;
    $("search-card-title").textContent = hitTitle(hit);
    $("search-card-details").textContent = hitDetails(hit);
    $("search-card").hidden = false;
  }

  /** From the chosen hit back to the list and the view it was opened from, without searching. */
  private back(): void {
    const camera = this.returnTo;
    this.clearSelection();
    this.renderResults(this.results);
    $("search-categories").hidden = $<HTMLInputElement>("search-input").value.trim().length > 0;
    if (camera) {
      this.overview = camera;
      this.view.moveTo(camera);
    }
  }

  /** A route to the hit is on the map; its marker takes the place of the pin and the dots. */
  routeDone(): void {
    window.clearTimeout(this.fitTimer);
    this.request++;
    this.selected = null;
    this.anchor = null;
    this.overview = null;
    this.returnTo = null;
    this.view.showSearchPin(null);
    this.renderResults(null);
    $("search-card").hidden = true;
  }

  private clearSelection(): void {
    this.host.selectionCleared();
    this.returnTo = null;
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
