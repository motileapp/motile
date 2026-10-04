#!/usr/bin/env bash
# Opens the GPUI app signed in to an account that only exists on this Mac: a local auth server
# with the dev login, and a local server whose agent is scripts/fake-agent, with a project and a
# few threads. All of it is kept in build/dev and left running, so the next run only builds what
# changed and restarts what was rebuilt. Its ports are apart from those of the Mac app's dev
# stack, so both can run at once; set MOTILE_DEV_PORTS="pg auth server" to pick others.
#
#   scripts/dev-app.sh           build, start what isn't running, open the app
#   scripts/dev-app.sh --mac     the same, and the Mac app on the same account, to compare
#   scripts/dev-app.sh --stop    stop the apps, the servers and PostgreSQL
set -euo pipefail
cd "$(dirname "$0")/.."

ROOT="$(cd ../.. && pwd)"
BIN="$ROOT/target/release"
DEV="$PWD/build/dev"
PROJECT="$DEV/api"
read -r PG_PORT AUTH_PORT SERVER_PORT <<< "${MOTILE_DEV_PORTS:-5483 3159 47683}"
AUTH_URL="http://127.0.0.1:$AUTH_PORT"
mkdir -p "$DEV"

if ! command -v initdb >/dev/null && command -v brew >/dev/null; then
    export PATH="$(brew --prefix postgresql@17)/bin:$PATH"
fi

version() { shasum "$1" | cut -d' ' -f1; }
pid_of() { cut -d' ' -f1 "$DEV/$1.pid" 2>/dev/null || true; }
alive() {
    local pid
    pid="$(pid_of "$1")"
    [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null
}
current() { alive "$1" && [ "$(cut -d' ' -f2 "$DEV/$1.pid")" = "$(version "$2")" ]; }
stop() {
    alive "$1" || return 0
    local pid
    pid="$(pid_of "$1")"
    kill "$pid"
    while kill -0 "$pid" 2>/dev/null; do sleep 0.1; done
}
started() { echo "$2 $(version "$3")" > "$DEV/$1.pid"; }
field() { sed -n "s/.*\"$1\":\"\([^\"]*\)\".*/\1/p"; }
post() { curl -fsS "$AUTH_URL$1" -H 'content-type: application/json' "${@:2}"; }

if [ "${1:-}" = "--stop" ]; then
    stop app
    stop mac
    stop server
    stop auth
    [ -d "$DEV/pg" ] && pg_ctl -D "$DEV/pg" stop -m fast >/dev/null 2>&1 || true
    exit 0
fi

echo "▸ Building…"
(cd "$ROOT" && cargo build --release -p motile-server -p motile-auth)
# The app is built for debugging unless MOTILE_GPUI_RELEASE=1: it builds much faster.
if [ "${MOTILE_GPUI_RELEASE:-}" = "1" ]; then
    cargo build --release
    APP_BIN="$PWD/target/release/motile-gpui"
else
    cargo build
    APP_BIN="$PWD/target/debug/motile-gpui"
fi

if ! pg_ctl -D "$DEV/pg" status >/dev/null 2>&1; then
    echo "▸ Starting PostgreSQL…"
    command -v initdb >/dev/null || { echo "PostgreSQL is missing: brew install postgresql@17" >&2; exit 1; }
    [ -d "$DEV/pg" ] || initdb -D "$DEV/pg" -U motile --auth=trust >/dev/null
    pg_ctl -D "$DEV/pg" -o "-p $PG_PORT -k $DEV -c listen_addresses=127.0.0.1" -l "$DEV/postgres.log" -w start >/dev/null
fi

if ! current auth "$BIN/motile-auth"; then
    echo "▸ Starting the auth server…"
    stop auth
    PUBLIC_URL="$AUTH_URL" PORT="$AUTH_PORT" DATABASE_URL="postgres://motile@127.0.0.1:$PG_PORT/postgres" \
        GOOGLE_CLIENT_ID=dev GOOGLE_CLIENT_SECRET=dev DEV_LOGIN=1 \
        nohup "$BIN/motile-auth" > "$DEV/auth.log" 2>&1 &
    started auth $! "$BIN/motile-auth"
fi
for _ in $(seq 1 50); do
    curl -fsS "$AUTH_URL/healthz" >/dev/null 2>&1 && break
    sleep 0.2
done

export MOTILE_AUTH_URL="$AUTH_URL" MOTILE_LOCAL=1 MOTILE_SERVER_ADDR="127.0.0.1:$SERVER_PORT"

if [ ! -d "$DEV/server" ]; then
    echo "▸ Setting up the server…"
    verifier="$(openssl rand -hex 32)"
    challenge="$(printf %s "$verifier" | shasum -a 256 | cut -d' ' -f1)"
    code="$(post /api/dev/login -d "{\"email\":\"demo@motile.app\",\"challenge\":\"$challenge\",\"web\":true}" | field code)"
    session="$(post /api/sessions -d "{\"code\":\"$code\",\"verifier\":\"$verifier\"}" | field token)"
    token="$(post /api/enroll-tokens -X POST -H "authorization: Bearer $session" | field token)"
    MOTILE_DATA_DIR="$DEV/server.new" "$BIN/motile" setup "$token" --name studio --no-service --yes
    mv "$DEV/server.new" "$DEV/server"
fi

if ! current server "$BIN/motile"; then
    echo "▸ Starting the server…"
    stop server
    MOTILE_DATA_DIR="$DEV/server" MOTILE_CLAUDE_PATH="$ROOT/scripts/fake-agent" MOTILE_CODEX_PATH="$ROOT/scripts/fake-agent" \
        FAKE_AGENT_DELAY=0.03 FAKE_AGENT_WATCH=6 \
        nohup "$BIN/motile" run --local --port "$SERVER_PORT" > "$DEV/server.log" 2>&1 &
    started server $! "$BIN/motile"
fi

if [ ! -d "$DEV/app" ]; then
    echo "▸ Signing in and making the threads…"
    mkdir -p "$PROJECT"
    printf 'def greet(name):\n    print("Hello " + name)\n\ngreet("world")\n' > "$PROJECT/greet.py"
    cat > "$PROJECT/favicon.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="7" fill="#0f766e"/><path d="M9 20l5-9 4 6 2-3 3 6z" fill="#fff"/></svg>
SVG
    git -C "$PROJECT" init -q -b main
    git -C "$PROJECT" -c user.name=Dev -c user.email=demo@motile.app add -A
    git -C "$PROJECT" -c user.name=Dev -c user.email=demo@motile.app commit -q -m "Start"
    git -C "$PROJECT" branch release
    rm -rf "$DEV/app.new"
    (cd "$ROOT" && cargo run --release -q -p motile-core --example seed -- "$DEV/app.new" "$AUTH_URL" "$PROJECT")
    mv "$DEV/app.new" "$DEV/app"
fi

if [ "${1:-}" = "--mac" ] && ! alive mac; then
    echo "▸ Opening the Mac app…"
    (cd ../macos && scripts/build-app.sh)
    if [ ! -d "$DEV/mac-data" ]; then
        # A device of its own on the same account, so the two apps don't share a key. The
        # example is built first, so the sign-in has its seconds to finish.
        (cd "$ROOT" && cargo build --release -q -p motile-core --example drive)
        { echo '{"id": 1, "type": "dev_sign_in", "email": "demo@motile.app"}'; sleep 4; } |
            (cd "$ROOT" && cargo run --release -q -p motile-core --example drive -- "$DEV/mac-data.new" "$AUTH_URL") >/dev/null
        mv "$DEV/mac-data.new" "$DEV/mac-data"
    fi
    rm -rf "$DEV/Motile.app"
    cp -R ../macos/build/Motile.app "$DEV/Motile.app"
    plutil -replace CFBundleIdentifier -string app.motile.mac.gpui-reference "$DEV/Motile.app/Contents/Info.plist"
    codesign --force --deep --sign - "$DEV/Motile.app" >/dev/null 2>&1
    MOTILE_DATA_DIR="$DEV/mac-data" nohup "$DEV/Motile.app/Contents/MacOS/Motile" > "$DEV/mac.log" 2>&1 &
    started mac $! "$DEV/Motile.app/Contents/MacOS/Motile"
fi

if ! current app "$APP_BIN"; then
    echo "▸ Opening the app…"
    stop app
    MOTILE_DATA_DIR="$DEV/app" nohup "$APP_BIN" > "$DEV/app.log" 2>&1 &
    started app $! "$APP_BIN"
    sleep 2
fi
echo "✓ The dev app is open (pid $(pid_of app)). scripts/shot.sh takes a picture of it."
