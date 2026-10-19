#!/bin/bash
# Writes the Swift and Kotlin copies of the OpenUI tree's types from
# crates/core/src/render/openui/tree.rs. Run it after changing that file.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cargo run --quiet -p motile-core --example ui_types -- \
  "$root/packages/apple/Sources/MotileKit/Shared/Core/UiNodes.swift" \
  "$root/apps/android/app/src/main/java/app/motile/core/UiNodes.kt"
