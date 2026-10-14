#!/usr/bin/env bash
# Has the GPUI app that scripts/dev-app.sh opened do something, as the user's mouse and keyboard
# would, and waits until it has. Each argument is a line that src/dev_drive.rs reads:
#
#   scripts/do.sh "click 85 330" "key cmd-n" "type hello" "mods cmd" "perform thread.next" "wait 1"
#
# Points count from the window's top left corner, as in a picture from scripts/shot.sh at 1x.
set -euo pipefail
cd "$(dirname "$0")/.."

DEV="$PWD/../macos/build/dev"
PIPE="$DEV/gpui/drive"
READY="$DEV/gpui/drive.ready"
PID="$(cut -d' ' -f1 "$DEV/gpui.pid" 2>/dev/null || true)"
if [ -z "$PID" ] || ! kill -0 "$PID" 2>/dev/null; then
    echo "The dev app isn't running. Start it with scripts/dev-app.sh." >&2
    exit 1
fi

# An app that was just started makes its pipe again; one left by the app before it has no reader.
for _ in $(seq 1 200); do
    [ "$(cat "$READY" 2>/dev/null)" = "$PID" ] && [ -p "$PIPE" ] && break
    sleep 0.05
done
if [ "$(cat "$READY" 2>/dev/null)" != "$PID" ] || [ ! -p "$PIPE" ]; then
    echo "The dev app isn't listening. Was it started with scripts/dev-app.sh?" >&2
    exit 1
fi

TOKEN="$$.$RANDOM"
rm -f "$DEV/gpui/drive.done"
# Opening the pipe waits for its reader, so a writer that can't get in is stopped.
{ printf '%s\n' "$@"; echo "done $TOKEN"; } > "$PIPE" &
WRITER=$!
for _ in $(seq 1 100); do
    kill -0 "$WRITER" 2>/dev/null || break
    sleep 0.05
done
if kill -0 "$WRITER" 2>/dev/null; then
    kill "$WRITER" 2>/dev/null || true
    echo "The dev app didn't take the lines." >&2
    exit 1
fi
for _ in $(seq 1 600); do
    [ "$(cat "$DEV/gpui/drive.done" 2>/dev/null)" = "$TOKEN" ] && exit 0
    sleep 0.05
done
echo "The dev app didn't finish in time." >&2
exit 1
