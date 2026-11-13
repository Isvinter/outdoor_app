/** The map view: base map, own position, live track and the selected track. */
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  addProtocol,
  LngLatBounds,
  Map as MapLibreMap,
  Marker,
  NavigationControl,
  ScaleControl,
  setWorkerUrl,
  type GeoJSONSource,
  type StyleSpecification,
} from "maplibre-gl";
import mlcontour from "maplibre-contour";
import "maplibre-gl/dist/maplibre-gl.css";
import type { Fix, Line, RegionsOverview } from "./api";

// The worker files are copied next to the bundle by vite.config.ts.
setWorkerUrl(new URL("maplibre/maplibre-gl-worker.mjs", document.baseURI).href);

export type BaseStyle = "topo" | "winter" | "online";

const OFFLINE_STYLES = { topo: "topo-summer.json", winter: "topo-winter.json" } as const;
const SLOPE = "slope";

const ONLINE_STYLE = "https://tiles.openfreemap.org/styles/liberty";
/** Base URL of the tile protocol served by the core, e.g. `tiles://localhost/`. */
const TILES = convertFileSrc("", "tiles");
const EMPTY_STYLE: StyleSpecification = { version: 8, sources: {}, layers: [] };
const LIVE = "live-track";
const SELECTED = "selected-track";
const OVERLAYS = [
  [SELECTED, "#7b3fb5"],
  [LIVE, "#d93025"],
] as const;

// Contour lines are computed from the elevation tiles of the installed regions.
const contours = new mlcontour.DemSource({
  url: `${TILES}dem/{z}/{x}/{y}`,
  encoding: "terrarium",
  maxzoom: 13,
  worker: true,
  cacheSize: 100,
  timeoutMs: 10_000,
});
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

const multiLine = (line: Line) => ({
  type: "Feature" as const,
  properties: {},
  geometry: { type: "MultiLineString" as const, coordinates: line.filter((segment) => segment.length > 1) },
});

/** Fills in the placeholders of a bundled style (see scripts/make-style.py). */
async function offlineStyle(file: string, overview: RegionsOverview | null): Promise<StyleSpecification> {
  const response = await fetch(new URL(`map/styles/${file}`, document.baseURI));
  const style = (await response.json()) as StyleSpecification;
  const bounds = overview?.installed_bounds ?? null;
  if (!bounds) {
    // Nothing installed: nothing to request, show the bare background.
    return { ...style, sources: {}, layers: style.layers.filter((layer) => layer.type === "background") };
  }
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
  /** Counts style changes so that a slow style load cannot overwrite a newer choice. */
  private styleRequest = 0;
  private slopeOpacity = 0.6;
  followPosition = true;

  constructor(container: string) {
    this.map = new MapLibreMap({ container, style: EMPTY_STYLE, center: [11.57, 47.68], zoom: 11, maxZoom: 19 });
    this.map.addControl(new NavigationControl(), "top-right");
    this.map.addControl(new ScaleControl({ unit: "metric" }), "bottom-right");

    const dot = document.createElement("div");
    dot.className = "position-marker";
    this.marker = new Marker({ element: dot });

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
    const style = base === "online" ? ONLINE_STYLE : await offlineStyle(OFFLINE_STYLES[base], overview);
    if (request === this.styleRequest) this.map.setStyle(style, { diff: false });
  }

  /** Opacity (0 to 1) of the slope angle colours of the winter style. */
  setSlopeOpacity(opacity: number): void {
    this.slopeOpacity = opacity;
    if (this.map.getLayer(SLOPE)) this.map.setPaintProperty(SLOPE, "raster-opacity", opacity);
  }

  private addOverlays(): void {
    for (const [id, color] of OVERLAYS) {
      if (this.map.getSource(id)) continue;
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
      this.map.easeTo({ center: [fix.lon, fix.lat], duration: 500 });
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

  /** Zooms to a box given as west, south, east, north. */
  showBounds([west, south, east, north]: [number, number, number, number]): void {
    this.map.fitBounds([west, south, east, north], { padding: 40, duration: 600 });
  }
}
