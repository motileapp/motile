#!/usr/bin/env bash
# Has the dev app do something, as a finger would: the steps are the ones
# app/src/main/java/app/motile/DemoDriver.kt knows.
#
#   scripts/do.sh "sidebar open"
#   scripts/do.sh "type Add a rate limiter" send
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/emulator.sh
SERIAL="$(serial)"
[ -n "$SERIAL" ] || { echo "The emulator $AVD isn't running: scripts/dev-app.sh starts it" >&2; exit 1; }
for step in "$@"; do
    printf '%s\n' "$step" | "$ADB" -s "$SERIAL" shell "run-as $PACKAGE sh -c 'cat >> files/demo.steps'"
done
# The client reads the steps a few times a second.
sleep 1
