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

# ---- Swift ------------------------------------------------------------------
# The Azul module from the generated package sources, then the program.
smoke_swift() {
  local out="$WORK_DIR/swift"; rm -rf "$out"; mkdir -p "$out"
  cp -R "$CODEGEN_DIR/swift/Sources/Azul" "$out/Azul" || return 1
  cp "$CODEGEN_DIR/azul.h" "$CODEGEN_DIR/module.modulemap" "$out/" || return 1
  ( cd "$out" \
    && swiftc -emit-library -emit-module -module-name Azul -parse-as-library -j4 -I. Azul/*.swift \
         -L"$LIB_DIR" -lazul -o libAzulSwift.dylib \
    && swiftc -I. "$SRC_DIR/swift.swift" -L. -lAzulSwift -L"$LIB_DIR" -lazul \
         -Xlinker -rpath -Xlinker "$LIB_DIR" -Xlinker -rpath -Xlinker "$out" -o smoke ) || return 1
  "$out/smoke"
}

# ---- D ----------------------------------------------------------------------
smoke_d() {
  local out="$WORK_DIR/d"; mkdir -p "$out"
  local DC=ldc2; command -v ldc2 >/dev/null || DC=dmd
  "$DC" -I"$CODEGEN_DIR/d/source" "$SRC_DIR/d.d" "$CODEGEN_DIR"/d/source/azul/*.d \
    -L-L"$LIB_DIR" -L-lazul -L-rpath -L"$LIB_DIR" -od="$out" -of="$out/smoke" || return 1
  "$out/smoke"
}

# ---- Crystal ----------------------------------------------------------------
# The shard laid out the way `shards install` does (lib/azul).
smoke_crystal() {
  local out="$WORK_DIR/crystal"; rm -rf "$out"; mkdir -p "$out/lib"
  cp -R "$CODEGEN_DIR/crystal" "$out/lib/azul" || return 1
  cp "$SRC_DIR/crystal.cr" "$out/" || return 1
  ( cd "$out" && crystal build crystal.cr -o smoke --link-flags "-L$LIB_DIR -Wl,-rpath,$LIB_DIR" ) || return 1
  "$out/smoke"
}

# ---- C# ---------------------------------------------------------------------
# A console project around the generated Azul.cs; libazul next to it, where
# the generated DllImportResolver looks.
smoke_csharp() {
  local out="$WORK_DIR/csharp"; rm -rf "$out"; mkdir -p "$out"
  cp "$CODEGEN_DIR/Azul.cs" "$SRC_DIR/csharp.cs" "$out/" || return 1
  cp "$LIB_DIR"/libazul.* "$out/" 2>/dev/null
  cat > "$out/Smoke.csproj" <<'XML'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <RollForward>Major</RollForward>
    <Nullable>enable</Nullable>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
  </PropertyGroup>
</Project>
XML
  ( cd "$out" && dotnet run -c Release )
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
