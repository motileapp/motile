#!/usr/bin/env bash
# Opens the client in the simulator, signed in to the account of the Mac's dev app: the local auth
# server with the dev login and the local server whose agent is scripts/fake-agent
# (apps/macos/scripts/dev-stack.sh). The simulator needs no window, so it works with the Mac's
# screen locked.
#
#   scripts/dev-app.sh           build, start what isn't running, open the client
#   scripts/dev-app.sh --stop    shut the simulator down, stop the servers and PostgreSQL
#
# MOTILE_SIM names the simulator (default "iPhone 17"), as `xcrun simctl list devices` does.
# Needs Xcode 26 or later, Rust with the aarch64-apple-ios-sim target, and PostgreSQL.
set -euo pipefail
cd "$(dirname "$0")/.."
source ../macos/scripts/dev-stack.sh
SIM="${MOTILE_SIM:-iPhone 17}"
BUNDLE=app.motile.ios

if [ "${1:-}" = "--stop" ]; then
    xcrun simctl shutdown "$SIM" 2>/dev/null || true
    stop_stack
    exit 0
fi

start_stack

echo "▸ Building the client…"
if ! xcodebuild -project Motile.xcodeproj -scheme Motile -configuration Debug \
    -destination "platform=iOS Simulator,name=$SIM" -derivedDataPath build/derived build > "$DEV/ios-build.log" 2>&1; then
    grep -E "error:" "$DEV/ios-build.log" | sort -u >&2
    exit 1
fi
APP="build/derived/Build/Products/Debug-iphonesimulator/Motile.app"

echo "▸ Opening the client in ${SIM}…"
xcrun simctl bootstatus "$SIM" -b >/dev/null
xcrun simctl install "$SIM" "$APP"
# The steps scripts/do.sh gives the client are read from here.
: > "$DEV/ios.steps"
SIMCTL_CHILD_MOTILE_AUTH_URL="$AUTH_URL" SIMCTL_CHILD_MOTILE_LOCAL=1 \
    SIMCTL_CHILD_MOTILE_SERVER_ADDR="127.0.0.1:$SERVER_PORT" \
    SIMCTL_CHILD_MOTILE_DEMO_SIGN_IN=demo@motile.app SIMCTL_CHILD_MOTILE_DEMO_SCRIPT="$DEV/ios.steps" \
    xcrun simctl launch --terminate-running-process "$SIM" "$BUNDLE" >/dev/null
sleep 2
echo "✓ The dev app is open in $SIM. scripts/shot.sh takes a picture of it, scripts/do.sh drives it."
