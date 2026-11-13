/** The map view: base map, own position, live track and the selected track. */
import {
  LngLatBounds,
  Map as MapLibreMap,
  Marker,
  NavigationControl,
  ScaleControl,
  setWorkerUrl,
  type GeoJSONSource,
} from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type { Fix, Line } from "./api";

// The worker files are copied next to the bundle by vite.config.ts.
setWorkerUrl(new URL("maplibre/maplibre-gl-worker.mjs", document.baseURI).href);

const ONLINE_STYLE = "https://tiles.openfreemap.org/styles/liberty";
const LIVE = "live-track";
const SELECTED = "selected-track";

const multiLine = (line: Line) => ({
  type: "Feature" as const,
  properties: {},
  geometry: { type: "MultiLineString" as const, coordinates: line },
});

export class MapView {
  readonly map: MapLibreMap;
  private readonly marker: Marker;
  private markerShown = false;
  private liveLine: Line = [];
  private ready: Promise<void>;
  followPosition = true;

  constructor(container: string) {
    this.map = new MapLibreMap({ container, style: ONLINE_STYLE, center: [11.57, 47.68], zoom: 11 });
    this.map.addControl(new NavigationControl(), "top-right");
    this.map.addControl(new ScaleControl({ unit: "metric" }), "bottom-right");

    const dot = document.createElement("div");
    dot.className = "position-marker";
    this.marker = new Marker({ element: dot });

    this.ready = new Promise((resolve) => {
      this.map.once("load", () => {
        for (const [id, color, width] of [
          [SELECTED, "#7b3fb5", 4],
          [LIVE, "#d93025", 4],
        ] as const) {
          this.map.addSource(id, { type: "geojson", data: multiLine([]) });
          this.map.addLayer({
            id,
            type: "line",
            source: id,
            layout: { "line-join": "round", "line-cap": "round" },
            paint: { "line-color": color, "line-width": width, "line-opacity": 0.9 },
          });
        }
        resolve();
      });
    });
  }

  private async setLine(id: string, line: Line): Promise<void> {
    await this.ready;
    this.map.getSource<GeoJSONSource>(id)?.setData(multiLine(line.filter((segment) => segment.length > 1)));
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
    this.liveLine = line.map((segment) => [...segment]);
    void this.setLine(LIVE, this.liveLine);
  }

  /** Adds newly recorded points to the live track. */
  appendLive(points: { segment: number; lon: number; lat: number }[]): void {
    if (points.length === 0) return;
    for (const p of points) {
      while (this.liveLine.length <= p.segment) this.liveLine.push([]);
      this.liveLine[p.segment]!.push([p.lon, p.lat]);
    }
    void this.setLine(LIVE, this.liveLine);
  }

  liveLinePointCount(): number {
    return this.liveLine.reduce((n, segment) => n + segment.length, 0);
  }

  /** Shows a stored track and zooms to it. An empty line clears it. */
  showTrack(line: Line): void {
    void this.setLine(SELECTED, line);
    const points = line.flat();
    if (points.length === 0) return;
    const bounds = points.reduce((b, p) => b.extend(p), new LngLatBounds(points[0], points[0]));
    this.map.fitBounds(bounds, { padding: 60, maxZoom: 16, duration: 600 });
  }
}
