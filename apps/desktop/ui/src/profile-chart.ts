/** Elevation profile of a route as a small area chart. Drawing only. */
import type { ProfilePoint } from "./api";
import { distance, elevation } from "./format";

const SVG = "http://www.w3.org/2000/svg";
const WIDTH = 300;
const HEIGHT = 100;

/** How a route's profile is coloured and divided. */
export interface ChartParts<P> {
  /** Colour of the area under a point. */
  colorOf(point: P): string;
  /** Distances where a stage ends. */
  stageEnds: number[];
}

/**
 * Draws `points` into `target`. While the pointer is over the chart, `onHover`
 * receives the nearest point of the profile, and `null` when it leaves. With
 * `parts`, the area is coloured run by run and stage ends get a line.
 */
export function renderProfileChart<P extends ProfilePoint>(
  target: HTMLElement,
  points: P[],
  onHover: (point: P | null) => void,
  parts?: ChartParts<P>,
): void {
  const known = points.filter((p): p is P & { ele: number } => p.ele !== null);
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
  const x = (p: { distance_m: number }) => (p.distance_m / total) * WIDTH;
  const y = (ele: number) => HEIGHT - 4 - ((ele - low) / span) * (HEIGHT - 8);

  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", `0 0 ${WIDTH} ${HEIGHT}`);
  svg.setAttribute("preserveAspectRatio", "none");
  // One area per run of points of the same colour; each run starts where the
  // one before ended, so that the areas join without a gap.
  const runs: { color: string | null; points: (P & { ele: number })[] }[] = [];
  for (const p of known) {
    const color = parts?.colorOf(p) ?? null;
    const run = runs[runs.length - 1];
    if (run && run.color === color) run.points.push(p);
    else {
      const previous = run?.points[run.points.length - 1];
      runs.push({ color, points: previous ? [previous, p] : [p] });
    }
  }
  const areas = runs.map((run) => {
    const area = document.createElementNS(SVG, "path");
    area.setAttribute("class", "area");
    const head = run.points[0] ?? first;
    const tail = run.points[run.points.length - 1] ?? last;
    const outline = run.points.map((p) => `L${x(p).toFixed(1)},${y(p.ele).toFixed(1)}`).join("");
    area.setAttribute("d", `M${x(head).toFixed(1)},${HEIGHT}${outline}L${x(tail).toFixed(1)},${HEIGHT}Z`);
    if (run.color) {
      // Inline style: the stylesheet's default colours would win over attributes.
      area.style.fill = run.color;
      area.style.fillOpacity = "0.22";
      area.style.stroke = run.color;
    }
    return area;
  });
  const stages = (parts?.stageEnds ?? []).map((at) => {
    const line = document.createElementNS(SVG, "line");
    line.setAttribute("class", "stage");
    line.setAttribute("x1", String(x({ distance_m: at })));
    line.setAttribute("x2", String(x({ distance_m: at })));
    line.setAttribute("y1", "0");
    line.setAttribute("y2", String(HEIGHT));
    return line;
  });
  const hair = document.createElementNS(SVG, "line");
  hair.setAttribute("class", "hair");
  hair.setAttribute("y1", "0");
  hair.setAttribute("y2", String(HEIGHT));
  hair.setAttribute("visibility", "hidden");
  svg.append(...areas, ...stages, hair);

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
