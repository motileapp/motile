# The account the dev apps are signed in to, which only exists on this Mac: PostgreSQL, an auth
# server with the dev login, and a server whose agent is scripts/fake-agent and whose GitHub is
# scripts/fake-gh, with a project and a few threads. It lives in apps/macos/build/dev and is left
# running, so the Mac's and the iOS dev app in one tree share it. Sourced by their dev-app.sh;
# `start_stack` brings it up.

STACK_HOME="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$(cd "$STACK_HOME/../.." && pwd)"
BIN="$ROOT/target/release"
DEV="$STACK_HOME/build/dev"
PROJECT="$DEV/api"
mkdir -p "$DEV"

if ! command -v initdb >/dev/null && command -v brew >/dev/null; then
    export PATH="$(brew --prefix postgresql@17)/bin:$PATH"
fi

# The first port from `$2` on that nothing has, of TCP's or UDP's: another tree's dev app may
# have the usual one.
free_port() {
    local port="$2"
    while lsof -nP -i"$1:$port" >/dev/null 2>&1; do port=$((port + 1)); done
    echo "$port"
}

# The ports are picked once and kept, since the server is set up against the auth server's.
if [ ! -f "$DEV/ports" ]; then
    echo "PG_PORT=$(free_port TCP 5436) AUTH_PORT=$(free_port TCP 3112) SERVER_PORT=$(free_port UDP 47614)" > "$DEV/ports"
fi
# shellcheck disable=SC1091
source "$DEV/ports"
AUTH_URL="http://127.0.0.1:$AUTH_PORT"

# A pid file holds the process and the version of the program it runs.
version() { shasum "$1" | cut -d' ' -f1; }
pid_of() { cut -d' ' -f1 "$DEV/$1.pid" 2>/dev/null || true; }
# A process that has exited but was not collected still answers `kill -0`.
alive() {
    local pid state
    pid="$(pid_of "$1")"
    [ -n "$pid" ] || return 1
    state="$(ps -o stat= -p "$pid" 2>/dev/null)" || return 1
    [ -n "$state" ] && [ "${state#Z}" = "$state" ]
}
current() { alive "$1" && [ "$(cut -d' ' -f2 "$DEV/$1.pid")" = "$(version "$2")" ]; }
stop() {
    alive "$1" || return 0
    local pid
    pid="$(pid_of "$1")"
    kill "$pid" 2>/dev/null || true
    local waited=0
    while alive "$1"; do
        sleep 0.1
        waited=$((waited + 1))
        [ "$waited" -ne 50 ] || kill -9 "$pid" 2>/dev/null || true
    done
}
started() { echo "$2 $(version "$3")" > "$DEV/$1.pid"; }
field() { sed -n "s/.*\"$1\":\"\([^\"]*\)\".*/\1/p"; }
post() { curl -fsS "$AUTH_URL$1" -H 'content-type: application/json' "${@:2}"; }

stop_stack() {
    stop server
    stop auth
    [ -d "$DEV/pg" ] && pg_ctl -D "$DEV/pg" stop -m fast >/dev/null 2>&1 || true
}

start_stack() {
    echo "▸ Building the servers…"
    (cd "$ROOT" && cargo build --release -p motile-server -p motile-auth)

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
        # A web session of the dev account asks for the install token, as the web app does.
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
            MOTILE_GH_PATH="$ROOT/scripts/fake-gh" FAKE_GH_STATE="$DEV/github.json" FAKE_AGENT_DELAY=0.03 FAKE_AGENT_WATCH=6 \
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
        # A remote of its own, so branches can be pushed and opened as pull requests.
        git init -q --bare -b main "$DEV/origin.git"
        git -C "$PROJECT" remote add origin "$DEV/origin.git"
        git -C "$PROJECT" push -q -u origin main release
        git -C "$PROJECT" remote set-head origin main
        rm -rf "$DEV/app.new"
        (cd "$ROOT" && cargo run --release -q -p motile-core --example seed -- "$DEV/app.new" "$AUTH_URL" "$PROJECT")
        mv "$DEV/app.new" "$DEV/app"
    fi
}
