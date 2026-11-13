/** Elevation profile of a route as a small area chart. Drawing only. */
import type { ProfilePoint } from "./api";
import { distance, elevation } from "./format";

const SVG = "http://www.w3.org/2000/svg";
const WIDTH = 300;
const HEIGHT = 100;

/**
 * Draws `points` into `target`. While the pointer is over the chart, `onHover`
 * receives the nearest point of the profile, and `null` when it leaves.
 */
export function renderProfileChart(
  target: HTMLElement,
  points: ProfilePoint[],
  onHover: (point: ProfilePoint | null) => void,
): void {
  const known = points.filter((p): p is ProfilePoint & { ele: number } => p.ele !== null);
  const first = known[0];
  const last = known[known.length - 1];
  if (!first || !last || last.distance_m <= 0) {
    target.replaceChildren();
    return;
  }
  const total = last.distance_m;
  const low = Math.min(...known.map((p) => p.ele));
  const high = Math.max(...known.map((p) => p.ele));
  // Flat routes should look flat: never stretch less than 50 m over the full height.
  const span = Math.max(high - low, 50);
  const x = (p: ProfilePoint) => (p.distance_m / total) * WIDTH;
  const y = (ele: number) => HEIGHT - 4 - ((ele - low) / span) * (HEIGHT - 8);

  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", `0 0 ${WIDTH} ${HEIGHT}`);
  svg.setAttribute("preserveAspectRatio", "none");
  const area = document.createElementNS(SVG, "path");
  area.setAttribute("class", "area");
  const outline = known.map((p) => `L${x(p).toFixed(1)},${y(p.ele).toFixed(1)}`).join("");
  area.setAttribute("d", `M${x(first).toFixed(1)},${HEIGHT}${outline}L${x(last).toFixed(1)},${HEIGHT}Z`);
  const hair = document.createElementNS(SVG, "line");
  hair.setAttribute("class", "hair");
  hair.setAttribute("y1", "0");
  hair.setAttribute("y2", String(HEIGHT));
  hair.setAttribute("visibility", "hidden");
  svg.append(area, hair);

  const scale = document.createElement("div");
  scale.className = "scale";
  const left = document.createElement("span");
  const right = document.createElement("span");
  const range = `${elevation(low)} – ${elevation(high)}`;
  left.textContent = range;
  right.textContent = distance(total);
  scale.append(left, right);

  svg.addEventListener("pointermove", (e) => {
    const box = svg.getBoundingClientRect();
    const at = ((e.clientX - box.left) / box.width) * total;
    let nearest = first;
    for (const p of known) {
      if (Math.abs(p.distance_m - at) < Math.abs(nearest.distance_m - at)) nearest = p;
    }
    hair.setAttribute("x1", String(x(nearest)));
    hair.setAttribute("x2", String(x(nearest)));
    hair.setAttribute("visibility", "visible");
    left.textContent = `${elevation(nearest.ele)} · ${distance(nearest.distance_m)}`;
    onHover(nearest);
  });
  svg.addEventListener("pointerleave", () => {
    hair.setAttribute("visibility", "hidden");
    left.textContent = range;
    onHover(null);
  });
  target.replaceChildren(svg, scale);
}
