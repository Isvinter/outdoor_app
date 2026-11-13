#!/usr/bin/env bash
# Builds an offline region package from free data sources.
#
#   scripts/build-region.sh <region-id> [--steps osm,basemap,outdoor,dem,slope,manifest] [--force]
#
# Reads regions/<region-id>.json and writes data/regions/<region-id>/ with
#   basemap.pmtiles   vector base map (OpenMapTiles schema, built with Planetiler)
#   outdoor.pmtiles   vector overlay: trails, routes, pistes, peaks, huts
#   dem.pmtiles       elevation tiles (Terrarium encoding, from Mapterhorn)
#   slope.pmtiles     slope angle classes for winter maps (tools/slope-tiles)
#   manifest.json     name, bounds, files with size and SHA-256, attribution
# and data/regions/catalog.json listing all built regions. That directory (or
# a web server publishing it) is what the app takes as its catalog.
#
# Needs: bash, curl, python3, java >= 21, docker, cargo. Downloads are cached in
# data/sources and data/tools; set OUTDOOR_DATA to move the data directory.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA="${OUTDOOR_DATA:-$ROOT/data}"
PLANETILER_VERSION="0.10.2"
PMTILES_VERSION="1.31.2"
MAPTERHORN="https://download.mapterhorn.com"
JAVA_HEAP="${JAVA_HEAP:-8g}"

usage() { sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }
log() { printf '\n== %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

[ $# -ge 1 ] || usage 1
case "$1" in -h|--help) usage ;; esac
ID="$1"; shift
STEPS="osm,basemap,outdoor,dem,slope,manifest"
FORCE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --steps) STEPS="${2:?--steps needs a value}"; shift 2 ;;
    --force) FORCE=1; shift ;;
    -h|--help) usage ;;
    *) die "unknown option: $1" ;;
  esac
done

CONFIG="$ROOT/regions/$ID.json"
[ -f "$CONFIG" ] || die "no region definition $CONFIG"
for tool in curl python3 java docker cargo; do
  command -v "$tool" >/dev/null || die "$tool is required"
done

# Reads a value from the region definition. Lists are printed one item per line.
config() {
  python3 -I -c '
import json, sys
value = json.load(open(sys.argv[1]))[sys.argv[2]]
if isinstance(value, list):
    print("\n".join(str(v) for v in value))
else:
    print(value)' "$CONFIG" "$1"
}

BBOX="$(config bbox | paste -sd, -)"            # west,south,east,north
BASEMAP_MAXZOOM="$(config basemap_maxzoom)"
DEM_MAXZOOM="$(config dem_maxzoom)"

TOOLS="$DATA/tools"; SOURCES="$DATA/sources"; BUILD="$DATA/build/$ID"; OUT="$DATA/regions/$ID"
mkdir -p "$TOOLS" "$SOURCES/osm" "$SOURCES/planetiler" "$BUILD" "$OUT"

wants() { case ",$STEPS," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }
# A step runs if its output is missing or --force was given.
stale() { [ "$FORCE" = 1 ] || [ ! -s "$1" ]; }

fetch() { # url, target; resumes partial downloads
  [ -s "$2" ] && return 0
  log "downloading $1"
  curl --fail --location --retry 3 --progress-bar --continue-at - --output "$2.part" "$1"
  mv "$2.part" "$2"
}

ensure_tools() {
  fetch "https://github.com/onthegomap/planetiler/releases/download/v$PLANETILER_VERSION/planetiler.jar" \
    "$TOOLS/planetiler.jar"
  if [ ! -x "$TOOLS/pmtiles" ]; then
    fetch "https://github.com/protomaps/go-pmtiles/releases/download/v$PMTILES_VERSION/go-pmtiles_${PMTILES_VERSION}_Linux_x86_64.tar.gz" \
      "$TOOLS/pmtiles.tar.gz"
    tar -xzf "$TOOLS/pmtiles.tar.gz" -C "$TOOLS" pmtiles
    rm "$TOOLS/pmtiles.tar.gz"
  fi
}

osmium() { # runs osmium in a container with the data directory mounted at the same path
  docker image inspect outdoor-osmium >/dev/null 2>&1 \
    || docker build --quiet --tag outdoor-osmium "$ROOT/scripts/docker/osmium" >/dev/null
  docker run --rm --user "$(id -u):$(id -g)" --volume "$DATA:$DATA" outdoor-osmium "$@"
}

planetiler() { java "-Xmx$JAVA_HEAP" "$@"; }

REGION_PBF="$BUILD/region.osm.pbf"

step_osm() {
  stale "$REGION_PBF" || return 0
  local clips=() url name
  while read -r url; do
    name="$(basename "$url")"
    fetch "$url" "$SOURCES/osm/$name"
    log "clipping $name to $BBOX"
    osmium extract --bbox "$BBOX" --strategy smart --overwrite \
      --output "$BUILD/clip-$name" "$SOURCES/osm/$name"
    clips+=("$BUILD/clip-$name")
  done < <(config osm)
  log "merging ${#clips[@]} extract(s)"
  osmium merge --overwrite --output "$REGION_PBF" "${clips[@]}"
  rm -f "${clips[@]}"
}

step_basemap() {
  stale "$OUT/basemap.pmtiles" || return 0
  [ -s "$REGION_PBF" ] || die "run the osm step first"
  log "building base map (OpenMapTiles schema, zoom 0-$BASEMAP_MAXZOOM)"
  planetiler -jar "$TOOLS/planetiler.jar" \
    --osm-path="$REGION_PBF" --bounds="$BBOX" --maxzoom="$BASEMAP_MAXZOOM" \
    --download --download-dir="$SOURCES/planetiler" --tmpdir="$BUILD/tmp-basemap" \
    --output="$BUILD/basemap.pmtiles" --force
  mv "$BUILD/basemap.pmtiles" "$OUT/basemap.pmtiles"
  rm -rf "$BUILD/tmp-basemap"
}

step_outdoor() {
  stale "$OUT/outdoor.pmtiles" || return 0
  [ -s "$REGION_PBF" ] || die "run the osm step first"
  log "building outdoor overlay"
  planetiler -cp "$TOOLS/planetiler.jar" "$ROOT/scripts/tiles/OutdoorOverlay.java" \
    --osm-path="$REGION_PBF" --bounds="$BBOX" --maxzoom=14 \
    --tmpdir="$BUILD/tmp-outdoor" --output="$BUILD/outdoor.pmtiles" --force
  mv "$BUILD/outdoor.pmtiles" "$OUT/outdoor.pmtiles"
  rm -rf "$BUILD/tmp-outdoor"
}

# Mapterhorn publishes zoom 0-12 in planet.pmtiles and zoom 13+ in one archive
# per zoom-6 tile, named 6-<x>-<y>.pmtiles. Prints those intersecting the bbox.
highres_archives() {
  curl --fail --silent "$MAPTERHORN/download_urls.json" | python3 -I -c '
import json, math, sys
west, south, east, north = map(float, sys.argv[1].split(","))
def tile(lon, lat, z=6):
    n = 2 ** z
    x = int((lon + 180) / 360 * n)
    y = int((1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n)
    return x, y
(x0, y0), (x1, y1) = tile(west, north), tile(east, south)
data = json.load(sys.stdin)
items = data if isinstance(data, list) else data.get("items", [])
available = {item["name"] for item in items}
for x in range(x0, x1 + 1):
    for y in range(y0, y1 + 1):
        name = f"6-{x}-{y}.pmtiles"
        if name in available:
            print(name)' "$BBOX"
}

step_dem() {
  stale "$OUT/dem.pmtiles" || return 0
  local parts=() low_max name
  low_max=$(( DEM_MAXZOOM < 12 ? DEM_MAXZOOM : 12 ))
  log "extracting elevation tiles, zoom 0-$low_max"
  "$TOOLS/pmtiles" extract "$MAPTERHORN/planet.pmtiles" "$BUILD/dem-low.pmtiles" \
    --bbox="$BBOX" --maxzoom="$low_max"
  parts+=("$BUILD/dem-low.pmtiles")
  if [ "$DEM_MAXZOOM" -gt 12 ]; then
    while read -r name; do
      log "extracting elevation tiles, zoom 13-$DEM_MAXZOOM from $name"
      "$TOOLS/pmtiles" extract "$MAPTERHORN/$name" "$BUILD/dem-$name" \
        --bbox="$BBOX" --maxzoom="$DEM_MAXZOOM"
      parts+=("$BUILD/dem-$name")
    done < <(highres_archives)
  fi
  if [ "${#parts[@]}" -eq 1 ]; then
    mv "${parts[0]}" "$OUT/dem.pmtiles"
  else
    "$TOOLS/pmtiles" merge "${parts[@]}" "$BUILD/dem.pmtiles"
    mv "$BUILD/dem.pmtiles" "$OUT/dem.pmtiles"
    rm -f "${parts[@]}"
  fi
}

step_slope() {
  stale "$OUT/slope.pmtiles" || return 0
  [ -s "$OUT/dem.pmtiles" ] || die "run the dem step first"
  log "deriving slope angle tiles"
  (cd "$ROOT" && cargo run --quiet --release -p slope-tiles -- \
    "$OUT/dem.pmtiles" "$BUILD/slope.pmtiles" --zoom "$DEM_MAXZOOM")
  mv "$BUILD/slope.pmtiles" "$OUT/slope.pmtiles"
}

step_manifest() {
  log "writing manifest"
  python3 -I - "$CONFIG" "$OUT" <<'PY'
import datetime, hashlib, json, os, sys

config = json.load(open(sys.argv[1]))
out = sys.argv[2]

def describe(name):
    path = os.path.join(out, name)
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return {"name": name, "size": os.path.getsize(path), "sha256": digest.hexdigest()}

files = [describe(n) for n in ("basemap.pmtiles", "outdoor.pmtiles", "dem.pmtiles", "slope.pmtiles")
         if os.path.exists(os.path.join(out, n))]
manifest = {
    "format": 1,
    "id": config["id"],
    "name": config["name"],
    "bbox": config["bbox"],
    "version": datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d%H%M"),
    "attribution": config["attribution"],
    "files": files,
    "size": sum(f["size"] for f in files),
}
with open(os.path.join(out, "manifest.json"), "w", encoding="utf-8") as f:
    json.dump(manifest, f, ensure_ascii=False, indent=2)
    f.write("\n")
# The catalog lists every region package next to this one.
regions = []
root = os.path.dirname(out)
for name in sorted(os.listdir(root)):
    path = os.path.join(root, name, "manifest.json")
    if os.path.isfile(path):
        m = json.load(open(path, encoding="utf-8"))
        regions.append({"id": m["id"], "name": m["name"], "bbox": m["bbox"], "version": m["version"],
                        "size": m["size"], "path": name})
with open(os.path.join(root, "catalog.json"), "w", encoding="utf-8") as f:
    json.dump({"format": 1, "regions": regions}, f, ensure_ascii=False, indent=2)
    f.write("\n")

for f in files:
    print(f"  {f['name']:18} {f['size'] / 1e6:8.1f} MB")
print(f"  {'total':18} {manifest['size'] / 1e6:8.1f} MB")
PY
}

ensure_tools
for step in osm basemap outdoor dem slope manifest; do
  if wants "$step"; then "step_$step"; fi
done
log "region package: $OUT"
