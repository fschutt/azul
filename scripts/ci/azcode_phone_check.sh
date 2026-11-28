#!/usr/bin/env bash
# azcode_phone_check.sh: does AzCode COMPILE for the phone targets? `cargo check` of the app's library (and the crates it brings: azul-termkit's
# PTY, azul-search, azul-storage, syntect) for Android x86_64 and the iOS Simulator and device, on a machine without the NDK or an Apple SDK: a
# check links nothing, and the C compilers the build scripts of dependencies would call are replaced by /bin/true (the way
# scripts/ci/ios_cross_check.sh does it). The failures it finds are the ones that cost a 25-minute library build and a macOS slot to reach.
#
#   BOX_BY=wp-e2-code python3 box.py run <ref> 'bash scripts/ci/azcode_phone_check.sh' --codegen      (or scripts/remote.sh run <ref> ubuntu-22.04 ... --codegen)
#
# AZ_PHONE_CHECK_APPS: the packages (default AzCode); AZ_PHONE_CHECK_TARGETS: the targets (default the three). Every target is checked even when
# an earlier one fails; the last lines are `TARGET <target> <exit>`, the exit status is 0 only when all passed.
set -uo pipefail
cd "$(dirname "$0")/../.."
apps="${AZ_PHONE_CHECK_APPS:-AzCode}"
targets="${AZ_PHONE_CHECK_TARGETS:-x86_64-linux-android aarch64-apple-ios-sim aarch64-apple-ios}"
names=()
codes=()
for target in $targets; do
  rustup target add "$target" >/dev/null 2>&1 || true
  key="${target//-/_}"
  export "CC_${key}=/bin/true" "CXX_${key}=/bin/true" "AR_${key}=/bin/true"
  echo "=== $target"
  status=0
  for app in $apps; do
    cargo check -p "$app" --lib --target "$target" --message-format short 2>&1 | tail -n 60
    [ "${PIPESTATUS[0]}" -eq 0 ] || status=1
  done
  names+=("$target")
  codes+=("$status")
done
echo
bad=0
for i in "${!names[@]}"; do
  echo "TARGET ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"
