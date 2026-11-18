#!/usr/bin/env bash
# Runs the checks CI runs on the parts of the repo this branch changes against origin/main,
# committed or not. Run it before every push, and push only once it passes.
#
#   scripts/check.sh          # what changed
#   scripts/check.sh --all    # everything
set -euo pipefail
cd "$(dirname "$0")/.."

git fetch -q origin main || echo "Couldn't fetch origin/main; comparing with the last one fetched."
changed="$(
    git diff --name-only "$(git merge-base HEAD origin/main)"
    git ls-files --others --exclude-standard
)"
[ "${1:-}" = --all ] && changed="everything"

touches() {
    [ "$changed" = everything ] || grep -qE "$1" <<<"$changed"
}

step() {
    printf '\n\033[1m== %s\033[0m\n' "$*"
    "$@"
}

# The compose Postgres where there is Docker, else one of its own on a free port, as the dev stack does.
database() {
    [ -n "${DATABASE_URL:-}" ] && return
    if command -v docker >/dev/null; then
        step docker compose up -d --wait postgres
        export DATABASE_URL=postgres://motile:motile@localhost:5435/motile
        return
    fi
    command -v initdb >/dev/null || PATH="$(brew --prefix postgresql@17 2>/dev/null)/bin:$PATH"
    command -v initdb >/dev/null || { echo "The tests need Postgres: Docker, or brew install postgresql@17" >&2; exit 1; }
    pg="$PWD/build/check/pg"
    port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
    [ -d "$pg" ] || initdb -D "$pg" -U motile --auth=trust >/dev/null
    pg_ctl -D "$pg" -o "-p $port -k /tmp -c listen_addresses=127.0.0.1" -l "$pg.log" -w start >/dev/null
    trap 'pg_ctl -D "$pg" stop -m fast >/dev/null' EXIT
    export DATABASE_URL="postgres://motile@127.0.0.1:$port/postgres"
}

ran=()
shared='^(crates/|packages/theme/|fixtures/)'

if touches "$shared|^apps/(server|auth|gateway)/|^scripts/fake-|^Cargo\.(toml|lock)$"; then
    ran+=(rust)
    database
    step cargo fmt --all --check
    step cargo clippy --workspace --all-targets -- -D warnings
    step cargo test --workspace
    step cargo run -q -p motile-core --example compare_frames -- crates/core/fixtures/frames/mac crates/core/fixtures/frames/mac
    step cargo run -q -p motile-core --example compare_frames -- crates/core/fixtures/frames/ios crates/core/fixtures/frames/ios --mobile
fi

if touches "$shared|^apps/gpui/"; then
    ran+=(gpui)
    (
        cd apps/gpui
        step scripts/patch-crates.sh
        step cargo fmt --check
        step cargo clippy --all-targets -- -D warnings
        step cargo test
    )
fi

if touches "$shared|^apps/android/"; then
    ran+=(android)
    [ -d /opt/homebrew/opt/openjdk@21 ] && export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@21}"
    [ -d "$HOME/Library/Android/sdk" ] && export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
    (cd apps/android && step ./gradlew lintDebug testDebugUnitTest assembleDebug)
fi

if touches "$shared|^packages/apple/|^apps/(macos|ios)/"; then
    ran+=(apple)
    step packages/apple/scripts/check-ui.sh
    if [ "$(uname)" = Darwin ]; then
        (cd apps/macos && step swift build --target Motile)
        step cargo build --release -p motile-core --lib
        step swift test --package-path packages/apple
    fi
fi

if touches '^(apps/(web|marketing)/|packages/theme/|package\.json$|pnpm-)'; then
    ran+=(web)
    step pnpm install --frozen-lockfile
    step pnpm -r lint
    step pnpm -r typecheck
    step pnpm -r build
fi

if [ ${#ran[@]} -eq 0 ]; then
    echo "Nothing CI checks changed."
    exit 0
fi
printf '\n\033[1;32mPassed: %s\033[0m\n' "${ran[*]}"
