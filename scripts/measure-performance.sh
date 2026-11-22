#!/usr/bin/env bash
# Measures what PLAN.md section 9 asks for and prints a report.
#
#   scripts/measure-performance.sh [region-id]
#
# Needs release builds (cargo build --release -p zerogrid-base -p track-cli),
# a built region in data/regions/<region-id> (default: isarwinkel-achensee)
# and a desktop session. Takes about three minutes. Nothing is written
# outside a temporary directory.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA="${OUTDOOR_DATA:-$ROOT/data}"
REGION="${1:-isarwinkel-achensee}"
APP="$ROOT/target/release/zerogrid-base"
CLI="$ROOT/target/release/track-cli"
GPX="$ROOT/fixtures/gpx/real/tour_sylvensteinsee.gpx"
# Lenggries station to the Brauneck summit, inside the test region.
ROUTE="${OUTDOOR_MEASURE_ROUTE:-47.6803,11.5741;47.6625,11.5233}"

[ -x "$APP" ] && [ -x "$CLI" ] || { echo "error: build the release binaries first" >&2; exit 1; }
[ -f "$DATA/regions/$REGION/manifest.json" ] || { echo "error: region $REGION is not built" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'kill $(jobs -p) 2>/dev/null || true; rm -rf "$WORK"' EXIT
mkdir -p "$WORK/data/regions"
ln -s "$DATA/regions/$REGION" "$WORK/data/regions/$REGION"
export OUTDOOR_DATA_DIR="$WORK/data"

# The app and the processes of its web view.
tree() {
  local pid="$1" child
  echo "$pid"
  for child in $(pgrep -P "$pid" || true); do tree "$child"; done
}
# CPU time used so far by a process tree, in clock ticks.
ticks() {
  local total=0 pid stat
  for pid in $(tree "$1"); do
    stat="$(cat "/proc/$pid/stat" 2>/dev/null)" || continue
    stat="${stat##*) }"                      # fields from the third on; 12 and 13 are utime, stime
    set -- $stat
    total=$((total + ${12} + ${13}))
  done
  echo "$total"
}
# Proportional memory of a process tree in MB.
memory() {
  local total=0 pid kb
  for pid in $(tree "$1"); do
    kb="$(awk '/^Pss:/ {print $2}' "/proc/$pid/smaps_rollup" 2>/dev/null)" || continue
    total=$((total + ${kb:-0}))
  done
  echo $((total / 1024))
}
# Runs the app for a while and reports CPU share of one core and memory.
observe() {
  local label="$1" settle="$2" seconds="$3"; shift 3
  env "$@" "$APP" >/dev/null 2>&1 &
  local pid=$! before after hz
  sleep "$settle"
  hz="$(getconf CLK_TCK)"
  before="$(ticks "$pid")"
  sleep "$seconds"
  after="$(ticks "$pid")"
  printf '%-28s CPU %5.1f %% of one core, memory %s MB\n' "$label" \
    "$(echo "($after - $before) * 100 / $hz / $seconds" | bc -l)" "$(memory "$pid")"
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}
json() { grep -o '^{.*}$' | tail -1; }

echo "== start (5 runs: process start to exit, page ready, map drawn)"
for _ in 1 2 3 4 5; do
  start="$(date +%s%N)"
  report="$(OUTDOOR_STARTUP_PROBE=1 "$APP" 2>/dev/null | json)"
  echo "  $(( ($(date +%s%N) - start) / 1000000 )) ms total  $report"
done

echo "== running"
observe "idle (map shown)" 15 30 OUTDOOR_NOTHING=1
observe "recording (1 fix per second)" 15 30 OUTDOOR_SELFTEST="$GPX" OUTDOOR_SELFTEST_SPEED=1

echo "== map animation (10 s zoom, rotation and tilt)"
OUTDOOR_FPS_PROBE=1 "$APP" 2>/dev/null | json

echo "== route (hike, $ROUTE; plan_ms: five computations in the core incl. elevation profile)"
OUTDOOR_SELFTEST_ROUTE="$ROUTE" "$APP" 2>/dev/null | json

echo "== long track (the real fixture 20 times in a row)"
python3 -I - "$GPX" "$WORK/long.gpx" <<'PY'
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
points = re.findall(r"<trkpt.*?</trkpt>", text, re.S)
head = text[: text.index("<trkseg>") + len("<trkseg>")]
with open(sys.argv[2], "w", encoding="utf-8") as out:
    out.write(head + "".join(points * 20) + "</trkseg></trk></gpx>\n")
print(f"  {len(points) * 20} points")
PY
for command in "stats $WORK/long.gpx --activity bike --json" "record $WORK/long.gpx --db $WORK/long.sqlite"; do
  start="$(date +%s%N)"
  "$CLI" $command >/dev/null
  echo "  track-cli ${command%% *}: $(( ($(date +%s%N) - start) / 1000000 )) ms"
done
