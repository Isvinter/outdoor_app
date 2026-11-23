#!/usr/bin/env bash
# Builds an offline region package from free data sources.
#
#   scripts/build-region.sh <region-id> [--steps osm,basemap,outdoor,dem,slope,contours,search,routing,manifest]
#                                       [--max-mtb-scale N] [--force] [--print-plan]
#                                       [--config FILE] [--package-dir DIR] [--refresh-osm]
#
# Reads regions/<region-id>.json (or FILE) and writes data/regions/<region-id>/
# (or DIR/<region-id>/) with
#   basemap.pmtiles   vector base map (OpenMapTiles schema, built with Planetiler)
#   outdoor.pmtiles   vector overlay: trails, routes, pistes, peaks, huts; layer `flying`
#                     (power lines, aerialways, free flying sites, obstacles) for Vario
#   dem.pmtiles       elevation tiles (Terrarium encoding, from Mapterhorn)
#   slope.pmtiles     slope angle classes for winter maps (tools/slope-tiles)
#   contours.pmtiles  contour lines as vector tiles up to zoom 14, for renderers
#                     that cannot derive them from dem.pmtiles (tools/contour-tiles)
#   search.sqlite     offline search: places, streets and POIs (tools/search-index)
#   routing.tar       routing tiles (Valhalla, see scripts/build-valhalla-tiles.sh);
#                     bicycles may not use ways above mtb:scale N (default 2)
#   manifest.json     name, bounds, template, files with size and SHA-256, attribution
# and data/regions/catalog.json listing all built regions. That directory (or
# a web server publishing it) is what the app takes as its catalog.
# The definition's optional "template" (urlaub, outdoor, fliegen; default outdoor)
# picks the default steps and the files in the manifest. Its id must end in
# -urlaub or -fliegen for those templates and in neither for outdoor (a definition
# without "template" is not checked). --print-plan prints steps, files and group
# and exits. --refresh-osm downloads the OSM extracts again instead of using the cached ones.
# Every step is announced on stderr as "== step <name> (<n>/<total>)".
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

usage() { sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }
log() { printf '\n== %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

[ $# -ge 1 ] || usage 1
case "$1" in -h|--help) usage ;; esac
ID="$1"; shift
STEPS=""
MAX_MTB_SCALE=2
FORCE=0
CONFIG=""
PACKAGE_DIR=""
REFRESH_OSM=0
PRINT_PLAN=0
while [ $# -gt 0 ]; do
  case "$1" in
    --steps) STEPS="${2:?--steps needs a value}"; shift 2 ;;
    --max-mtb-scale) MAX_MTB_SCALE="${2:?--max-mtb-scale needs a value}"; shift 2 ;;
    --force) FORCE=1; shift ;;
    --config) CONFIG="${2:?--config needs a value}"; shift 2 ;;
    --package-dir) PACKAGE_DIR="${2:?--package-dir needs a value}"; shift 2 ;;
    --refresh-osm) REFRESH_OSM=1; shift ;;
    --print-plan) PRINT_PLAN=1; shift ;;
    -h|--help) usage ;;
    *) die "unknown option: $1" ;;
  esac
done

[ -n "$CONFIG" ] || CONFIG="$ROOT/regions/$ID.json"
[ -f "$CONFIG" ] || die "no region definition $CONFIG"
command -v python3 >/dev/null || die "python3 is required"

# Reads a value from the region definition. Lists are printed one item per line.
# A second argument is the default for a missing key.
config() {
  python3 -I -c '
import json, sys
config = json.load(open(sys.argv[1]))
value = config[sys.argv[2]] if len(sys.argv) < 4 else config.get(sys.argv[2], sys.argv[3])
if isinstance(value, list):
    print("\n".join(str(v) for v in value))
else:
    print(value)' "$CONFIG" "$@"
}

[ "$(config id)" = "$ID" ] || die "$CONFIG describes region $(config id), not $ID"

# The template decides the default steps and the files of the package.
TEMPLATE_GIVEN="$(python3 -I -c 'import json, sys; print("template" in json.load(open(sys.argv[1])))' "$CONFIG")"
TEMPLATE="$(config template outdoor)"
case "$TEMPLATE" in
  urlaub)
    DEFAULT_STEPS="osm,basemap,search,routing,manifest"
    PACKAGE_FILES="basemap.pmtiles,search.sqlite,routing.tar" ;;
  outdoor)
    DEFAULT_STEPS="osm,basemap,outdoor,dem,slope,contours,search,routing,manifest"
    PACKAGE_FILES="basemap.pmtiles,outdoor.pmtiles,dem.pmtiles,slope.pmtiles,contours.pmtiles,search.sqlite,routing.tar" ;;
  fliegen)
    DEFAULT_STEPS="osm,basemap,outdoor,dem,contours,search,manifest"
    PACKAGE_FILES="basemap.pmtiles,outdoor.pmtiles,dem.pmtiles,contours.pmtiles,search.sqlite" ;;
  *) die "unknown template \"$TEMPLATE\" in $CONFIG (templates: urlaub, outdoor, fliegen)" ;;
esac
[ -n "$STEPS" ] || STEPS="$DEFAULT_STEPS"

# The id must tell the template, so a build never overwrites a package of another
# template. A definition without a template is old and not checked.
GROUP="$ID"
case "$TEMPLATE" in
  urlaub) GROUP="${ID%-urlaub}" ;;
  fliegen) GROUP="${ID%-fliegen}" ;;
esac
if [ "$TEMPLATE_GIVEN" = True ]; then
  case "$TEMPLATE:$ID" in
    urlaub:*-urlaub|fliegen:*-fliegen) ;;
    outdoor:*-urlaub|outdoor:*-fliegen|urlaub:*|fliegen:*)
      die "region id $ID does not fit template $TEMPLATE (urlaub ids end in -urlaub, fliegen ids in -fliegen, outdoor ids in neither)" ;;
  esac
fi

ALL_STEPS="osm basemap outdoor dem slope contours search routing manifest"
for step in ${STEPS//,/ }; do
  case " $ALL_STEPS " in *" $step "*) ;; *) die "unknown step: $step (steps: $ALL_STEPS)" ;; esac
done

if [ "$PRINT_PLAN" = 1 ]; then
  printf 'steps=%s\nfiles=%s\ngroup=%s\n' "$STEPS" "$PACKAGE_FILES" "$GROUP"
  exit 0
fi

BBOX="$(config bbox | paste -sd, -)"            # west,south,east,north
BASEMAP_MAXZOOM="$(config basemap_maxzoom)"
DEM_MAXZOOM="$(config dem_maxzoom)"

TOOLS="$DATA/tools"; SOURCES="$DATA/sources"; BUILD="$DATA/build/$ID"
if [ -n "$PACKAGE_DIR" ]; then
  case "$PACKAGE_DIR" in /*) ;; *) PACKAGE_DIR="$PWD/$PACKAGE_DIR" ;; esac
  OUT="$PACKAGE_DIR/$ID"
else
  OUT="$DATA/regions/$ID"
fi
# A package of another template in the directory must not be overwritten.
if [ -f "$OUT/manifest.json" ]; then
  EXISTING="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1], encoding="utf-8")).get("template", "outdoor"))' "$OUT/manifest.json")"
  [ "$EXISTING" = "$TEMPLATE" ] \
    || die "$OUT holds a $EXISTING package, but $ID is a $TEMPLATE definition"
fi
if [ "$STEPS" != manifest ]; then
  for tool in curl java docker cargo; do
    command -v "$tool" >/dev/null || die "$tool is required"
  done
fi
if [ -n "$PACKAGE_DIR" ]; then
  mkdir -p "$PACKAGE_DIR"
  PACKAGE_DIR="$(cd "$PACKAGE_DIR" && pwd)"
  OUT="$PACKAGE_DIR/$ID"
fi
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
  [ "$REFRESH_OSM" = 1 ] || stale "$REGION_PBF" || return 0
  local clips=() url name
  while read -r url; do
    name="$(basename "$url")"
    [ "$REFRESH_OSM" = 1 ] && rm -f "$SOURCES/osm/$name"
    fetch "$url" "$SOURCES/osm/$name"
    log "clipping $name to $BBOX"
    osmium extract --bbox "$BBOX" --strategy smart --overwrite \
      --output "$BUILD/clip-$name" "$SOURCES/osm/$name"
    clips+=("$BUILD/clip-$name")
  done < <(config osm)
  log "merging ${#clips[@]} extract(s)"
  if [ "${#clips[@]}" -eq 1 ]; then
    mv "${clips[0]}" "$REGION_PBF"
  else
    # Extracts downloaded on different days can hold the same object in two versions;
    # Planetiler rejects that. time-filter keeps only the newest version of each object.
    osmium merge --overwrite --output "$BUILD/merged.osm.pbf" "${clips[@]}"
    osmium time-filter --overwrite --output "$REGION_PBF" "$BUILD/merged.osm.pbf"
    rm -f "${clips[@]}" "$BUILD/merged.osm.pbf"
  fi
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

step_contours() {
  stale "$OUT/contours.pmtiles" || return 0
  [ -s "$OUT/dem.pmtiles" ] || die "run the dem step first"
  log "deriving contour line tiles"
  (cd "$ROOT" && cargo run --quiet --release -p contour-tiles -- \
    "$OUT/dem.pmtiles" "$BUILD/contours.pmtiles")
  mv "$BUILD/contours.pmtiles" "$OUT/contours.pmtiles"
}

step_search() {
  # Fresh OSM data makes the search file stale as well.
  [ "$REFRESH_OSM" = 1 ] || stale "$OUT/search.sqlite" || return 0
  [ -s "$REGION_PBF" ] || die "run the osm step first"
  log "building search file"
  local timestamp args=()
  timestamp="$(osmium fileinfo -g header.option.osmosis_replication_timestamp "$REGION_PBF" || true)"
  [ -z "$timestamp" ] || args+=(--osm-timestamp "$timestamp")
  osmium tags-filter --overwrite --output "$BUILD/search.osm.pbf" "$REGION_PBF" \
    n/place=city,town,village,suburb,quarter,neighbourhood,hamlet \
    w/highway=motorway,trunk,primary,secondary,tertiary,unclassified,residential,living_street,pedestrian,service,road \
    nwr/amenity=fuel,pharmacy,restaurant,cafe,fast_food,bar,pub,biergarten,parking \
    nwr/shop \
    nwr/tourism=hotel,hostel,guest_house,motel,apartment,chalet,alpine_hut,camp_site,caravan_site,attraction,museum,viewpoint,gallery,zoo,theme_park \
    nwr/historic=castle,monument,ruins,archaeological_site,fort,city_gate
  osmium export --overwrite --add-unique-id=type_id -f geojsonseq \
    --geometry-types=point,linestring,polygon \
    --output "$BUILD/search.geojsonseq" "$BUILD/search.osm.pbf"
  (cd "$ROOT" && cargo run --quiet --release -p search-index -- \
    "$BUILD/search.geojsonseq" "$BUILD/search.sqlite" --info "$OUT/search.json" ${args[@]+"${args[@]}"})
  mv "$BUILD/search.sqlite" "$OUT/search.sqlite"
  rm -f "$BUILD/search.osm.pbf" "$BUILD/search.geojsonseq"
}

step_routing() {
  local args=("$ID" --max-mtb-scale "$MAX_MTB_SCALE" --config "$CONFIG")
  [ -n "$PACKAGE_DIR" ] && args+=(--package-dir "$PACKAGE_DIR")
  # Fresh OSM data makes the existing routing tiles stale as well.
  if [ "$FORCE" = 1 ] || [ "$REFRESH_OSM" = 1 ]; then args+=(--force); fi
  "$ROOT/scripts/build-valhalla-tiles.sh" "${args[@]}"
}

step_manifest() {
  log "writing manifest"
  python3 -I - "$CONFIG" "$OUT" "$TEMPLATE" "$GROUP" "$PACKAGE_FILES" <<'PY'
import datetime, hashlib, json, os, sys

config = json.load(open(sys.argv[1]))
out, template, group = sys.argv[2:5]
package_files = sys.argv[5].split(",")
# An installed package has no routing.json any more; amending it keeps what
# its manifest says about the routing tiles.
previous = {}
if os.path.isfile(os.path.join(out, "manifest.json")):
    previous = json.load(open(os.path.join(out, "manifest.json"), encoding="utf-8"))

def describe(name):
    path = os.path.join(out, name)
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return {"name": name, "size": os.path.getsize(path), "sha256": digest.hexdigest()}

# Only the files of the template; an earlier build may have left others.
files = [describe(n) for n in package_files if os.path.exists(os.path.join(out, n))]
manifest = {
    "format": 1,
    "id": config["id"],
    "name": config["name"],
    "bbox": config["bbox"],
    "template": template,
    "group": group,
    "version": datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d%H%M"),
    "attribution": config["attribution"],
    "files": files,
    "size": sum(f["size"] for f in files),
}
# How the routing tiles were built, as noted by build-valhalla-tiles.sh.
if any(f["name"] == "routing.tar" for f in files):
    routing_json = os.path.join(out, "routing.json")
    if os.path.isfile(routing_json):
        manifest["routing"] = json.load(open(routing_json))
    elif "routing" in previous:
        manifest["routing"] = previous["routing"]
    else:
        sys.exit(f"{routing_json} is missing and the manifest does not say how the routing tiles were built")
# Counts of the search file, as noted by the search step.
if any(f["name"] == "search.sqlite" for f in files):
    search_json = os.path.join(out, "search.json")
    if os.path.isfile(search_json):
        manifest["search"] = json.load(open(search_json))
    elif "search" in previous:
        manifest["search"] = previous["search"]
    else:
        sys.exit(f"{search_json} is missing and the manifest does not describe the search file")
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
                        "size": m["size"], "path": name}
                       | {k: m[k] for k in ("template", "group") if k in m})
with open(os.path.join(root, "catalog.json"), "w", encoding="utf-8") as f:
    json.dump({"format": 1, "regions": regions}, f, ensure_ascii=False, indent=2)
    f.write("\n")

for f in files:
    print(f"  {f['name']:18} {f['size'] / 1e6:8.1f} MB")
print(f"  {'total':18} {manifest['size'] / 1e6:8.1f} MB")
PY
}

if [ "$STEPS" != manifest ]; then ensure_tools; fi
TOTAL=0
for step in $ALL_STEPS; do
  if wants "$step"; then TOTAL=$((TOTAL + 1)); fi
done
N=0
for step in $ALL_STEPS; do
  if wants "$step"; then
    N=$((N + 1))
    log "step $step ($N/$TOTAL)"
    "step_$step"
  fi
done
log "region package: $OUT"
