#!/bin/bash
# Runs the tests in the simulator; arguments go to xcodebuild, e.g.
#   scripts/test.sh -only-testing:OutdoorTests/RegionImportTests
set -euo pipefail
cd "$(dirname "$0")/.."
xcodegen generate --quiet
xcodebuild test -project Outdoor.xcodeproj -scheme Outdoor \
  -destination "platform=iOS Simulator,name=${SIMULATOR:-iPhone 17}" "$@"
