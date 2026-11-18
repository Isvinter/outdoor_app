#!/usr/bin/env bash
# Builds the outdoor overlay from fixtures/osm/flying.osm and checks the layer
# `flying` (and that `poi` still works). Needs Docker (image outdoor-osmium, built by
# scripts/build-region.sh), Java, data/tools/planetiler.jar and jq.
#
#   scripts/tiles/test-outdoor-overlay.sh
#
# Prints `ok` on success; otherwise the failed check, exit code 1.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="${OUTDOOR_DATA:-$ROOT/data}"
mkdir -p "$DATA/build"
WORK="$(mktemp -d "$DATA/build/test-outdoor-overlay.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

cp "$ROOT/fixtures/osm/flying.osm" "$WORK/flying.osm"
docker run --rm --user "$(id -u):$(id -g)" --volume "$WORK:$WORK" outdoor-osmium \
  cat --overwrite --output "$WORK/flying.osm.pbf" "$WORK/flying.osm"
java -Xmx1g -cp "$DATA/tools/planetiler.jar" "$ROOT/scripts/tiles/OutdoorOverlay.java" \
  --osm-path="$WORK/flying.osm.pbf" --bounds=10.99,46.99,11.02,47.02 --maxzoom=14 \
  --tmpdir="$WORK/tmp" --output="$WORK/outdoor.pmtiles" --force >"$WORK/planetiler.log" 2>&1 \
  || { tail -20 "$WORK/planetiler.log"; exit 1; }
cargo build -q -p tile-inspect --manifest-path "$ROOT/Cargo.toml"
INSPECT="$ROOT/target/debug/tile-inspect"

# All fixture objects lie inside one tile at every zoom (z14: x 8692, y 5762). Reading
# just that tile avoids counting objects twice that also reach a neighbour's buffer.
features() { # zoom layer
  "$INSPECT" "$WORK/outdoor.pmtiles" --zoom "$1" --bbox 11.0005,47.0005,11.0006,47.0006 --layer "$2"
}
for z in 9 11 12 14; do features "$z" flying >"$WORK/z$z.jsonl"; done
features 14 poi >"$WORK/poi.jsonl"

FAILED=0
check() { # description file jq-expression (must yield true)
  if [ "$(jq -s "$3" "$2")" != "true" ]; then
    echo "FAIL: $1"
    FAILED=1
  fi
}
count() { echo "map(select(.props.kind == \"$1\")) | length == $2"; }
Z14="$WORK/z14.jsonl"

check "one power_line with 110 kV and name" "$Z14" \
  'map(select(.props.kind == "power_line")) | length == 1 and .[0].props.voltage == 110 and .[0].props.name == "Test 110" and .[0].geometry == "line"'
check "two minor lines, one with 20 kV, one without voltage" "$Z14" \
  'map(select(.props.kind == "power_minor_line")) | length == 2 and (map(.props.voltage) | sort == [null, 20])'
check "no underground line or cable" "$Z14" 'map(select(.props.kind == "cable" or .props.location != null)) | length == 0'
check "goods ropeway" "$Z14" "$(count goods 1)"
check "zip line" "$Z14" "$(count zip_line 1)"
check "chair lift with name" "$Z14" 'map(select(.props.kind == "chair_lift" and .props.name == "Testlift")) | length == 1'
check "no pylon or station" "$Z14" 'map(select(.props.kind == "pylon" or .props.kind == "station")) | length == 0'
check "four takeoffs" "$Z14" "$(count takeoff 4)"
check "takeoff Startplatz Ost with attributes" "$Z14" \
  'map(select(.props.name == "Startplatz Ost")) | length == 1 and .[0].props == {"kind":"takeoff","name":"Startplatz Ost","ele":1500,"paragliding":true,"hanggliding":false,"direction":"E-SE"} and .[0].geometry == "point"'
check "two landings" "$Z14" "$(count landing 2)"
check "Landewiese is one point" "$Z14" \
  'map(select(.props.name == "Landewiese")) | length == 1 and .[0].geometry == "point" and .[0].props.kind == "landing"'
check "training site with winch" "$Z14" 'map(select(.props.kind == "training" and .props.winch == true)) | length == 1'
check "mast 45 m" "$Z14" 'map(select(.props.kind == "mast" and .props.height == 45)) | length == 1'
check "tower 30 m" "$Z14" 'map(select(.props.kind == "tower" and .props.height == 30)) | length == 1'
check "chimney without height" "$Z14" 'map(select(.props.kind == "chimney" and .props.height == null)) | length == 1'
check "two wind turbines, one 140 m" "$Z14" \
  'map(select(.props.kind == "wind_turbine")) | length == 2 and (map(.props.height) | sort == [null, 140])'

check "z9: only sites" "$WORK/z9.jsonl" \
  'length == 7 and all(.props.kind == "takeoff" or .props.kind == "landing" or .props.kind == "training")'
check "z11: power_line and chair_lift, no minor lines, goods, zip lines or obstacles" "$WORK/z11.jsonl" \
  '(map(.props.kind) | unique) == ["chair_lift", "landing", "power_line", "takeoff", "training"]'
check "z12: all line kinds" "$WORK/z12.jsonl" \
  '(map(.props.kind) | unique) == ["chair_lift", "chimney", "goods", "landing", "mast", "power_line", "power_minor_line", "takeoff", "tower", "training", "wind_turbine", "zip_line"]'

check "poi still has Testgipfel" "$WORK/poi.jsonl" \
  'map(select(.props.kind == "peak" and .props.name == "Testgipfel")) | length == 1'

if [ "$FAILED" = 1 ]; then
  echo "outputs kept in $WORK.kept"
  cp -r "$WORK" "$WORK.kept"
  exit 1
fi
echo ok
