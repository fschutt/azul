#!/usr/bin/env bash
# The macOS gate of AzReview's whiteboard, for scripts/remote.sh (run it as `bash scripts/gate_azreview_macos.sh`):
# the azul library, the whiteboard demo built against it (left in out/ with the library, for the e2e), and the tests of
# the AzReview crate.
set -euo pipefail
cargo build -r -p azul-dll --features build-dll
mkdir -p target/azul-lib out
cp target/release/libazul.dylib target/azul-lib/
export AZ_LINK_PATH="$PWD/target/azul-lib"
export DYLD_LIBRARY_PATH="$PWD/target/azul-lib"
cargo build -p AzReview --example whiteboard_demo
cp target/debug/examples/whiteboard_demo out/
cp target/azul-lib/libazul.dylib out/
cargo test -p AzReview --lib --examples
