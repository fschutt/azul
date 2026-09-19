#!/usr/bin/env bash
# =============================================================================
# scripts/conformance.sh - build and run the GENERATED conformance programs.
#
# `azul-doc codegen all` renders one conformance plan (every api.json
# constant, every derive round-trip, every enum variant constructor, every Vec,
# every host-invokable callback kind) into target/codegen/conformance/<lang>/,
# once per binding. Each program exits non-zero on any failed check and prints
# every failure, so a binding that drops, mis-wraps or leaks a class of API
# fails here as a class.
#
# Each program also honours AZ_MEMTEST_N (repeat the whole plan N times), so
# `--memtest` runs it under scripts/run_memtest.sh: a debugger run (crashes,
# double frees) plus a small-N / large-N peak-RSS comparison (leaks).
#
#   bash scripts/conformance.sh [--memtest] [lang ...]
#
# With no languages, every language that has a generated program runs.
# Prerequisites (this script builds neither):
#   cargo build --release -p azul-dll --features build-dll   -> target/release
#   cargo run -r -p azul-doc -- codegen all                  -> target/codegen
# Exit status: 0 iff every requested program built and passed.
# =============================================================================
set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEGEN_DIR="$REPO_ROOT/target/codegen"
CONF_DIR="$CODEGEN_DIR/conformance"
RELEASE_DIR="$REPO_ROOT/target/release"
WORK_DIR="$REPO_ROOT/target/conformance"

MEMTEST=0
LANGS=()
for a in "$@"; do
  case "$a" in
    --memtest) MEMTEST=1 ;;
    *) for l in ${a//,/ }; do LANGS+=("$l"); done ;;
  esac
done

if [ ! -d "$CONF_DIR" ]; then
  echo "error: $CONF_DIR is missing - run: cargo run -r -p azul-doc -- codegen all" >&2
  exit 2
fi
if [ "${#LANGS[@]}" -eq 0 ]; then
  for d in "$CONF_DIR"/*/; do LANGS+=("$(basename "$d")"); done
fi

IS_MACOS=0; IS_WINDOWS=0
case "$(uname -s)" in
  Darwin) IS_MACOS=1 ;;
  MINGW*|MSYS*|CYGWIN*) IS_WINDOWS=1 ;;
esac

export NO_COLOR=1
export AZ_LOG=off
if [ "$IS_MACOS" = 1 ]; then
  export DYLD_LIBRARY_PATH="$RELEASE_DIR${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
elif [ "$IS_WINDOWS" = 1 ]; then
  export PATH="$RELEASE_DIR:$PATH"
else
  export LD_LIBRARY_PATH="$RELEASE_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi

# run <lang> <command...>: the program itself, or under the memtest harness.
run() {
  local lang="$1"; shift
  if [ "$MEMTEST" = 1 ]; then
    bash "$REPO_ROOT/scripts/run_memtest.sh" "conformance-$lang" "$@"
  else
    "$@"
  fi
}

# ---- C ----------------------------------------------------------------------
conf_c() {
  local CC; CC="$(command -v clang || command -v gcc || command -v cc || true)"
  [ -n "$CC" ] || { echo "[c] no C compiler" >&2; return 1; }
  local out="$WORK_DIR/c"; mkdir -p "$out"
  local bin="$out/conformance"
  if [ "$IS_MACOS" = 1 ]; then
    "$CC" -std=c11 -g -O0 -w -I"$CODEGEN_DIR" "$CONF_DIR/c/conformance.c" -L"$RELEASE_DIR" -lazul -o "$bin" || return 1
  elif [ "$IS_WINDOWS" = 1 ]; then
    bin="$bin.exe"
    "$CC" -std=c11 -g -O0 -w -I"$CODEGEN_DIR" "$CONF_DIR/c/conformance.c" "$RELEASE_DIR/azul.dll.lib" -o "$bin" || return 1
  else
    "$CC" -std=c11 -g -O0 -w -I"$CODEGEN_DIR" "$CONF_DIR/c/conformance.c" -L"$RELEASE_DIR" -lazul -lpthread -lm -ldl -o "$bin" || return 1
  fi
  run c "$bin"
}

# The shipped tier, from its one definition (e2e_language_matrix.sh). A shipped
# binding MUST have a conformance program; any other language is skipped when
# it has none.
SHIPPED=" $(sed -n '/^SHIPPED_LANGS=(/,/^)/p' "$REPO_ROOT/scripts/e2e_language_matrix.sh" \
  | sed 's/#.*//' | tr -d '()' | sed 's/SHIPPED_LANGS=//' | tr '\n' ' ') "

status=0
for lang in "${LANGS[@]}"; do
  echo "=== conformance [$lang] ==="
  if ! declare -F "conf_$lang" >/dev/null || [ ! -d "$CONF_DIR/$lang" ]; then
    case "$SHIPPED" in
      *" $lang "*)
        echo "[$lang] FAIL: shipped binding without a generated conformance program (target/codegen/conformance/$lang + conf_$lang)" >&2
        status=1 ;;
      *) echo "[$lang] SKIP: not shipped, no conformance program" ;;
    esac
    continue
  fi
  if "conf_$lang"; then
    echo "[$lang] OK"
  else
    echo "[$lang] FAIL" >&2
    status=1
  fi
done
exit "$status"
