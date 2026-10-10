#!/usr/bin/env bash
# Opens the client signed in to an account that only exists on this Mac: a local auth server with
# the dev login, and a local server whose agent is scripts/fake-agent, with a project and a few
# threads (scripts/dev-stack.sh). All of it is kept in build/dev and left running, so the next
# run only builds what changed and restarts what was rebuilt.
#
#   scripts/dev-app.sh           build, start what isn't running, open the client
#   scripts/dev-app.sh --stop    stop the client, the servers and PostgreSQL
#
# Needs Xcode 26 or later, Rust and PostgreSQL (`brew install postgresql@17`). Removing
# build/dev after --stop starts over.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/dev-stack.sh
APP="$DEV/Motile.app"

if [ "${1:-}" = "--stop" ]; then
    stop app
    stop_stack
    exit 0
fi

start_stack
MOTILE_DEV_BUILD=1 scripts/build-app.sh

# The dev app is a copy with its own identifier, so it keeps its preferences apart from an
# installed Motile's. Signing changes the copy, so the version is that of what was built.
BUILT="$(swift build -c debug -Xswiftc -O --scratch-path .build/dev --show-bin-path)/Motile"
if ! current app "$BUILT"; then
    echo "▸ Opening the client…"
    stop app
    rm -rf "$APP"
    cp -R "build/Motile Dev.app" "$APP"
    plutil -replace CFBundleIdentifier -string app.motile.mac.dev "$APP/Contents/Info.plist"
    codesign --force --deep --sign - "$APP" >/dev/null 2>&1
    MOTILE_DATA_DIR="$DEV/app" nohup "$APP/Contents/MacOS/Motile" > "$DEV/app.log" 2>&1 &
    started app $! "$BUILT"
    # The window is laid out a moment after it appears.
    sleep 2
fi
echo "✓ The dev app is open (pid $(pid_of app)). scripts/shot.sh takes a picture of it."
