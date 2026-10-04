#!/usr/bin/env bash
# Gives a build the version of its commit and prints it: the first two numbers of the version in
# Cargo.toml, and as the third how many commits lead up to this one, so that every push to main
# is a newer version than the last. It writes the version into Cargo.toml, which is where the
# server and the Mac app take theirs from. It needs the whole history (`fetch-depth: 0`).
set -euo pipefail
cd "$(dirname "$0")/.."

BASE="$(sed -n 's/^version = "\([0-9]*\.[0-9]*\)\..*"/\1/p' Cargo.toml | head -1)"
VERSION="$BASE.$(git rev-list --count HEAD)"
sed -i.bak "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
rm Cargo.toml.bak
echo "$VERSION"
