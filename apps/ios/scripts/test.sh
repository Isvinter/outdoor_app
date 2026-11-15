#!/bin/bash
# Runs the tests in the simulator; arguments go to xcodebuild, e.g.
#   scripts/test.sh -only-testing:OutdoorTests/RegionImportTests
#
# The tests with real routing data need a package of `isarwinkel-achensee`.
# ISARWINKEL_PACKAGE names it; without it, the script takes
# data/regions/isarwinkel-achensee, and if only the routing data lies there
# (routing.tar, as for the spike), it makes a package of that alone.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$(cd ../.. && pwd)"

package="${ISARWINKEL_PACKAGE:-}"
region="$root/data/regions/isarwinkel-achensee"
if [ -z "$package" ] && [ -f "$region/manifest.json" ]; then
  package="$region"
elif [ -z "$package" ] && [ -f "$region/routing.tar" ]; then
  package="$PWD/build/isarwinkel-achensee"
  if [ ! "$package/manifest.json" -nt "$region/routing.tar" ]; then
    rm -rf "$package"
    mkdir -p "$package"
    cp -c "$region/routing.tar" "$package/" 2>/dev/null || cp "$region/routing.tar" "$package/"
    size=$(stat -f %z "$package/routing.tar")
    sum=$(shasum -a 256 "$package/routing.tar" | cut -d' ' -f1)
    cat > "$package/manifest.json" <<JSON
{
  "format": 1,
  "id": "isarwinkel-achensee",
  "name": "Isarwinkel – Achensee (nur Routing, Test)",
  "bbox": [11.35, 47.35, 11.95, 47.80],
  "version": "202611150000",
  "attribution": ["© OpenStreetMap-Mitwirkende (ODbL)"],
  "routing": $(cat "$region/routing.json"),
  "files": [{ "name": "routing.tar", "size": $size, "sha256": "$sum" }],
  "size": $size
}
JSON
  fi
fi
# xcodebuild hands variables named TEST_RUNNER_… to the tests, without the prefix.
if [ -n "$package" ]; then
  export TEST_RUNNER_ISARWINKEL_PACKAGE="$package"
fi

xcodegen generate --quiet
xcodebuild test -project Outdoor.xcodeproj -scheme Outdoor \
  -destination "platform=iOS Simulator,name=${SIMULATOR:-iPhone 17}" "$@"
