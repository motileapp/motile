#!/usr/bin/env bash
# Opens the client in the Android emulator, signed in to the account of the Mac's dev app: the
# local auth server with the dev login and the local server whose agent is scripts/fake-agent
# (apps/macos/scripts/dev-stack.sh). The emulator has no window, so it works with the Mac's
# screen locked; scripts/shot.sh takes a picture of it.
#
#   scripts/dev-app.sh           build, start what isn't running, open the client
#   scripts/dev-app.sh --stop    shut the emulator down, stop the servers and PostgreSQL
#
# Needs a JDK 21, the Android SDK with an NDK, cargo-ndk and Rust's aarch64-linux-android target.
set -euo pipefail
cd "$(dirname "$0")/.."
source ../macos/scripts/dev-stack.sh
source scripts/emulator.sh

if [ "${1:-}" = "--stop" ]; then
    stop_emulator
    stop_stack
    exit 0
fi

start_stack
boot_emulator

echo "▸ Building the client…"
if ! ANDROID_SERIAL="$SERIAL" ./gradlew installDebug > "$DEV/android-build.log" 2>&1; then
    grep -E "^e: |error:|FAILED" "$DEV/android-build.log" | sort -u >&2
    exit 1
fi

echo "▸ Opening the client in $AVD…"
"$ADB" -s "$SERIAL" shell am force-stop "$PACKAGE"
"$ADB" -s "$SERIAL" shell run-as "$PACKAGE" sh -c "mkdir -p files && : > files/demo.steps"
# The emulator reaches the Mac at 10.0.2.2.
"$ADB" -s "$SERIAL" shell am start -n "$PACKAGE/app.motile.MainActivity" \
    --es MOTILE_AUTH_URL "http://10.0.2.2:$AUTH_PORT" --es MOTILE_LOCAL 1 \
    --es MOTILE_SERVER_ADDR "10.0.2.2:$SERVER_PORT" --es MOTILE_DEMO_SIGN_IN demo@motile.app \
    --es MOTILE_DEMO_SCRIPT "$STEPS" >/dev/null
sleep 2
echo "✓ The dev app is open in $AVD. scripts/shot.sh takes a picture of it, scripts/do.sh drives it."
