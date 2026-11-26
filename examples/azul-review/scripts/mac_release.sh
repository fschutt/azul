#!/usr/bin/env bash
# The engine (libazul, release), AzReview (release, linked to it) and the app bundle, built on a macOS runner: what the user's Mac
# runs, made where the disk is not shared. Run by scripts/remote.sh on macos-15 with --codegen:
#
#   scripts/remote.sh run <full sha> macos-15 'bash examples/azul-review/scripts/mac_release.sh' --codegen --artifacts 'dist/**'
#
# dist/ then holds AzReview.app (its libazul inside), the plain binary AzReview and libazul.dylib beside it. Nothing here deletes
# anything. The Mac signs them again (ad hoc) after the download: a signature does not survive a changed install name.
set -euo pipefail
cargo build -r -p azul-dll --features build-dll
mkdir -p target/azul-lib dist
cp target/release/libazul.dylib target/azul-lib/
export AZ_LINK_PATH="$PWD/target/azul-lib"
cargo build -r -p AzReview
cargo build -r -p azul-doc
./target/release/azul-doc bundle macos examples/azul-review --out dist --no-register
cp target/release/AzReview dist/AzReview
cp target/azul-lib/libazul.dylib dist/libazul.dylib
ls -la dist
