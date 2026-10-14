#!/usr/bin/env bash
# Opens the GPUI app on the dev stack of the Mac app (apps/macos/scripts/dev-stack.sh): a local
# auth server with the dev login and a local server whose agent is scripts/fake-agent, with a
# project and a few threads, on ports this tree claimed. The app is a device of its own on the
# account, with its data in the stack's `gpui` folder, so the Mac app can run beside it.
#
#   scripts/dev-app.sh           build, start what isn't running, open the app
#   scripts/dev-app.sh --mac     the same, and the Mac's dev app on the same account, to compare
#   scripts/dev-app.sh --stop    stop the apps, the servers and PostgreSQL
set -euo pipefail
cd "$(dirname "$0")/.."
GPUI="$PWD"
source ../macos/scripts/dev-stack.sh

if [ "${1:-}" = "--stop" ]; then
    stop gpui
    stop app
    stop_stack
    exit 0
fi

start_stack
# The app is built for debugging unless MOTILE_GPUI_RELEASE=1: it builds much faster.
if [ "${MOTILE_GPUI_RELEASE:-}" = "1" ]; then
    (cd "$GPUI" && cargo build --release)
    APP_BIN="$GPUI/target/release/motile-gpui"
else
    (cd "$GPUI" && cargo build)
    APP_BIN="$GPUI/target/debug/motile-gpui"
fi

if [ ! -d "$DEV/gpui" ]; then
    echo "▸ Signing the GPUI app in…"
    # The example is built first, so the sign-in has its seconds to finish.
    (cd "$ROOT" && cargo build --release -q -p motile-core --example drive)
    { echo '{"id": 1, "type": "dev_sign_in", "email": "demo@motile.app"}'; sleep 4; } |
        (cd "$ROOT" && cargo run --release -q -p motile-core --example drive -- "$DEV/gpui.new" "$AUTH_URL") >/dev/null
    mv "$DEV/gpui.new" "$DEV/gpui"
fi

if [ "${1:-}" = "--mac" ]; then
    ../macos/scripts/dev-app.sh
fi

if ! current gpui "$APP_BIN"; then
    echo "▸ Opening the app…"
    stop gpui
    MOTILE_DEV_DRIVE=1 MOTILE_DATA_DIR="$DEV/gpui" nohup "$APP_BIN" > "$DEV/gpui.log" 2>&1 &
    started gpui $! "$APP_BIN"
    # Ready once its driver listens, which scripts/do.sh waits for too.
    for _ in $(seq 1 200); do
        [ "$(cat "$DEV/gpui/drive.ready" 2>/dev/null)" = "$(pid_of gpui)" ] && break
        sleep 0.1
    done
fi
echo "✓ The GPUI app is open (pid $(pid_of gpui)). scripts/shot.sh takes a picture of it, scripts/do.sh drives it."
