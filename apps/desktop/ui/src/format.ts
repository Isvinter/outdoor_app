/** Presentation of numbers and dates. Values come from the core as SI units. */
import type { TrackStats } from "./api";
import { currentLocale, t, type MessageKey } from "./i18n";

const number = (value: number, digits: number): string =>
  value.toLocaleString(currentLocale(), { minimumFractionDigits: digits, maximumFractionDigits: digits });

export function distance(metres: number): string {
  return metres < 1000 ? `${number(metres, 0)} m` : `${number(metres / 1000, 2)} km`;
}

export function duration(seconds: number): string {
  const s = Math.round(seconds);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${Math.floor(s / 3600)}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
}

export function speed(metresPerSecond: number): string {
  return `${number(metresPerSecond * 3.6, 1)} km/h`;
}

export function elevation(metres: number | null): string {
  return metres === null ? "–" : `${number(metres, 0)} m`;
}

/** Storage size in megabytes (or gigabytes from 1000 MB on). */
export function bytes(count: number): string {
  const mb = count / 1e6;
  return mb < 1000 ? `${number(mb, mb < 10 ? 1 : 0)} MB` : `${number(mb / 1000, 1)} GB`;
}

export function dateTime(ms: number): string {
  return new Date(ms).toLocaleString(currentLocale(), { dateStyle: "medium", timeStyle: "short" });
}

/** Label/value pairs of a statistics block, in display order. */
export function statsRows(s: TrackStats): [MessageKey, string][] {
  return [
    ["stats.distance", distance(s.distance_m)],
    ["stats.total_time", duration(s.total_time_s)],
    ["stats.moving_time", duration(s.moving_time_s)],
    ["stats.avg_speed", speed(s.avg_moving_speed_mps)],
    ["stats.max_speed", speed(s.max_speed_mps)],
    ["stats.ascent", elevation(s.ascent_m)],
    ["stats.descent", elevation(s.descent_m)],
    ["stats.elevation", `${elevation(s.min_ele_m)} / ${elevation(s.max_ele_m)}`],
  ];
}

export function renderStats(target: HTMLElement, stats: TrackStats | null): void {
  target.replaceChildren(
    ...(stats ? statsRows(stats) : []).map(([key, value]) => {
      const row = document.createElement("div");
      const label = document.createElement("dt");
      const data = document.createElement("dd");
      label.textContent = t(key);
      data.textContent = value;
      row.append(label, data);
      return row;
    }),
  );
}
