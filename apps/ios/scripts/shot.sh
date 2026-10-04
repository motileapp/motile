#!/usr/bin/env bash
# Takes a picture of the simulator's screen.
#
#   scripts/shot.sh shot.png
set -euo pipefail
xcrun simctl io "${MOTILE_SIM:-iPhone 17}" screenshot "${1:-shot.png}" >/dev/null 2>&1
