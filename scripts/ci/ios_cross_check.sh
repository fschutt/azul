#!/usr/bin/env bash
# ios_cross_check.sh: type-check the iOS-only code of azul-dll on a machine WITHOUT Xcode (an ubuntu runner, 1.5 minutes warm).
#
#   scripts/ci/ios_cross_check.sh [features]        default: std,logging,link-static,a11y,map-tiles,pdf
#   AZ_IOS_CHECK_TARGET=aarch64-apple-ios scripts/ci/ios_cross_check.sh      (the device target; default aarch64-apple-ios-sim)
#
# The shell (dll/src/desktop/shell2/ios), the UIKit and AVFoundation parts of dll/src/desktop/extra and everything else behind
# `#[cfg(target_os = "ios")]` compile under `cargo check` without an Apple SDK, because `check` compiles no C and links nothing:
# only the build scripts of the dependencies call a C compiler (cc-rs), and their archives are needed by a link alone. Those
# compilers are replaced by /bin/true. On a Mac the real ones are used and nothing is replaced.
#
# What it finds: type, borrow, cfg and unsafe-lint errors in the iOS code, the failures that cost a macOS runner 15 minutes to reach.
# What it does NOT find: a link error (ld, an unresolved framework symbol), a crash at run time, a symbol newer than the deployment
# target (scripts/ci/ios_symbol_gate.py reads the binary for that), what the Simulator shows.
#
# Needs the generated bindings first: `cargo run -r -p azul-doc codegen all`
# (scripts/remote.sh run <ref> ubuntu-22.04 'scripts/ci/ios_cross_check.sh' --codegen does both).
# The QA variant of the shipped lib adds `debug-server,e2e-scripting,test-hooks` to the features.
set -uo pipefail

target="${AZ_IOS_CHECK_TARGET:-aarch64-apple-ios-sim}"
features="${1:-std,logging,link-static,a11y,map-tiles,pdf}"
cd "$(dirname "$0")/../.."

rustup target add "$target" >/dev/null 2>&1 || true

if [ "$(uname -s)" != "Darwin" ]; then
  # cc-rs reads CC_<target with underscores>, CXX_ and AR_ first
  key="${target//-/_}"
  export "CC_${key}=/bin/true" "CXX_${key}=/bin/true" "AR_${key}=/bin/true"
fi

log="$(mktemp)"
cargo check -p azul-dll --target "$target" --no-default-features --features "$features" >"$log" 2>&1
status=$?

# the errors first, then proof that the iOS files were part of the build (the dependency info of the crate lists every source
# file it compiled: a feature set without the desktop module would leave shell2/ios out and say so)
grep -n "^error" -A14 "$log" | head -300
echo "ios_cross_check: target=$target features=$features"
if grep -l "shell2/ios/mod.rs" "${CARGO_TARGET_DIR:-target}/$target/debug/deps/"*.d >/dev/null 2>&1; then
  echo "ios_cross_check: shell2/ios/mod.rs was compiled"
else
  echo "ios_cross_check: shell2/ios/mod.rs was NOT part of this build (needs link-static or the desktop module)"
  [ "$status" -eq 0 ] && status=3
fi
tail -3 "$log"
echo "ios_cross_check: exit status $status"
exit "$status"
