#!/bin/bash
set -euo pipefail
export PATH=/usr/bin:/bin:/usr/sbin:/sbin
export LC_ALL=C

# This privileged helper never reads user configuration, passwords, or requests.
# Its only writable route is the interface-scoped /32 for the fixed SHU portal.
mode="${1:-ensure}"
if [[ "$#" -gt 1 || ( "$mode" != ensure && "$mode" != remove && "$mode" != --dry-run ) ]]; then
  echo "Usage: direct-route.sh [ensure|remove|--dry-run]" >&2
  exit 1
fi
if [[ "$mode" != --dry-run && "$EUID" != 0 ]]; then
  echo "Route maintenance requires administrator installation." >&2
  exit 1
fi
interface="$(/usr/sbin/networksetup -listallhardwareports | /usr/bin/awk '
  /^Hardware Port: (Wi-Fi|AirPort)$/ { wifi=1; next }
  wifi && /^Device: / { print $2; exit }
')"
[[ "$interface" =~ ^en[0-9]+$ ]] || exit 0
target=10.10.9.9

if [[ "$mode" == remove ]]; then
  /sbin/route -q -n delete -host -ifscope "$interface" "$target" 2>/dev/null || true
  exit 0
fi
gateway="$(/usr/sbin/ipconfig getoption "$interface" router 2>/dev/null || true)"
[[ "$gateway" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 0
IFS=. read -r octet1 octet2 octet3 octet4 <<< "$gateway"
for octet in "$octet1" "$octet2" "$octet3" "$octet4"; do
  (( 10#$octet <= 255 )) || exit 1
done
[[ "$gateway" != 0.0.0.0 && "$gateway" != 127.* && "$gateway" != 169.254.* ]] || exit 0

if [[ "$mode" == --dry-run ]]; then
  printf '/sbin/route -q -n add -host -ifscope %s %s %s -ifp %s\n' "$interface" "$target" "$gateway" "$interface"
  exit 0
fi
existing="$(/sbin/route -n get -ifscope "$interface" "$target" 2>/dev/null || true)"
old_destination="$(printf '%s\n' "$existing" | /usr/bin/awk '$1 == "destination:" {print $2}')"
old_gateway="$(printf '%s\n' "$existing" | /usr/bin/awk '$1 == "gateway:" {print $2}')"
old_interface="$(printf '%s\n' "$existing" | /usr/bin/awk '$1 == "interface:" {print $2}')"
if [[ "$old_destination" == "$target" && "$old_gateway" == "$gateway" && "$old_interface" == "$interface" ]]; then
  exit 0
fi
if [[ "$old_destination" == "$target" && "$old_interface" == "$interface" ]]; then
  /sbin/route -q -n change -host -ifscope "$interface" "$target" "$gateway" -ifp "$interface"
else
  /sbin/route -q -n add -host -ifscope "$interface" "$target" "$gateway" -ifp "$interface"
fi
echo "SHU portal scoped direct route refreshed: $target via $gateway on $interface"
