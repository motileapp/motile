#!/usr/bin/env bash
# Builds the Rust core for Android as libmotile_android.so, into app/build/jniLibs/<abi>, which
# Gradle packs into the app, and for this machine into app/build/hostLibs, which the unit tests lay
# programs out with. Gradle runs it before it builds the app.
#
#   MOTILE_ABIS=arm64-v8a,x86_64 scripts/build-core.sh
#
# MOTILE_ABIS lists the ABIs, arm64-v8a without it: phones and the emulator on a Mac.
# Needs cargo-ndk, the Android NDK and Rust's aarch64-linux-android or x86_64-linux-android target.
set -euo pipefail
cd "$(dirname "$0")/.."
ANDROID="$(pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"

ABIS="$(echo "${MOTILE_ABIS:-arm64-v8a}" | tr ',' ' ')"
targets=()
for abi in $ABIS; do targets+=(-t "$abi"); done
cd ../..
cargo ndk "${targets[@]}" --platform 29 build --release -p motile-android

# Only the core: cargo-ndk would copy every library the build made.
rm -rf "$ANDROID/app/build/jniLibs"
for abi in $ABIS; do
    case "$abi" in
        arm64-v8a) triple=aarch64-linux-android ;;
        x86_64) triple=x86_64-linux-android ;;
        *) echo "Unknown ABI $abi" >&2; exit 1 ;;
    esac
    mkdir -p "$ANDROID/app/build/jniLibs/$abi"
    cp "target/$triple/release/libmotile_android.so" "$ANDROID/app/build/jniLibs/$abi/"
done

cargo build --release -p motile-android
rm -rf "$ANDROID/app/build/hostLibs"
mkdir -p "$ANDROID/app/build/hostLibs"
for library in target/release/libmotile_android.so target/release/libmotile_android.dylib; do
    if [ -f "$library" ]; then cp "$library" "$ANDROID/app/build/hostLibs/"; fi
done
