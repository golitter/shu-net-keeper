#!/bin/bash
set -euo pipefail

task_dir="$(cd "$(dirname "$0")/.." && pwd)"
if [[ "$(uname -s)" != Darwin ]]; then
  echo "This service must be built on macOS." >&2
  exit 1
fi
app_dir="$task_dir/dist/SHU WiFi Keeper.app"
mkdir -p "$app_dir/Contents/MacOS" "$task_dir/build/module-cache"
cargo build --locked --release --manifest-path "$task_dir/Cargo.toml"
xcrun swiftc -swift-version 5 -O \
  -target "$(uname -m)-apple-macos13.0" \
  -module-cache-path "$task_dir/build/module-cache" \
  -framework AppKit -framework CoreLocation -framework CoreWLAN \
  "$task_dir/src/Service.swift" -o "$app_dir/Contents/MacOS/shu-wifi-service"
cp "$task_dir/target/release/shu-wifi-login" "$app_dir/Contents/MacOS/shu-wifi-login"
cp "$task_dir/Info.plist" "$app_dir/Contents/Info.plist"

# Set SHU_SIGN_IDENTITY to a Developer ID identity for distribution.
# Ad-hoc signing is intended for local builds; SSID access must be verified locally.
codesign --force --sign "${SHU_SIGN_IDENTITY:--}" "$app_dir/Contents/MacOS/shu-wifi-login"
codesign --force --sign "${SHU_SIGN_IDENTITY:--}" \
  --entitlements "$task_dir/entitlements.plist" "$app_dir"
codesign --verify --deep --strict "$app_dir"
echo "Built: $app_dir"
