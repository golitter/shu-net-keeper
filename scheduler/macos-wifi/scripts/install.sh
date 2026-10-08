#!/bin/bash
set -euo pipefail

task_dir="$(cd "$(dirname "$0")/.." && pwd)"
label="com.shu-net-keeper.macos-wifi"
domain="gui/$(id -u)"
app_dir="$HOME/Applications/SHU WiFi Keeper.app"
config_dir="$HOME/Library/Application Support/SHUWiFiKeeper"
config_path="$config_dir/config.toml"
log_dir="$HOME/Library/Logs/SHUWiFiKeeper"
plist_path="$HOME/Library/LaunchAgents/$label.plist"

if [[ "$(uname -s)" != Darwin || "$(id -u)" == 0 ]]; then
  echo "Run this script as the logged-in macOS user, without sudo." >&2
  exit 1
fi
if [[ "$#" -gt 1 ]]; then
  echo "Usage: bash scripts/install.sh [CONFIG.toml]" >&2
  exit 1
fi
source_config="${1:-$config_path}"
if [[ ! -f "$source_config" ]]; then
  echo "First copy config.example.toml to config.toml, fill in the exact SSID and credentials," >&2
  echo "then run: bash scripts/install.sh /absolute/path/to/config.toml" >&2
  exit 1
fi

bash "$task_dir/scripts/build.sh"
"$task_dir/dist/SHU WiFi Keeper.app/Contents/MacOS/shu-wifi-login" --check-config "$source_config"
umask 077
mkdir -p "$HOME/Applications" "$config_dir" "$log_dir" "$HOME/Library/LaunchAgents"
chmod 700 "$config_dir" "$log_dir"

# Stop the old job before updating the exact same bundle and service configuration.
launchctl bootout "$domain/$label" 2>/dev/null || true
if [[ "$source_config" != "$config_path" ]]; then
  if [[ -f "$config_path" ]]; then
    cp "$config_path" "$config_path.backup"
    chmod 600 "$config_path.backup"
  fi
  cp "$source_config" "$config_path"
fi
chmod 600 "$config_path"
ditto "$task_dir/dist/SHU WiFi Keeper.app" "$app_dir"

bash "$task_dir/scripts/write-agent.sh" "$plist_path" "$app_dir" "$log_dir/service.log"
launchctl enable "$domain/$label"
launchctl bootstrap "$domain" "$plist_path"
echo "Installed and started. Allow Location Services for SHU WiFi Keeper when prompted."
echo "Log: $log_dir/service.log"
echo "Status: launchctl print $domain/$label"
