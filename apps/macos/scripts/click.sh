#!/usr/bin/env bash
# Clicks in the window of the app that scripts/dev-app.sh opened, at a point counted from the
# window's top left corner, as in a picture from scripts/shot.sh. It moves the mouse pointer.
#
#   scripts/click.sh <x> <y>
set -euo pipefail
cd "$(dirname "$0")/.."

PID="$(cut -d' ' -f1 build/dev/app.pid 2>/dev/null || true)"
if [ -z "$PID" ] || ! kill -0 "$PID" 2>/dev/null; then
    echo "The dev app isn't running. Start it with scripts/dev-app.sh." >&2
    exit 1
fi

osascript -l JavaScript - "$PID" "$1" "$2" <<'JS'
ObjC.import('CoreGraphics')
function run([pid, x, y]) {
    const app = Application('System Events').processes.whose({ unixId: Number(pid) })[0]
    app.frontmost = true
    delay(0.2)
    const [left, top] = app.windows[0].position()
    const point = $.CGPointMake(left + Number(x), top + Number(y))
    for (const type of [$.kCGEventMouseMoved, $.kCGEventLeftMouseDown, $.kCGEventLeftMouseUp]) {
        $.CGEventPost($.kCGHIDEventTap, $.CGEventCreateMouseEvent(null, type, point, $.kCGMouseButtonLeft))
        delay(0.05)
    }
}
JS
