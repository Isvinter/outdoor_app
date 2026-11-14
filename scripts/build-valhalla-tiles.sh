#!/usr/bin/env bash
# Builds the routing tiles (Valhalla) of a region.
#
#   scripts/build-valhalla-tiles.sh <region-id> [--max-mtb-scale N] [--force]
#                                           [--config FILE] [--package-dir DIR]
#
# Reads the clipped OSM extract written by build-region.sh (step "osm") and
# writes data/regions/<region-id>/routing.tar plus routing.json next to it.
#
# --max-mtb-scale N   Ways with mtb:scale above N are closed to bicycles
#                     (default 2: S0-S2 rideable, S3 and harder on foot only).
#                     Valhalla keeps no per-way MTB difficulty, so the limit is
#                     fixed when the tiles are built, not per route.
#
# --config FILE       Region definition instead of regions/<region-id>.json.
# --package-dir DIR   Write to DIR/<region-id>/ instead of data/regions/<region-id>/.
#
# Needs: bash, curl, docker. Uses the official Valhalla image; set
# OUTDOOR_DATA to move the data directory.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA="${OUTDOOR_DATA:-$ROOT/data}"
VALHALLA_VERSION="3.9.1"
IMAGE="ghcr.io/valhalla/valhalla:$VALHALLA_VERSION"

usage() { sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }
log() { printf '\n== %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

[ $# -ge 1 ] || usage 1
case "$1" in -h|--help) usage ;; esac
ID="$1"; shift
MAX_MTB_SCALE=2
FORCE=0
CONFIG=""
PACKAGE_DIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --max-mtb-scale) MAX_MTB_SCALE="${2:?--max-mtb-scale needs a value}"; shift 2 ;;
    --force) FORCE=1; shift ;;
    --config) CONFIG="${2:?--config needs a value}"; shift 2 ;;
    --package-dir) PACKAGE_DIR="${2:?--package-dir needs a value}"; shift 2 ;;
    -h|--help) usage ;;
    *) die "unknown option: $1" ;;
  esac
done
case "$MAX_MTB_SCALE" in [0-6]) ;; *) die "--max-mtb-scale must be 0 to 6" ;; esac

[ -n "$CONFIG" ] || CONFIG="$ROOT/regions/$ID.json"
[ -f "$CONFIG" ] || die "no region definition $CONFIG"
for tool in curl docker python3; do
  command -v "$tool" >/dev/null || die "$tool is required"
done

BUILD="$DATA/build/$ID"; OUT="${PACKAGE_DIR:-$DATA/regions}/$ID"; WORK="$BUILD/valhalla"
PBF="$BUILD/region.osm.pbf"
[ -s "$PBF" ] || die "$PBF is missing; run scripts/build-region.sh $ID --steps osm first"
if [ "$FORCE" = 0 ] && [ -s "$OUT/routing.tar" ] && \
   grep -q "\"max_mtb_scale\": $MAX_MTB_SCALE" "$OUT/routing.json" 2>/dev/null; then
  exit 0
fi

BBOX="$(python3 -I -c 'import json, sys; print(",".join(str(v) for v in json.load(open(sys.argv[1]))["bbox"]))' "$CONFIG")"
rm -rf "$WORK"
mkdir -p "$WORK" "$OUT" "$DATA/tools" "$DATA/sources/elevation"

# Valhalla decides in lua/graph.lua who may use a way. The stock file is kept
# as it is; our rule runs before it and only rewrites the tags it looks at.
STOCK_LUA="$DATA/tools/valhalla-graph-$VALHALLA_VERSION.lua"
if [ ! -s "$STOCK_LUA" ]; then
  log "downloading graph.lua of Valhalla $VALHALLA_VERSION"
  curl --fail --location --silent --show-error --output "$STOCK_LUA.part" \
    "https://raw.githubusercontent.com/valhalla/valhalla/$VALHALLA_VERSION/lua/graph.lua"
  mv "$STOCK_LUA.part" "$STOCK_LUA"
fi
{
  cat "$STOCK_LUA"
  cat <<LUA

-- outdoor_app: close ways harder than mtb:scale $MAX_MTB_SCALE to bicycles.
-- The MTB tags are removed as well, because the stock rules take any of them
-- as permission to ride.
local outdoor_stock_ways_proc = ways_proc
function ways_proc(kv, nokeys)
  local scale = tonumber(string.match(kv["mtb:scale"] or "", "^%s*(%d+)"))
  if scale and scale > $MAX_MTB_SCALE then
    kv["bicycle"] = "no"
    kv["mtb:scale"] = nil
    kv["mtb:scale:uphill"] = nil
    kv["mtb:scale:imba"] = nil
  end
  return outdoor_stock_ways_proc(kv, nokeys)
end
LUA
} > "$WORK/graph.lua"
cp "$PBF" "$WORK/region.osm.pbf"

valhalla() {
  docker run --rm --user "$(id -u):$(id -g)" --entrypoint "$1" \
    --volume "$WORK:/work" --volume "$DATA/sources/elevation:/elevation" "$IMAGE" "${@:2}"
}

log "configuring Valhalla"
valhalla valhalla_build_config \
  --mjolnir-tile-dir /work/tiles --mjolnir-tile-extract /work/routing.tar \
  --mjolnir-graph-lua-name /work/graph.lua --additional-data-elevation /elevation \
  --mjolnir-concurrency "$(nproc)" > "$WORK/valhalla.json"

log "fetching elevation for hill costs ($BBOX)"
valhalla valhalla_build_elevation --from-bbox "$BBOX" --config /work/valhalla.json --decompress

log "building routing tiles (bicycles up to mtb:scale $MAX_MTB_SCALE)"
valhalla valhalla_build_tiles --config /work/valhalla.json /work/region.osm.pbf

log "packing routing.tar"
valhalla valhalla_build_extract --config /work/valhalla.json --overwrite

mv "$WORK/routing.tar" "$OUT/routing.tar"
printf '{\n  "valhalla": "%s",\n  "max_mtb_scale": %s\n}\n' "$VALHALLA_VERSION" "$MAX_MTB_SCALE" > "$OUT/routing.json"
rm -rf "$WORK"
log "routing tiles: $OUT/routing.tar ($(du -h "$OUT/routing.tar" | cut -f1))"
