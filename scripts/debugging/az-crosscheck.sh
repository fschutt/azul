#!/usr/bin/env bash
#
# scripts/az-crosscheck.sh — cargo check azul-dll for every target we ship.
#
# WHY: the platform shells in dll/src/desktop/shell2/ are `#[cfg(target_os)]`
# gated, so a host-only `cargo check` compiles roughly a quarter of them. Any
# change to the windows/, linux/ or macos/ backends is UNVERIFIED until this
# script is green. Blind implementation makes that the only gate we have.
#
# Usage:  ./scripts/az-crosscheck.sh [extra cargo args...]
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck disable=SC1091
. ./.envrc.az

TARGETS=(
  aarch64-apple-darwin
  x86_64-unknown-linux-gnu
  x86_64-pc-windows-gnu
)

rc=0
declare -a RESULTS=()
for t in "${TARGETS[@]}"; do
  echo "══════════════════════════════════════════════════════════"
  echo "  cargo check -p azul-dll --target $t"
  echo "══════════════════════════════════════════════════════════"
  if cargo check --release -p azul-dll --target "$t" "$@" 2>&1; then
    RESULTS+=("PASS  $t")
  else
    RESULTS+=("FAIL  $t")
    rc=1
  fi
done

echo
echo "───────────────── cross-check summary ─────────────────"
for r in "${RESULTS[@]}"; do echo "  $r"; done
exit $rc
