#!/usr/bin/env bash
# Takes a picture of the window of the app that scripts/dev-app.sh opened.
#
#   scripts/shot.sh [file.png] [app|mac] [title]    (default build/dev/shot.png, of the GPUI app's
#                                                    largest window, or the one whose title has `title`)
set -euo pipefail
cd "$(dirname "$0")/.."

DEV="$PWD/build/dev"
OUT="${1:-$DEV/shot.png}"
PID="$(cut -d' ' -f1 "$DEV/${2:-app}.pid" 2>/dev/null || true)"
if [ -z "$PID" ] || ! kill -0 "$PID" 2>/dev/null; then
    echo "The dev app isn't running. Start it with scripts/dev-app.sh." >&2
    exit 1
fi

if [ ! -x "$DEV/window-id" ] || [ scripts/window-id.swift -nt "$DEV/window-id" ]; then
    swiftc -O scripts/window-id.swift -o "$DEV/window-id"
fi
for _ in $(seq 1 50); do
    WINDOW="$("$DEV/window-id" "$PID" ${3:+"$3"})" && break
    sleep 0.2
done
[ -n "${WINDOW:-}" ] || { echo "The dev app has no window." >&2; exit 1; }
screencapture -x -o -l "$WINDOW" "$OUT"
echo "$OUT"
