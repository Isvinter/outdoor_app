/** Map legend: what the line colours of the offline styles mean. Rendering only. */
import { t, type MessageKey } from "./i18n";
import type { BaseStyle } from "./map";

type Stroke = "solid" | "dashed" | "dotted" | "band";
type Entry = [key: MessageKey, color: string, stroke: Stroke];

// Colours as in scripts/make-style.py.
const TRAILS: Entry[] = [
  ["legend.path", "#5b4a36", "dashed"],
  ["legend.path_mountain", "#c4261d", "dashed"],
  ["legend.path_alpine", "#1f4fa3", "dotted"],
  ["legend.mtb_s0", "#2e9d4f", "band"],
  ["legend.mtb_s1", "#2b7bd6", "band"],
  ["legend.mtb_s2", "#d9662b", "band"],
  ["legend.mtb_s3", "#2a2a2a", "band"],
  ["legend.route_hiking", "#d32f2f", "band"],
  ["legend.route_bicycle", "#0a86b8", "band"],
  ["legend.route_mtb", "#e07b00", "band"],
];
const WINTER: Entry[] = [
  ["legend.piste_novice", "#3aa655", "solid"],
  ["legend.piste_easy", "#2b7bd6", "solid"],
  ["legend.piste_intermediate", "#d62b2b", "solid"],
  ["legend.piste_advanced", "#222222", "solid"],
  ["legend.nordic", "#0f8f8f", "dashed"],
  ["legend.skitour", "#c2187a", "dashed"],
  ["legend.sled", "#8a5fb0", "dotted"],
];
const OWN: Entry[] = [
  ["legend.live", "#d93025", "solid"],
  ["legend.track", "#7b3fb5", "solid"],
  ["legend.planned", "#0b63ce", "solid"],
];

/** Fills `target` with the legend of a base style. */
export function renderLegend(target: HTMLElement, base: BaseStyle): void {
  const entries = [...(base === "topo" ? TRAILS : base === "winter" ? WINTER : []), ...OWN];
  target.replaceChildren(
    ...entries.map(([key, color, stroke]) => {
      const item = document.createElement("li");
      const swatch = document.createElement("i");
      swatch.className = stroke;
      swatch.style.setProperty("--c", color);
      item.append(swatch, t(key));
      return item;
    }),
  );
}
