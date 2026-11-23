/** The map view: base map, own position, live track and the selected track. */
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  addProtocol,
  LngLatBounds,
  Map as MapLibreMap,
  Marker,
  NavigationControl,
  Popup,
  ScaleControl,
  setWorkerUrl,
  type GeoJSONSource,
  type MapMouseEvent,
  type StyleSpecification,
} from "maplibre-gl";
import mlcontour from "maplibre-contour";
import "maplibre-gl/dist/maplibre-gl.css";
import type { Bbox, Fix, LatLon, Line, RegionsOverview } from "./api";

// The worker files are copied next to the bundle by vite.config.ts.
setWorkerUrl(new URL("maplibre/maplibre-gl-worker.mjs", document.baseURI).href);

export type BaseStyle = "topo" | "winter" | "online";

const OFFLINE_STYLES = { topo: "topo-summer.json", winter: "topo-winter.json" } as const;
const SLOPE = "slope";

const ONLINE_STYLE = "https://tiles.openfreemap.org/styles/liberty";
/** Base URL of the tile protocol served by the core, e.g. `tiles://localhost/`. */
const TILES = convertFileSrc("", "tiles");
const EMPTY_STYLE: StyleSpecification = { version: 8, sources: {}, layers: [] };
/** Following the position moves the map at once unless it has to move further than this. */
const FOLLOW_GLIDE_PX = 60;
const LIVE = "live-track";
const SELECTED = "selected-track";
const ROUTE = "planned-route";
/** The area chosen for a new region. */
const AREA = "build-area";
/** Which edges of the area a handle moves: west or east, south or north. */
const AREA_HANDLES: ["w" | "e" | null, "s" | "n" | null][] = [
  ["w", "n"], [null, "n"], ["e", "n"], ["e", null], ["e", "s"], [null, "s"], ["w", "s"], ["w", null],
];

/** Corners in order: west < east, south < north. */
function normalize([a, b, c, d]: Bbox): Bbox {
  return [Math.min(a, c), Math.min(b, d), Math.max(a, c), Math.max(b, d)];
}

function areaData(area: Bbox | null) {
  if (!area) return { type: "FeatureCollection" as const, features: [] };
  const [w, s, e, n] = area;
  const ring = [[w, s], [e, s], [e, n], [w, n], [w, s]];
  return {
    type: "FeatureCollection" as const,
    features: [
      { type: "Feature" as const, properties: {}, geometry: { type: "Polygon" as const, coordinates: [ring] } },
    ],
  };
}

const OVERLAYS = [
  [SELECTED, "#7b3fb5"],
  [ROUTE, "#0b63ce"],
  [LIVE, "#d93025"],
] as const;

/** One leg of the planned route as drawn on the map. */
export interface RouteLeg {
  line: LatLon[];
  color: string;
  /** Drawn dashed: the leg could not be computed, `line` is the straight way. */
  failed: boolean;
}

function routeData(legs: RouteLeg[]) {
  return {
    type: "FeatureCollection" as const,
    features: legs
      .filter((leg) => leg.line.length >= 2)
      .map((leg) => ({
        type: "Feature" as const,
        properties: { color: leg.color, failed: leg.failed },
        geometry: { type: "LineString" as const, coordinates: leg.line.map((p) => [p.lon, p.lat]) },
      })),
  };
}

/** What the route panel wants to know about its waypoint markers. */
export interface WaypointHandlers {
  moved(index: number, to: LatLon): void;
  removed(index: number): void;
  stageToggled(index: number): void;
}

/** Texts of the waypoint menu, given by the caller (the map knows no language). */
export interface WaypointMenuTexts {
  stageEnd: string;
  remove: string;
}

// Contour lines are computed from the elevation tiles of the installed regions.
// `contour-dem` answers a missing tile with a tile of "no value", see tiles.rs.
const CONTOUR_DEM = {
  url: `${TILES}contour-dem/{z}/{x}/{y}`,
  encoding: "terrarium",
  maxzoom: 13,
  cacheSize: 100,
  timeoutMs: 10_000,
} as const;
const contours = new mlcontour.DemSource({ ...CONTOUR_DEM, worker: true });
addProtocol(contours.sharedDemProtocolId, contours.sharedDemProtocolV4);
addProtocol(contours.contourProtocolId, contours.contourProtocolV4);
const CONTOUR_TILES = contours.contourProtocolUrl({
  // Minor and major interval in metres from each zoom level on.
  thresholds: { 11: [100, 500], 12: [50, 250], 13: [20, 100], 15: [10, 50] },
  elevationKey: "ele",
  levelKey: "level",
  contourLayer: "contours",
  overzoom: 1,
});

/** The part of the contour source's worker connection that is not in its types. */
interface ContourWorker {
  managerId: number;
  actor: {
    send(name: "init", transfer: [], abort: AbortController, timer: undefined, message: object): Promise<unknown>;
  };
}

/**
 * Drops every cached elevation and contour tile, among them the "no value"
 * answers for places that may have elevation data now. Initialising the same
 * manager again replaces it inside the one shared worker, so neither a worker
 * nor a protocol is added. maplibre-contour is pinned, as this uses its internals.
 */
function forgetContourTiles(): Promise<unknown> {
  const { managerId, actor } = contours.manager as unknown as ContourWorker;
  const { url: demUrlPattern, encoding, maxzoom, cacheSize, timeoutMs } = CONTOUR_DEM;
  return actor.send("init", [], new AbortController(), undefined, {
    managerId,
    demUrlPattern,
    encoding,
    maxzoom,
    cacheSize,
    timeoutMs,
  });
}

const multiLine = (line: Line) => ({
  type: "Feature" as const,
  properties: {},
  geometry: { type: "MultiLineString" as const, coordinates: line.filter((segment) => segment.length > 1) },
});

/** Takes hillshading and contour lines out of a style, for regions without elevation data. */
function removeElevation(style: StyleSpecification): void {
  const elevation = ["dem", "contours"];
  style.layers = style.layers.filter((layer) => !("source" in layer && elevation.includes(layer.source)));
  for (const id of elevation) delete style.sources[id];
}

/** Fills in the placeholders of a bundled style (see scripts/make-style.py). */
async function offlineStyle(file: string, overview: RegionsOverview | null): Promise<StyleSpecification> {
  const response = await fetch(new URL(`map/styles/${file}`, document.baseURI));
  const style = (await response.json()) as StyleSpecification;
  const bounds = overview?.installed_bounds ?? null;
  if (!bounds) {
    // Nothing installed: nothing to request, show the bare background.
    return { ...style, sources: {}, layers: style.layers.filter((layer) => layer.type === "background") };
  }
  if (!overview?.dem_regions.length) removeElevation(style);
  const fill = (url: string) => url.replace("__TILES__", TILES).replace("__CONTOURS__", CONTOUR_TILES);
  style.glyphs = style.glyphs?.replace("__FONTS__", `${TILES}fonts/`);
  for (const [id, source] of Object.entries(style.sources)) {
    if (source.type !== "vector" && source.type !== "raster-dem" && source.type !== "raster") continue;
    source.tiles = source.tiles?.map(fill);
    // There are no tiles outside the installed regions; do not ask for them.
    source.bounds = bounds;
    if (id === "basemap") source.attribution = overview?.attribution.join(" · ");
  }
  return style;
}

/** Small pictograms for the points of interest, drawn once per style. */
function drawIcon(name: string): ImageData | null {
  const size = 32;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const c = canvas.getContext("2d");
  if (!c) return null;
  c.lineJoin = "round";
  c.lineWidth = 3;
  c.strokeStyle = "#ffffff";
  const shape = (fill: string, points: [number, number][]) => {
    c.beginPath();
    points.forEach(([x, y], i) => (i === 0 ? c.moveTo(x, y) : c.lineTo(x, y)));
    c.closePath();
    c.stroke();
    c.fillStyle = fill;
    c.fill();
  };
  const circle = (fill: string, radius: number) => {
    c.beginPath();
    c.arc(16, 16, radius, 0, 2 * Math.PI);
    c.stroke();
    c.fillStyle = fill;
    c.fill();
  };
  const house: [number, number][] = [[16, 5], [28, 15], [25, 15], [25, 26], [7, 26], [7, 15], [4, 15]];
  switch (name) {
    case "peak":
      shape("#4a3b28", [[16, 6], [27, 25], [5, 25]]);
      break;
    case "saddle":
      shape("#6d5a43", [[5, 9], [16, 16], [27, 9], [27, 14], [16, 22], [5, 14]]);
      break;
    case "hut":
      shape("#a3261f", house);
      break;
    case "shelter":
      shape("#6d5a43", house);
      break;
    case "viewpoint":
      shape("#6d5a43", [[16, 24], [4, 10], [28, 10]]);
      break;
    case "spring":
      circle("#2f7fc4", 7);
      break;
    default:
      return null;
  }
  return c.getImageData(0, 0, size, size);
}

export class MapView {
  readonly map: MapLibreMap;
  private readonly marker: Marker;
  private markerShown = false;
  private readonly lines: Record<string, Line> = { [LIVE]: [], [SELECTED]: [] };
  private waypointMarkers: Marker[] = [];
  private waypointMenu: Popup | null = null;
  private routeLegs: RouteLeg[] = [];
  private readonly cursor: Marker;
  private readonly searchPin = new Marker({ color: "#b3261e" });
  /** Counts style changes so that a slow style load cannot overwrite a newer choice. */
  private styleRequest = 0;
  /** The elevation data the contour cache was filled from. */
  private contourData: string | null = null;
  private slopeOpacity = 0.6;
  private area: Bbox | null = null;
  private areaHandles: Marker[] = [];
  private areaListener: ((area: Bbox | null) => void) | null = null;
  private drawStart: [number, number] | null = null;
  /** Where the rectangle was grabbed for moving, and where it was then. */
  private grab: { at: [number, number]; area: Bbox } | null = null;
  followPosition = true;

  constructor(container: string) {
    this.map = new MapLibreMap({ container, style: EMPTY_STYLE, center: [11.57, 47.68], zoom: 11, maxZoom: 19 });
    this.map.addControl(new NavigationControl(), "top-right");
    this.map.addControl(new ScaleControl({ unit: "metric" }), "bottom-right");

    const dot = document.createElement("div");
    dot.className = "position-marker";
    this.marker = new Marker({ element: dot });
    const cursor = document.createElement("div");
    cursor.className = "profile-cursor";
    this.cursor = new Marker({ element: cursor });

    this.map.on("styleimagemissing", (e) => {
      const image = drawIcon(e.id);
      if (image && !this.map.hasImage(e.id)) this.map.addImage(e.id, image, { pixelRatio: 2 });
    });
    // Every base style replaces all layers; put the tracks back on top.
    this.map.on("style.load", () => {
      this.addOverlays();
      this.setSlopeOpacity(this.slopeOpacity);
    });
  }

  /** Switches the base map. The offline style only covers the installed regions. */
  async setBaseStyle(base: BaseStyle, overview: RegionsOverview | null): Promise<void> {
    const request = ++this.styleRequest;
    const demData = overview ? JSON.stringify(overview.dem_regions) : this.contourData;
    if (this.contourData !== null && demData !== this.contourData) await forgetContourTiles();
    this.contourData = demData;
    const style = base === "online" ? ONLINE_STYLE : await offlineStyle(OFFLINE_STYLES[base], overview);
    if (request === this.styleRequest) this.map.setStyle(style, { diff: false });
  }

  /** Opacity (0 to 1) of the slope angle colours of the winter style. */
  setSlopeOpacity(opacity: number): void {
    this.slopeOpacity = opacity;
    if (this.map.getLayer(SLOPE)) this.map.setPaintProperty(SLOPE, "raster-opacity", opacity);
  }

  private addOverlays(): void {
    if (!this.map.getSource(AREA)) {
      this.map.addSource(AREA, { type: "geojson", data: areaData(this.area) });
      this.map.addLayer({ id: `${AREA}-fill`, type: "fill", source: AREA, paint: { "fill-color": "#0b63ce", "fill-opacity": 0.12 } });
      this.map.addLayer({
        id: `${AREA}-line`,
        type: "line",
        source: AREA,
        paint: { "line-color": "#0b63ce", "line-width": 2, "line-dasharray": [3, 2] },
      });
    }
    for (const [id, color] of OVERLAYS) {
      if (this.map.getSource(id)) continue;
      if (id === ROUTE) {
        this.addRouteLayers();
        continue;
      }
      this.map.addSource(id, { type: "geojson", data: multiLine(this.lines[id] ?? []) });
      this.map.addLayer({
        id,
        type: "line",
        source: id,
        layout: { "line-join": "round", "line-cap": "round" },
        paint: { "line-color": color, "line-width": 4, "line-opacity": 0.9 },
      });
    }
  }

  /** The planned route: one line per leg in its colour, failed legs dashed. */
  private addRouteLayers(): void {
    this.map.addSource(ROUTE, { type: "geojson", data: routeData(this.routeLegs) });
    // A white casing sets the route apart from trails drawn in similar colours.
    this.map.addLayer({
      id: `${ROUTE}-casing`,
      type: "line",
      source: ROUTE,
      filter: ["!", ["get", "failed"]],
      layout: { "line-join": "round", "line-cap": "round" },
      paint: { "line-color": "#ffffff", "line-width": 8, "line-opacity": 0.95 },
    });
    this.map.addLayer({
      id: ROUTE,
      type: "line",
      source: ROUTE,
      filter: ["!", ["get", "failed"]],
      layout: { "line-join": "round", "line-cap": "round" },
      paint: { "line-color": ["get", "color"], "line-width": 4.5 },
    });
    this.map.addLayer({
      id: `${ROUTE}-failed`,
      type: "line",
      source: ROUTE,
      filter: ["get", "failed"],
      paint: { "line-color": ["get", "color"], "line-width": 3, "line-dasharray": [2, 2] },
    });
  }

  private setLine(id: string, line: Line): void {
    this.lines[id] = line;
    this.map.getSource<GeoJSONSource>(id)?.setData(multiLine(line));
  }

  showPosition(fix: Fix): void {
    this.marker.setLngLat([fix.lon, fix.lat]);
    if (!this.markerShown) {
      this.marker.addTo(this.map);
      this.markerShown = true;
      if (this.followPosition) this.map.jumpTo({ center: [fix.lon, fix.lat], zoom: Math.max(this.map.getZoom(), 14) });
    } else if (this.followPosition) {
      // An animation redraws the map for its whole duration, every second while
      // recording. Small steps are not visible as jumps; only glide over large ones.
      const centre = this.map.project(this.map.getCenter());
      const target = this.map.project([fix.lon, fix.lat]);
      const pixels = Math.hypot(target.x - centre.x, target.y - centre.y);
      if (pixels < FOLLOW_GLIDE_PX) this.map.jumpTo({ center: [fix.lon, fix.lat] });
      else this.map.easeTo({ center: [fix.lon, fix.lat], duration: 500 });
    }
  }

  /** Replaces the live track, e.g. after a reload or when a recording ends. */
  setLiveLine(line: Line): void {
    this.setLine(
      LIVE,
      line.map((segment) => [...segment]),
    );
  }

  /** Adds newly recorded points to the live track. */
  appendLive(points: { segment: number; lon: number; lat: number }[]): void {
    if (points.length === 0) return;
    const line = this.lines[LIVE] ?? [];
    for (const p of points) {
      while (line.length <= p.segment) line.push([]);
      line[p.segment]!.push([p.lon, p.lat]);
    }
    this.setLine(LIVE, line);
  }

  liveLinePointCount(): number {
    return (this.lines[LIVE] ?? []).reduce((n, segment) => n + segment.length, 0);
  }

  /** Shows a stored track and zooms to it. An empty line clears it. */
  showTrack(line: Line): void {
    this.setLine(SELECTED, line);
    const points = line.flat();
    if (points.length === 0) return;
    const bounds = points.reduce((b, p) => b.extend(p), new LngLatBounds(points[0], points[0]));
    this.map.fitBounds(bounds, { padding: 60, maxZoom: 16, duration: 600 });
  }

  /** The colours of the route legs drawn on the map, failed ones included; for tests. */
  routeColors(): string[] {
    return this.routeLegs.filter((leg) => leg.line.length >= 2).map((leg) => leg.color);
  }

  /** Shows the planned route leg by leg; no legs clear it. */
  showRoute(legs: RouteLeg[], fit = false): void {
    this.routeLegs = legs;
    this.map.getSource<GeoJSONSource>(ROUTE)?.setData(routeData(legs));
    const points = legs.flatMap((leg) => leg.line.map((p): [number, number] => [p.lon, p.lat]));
    if (!fit || points.length === 0) return;
    const bounds = points.reduce((b, p) => b.extend(p), new LngLatBounds(points[0], points[0]));
    this.map.fitBounds(bounds, { padding: 60, maxZoom: 16, duration: 600 });
  }

  /** Replaces the numbered waypoint markers. They can be dragged; a click removes one. */
  /**
   * Shows the numbered waypoints; inner points in `stageEnds` end a stage.
   * Points can be dragged; a click opens a menu to end a stage there or to
   * remove the point.
   */
  setWaypoints(points: LatLon[], stageEnds: number[], handlers: WaypointHandlers, texts: WaypointMenuTexts): void {
    this.closeWaypointMenu();
    for (const marker of this.waypointMarkers) marker.remove();
    this.waypointMarkers = points.map((point, index) => {
      const element = document.createElement("div");
      element.className = stageEnds.includes(index) ? "waypoint-marker stage-end" : "waypoint-marker";
      element.textContent = String(index + 1);
      const marker = new Marker({ element, draggable: true }).setLngLat([point.lon, point.lat]).addTo(this.map);
      marker.on("dragend", () => {
        const { lng, lat } = marker.getLngLat();
        handlers.moved(index, { lat, lon: lng });
      });
      element.addEventListener("click", (e) => {
        // Not a click on the map: do not add a waypoint here.
        e.stopPropagation();
        const inner = index > 0 && index < points.length - 1;
        this.openWaypointMenu(marker, inner, stageEnds.includes(index), {
          stage: () => handlers.stageToggled(index),
          remove: () => handlers.removed(index),
        }, texts);
      });
      return marker;
    });
  }

  private openWaypointMenu(
    marker: Marker,
    inner: boolean,
    stageEnd: boolean,
    actions: { stage(): void; remove(): void },
    texts: WaypointMenuTexts,
  ): void {
    this.closeWaypointMenu();
    const menu = document.createElement("div");
    menu.className = "waypoint-menu";
    const button = (text: string, action: () => void, className = "") => {
      const b = document.createElement("button");
      b.textContent = text;
      b.className = className;
      b.addEventListener("click", (e) => {
        e.stopPropagation();
        this.closeWaypointMenu();
        action();
      });
      return b;
    };
    if (inner) {
      const stage = button(texts.stageEnd, actions.stage, "stage-toggle");
      stage.setAttribute("aria-pressed", String(stageEnd));
      menu.append(stage);
    }
    menu.append(button(texts.remove, actions.remove, "remove"));
    this.waypointMenu = new Popup({ closeButton: false, offset: 16, className: "waypoint-popup" })
      .setLngLat(marker.getLngLat())
      .setDOMContent(menu)
      .addTo(this.map);
  }

  private closeWaypointMenu(): void {
    this.waypointMenu?.remove();
    this.waypointMenu = null;
  }

  /** Calls `handler` with the position of every click on the map. */
  onClick(handler: (at: LatLon) => void): void {
    // While an area is being chosen, clicks belong to the area selection.
    this.map.on("click", (e) => {
      if (this.areaListener === null) handler({ lat: e.lngLat.lat, lon: e.lngLat.lng });
    });
  }

  /** Cross-hair cursor while clicks set waypoints. */
  setPicking(picking: boolean): void {
    this.map.getCanvas().style.cursor = picking ? "crosshair" : "";
  }

  /** Marks the chosen search result; `null` takes the mark away. */
  showSearchPin(at: LatLon | null): void {
    if (at) this.searchPin.setLngLat([at.lon, at.lat]).addTo(this.map);
    else this.searchPin.remove();
  }

  /** Where the waypoint markers are, in order; for the self-tests. */
  waypointPositions(): LatLon[] {
    return this.waypointMarkers.map((marker) => {
      const { lat, lng } = marker.getLngLat();
      return { lat, lon: lng };
    });
  }

  /** Where the search pin is, if it is shown; for the self-tests. */
  searchPinPosition(): LatLon | null {
    if (!this.searchPin.getElement().isConnected) return null;
    const { lat, lng } = this.searchPin.getLngLat();
    return { lat, lon: lng };
  }

  /** The middle of what the map shows. */
  centre(): LatLon {
    const { lat, lng } = this.map.getCenter();
    return { lat, lon: lng };
  }

  /** Moves the map to a search result, close enough to see the streets around it. */
  flyToResult(at: LatLon): void {
    this.map.easeTo({ center: [at.lon, at.lat], zoom: Math.max(this.map.getZoom(), 15), duration: 600 });
  }

  /** Marks the place the pointer is at in the elevation profile; `null` hides the mark. */
  showProfileCursor(at: LatLon | null): void {
    if (at) this.cursor.setLngLat([at.lon, at.lat]).addTo(this.map);
    else this.cursor.remove();
  }

  /**
   * Lets the user choose a rectangle on the map: a click sets one corner, the
   * rectangle follows the pointer, a second click sets the opposite corner.
   * Dragging keeps moving the map. A finished rectangle is moved by dragging
   * inside it and resized at its corner and edge handles. `onChange` hears of
   * every finished change, always with west < east and south < north.
   */
  startAreaSelection(onChange: (area: Bbox | null) => void): void {
    this.stopAreaSelection();
    this.areaListener = onChange;
    this.map.getCanvas().style.cursor = "crosshair";
    this.map.on("click", this.onAreaClick);
    this.map.on("mousemove", this.onAreaMove);
    this.map.on("mousedown", `${AREA}-fill`, this.onAreaGrab);
    this.map.on("mouseenter", `${AREA}-fill`, this.onAreaEnter);
    this.map.on("mouseleave", `${AREA}-fill`, this.onAreaLeave);
  }

  /** Ends the area selection and removes the rectangle. */
  stopAreaSelection(): void {
    this.map.off("click", this.onAreaClick);
    this.map.off("mousemove", this.onAreaMove);
    this.map.off("mousedown", `${AREA}-fill`, this.onAreaGrab);
    this.map.off("mouseenter", `${AREA}-fill`, this.onAreaEnter);
    this.map.off("mouseleave", `${AREA}-fill`, this.onAreaLeave);
    this.drawStart = null;
    this.grab = null;
    this.areaListener = null;
    this.setAreaShape(null);
    this.map.dragPan.enable();
    this.map.getCanvas().style.cursor = "";
  }

  /** Sets the chosen area as if the user had drawn it. */
  setArea(area: Bbox): void {
    this.drawStart = null;
    this.finishArea(normalize(area));
  }

  /** The rectangle as it is drawn now, finished or not. */
  currentArea(): Bbox | null {
    return this.area;
  }

  /** Whether the first corner of a new rectangle is set and the second is awaited. */
  isDrawingArea(): boolean {
    return this.drawStart !== null;
  }

  private readonly onAreaClick = (e: MapMouseEvent): void => {
    const at: [number, number] = [e.lngLat.lng, e.lngLat.lat];
    if (this.drawStart) {
      const area = normalize([...this.drawStart, ...at]);
      this.drawStart = null;
      // Two clicks on the same spot give no area; start over.
      if (area[0] === area[2] || area[1] === area[3]) {
        this.setAreaShape(null);
        this.areaListener?.(null);
      } else this.finishArea(area);
    } else if (!this.area) {
      this.drawStart = at;
      this.setAreaShape(normalize([...at, ...at]));
      this.areaListener?.(null);
    }
  };

  private readonly onAreaMove = (e: MapMouseEvent): void => {
    if (this.drawStart) {
      this.setAreaShape(normalize([...this.drawStart, e.lngLat.lng, e.lngLat.lat]));
    } else if (this.grab && this.area) {
      const dx = e.lngLat.lng - this.grab.at[0];
      const dy = e.lngLat.lat - this.grab.at[1];
      const [w, s, east, n] = this.grab.area;
      this.setAreaShape([w + dx, s + dy, east + dx, n + dy]);
      this.placeAreaHandles();
    }
  };

  /** Pressing inside a finished rectangle drags the rectangle instead of the map. */
  private readonly onAreaGrab = (e: MapMouseEvent): void => {
    if (this.drawStart || !this.area) return;
    e.preventDefault();
    this.grab = { at: [e.lngLat.lng, e.lngLat.lat], area: this.area };
    this.map.getCanvas().style.cursor = "grabbing";
    window.addEventListener(
      "mouseup",
      () => {
        this.grab = null;
        this.map.getCanvas().style.cursor = "move";
        if (this.area) this.areaListener?.(this.area);
      },
      { once: true },
    );
  };

  private readonly onAreaEnter = (): void => {
    if (!this.drawStart && this.area && !this.grab) this.map.getCanvas().style.cursor = "move";
  };

  private readonly onAreaLeave = (): void => {
    if (!this.grab) this.map.getCanvas().style.cursor = this.area ? "" : "crosshair";
  };

  private finishArea(area: Bbox): void {
    this.setAreaShape(area);
    this.map.getCanvas().style.cursor = "";
    this.placeAreaHandles();
    this.areaListener?.(area);
  }

  private setAreaShape(area: Bbox | null): void {
    this.area = area;
    this.map.getSource<GeoJSONSource>(AREA)?.setData(areaData(area));
    if (!area) {
      for (const handle of this.areaHandles) handle.remove();
      this.areaHandles = [];
    }
  }

  /** Puts a handle on every corner and edge of the area; dragging one moves its edges. */
  private placeAreaHandles(dragged: Marker | null = null): void {
    const area = this.area;
    if (!area) return;
    const [w, s, e, n] = area;
    const position = ([x, y]: (typeof AREA_HANDLES)[number]): [number, number] => [
      x === "w" ? w : x === "e" ? e : (w + e) / 2,
      y === "s" ? s : y === "n" ? n : (s + n) / 2,
    ];
    if (this.areaHandles.length === 0) {
      this.areaHandles = AREA_HANDLES.map((edges) => {
        const element = document.createElement("div");
        element.className = "area-handle";
        const marker = new Marker({ element, draggable: true }).setLngLat(position(edges)).addTo(this.map);
        const moved = () => {
          const current = this.area;
          if (!current) return;
          const { lng, lat } = marker.getLngLat();
          const next: Bbox = [...current];
          if (edges[0] === "w") next[0] = lng;
          if (edges[0] === "e") next[2] = lng;
          if (edges[1] === "s") next[1] = lat;
          if (edges[1] === "n") next[3] = lat;
          this.setAreaShape(normalize(next));
        };
        marker.on("drag", () => {
          moved();
          this.placeAreaHandles(marker);
        });
        marker.on("dragend", () => {
          moved();
          this.placeAreaHandles();
          if (this.area) this.areaListener?.(this.area);
        });
        return marker;
      });
    } else {
      this.areaHandles.forEach((marker, i) => {
        const edges = AREA_HANDLES[i];
        if (marker !== dragged && edges) marker.setLngLat(position(edges));
      });
    }
  }

  /** Zooms to a box given as west, south, east, north. */
  showBounds([west, south, east, north]: [number, number, number, number]): void {
    this.map.fitBounds([west, south, east, north], { padding: 40, duration: 600 });
  }
}
