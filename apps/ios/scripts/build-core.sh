#!/bin/bash
# Builds the Rust core for iOS: apps/ios/build/OutdoorCore.xcframework (device
# and simulator, arm64) and the Swift bindings in apps/ios/Generated/.
# Needs: rustup target add aarch64-apple-ios aarch64-apple-ios-sim
set -euo pipefail

ios="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$ios/../.." && pwd)"
cd "$root"

# Also for the C code in the core (SQLite); the same as in project.yml.
export IPHONEOS_DEPLOYMENT_TARGET=16.4
target_dir="${CARGO_TARGET_DIR:-$root/target}"

targets=(aarch64-apple-ios aarch64-apple-ios-sim)
for target in "${targets[@]}"; do
  cargo build --release -p core-ffi --target "$target"
done

generated="$ios/Generated"
rm -rf "$generated"
cargo run -p core-ffi --features bindgen --bin uniffi-bindgen -- generate \
  --library "$target_dir/${targets[0]}/release/liboutdoor_core.a" \
  --language swift --out-dir "$generated"

# The C side of the bindings, as a module the generated Swift file imports.
headers="$ios/build/Headers"
rm -rf "$headers" "$ios/build/OutdoorCore.xcframework"
mkdir -p "$headers"
mv "$generated/outdoor_coreFFI.h" "$headers/"
mv "$generated/outdoor_coreFFI.modulemap" "$headers/module.modulemap"

libraries=()
for target in "${targets[@]}"; do
  libraries+=(-library "$target_dir/$target/release/liboutdoor_core.a" -headers "$headers")
done
xcodebuild -create-xcframework "${libraries[@]}" -output "$ios/build/OutdoorCore.xcframework"
