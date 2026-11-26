#!/usr/bin/env bash
# The engine (libazul, release), AzReview (release, linked to it) and the app bundle, built on a macOS runner: what the user's Mac
# runs, made where the disk is not shared. Run by scripts/remote.sh on macos-15 with --codegen:
#
#   scripts/remote.sh run <full sha> macos-15 'bash examples/azul-review/scripts/mac_release.sh test' --codegen --artifacts 'dist/**'
#
# dist/ then holds AzReview.app (its libazul inside), the plain binary AzReview and libazul.dylib beside it. With the word `test`
# the check of every target and the unit tests of AzReview run last (the artifacts are made first, so that they are there when a
# test fails); the job's result is then the gate's. Nothing here deletes anything. The Mac signs the files again (ad hoc) after
# the download: a signature does not survive a changed install name.
set -uo pipefail
cargo build -r -p azul-dll --features build-dll || exit 1
mkdir -p target/azul-lib dist
cp target/release/libazul.dylib target/azul-lib/
export AZ_LINK_PATH="$PWD/target/azul-lib"
cargo build -r -p AzReview || exit 1
cargo build -r -p azul-doc || exit 1
./target/release/azul-doc bundle macos examples/azul-review --out dist --no-register || exit 1
cp target/release/AzReview dist/AzReview
cp target/azul-lib/libazul.dylib dist/libazul.dylib
ls -la dist
if [ "${1:-}" = "test" ]; then
  export DYLD_LIBRARY_PATH="$AZ_LINK_PATH"
  cargo check -p AzReview --all-targets || exit 1
  cargo test -p AzReview --lib || exit 1
fi
