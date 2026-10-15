#!/usr/bin/env bash
# Runs the tests that use the client with fingers (gestures/) in the emulator, against
# the dev account of scripts/dev-app.sh.
#
#   scripts/ui-test.sh [-Pandroid.testInstrumentationRunnerArguments.class=app.motile.gestures.GestureTests#<test>]
set -euo pipefail
cd "$(dirname "$0")/.."
source ../macos/scripts/dev-stack.sh
source scripts/emulator.sh

start_stack
boot_emulator
ARGUMENTS=android.testInstrumentationRunnerArguments
ANDROID_SERIAL="$SERIAL" ./gradlew :app:installDebug :gestures:connectedCheck \
    -P$ARGUMENTS.MOTILE_AUTH_URL="http://10.0.2.2:$AUTH_PORT" -P$ARGUMENTS.MOTILE_LOCAL=1 \
    -P$ARGUMENTS.MOTILE_SERVER_ADDR="10.0.2.2:$SERVER_PORT" -P$ARGUMENTS.MOTILE_DEMO_SIGN_IN=demo@motile.app \
    "$@" > "$DEV/android-test.log" 2>&1 || true
grep -E "Tests? (on|run)|FAILED|PASSED|tests completed|BUILD" "$DEV/android-test.log" || tail -20 "$DEV/android-test.log"
