#!/usr/bin/env bash
# Runs the app's scripted demo against a real auth server and a real server on this machine, with
# scripts/fake-agent as the server's agent, and collects screenshots in apps/macos/screenshots.
#
# Needs target/release/{motile,motile-auth}, build/Motile.app and a PostgreSQL to run.
set -euo pipefail
cd "$(dirname "$0")/.."

ROOT="$(cd ../.. && pwd)"
BIN="$ROOT/target/release"
APP="$PWD/build/Motile.app/Contents/MacOS/Motile"
OUT="$PWD/screenshots"
WORK="$(mktemp -d)"
AUTH_URL="http://127.0.0.1:3111"
SERVER_PORT=47613
rm -rf "$OUT"
mkdir -p "$OUT"

cleanup() {
    kill "${SERVER_PID:-}" "${AUTH_PID:-}" "${APP_PID:-}" 2>/dev/null || true
    [ -d "$WORK/pg" ] && pg_ctl -D "$WORK/pg" stop -m immediate >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "▸ Starting PostgreSQL…"
if ! command -v initdb >/dev/null; then
    brew install postgresql@17 >/dev/null
    export PATH="$(brew --prefix postgresql@17)/bin:$PATH"
fi
initdb -D "$WORK/pg" -U motile --auth=trust >/dev/null
pg_ctl -D "$WORK/pg" -o "-p 5435 -k $WORK" -l "$OUT/postgres.log" -w start >/dev/null

echo "▸ Starting the auth server…"
PUBLIC_URL="$AUTH_URL" PORT=3111 DATABASE_URL="postgres://motile@127.0.0.1:5435/postgres" \
    GOOGLE_CLIENT_ID=demo GOOGLE_CLIENT_SECRET=demo DEV_LOGIN=1 "$BIN/motile-auth" > "$OUT/auth.log" 2>&1 &
AUTH_PID=$!
for _ in $(seq 1 50); do
    curl -fsS "$AUTH_URL/healthz" >/dev/null 2>&1 && break
    sleep 0.2
done

PROJECT=/tmp/motile-demo/api
# A project left by an earlier run has what the agent makes committed already.
rm -rf "$PROJECT"
mkdir -p "$PROJECT/src"
printf 'def greet(name):\n    print("Hello " + name)\n\ngreet("world")\n' > "$PROJECT/greet.py"
cat > "$PROJECT/favicon.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="7" fill="#0f766e"/><path d="M9 20l5-9 4 6 2-3 3 6z" fill="#fff"/></svg>
SVG
git -C "$PROJECT" init -q -b main 2>/dev/null || true
git -C "$PROJECT" config user.name Demo && git -C "$PROJECT" config user.email demo@motile.app
git -C "$PROJECT" -c user.name=Demo -c user.email=demo@motile.app add -A && git -C "$PROJECT" -c user.name=Demo -c user.email=demo@motile.app commit -q -m "Start" || true
git -C "$PROJECT" branch release 2>/dev/null || true

echo "▸ Starting the app…"
defaults delete app.motile.mac >/dev/null 2>&1 || true
MOTILE_DEMO=1 MOTILE_DEMO_OUTPUT="$OUT" MOTILE_DEMO_TOKEN_FILE="$WORK/token" MOTILE_DEMO_PROJECT="$PROJECT" \
    MOTILE_DATA_DIR="$WORK/app" MOTILE_AUTH_URL="$AUTH_URL" MOTILE_LOCAL=1 MOTILE_SERVER_ADDR="127.0.0.1:$SERVER_PORT" \
    "$APP" > "$OUT/app.log" 2>&1 &
APP_PID=$!

# The app writes the token of the install command it shows; the server is set up with it, as
# the installer would.
for _ in $(seq 1 300); do
    [ -s "$WORK/token" ] && break
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 0.2
done
if [ -s "$WORK/token" ]; then
    echo "▸ Setting up the server…"
    export MOTILE_DATA_DIR="$WORK/server" MOTILE_CLAUDE_PATH="$ROOT/scripts/fake-agent" MOTILE_CODEX_PATH="$ROOT/scripts/fake-agent"
    export FAKE_AGENT_DELAY=0.03 FAKE_AGENT_WATCH=6
    MOTILE_AUTH_URL="$AUTH_URL" "$BIN/motile" setup "$(cat "$WORK/token")" --name studio --no-service --yes
    "$BIN/motile" run --local --port "$SERVER_PORT" > "$OUT/server.log" 2>&1 &
    SERVER_PID=$!
fi

for _ in $(seq 1 600); do
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 1
done
kill "$APP_PID" 2>/dev/null || true

ls -la "$OUT"
echo "Checks:"
cat "$OUT/checks.txt"
if grep -q '^FAIL' "$OUT/checks.txt"; then
    exit 1
fi
