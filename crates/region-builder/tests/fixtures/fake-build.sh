#!/usr/bin/env bash
# Stands in for scripts/build-region.sh in tests. Takes the same arguments and
# behaves as FAKE_MODE says; without it the region id picks the mode ("mini"
# builds the fixture region, "fail", "hang" and "binary" do what they say,
# "interleaved" writes to stdout and stderr in turn and fails).
set -euo pipefail
ID="$1"; shift
PACKAGE_DIR=""
ARGS="$*"
while [ $# -gt 0 ]; do
  case "$1" in
    --package-dir) PACKAGE_DIR="$2"; shift 2 ;;
    --config) [ -f "$2" ] || { echo "error: no region definition $2" >&2; exit 1; }; shift 2 ;;
    *) shift ;;
  esac
done
MODE="${FAKE_MODE:-$ID}"
[ "$MODE" = mini ] && MODE=ok
FIXTURE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../fixtures/regions/mini" && pwd)"

case "$MODE" in
  ok)
    printf '\n== step osm (1/2)\n' >&2
    echo "arguments: $ARGS"
    printf '\n== step manifest (2/2)\n' >&2
    mkdir -p "$PACKAGE_DIR/$ID"
    cp "$FIXTURE"/* "$PACKAGE_DIR/$ID/"
    ;;
  fail)
    printf '\n== step osm (1/7)\n' >&2
    for i in $(seq 1 20); do echo "line $i"; done
    echo "error: something broke" >&2
    exit 3
    ;;
  interleaved)
    for i in $(seq 1 200); do
      if [ $((i % 2)) = 0 ]; then echo "out $i"; else echo "err $i" >&2; fi
    done
    exit 1
    ;;
  hang)
    printf '\n== step osm (1/7)\n' >&2
    sleep 611 &
    wait
    ;;
  binary)
    printf '\xff\xfe no newline at the end'
    ;;
  *) echo "error: unknown mode $MODE" >&2; exit 2 ;;
esac
