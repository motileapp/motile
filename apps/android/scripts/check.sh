#!/usr/bin/env bash
# Compiles the app's Kotlin and lists the errors, one build at a time however many ask at once, and
# with the core built last. With a path, only the errors of files under it.
#
#   scripts/check.sh [app/src/main/java/app/motile/transcript]
set -uo pipefail
cd "$(dirname "$0")/.."
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@21}"
LOCK=build/check.lock
mkdir -p build
until mkdir "$LOCK" 2>/dev/null; do sleep 2; done
trap 'rmdir "$LOCK"' EXIT
./gradlew compileDebugKotlin -Pmotile.skipCore --console=plain > build/check.log 2>&1
status=$?
grep -E "^e: " build/check.log | sed "s|file://$(pwd)/||" | grep -F "${1:-}" || true
[ $status -eq 0 ] && echo "✓ compiled" || echo "✗ the build failed (build/check.log)"
