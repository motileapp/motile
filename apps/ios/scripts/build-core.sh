#!/usr/bin/env bash
# Builds the Rust core as a static library for iOS, into target/<triple>/release. Xcode runs it
# before it compiles the app, with the platform it builds for; by hand it builds for the simulator.
#
#   scripts/build-core.sh [simulator|device]
set -euo pipefail

cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

case "${PLATFORM_NAME:-${1:-simulator}}" in
    iphoneos | device) TARGET=aarch64-apple-ios ;;
    *) TARGET=aarch64-apple-ios-sim ;;
esac

# Xcode's variables are for the app's own compiler; they would send cargo's build scripts,
# which run on this Mac, to the iOS SDK.
unset SDKROOT LIBRARY_PATH
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-18.0}"
cargo build --release -p motile-core --target "$TARGET"
