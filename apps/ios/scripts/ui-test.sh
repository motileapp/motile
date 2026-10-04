#!/usr/bin/env bash
# Runs the tests that use the app with fingers (MotileUITests) in the simulator, against the
# dev account of scripts/dev-app.sh.
#
#   scripts/ui-test.sh [-only-testing:MotileUITests/GestureTests/<test>]
set -euo pipefail
cd "$(dirname "$0")/.."
source ../macos/scripts/dev-stack.sh
SIM="${MOTILE_SIM:-iPhone 17}"

start_stack
TEST_RUNNER_MOTILE_AUTH_URL="$AUTH_URL" TEST_RUNNER_MOTILE_LOCAL=1 \
    TEST_RUNNER_MOTILE_SERVER_ADDR="127.0.0.1:$SERVER_PORT" TEST_RUNNER_MOTILE_DEMO_SIGN_IN=demo@motile.app \
    xcodebuild test -project Motile.xcodeproj -scheme Motile -destination "platform=iOS Simulator,name=$SIM" \
    -derivedDataPath build/derived "$@" > "$DEV/ios-test.log" 2>&1 || true
grep -E "error:|Test Case|\*\* TEST|Executed" "$DEV/ios-test.log"
