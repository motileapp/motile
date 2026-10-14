#!/usr/bin/env bash
# Builds the GPUI app for Linux or Windows and packs it as the release's archive, in build/:
# the program in a folder named motile-gpui, which the client's updater unpacks.
#
#   scripts/package.sh x86_64-unknown-linux-gnu    # build/Motile-GPUI-linux-x86_64.tar.gz
#   scripts/package.sh aarch64-unknown-linux-gnu   # build/Motile-GPUI-linux-aarch64.tar.gz
#   scripts/package.sh x86_64-pc-windows-msvc      # build/Motile-GPUI-windows-x86_64.zip
#   scripts/package.sh aarch64-pc-windows-msvc     # build/Motile-GPUI-windows-aarch64.zip
#
# On Windows it runs in Git Bash, and zips with 7-Zip, or PowerShell without it.
set -euo pipefail

cd "$(dirname "$0")/.."
TARGET="${1:?Give the Rust target to package, as in x86_64-unknown-linux-gnu.}"
case "$TARGET" in
  x86_64-unknown-linux-gnu) ASSET=Motile-GPUI-linux-x86_64.tar.gz PROGRAM=motile-gpui ;;
  aarch64-unknown-linux-gnu) ASSET=Motile-GPUI-linux-aarch64.tar.gz PROGRAM=motile-gpui ;;
  x86_64-pc-windows-msvc) ASSET=Motile-GPUI-windows-x86_64.zip PROGRAM=motile-gpui.exe ;;
  aarch64-pc-windows-msvc) ASSET=Motile-GPUI-windows-aarch64.zip PROGRAM=motile-gpui.exe ;;
  *)
    echo "A release has no archive for $TARGET." >&2
    exit 1
    ;;
esac

echo "▸ Building for $TARGET…"
rustup target add "$TARGET"
cargo build --release --target "$TARGET"

STAGE="build/package-$TARGET"
rm -rf "$STAGE" "build/$ASSET"
mkdir -p "$STAGE/motile-gpui"
cp "${CARGO_TARGET_DIR:-target}/$TARGET/release/$PROGRAM" "$STAGE/motile-gpui/"

echo "▸ Packing $ASSET…"
case "$ASSET" in
  *.tar.gz) tar -czf "build/$ASSET" -C "$STAGE" motile-gpui ;;
  *.zip)
    if command -v 7z >/dev/null; then
      (cd "$STAGE" && 7z a -tzip "../$ASSET" motile-gpui >/dev/null)
    else
      (cd "$STAGE" && powershell -NoProfile -Command "Compress-Archive -Path motile-gpui -DestinationPath ../$ASSET")
    fi
    ;;
esac
rm -rf "$STAGE"
echo "✓ build/$ASSET"
