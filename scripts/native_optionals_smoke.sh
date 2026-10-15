#!/usr/bin/env bash
# =============================================================================
# scripts/native_optionals_smoke.sh - compile and run the native-optionals
# smoke programs in tests/native_optionals/, one per shipped binding.
#
# Each program checks the same contract in its own language: an API
# parameter of an `Option*` type takes the language's own "no value"
# (None / null / nil / Nothing), a field or return value gives one back
# (None in -> None out, Some in -> Some out, through `IconVariants.light`), a
# `Result*` becomes the language's result or exception (`Url.parse`), and the
# guide's file-dialog callbacks compile in their short form.
#
#   bash scripts/native_optionals_smoke.sh [lang ...]
#
# Prerequisites (this script builds neither):
#   cargo run -r -p azul-doc -- codegen all                 -> target/codegen
#   cargo build --release -p azul-dll --features build-dll  -> target/release
# AZ_CODEGEN_DIR / AZ_LIB_DIR override the two directories.
# Exit status: 0 iff every requested program built and passed.
# =============================================================================
set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEGEN_DIR="${AZ_CODEGEN_DIR:-$REPO_ROOT/target/codegen}"
LIB_DIR="${AZ_LIB_DIR:-$REPO_ROOT/target/release}"
SRC_DIR="$REPO_ROOT/tests/native_optionals"
WORK_DIR="$REPO_ROOT/target/native_optionals"
mkdir -p "$WORK_DIR"

export NO_COLOR=1
export AZ_LOG=off
export DYLD_LIBRARY_PATH="$LIB_DIR${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="$LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

LANGS=("$@")
if [ "${#LANGS[@]}" -eq 0 ]; then
  for f in "$SRC_DIR"/*; do
    b="$(basename "$f")"; LANGS+=("${b%%.*}")
  done
fi

# ---- Rust -------------------------------------------------------------------
# The link-dynamic binding compiled into one file, stable and then nightly
# (`try-trait`, through RUSTC_BOOTSTRAP so a stable toolchain can check it).
smoke_rust() {
  local out="$WORK_DIR/rust"; mkdir -p "$out"
  AZ_CODEGEN_DIR="$CODEGEN_DIR" rustc --edition 2021 -C opt-level=0 -C debuginfo=0 \
    -L "$LIB_DIR" -l dylib=azul -C link-arg=-Wl,-rpath,"$LIB_DIR" \
    "$SRC_DIR/rust.rs" -o "$out/stable" || return 1
  "$out/stable" || return 1
  AZ_CODEGEN_DIR="$CODEGEN_DIR" RUSTC_BOOTSTRAP=1 rustc --edition 2021 -C opt-level=0 -C debuginfo=0 \
    --cfg 'feature="try-trait"' \
    -L "$LIB_DIR" -l dylib=azul -C link-arg=-Wl,-rpath,"$LIB_DIR" \
    "$SRC_DIR/rust.rs" -o "$out/nightly" || return 1
  "$out/nightly"
}

status=0
for lang in "${LANGS[@]}"; do
  echo "=== native optionals [$lang] ==="
  if ! declare -F "smoke_$lang" >/dev/null; then
    echo "[$lang] FAIL: no smoke_$lang in this script" >&2; status=1; continue
  fi
  if "smoke_$lang"; then echo "[$lang] OK"; else echo "[$lang] FAIL" >&2; status=1; fi
done
exit "$status"
