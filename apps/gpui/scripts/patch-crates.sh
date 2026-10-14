#!/usr/bin/env bash
# Unpacks the crates listed in patches/crates into patched/, each with its patches/<name>.patch,
# for [patch.crates-io] in Cargo.toml. Run it before cargo; it skips the crates already current.
#
#   scripts/patch-crates.sh
#   scripts/patch-crates.sh --save    # writes the patches back from the edited crates in patched/
set -euo pipefail

cd "$(dirname "$0")/.."
stamp() {
    echo "$2 $3 $(git hash-object "patches/$1.patch")"
}

if [ "${1:-}" = "--save" ]; then
    while read -r name version checksum; do
        git -C "patched/$name" add -A -N
        git -C "patched/$name" diff > "patches/$name.patch"
        stamp "$name" "$version" "$checksum" > "patched/$name.stamp"
    done < patches/crates
    exit 0
fi

if grep -q '^\[\[patch.unused\]\]' Cargo.lock; then
    echo "Cargo.lock leaves a patch unused: redo patches/ for the versions it locks." >&2
    exit 1
fi
mkdir -p patched

sha256() {
    if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}

while read -r name version checksum; do
    dir="patched/$name"
    [ "$(cat "$dir.stamp" 2>/dev/null)" = "$(stamp "$name" "$version" "$checksum")" ] && continue

    echo "▸ Patching ${name} ${version}…"
    archive="patched/$name-$version.crate"
    curl -fsSL "https://static.crates.io/crates/$name/$name-$version.crate" -o "$archive"
    if [ "$(sha256 "$archive")" != "$checksum" ]; then
        echo "$archive does not match its checksum in patches/crates." >&2
        exit 1
    fi
    rm -rf "$dir" "patched/$name-$version"
    tar -xzf "$archive" -C patched
    rm "$archive"
    mv "patched/$name-$version" "$dir"

    git -C "$dir" init -q
    git -C "$dir" config core.autocrlf false
    git -C "$dir" add -A
    git -C "$dir" -c user.name=crates.io -c user.email=crates.io commit -qm "$name $version"
    if [ -s "patches/$name.patch" ]; then
        git -C "$dir" apply "$PWD/patches/$name.patch"
    fi
    stamp "$name" "$version" "$checksum" > "$dir.stamp"
done < patches/crates
