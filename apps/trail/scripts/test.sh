#!/bin/bash
# Runs the tests in the simulator; arguments go to xcodebuild, e.g.
#   scripts/test.sh -only-testing:OutdoorTests/RegionImportTests
#
# On a device: DEVICE=<udid> scripts/test.sh -only-testing:OutdoorUITests/AcceptanceUITests
# (the device unlocked, `isarwinkel-achensee` installed on it; the app keeps its data).
#
# The tests with real routing data need a package of `isarwinkel-achensee`.
# ISARWINKEL_PACKAGE names it; without it, the script takes
# data/regions/isarwinkel-achensee, and if only the routing data lies there
# (routing.tar, as for the spike), it makes a package of that alone.
#
# The tests of the region manager and of the routes from the desktop load over
# real HTTP: the script runs `share-serve` twice while the tests run, on port
# 8766 and, slowed down for the interrupted download, on 8767 (SHARE_PORT
# names another first port).
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

destination="platform=iOS Simulator,name=${SIMULATOR:-iPhone 17}"
# The simulator shares the Mac's network.
share_host=127.0.0.1
if [ -n "${DEVICE:-}" ]; then
  # The Mac's files are out of reach there: the test takes the installed region.
  destination="id=$DEVICE"
  unset TEST_RUNNER_ISARWINKEL_PACKAGE
  export TEST_RUNNER_ISARWINKEL_INSTALLED=1
  # A device reaches the Mac over the WLAN, if at all.
  share_host="$(ipconfig getifaddr en0 || true)"
fi

# The desktop's share, without the desktop app: the fixture region and a route.
cargo build --release --manifest-path "$root/Cargo.toml" -p core-share --bin share-serve
share_serve="${CARGO_TARGET_DIR:-$root/target}/release/share-serve"
share_data="$(mktemp -d)"
share_pids=()
trap 'kill "${share_pids[@]}" 2>/dev/null || true; rm -rf "$share_data"' EXIT
share_port="${SHARE_PORT:-8766}"
serve() { # <name> <port> [options of share-serve]
  local name="$1" port="$2"
  shift 2
  "$share_serve" --data "$share_data/$name" --port "$port" --seed-region "$root/fixtures/regions/mini" \
    --seed-route "Sylvenstein Test=47.572,11.545;47.578,11.555" "$@" > "$share_data/$name.log" 2>&1 &
  share_pids+=($!)
  for _ in $(seq 50); do
    grep -q '^serving ' "$share_data/$name.log" && return
    sleep 0.1
  done
  echo "share-serve did not start on port $port:" >&2
  cat "$share_data/$name.log" >&2
  exit 1
}
serve fast "$share_port"
# 200 KiB/s: the fixture takes about four seconds.
serve slow "$((share_port + 1))" --throttle 200
if [ -n "$share_host" ]; then
  export TEST_RUNNER_SHARE_URL="http://$share_host:$share_port/"
  export TEST_RUNNER_SHARE_SLOW_URL="http://$share_host:$((share_port + 1))/"
fi

xcodegen generate --quiet
xcodebuild test -project Outdoor.xcodeproj -scheme Outdoor -destination "$destination" "$@"
