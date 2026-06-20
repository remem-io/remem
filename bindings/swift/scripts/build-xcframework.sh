#!/usr/bin/env bash
# Build librememhq_core.xcframework, bundling static libraries for every
# Apple platform/architecture slice this binding targets:
#   - macOS:            arm64, x86_64  (lipo'd into one macOS slice)
#   - iOS (device):      arm64
#   - iOS (simulator):   arm64, x86_64 (lipo'd into one simulator slice)
#
# Requires: macOS with Xcode installed, and the corresponding Rust targets:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin \
#                      aarch64-apple-ios aarch64-apple-ios-sim \
#                      x86_64-apple-ios-sim
#
# Usage (from anywhere):
#   bindings/swift/scripts/build-xcframework.sh
#
# Output: bindings/swift/build/RememHQCore.xcframework
set -euo pipefail

if [[ "$(uname)" != "Darwin" ]]; then
  echo "error: this script must be run on macOS (it invokes xcodebuild)." >&2
  exit 1
fi

if ! command -v xcodebuild &>/dev/null; then
  echo "error: xcodebuild not found. Install Xcode (not just the CLT)." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="$SCRIPT_DIR/../build"
FRAMEWORK_NAME="RememHQCore"
LIB_NAME="librememhq_core.a"
HEADER_DIR="$REPO_ROOT/rememhq-core/include"

DARWIN_TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
IOS_DEVICE_TARGETS=(aarch64-apple-ios)
IOS_SIM_TARGETS=(aarch64-apple-ios-sim x86_64-apple-ios-sim)
ALL_TARGETS=("${DARWIN_TARGETS[@]}" "${IOS_DEVICE_TARGETS[@]}" "${IOS_SIM_TARGETS[@]}")

echo "==> Checking required Rust targets are installed"
for target in "${ALL_TARGETS[@]}"; do
  if ! rustup target list --installed | grep -q "^${target}\$"; then
    echo "error: missing rust target '$target'." >&2
    echo "  run: rustup target add ${ALL_TARGETS[*]}" >&2
    exit 1
  fi
done

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

echo "==> Building rememhq-core (release) for each target"
for target in "${ALL_TARGETS[@]}"; do
  echo "    -> $target"
  (cd "$REPO_ROOT" && cargo build --release -p rememhq-core --target "$target")
done

# A "slice" here is one .a file that goes into one of the xcframework's
# platform/variant directories. macOS and iOS-simulator each need their
# multiple architectures lipo'd together into a single fat binary; iOS
# device today is arm64-only, so no lipo is needed for it, but we still
# go through the same staging step for consistency.
stage_slice() {
  local slice_name="$1"
  shift
  local inputs=("$@")

  local slice_dir="$BUILD_DIR/slices/$slice_name"
  mkdir -p "$slice_dir"

  if [[ ${#inputs[@]} -eq 1 ]]; then
    cp "${inputs[0]}" "$slice_dir/$LIB_NAME"
  else
    lipo -create "${inputs[@]}" -output "$slice_dir/$LIB_NAME"
  fi

  echo "    staged $slice_name: $(lipo -info "$slice_dir/$LIB_NAME" 2>/dev/null || echo 'single-arch')"
}

echo "==> Staging fat libraries per platform"

darwin_inputs=()
for target in "${DARWIN_TARGETS[@]}"; do
  darwin_inputs+=("$REPO_ROOT/target/$target/release/$LIB_NAME")
done
stage_slice "macos" "${darwin_inputs[@]}"

ios_device_inputs=()
for target in "${IOS_DEVICE_TARGETS[@]}"; do
  ios_device_inputs+=("$REPO_ROOT/target/$target/release/$LIB_NAME")
done
stage_slice "ios" "${ios_device_inputs[@]}"

ios_sim_inputs=()
for target in "${IOS_SIM_TARGETS[@]}"; do
  ios_sim_inputs+=("$REPO_ROOT/target/$target/release/$LIB_NAME")
done
stage_slice "ios-simulator" "${ios_sim_inputs[@]}"

# Each xcframework slice needs its own copy of the headers plus a module
# map, since xcodebuild -create-xcframework bundles a self-contained
# headers+lib pair per slice but does NOT generate a Clang module map for
# you — without one, `import CRemem` fails to resolve once this
# xcframework is consumed as a SwiftPM binaryTarget. The module map and
# umbrella header here intentionally mirror Sources/CRemem exactly, so
# Package.swift can swap between the local-build CRemem target and this
# xcframework's CRemem module without consumers changing their imports.
echo "==> Staging headers + module map for each slice"
for slice in macos ios ios-simulator; do
  headers_dir="$BUILD_DIR/slices/$slice/headers"
  mkdir -p "$headers_dir"
  cp "$HEADER_DIR/rememhq.h" "$headers_dir/"

  cat > "$headers_dir/crem_shim.h" <<'EOF'
#ifndef CREM_SHIM_H
#define CREM_SHIM_H
#include "rememhq.h"
#endif // CREM_SHIM_H
EOF

  cat > "$headers_dir/module.modulemap" <<'EOF'
module CRemem {
    header "crem_shim.h"
    export *
}
EOF
done

echo "==> Assembling $FRAMEWORK_NAME.xcframework"
xcodebuild -create-xcframework \
  -library "$BUILD_DIR/slices/macos/$LIB_NAME" -headers "$BUILD_DIR/slices/macos/headers" \
  -library "$BUILD_DIR/slices/ios/$LIB_NAME" -headers "$BUILD_DIR/slices/ios/headers" \
  -library "$BUILD_DIR/slices/ios-simulator/$LIB_NAME" -headers "$BUILD_DIR/slices/ios-simulator/headers" \
  -output "$BUILD_DIR/$FRAMEWORK_NAME.xcframework"

echo ""
echo "✅ Built $BUILD_DIR/$FRAMEWORK_NAME.xcframework"
echo ""
echo "This bundles its own CRemem module map, so it's a drop-in replacement"
echo "for the local CRemem source target. To consume it, change Package.swift:"
echo "  - replace the CRemem .target(...) with:"
echo "      .binaryTarget(name: \"CRemem\", path: \"build/$FRAMEWORK_NAME.xcframework\")"
echo "  - remove the unsafeFlags linkerSettings from the Remem target"
echo "    (the static library is now linked automatically via the binary target)"
echo "  - add \"-lc++\" to the Remem target's linkerSettings instead. Unlike the"
echo "    current cdylib, which resolves its C++ runtime dependency at load"
echo "    time, this static .a needs the consumer to link libc++ explicitly"
echo "    (libremem's C++ sources are compiled into this archive)."
echo "See bindings/swift/README.md for the full distribution plan."
