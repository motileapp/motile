#!/usr/bin/env bash
# Takes a picture of the emulator's screen.
#
#   scripts/shot.sh shot.png
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/emulator.sh
SERIAL="$(serial)"
[ -n "$SERIAL" ] || { echo "The emulator $AVD isn't running: scripts/dev-app.sh starts it" >&2; exit 1; }
"$ADB" -s "$SERIAL" exec-out screencap -p > "${1:-shot.png}"
