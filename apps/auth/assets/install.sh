#!/bin/sh
# Installs the Motile host on this machine and links it to your account:
#
#   curl -fsSL __PUBLIC_URL__/install | sh -s -- <token>
#
# The token comes from the install command the Motile app shows.
set -eu

MOTILE_URL="${MOTILE_URL:-__PUBLIC_URL__}"
BINARY=/usr/local/bin/motile

fail() {
    echo "motile: $1" >&2
    exit 1
}

as_root() {
    if [ "$(id -u)" = 0 ]; then
        "$@"
        return
    fi
    command -v sudo >/dev/null 2>&1 || fail "installing to $BINARY needs root; run this as root or install sudo"
    sudo "$@"
}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
        return
    fi
    command -v wget >/dev/null 2>&1 || fail "curl or wget is needed"
    wget -qO "$2" "$1"
}

main() {
    [ "$(uname -s)" = Linux ] || fail "hosts run on Linux for now"
    case "$(uname -m)" in
        x86_64 | amd64) target=x86_64-unknown-linux-musl ;;
        aarch64 | arm64) target=aarch64-unknown-linux-musl ;;
        *) fail "unsupported machine: $(uname -m)" ;;
    esac

    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT

    echo "Downloading Motile…"
    download "$MOTILE_URL/download/motile-$target.tar.gz" "$tmp/motile.tar.gz" || fail "the download failed"
    tar -xzf "$tmp/motile.tar.gz" -C "$tmp"
    chmod 755 "$tmp/motile"

    # Moved into place in one step, so a running host is replaced, not overwritten.
    as_root cp "$tmp/motile" "$BINARY.new"
    as_root mv -f "$BINARY.new" "$BINARY"

    MOTILE_AUTH_URL="$MOTILE_URL" "$BINARY" setup "$@"
}

main "$@"
