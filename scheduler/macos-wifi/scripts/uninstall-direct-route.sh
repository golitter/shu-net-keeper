#!/bin/bash
set -euo pipefail
if [[ "$EUID" != 0 || "$(/usr/bin/uname -s)" != Darwin ]]; then
  echo "Run with sudo on macOS." >&2
  exit 1
fi
plist_path="/Library/LaunchDaemons/com.shu-net-keeper.macos-wifi.direct-route.plist"
/bin/launchctl bootout system/com.shu-net-keeper.macos-wifi.direct-route 2>/dev/null || true
if [[ -f "/Library/Application Support/SHUWiFiKeeper/direct-route.sh" ]]; then
  /bin/bash "/Library/Application Support/SHUWiFiKeeper/direct-route.sh" remove
fi
if [[ -f "$plist_path" ]]; then
  /bin/mv "$plist_path" "$plist_path.disabled"
fi
echo "Direct routing helper stopped; its scoped SHU portal route removed. Files preserved."
