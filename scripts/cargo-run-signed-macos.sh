#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: cargo-run-signed-macos.sh <binary> [args...]" >&2
  exit 64
fi

binary="$1"
shift

if [[ "$(uname -s)" == "Darwin" && -n "${SPROUT_TAURI_IDENTIFIER:-}" ]]; then
  entitlements="${SPROUT_TAURI_ENTITLEMENTS:-}"

  if [[ -z "$entitlements" ]]; then
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    entitlements="${script_dir}/../desktop/src-tauri/Entitlements.plist"
  fi

  if [[ -f "$binary" && -f "$entitlements" ]]; then
    product_name="${SPROUT_TAURI_PRODUCT_NAME:-Sprout Dev}"
    bundle_dir="$(dirname "$binary")/${product_name}.app"
    bundle_binary="$bundle_dir/Contents/MacOS/$(basename "$binary")"
    info_plist="$bundle_dir/Contents/Info.plist"

    rm -rf "$bundle_dir"
    mkdir -p "$bundle_dir/Contents/MacOS" "$bundle_dir/Contents/Resources"
    cp "$binary" "$bundle_binary"
    chmod +x "$bundle_binary"

    export SPROUT_TAURI_BINARY="$bundle_binary"
    python3 - "$info_plist" <<'PY'
import os
import plistlib
import sys

plist_path = sys.argv[1]
executable = os.path.basename(os.environ["SPROUT_TAURI_BINARY"])
identifier = os.environ["SPROUT_TAURI_IDENTIFIER"]
product_name = os.environ.get("SPROUT_TAURI_PRODUCT_NAME", "Sprout Dev")

plist = {
    "CFBundleDevelopmentRegion": "en",
    "CFBundleDisplayName": product_name,
    "CFBundleExecutable": executable,
    "CFBundleIdentifier": identifier,
    "CFBundleName": product_name,
    "CFBundlePackageType": "APPL",
    "CFBundleShortVersionString": "0.3.5",
    "CFBundleVersion": "0.3.5",
    "LSMinimumSystemVersion": "13.0",
    "NSCameraUsageDescription": "Sprout needs camera access to take a profile photo.",
    "NSMicrophoneUsageDescription": "Sprout needs microphone access for voice huddles.",
    "NSPrincipalClass": "NSApplication",
}

with open(plist_path, "wb") as file:
    plistlib.dump(plist, file)
PY

    codesign \
      --force \
      --sign - \
      --entitlements "$entitlements" \
      "$bundle_dir" >/dev/null

    exec "$bundle_binary" "$@"
  fi
fi

exec "$binary" "$@"
