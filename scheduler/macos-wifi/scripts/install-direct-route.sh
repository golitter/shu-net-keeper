#!/bin/bash
set -euo pipefail
export PATH=/usr/bin:/bin:/usr/sbin:/sbin
if [[ "$EUID" != 0 || "$(/usr/bin/uname -s)" != Darwin ]]; then
  echo "Run with sudo on macOS to install the narrowly scoped routing helper." >&2
  exit 1
fi
task_dir="$(cd "$(dirname "$0")/.." && pwd)"
helper_dir="/Library/Application Support/SHUWiFiKeeper"
log_dir="/Library/Logs/SHUWiFiKeeper"
plist_path="/Library/LaunchDaemons/com.shu-net-keeper.macos-wifi.direct-route.plist"
for path in "$helper_dir" "$log_dir" "$plist_path"; do
  if [[ -L "$path" ]]; then
    echo "Refusing symlink installation target: $path" >&2
    exit 1
  fi
done
/bin/bash -n "$task_dir/scripts/direct-route.sh"
/usr/bin/plutil -lint "$task_dir/route-daemon.plist"
/bin/launchctl bootout system/com.shu-net-keeper.macos-wifi.direct-route 2>/dev/null || true
/usr/bin/install -d -o root -g wheel -m 755 "$helper_dir"
/usr/bin/install -d -o root -g wheel -m 700 "$log_dir"
/usr/bin/install -o root -g wheel -m 755 "$task_dir/scripts/direct-route.sh" "$helper_dir/direct-route.sh"
/usr/bin/install -o root -g wheel -m 644 "$task_dir/route-daemon.plist" "$plist_path"
/bin/bash "$helper_dir/direct-route.sh"
/bin/launchctl enable system/com.shu-net-keeper.macos-wifi.direct-route
/bin/launchctl bootstrap system "$plist_path"
echo "Installed SHU portal-only WiFi scoped routing helper."
