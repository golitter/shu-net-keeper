#!/bin/bash
set -euo pipefail
if [[ "$#" != 3 ]]; then
  echo "Usage: write-agent.sh PLIST_PATH APP_PATH LOG_PATH" >&2
  exit 1
fi
plist_path="$1"
app_dir="$2"
log_path="$3"

# plutil escapes spaces and XML special characters in all paths.
plutil -create xml1 "$plist_path"
plutil -insert Label -string com.shu-net-keeper.macos-wifi "$plist_path"
plutil -insert ProgramArguments -array "$plist_path"
plutil -insert ProgramArguments.0 -string "$app_dir/Contents/MacOS/shu-wifi-service" "$plist_path"
plutil -insert RunAtLoad -bool YES "$plist_path"
plutil -insert KeepAlive -dictionary "$plist_path"
plutil -insert KeepAlive.SuccessfulExit -bool NO "$plist_path"
plutil -insert ThrottleInterval -integer 60 "$plist_path"
plutil -insert ProcessType -string Background "$plist_path"
plutil -insert LimitLoadToSessionType -string Aqua "$plist_path"
plutil -insert StandardOutPath -string "$log_path" "$plist_path"
plutil -insert StandardErrorPath -string "$log_path" "$plist_path"
plutil -insert Umask -integer 63 "$plist_path"
plutil -lint "$plist_path"
