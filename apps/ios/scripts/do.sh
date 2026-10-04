#!/usr/bin/env bash
# Has the dev app do something, as a finger would: the steps are the ones
# packages/apple/Sources/MotileKit/iOS/DemoDriverIOS.swift knows.
#
#   scripts/do.sh "sidebar open"
#   scripts/do.sh "type Add a rate limiter" send
set -euo pipefail
cd "$(dirname "$0")/.."
for step in "$@"; do
    echo "$step" >> ../macos/build/dev/ios.steps
done
# The client reads the steps a few times a second.
sleep 1
