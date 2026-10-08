#!/bin/bash
set -euo pipefail

label="com.shu-net-keeper.macos-wifi"
plist_path="$HOME/Library/LaunchAgents/$label.plist"
if [[ "$(uname -s)" != Darwin || "$(id -u)" == 0 ]]; then
  echo "Run as the logged-in macOS user, without sudo." >&2
  exit 1
fi
launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
if [[ -f "$plist_path" ]]; then
  backup_path="$plist_path.disabled"
  mv "$plist_path" "$backup_path"
  echo "LaunchAgent stopped; plist preserved at $backup_path"
fi
echo "The app, credentials and logs are preserved. Automatic startup is now disabled."
