/** One colour per routing profile, the same on the map and in the elevation profile. */
import type { Profile } from "./api";

export const PROFILE_COLORS: Record<Profile, string> = {
  road_bike: "#1565c0",
  trekking: "#00897b",
  mtb: "#e07b00",
  hike: "#c62828",
  car: "#546e7a",
  manual: "#6a1b9a",
};

/** A leg that could not be computed: drawn as a dashed straight line. */
export const FAILED_COLOR = "#d93025";
