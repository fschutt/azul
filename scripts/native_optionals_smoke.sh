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

# ---- Java / Scala -------------------------------------------------------------
# Every generated class plus the program, against JNA. Scala compiles against
# the same classes.
JNA_JAR="${JNA_JAR:-$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar}"
JDK_BIN="${JAVA_HOME:+$JAVA_HOME/bin/}"
[ -z "$JDK_BIN" ] && [ -x /opt/homebrew/opt/openjdk/bin/javac ] && JDK_BIN=/opt/homebrew/opt/openjdk/bin/

java_classes() {
  local out="$WORK_DIR/java"
  [ -f "$out/classes/com/azul/AzulOptions.class" ] && [ "$out/classes" -nt "$CODEGEN_DIR/java" ] && return 0
  rm -rf "$out"; mkdir -p "$out/classes"
  find "$CODEGEN_DIR/java" -name '*.java' > "$out/sources.txt"
  "${JDK_BIN}javac" -nowarn -encoding UTF-8 -J-Xmx3g -cp "$JNA_JAR" -d "$out/classes" @"$out/sources.txt"
}

smoke_java() {
  java_classes || return 1
  local out="$WORK_DIR/java"
  "${JDK_BIN}javac" -nowarn -encoding UTF-8 -cp "$out/classes:$JNA_JAR" -d "$out/smoke" "$SRC_DIR/java.java" || return 1
  "${JDK_BIN}java" -Djna.library.path="$LIB_DIR" -cp "$out/smoke:$out/classes:$JNA_JAR" NativeOptionals
}

smoke_scala() {
  java_classes || return 1
  local out="$WORK_DIR/scala"; rm -rf "$out"; mkdir -p "$out"
  # The JDK the classes were compiled with, not one scala-cli would fetch.
  local jhome="${JAVA_HOME:-}"
  [ -z "$jhome" ] && jhome="$(cd "$("${JDK_BIN}java" -XshowSettings:properties -version 2>&1 \
    | sed -n 's/^ *java.home = //p')" 2>/dev/null && pwd)"
  # A copy, so scala-cli's .scala-build / .bsp land in the work dir.
  cp "$SRC_DIR/scala.scala" "$out/" || return 1
  scala-cli run "$out/scala.scala" --server=false ${jhome:+--java-home "$jhome"} -J -Djna.library.path="$LIB_DIR" \
    --extra-jars "$WORK_DIR/java/classes" --extra-jars "$JNA_JAR" --main-class NativeOptionalsScala
}

# ---- Kotlin -------------------------------------------------------------------
smoke_kotlin() {
  local out="$WORK_DIR/kotlin"; rm -rf "$out"; mkdir -p "$out"
  JAVA_HOME="${JAVA_HOME:-$(cd "$("${JDK_BIN}java" -XshowSettings:properties -version 2>&1 \
    | sed -n 's/^ *java.home = //p')" 2>/dev/null && pwd)}" \
    kotlinc -J-Xmx4g -nowarn -cp "$JNA_JAR" "$CODEGEN_DIR/kotlin/Azul.kt" "$SRC_DIR/kotlin.kt" \
      -include-runtime -d "$out/smoke.jar" || return 1
  "${JDK_BIN}java" -Djna.library.path="$LIB_DIR" -cp "$out/smoke.jar:$JNA_JAR" com.azul.KotlinKt
}

# ---- Haskell ------------------------------------------------------------------
# The generated package next to a one-module executable, the way
# examples/haskell builds it (-O0: the binding gains nothing from optimisation).
smoke_haskell() {
  local out="$WORK_DIR/haskell"; rm -rf "$out"; mkdir -p "$out"
  cp -R "$CODEGEN_DIR/haskell" "$out/azul-haskell" || return 1
  cp "$CODEGEN_DIR/azul.h" "$out/azul-haskell/cbits/" || return 1
  cp "$SRC_DIR/haskell.hs" "$out/Main.hs" || return 1
  cat > "$out/smoke.cabal" <<'CABAL'
cabal-version:      2.4
name:               smoke
version:            0.1.0
build-type:         Simple

executable smoke
    main-is:            Main.hs
    build-depends:      base, azul, transformers
    default-language:   Haskell2010
    ghc-options:        -threaded
CABAL
  printf 'packages: . ./azul-haskell\nshared: False\n' > "$out/cabal.project"
  ( cd "$out" \
    && cabal build -v1 --ghc-options=-O0 --extra-lib-dirs="$LIB_DIR" \
         --ghc-options="-optl-Wl,-rpath,$LIB_DIR" smoke \
    && "$(cabal list-bin smoke)" )
}

# ---- Zig ----------------------------------------------------------------------
smoke_zig() {
  local out="$WORK_DIR/zig"; rm -rf "$out"; mkdir -p "$out"
  cp "$CODEGEN_DIR/azul.zig" "$out/" || return 1
  cp "$SRC_DIR/zig.zig" "$out/main.zig" || return 1
  ( cd "$out" && zig build-exe main.zig -lc -L"$LIB_DIR" -lazul -rpath "$LIB_DIR" -femit-bin=smoke ) || return 1
  "$out/smoke"
}

# ---- Go -----------------------------------------------------------------------
# A module next to the generated package, the way examples/go uses it.
smoke_go() {
  local out="$WORK_DIR/go"; rm -rf "$out"; mkdir -p "$out"
  cp -R "$CODEGEN_DIR/go" "$out/azul-go" || return 1
  cp "$SRC_DIR/go.go" "$out/main.go" || return 1
  cat > "$out/go.mod" <<'GOMOD'
module smoke

go 1.21

require azul.rs/ui/go v0.0.0

require github.com/ebitengine/purego v0.10.2 // indirect

replace azul.rs/ui/go => ./azul-go
GOMOD
  cp "$out/azul-go/go.sum" "$out/go.sum" 2>/dev/null
  ( cd "$out" && GOFLAGS=-mod=mod go build -o smoke . ) || return 1
  ( cd "$LIB_DIR" && AZ_LIB_DIR="$LIB_DIR/libazul.dylib" "$out/smoke" )
}

# ---- Node -----------------------------------------------------------------------
# azul.js loads libazul through koffi (installed once into the work dir).
smoke_node() {
  local out="$WORK_DIR/node"; mkdir -p "$out"
  cp "$CODEGEN_DIR/node/azul.js" "$SRC_DIR/node.js" "$out/" || return 1
  [ -d "$out/node_modules/koffi" ] || ( cd "$out" && npm install --no-audit --no-fund koffi@2.16.3 >/dev/null 2>&1 ) || return 1
  ( cd "$out" && AZ_LIB="$LIB_DIR/libazul.dylib" node node.js )
}

# ---- Ruby -----------------------------------------------------------------------
# The ffi gem (the system Ruby's, under ~/.gem) loads libazul through AZ_LIB.
smoke_ruby() {
  local out="$WORK_DIR/ruby"; mkdir -p "$out"
  cp "$CODEGEN_DIR/azul.rb" "$SRC_DIR/ruby.rb" "$out/" || return 1
  ( cd "$out" && AZ_LIB_DIR="$LIB_DIR" "${RUBY:-ruby}" ruby.rb )
}

# ---- Lua ------------------------------------------------------------------------
smoke_lua() {
  local out="$WORK_DIR/lua"; mkdir -p "$out"
  cp "$CODEGEN_DIR/azul.lua" "$SRC_DIR/lua.lua" "$out/" || return 1
  ( cd "$out" && LUA_PATH="./?.lua;;" luajit lua.lua )
}

# ---- OCaml ----------------------------------------------------------------------
# The generated units as one dune library next to the program (ctypes from opam).
smoke_ocaml() {
  local out="$WORK_DIR/ocaml"; rm -rf "$out"; mkdir -p "$out"
  cp "$CODEGEN_DIR"/ocaml/*.ml "$CODEGEN_DIR"/ocaml/*.mli "$out/" 2>/dev/null
  rm -f "$out/hello_world.ml"
  cp "$SRC_DIR/ocaml.ml" "$out/smoke.ml" || return 1
  printf '(lang dune 3.0)\n' > "$out/dune-project"
  cat > "$out/dune" <<'DUNE'
(library
 (name azul)
 (flags (:standard -w -32-26-27-33-34-35-37-39))
 (libraries ctypes ctypes-foreign)
 (modules (:standard \ smoke)))

(executable
 (name smoke)
 (modules smoke)
 (libraries ctypes ctypes-foreign azul))
DUNE
  command -v opam >/dev/null && eval "$(opam env 2>/dev/null)"
  ( cd "$out" && dune build ./smoke.exe ) || return 1
  "$out/_build/default/smoke.exe"
}

# ---- C++ ------------------------------------------------------------------------
# C++17 (std::optional both ways) and C++23 (std::expected for a Result).
smoke_cpp() {
  local out="$WORK_DIR/cpp"; mkdir -p "$out"
  local CXX; CXX="$(command -v clang++ || command -v g++)"
  # A Command Line Tools install whose own libc++ headers are incomplete
  # finds no <cstdint>; the SDK's copy is complete.
  local extra=()
  if [ "$(uname -s)" = Darwin ] && ! printf '#include <cstdint>\n' | "$CXX" -std=c++17 -x c++ -fsyntax-only - 2>/dev/null; then
    extra=(-nostdinc++ -isystem "$(xcrun --show-sdk-path)/usr/include/c++/v1")
  fi
  local std hdr
  for pair in "17:azul17.hpp" "2b:azul23.hpp"; do
    std="${pair%%:*}"; hdr="${pair#*:}"
    "$CXX" -std=c++"$std" -O0 -w "${extra[@]}" -I"$CODEGEN_DIR" -DAZUL_HEADER="\"$hdr\"" "$SRC_DIR/cpp.cpp" \
      -L"$LIB_DIR" -lazul -Wl,-rpath,"$LIB_DIR" -o "$out/smoke$std" || return 1
    "$out/smoke$std" || return 1
  done
}

# ---- C --------------------------------------------------------------------------
smoke_c() {
  local out="$WORK_DIR/c"; mkdir -p "$out"
  local CC; CC="$(command -v clang || command -v gcc)"
  "$CC" -std=c11 -O0 -w -I"$CODEGEN_DIR" "$SRC_DIR/c.c" -L"$LIB_DIR" -lazul -Wl,-rpath,"$LIB_DIR" \
    -o "$out/smoke" || return 1
  "$out/smoke"
}

smoke_pascal() {
  local out="$WORK_DIR/pascal"; mkdir -p "$out"
  cp "$CODEGEN_DIR/azul.pas" "$SRC_DIR/pascal.pas" "$out/" || return 1
  (cd "$out" && fpc -O1 -v0 -Fl"$LIB_DIR" -k-rpath -k"$LIB_DIR" -opascal_smoke pascal.pas >/dev/null) || return 1
  "$out/pascal_smoke"
}

# Needs the python-extension build (cargo build --release -p azul-dll
# --no-default-features --features python-extension) as azul.so / azul.pyd
# in AZ_PY_DIR (default: the library directory).
smoke_python() {
  local dir="${AZ_PY_DIR:-$LIB_DIR}"
  if [ ! -e "$dir/azul.so" ] && [ ! -e "$dir/azul.pyd" ] && ! ls "$dir"/azul*.so >/dev/null 2>&1; then
    echo "[python] SKIP: no python-extension build (azul.so) in $dir" >&2; return 0
  fi
  PYTHONPATH="$dir" python3 "$SRC_DIR/python.py"
}

# The binding's modules take a minute or two with gfortran; make -j builds
# them in parallel.
smoke_fortran() {
  local out="$WORK_DIR/fortran"; mkdir -p "$out"
  cp "$CODEGEN_DIR"/fortran/*.f90 "$CODEGEN_DIR/fortran/Makefile" "$out/" || return 1
  (cd "$out" && make -s -j4 azul.o >/dev/null) || return 1
  (cd "$out" && gfortran -O0 -std=f2008 -ffree-line-length-none -fimplicit-none \
    "$SRC_DIR/fortran.f90" azul*.o -L"$LIB_DIR" -lazul -Wl,-rpath,"$LIB_DIR" -o smoke) || return 1
  "$out/smoke"
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
