#!/usr/bin/env bash
# =============================================================================
# scripts/conformance.sh - build and run the GENERATED conformance programs.
#
# `azul-doc codegen all` renders one conformance plan (every api.json
# constant, every derive round-trip, every enum variant constructor, every Vec,
# every host-invokable callback kind) into target/codegen/conformance/<lang>/.
# Each program exits non-zero on any failed check and prints every failure, so
# a binding that drops, mis-wraps or leaks a class of API fails here as a class.
#
# The plan has ONE renderer today: C ("C first", 20eadd85b5;
# doc/src/codegen/v2/conformance/c.rs). A program is a rendered folder
# (target/codegen/conformance/<lang>/) plus a conf_<lang> function below that
# builds and runs it, and `c` is the only language that has both. Every other
# SHIPPED binding (SHIPPED_LANGS of scripts/e2e_language_matrix.sh) is listed in
# scripts/conformance_pending.txt, the ledger of the shipped bindings that have
# no program yet. A ledger language is reported
#     [<lang>] PENDING (no generated conformance program yet)
# and does not fail. The ledger is exact and can only shrink, whichever
# languages a run is asked for; the script FAILS when
#   - a shipped binding has neither a program nor a ledger line,
#   - a ledger line names a language that has a program now (the line goes in
#     the commit that adds the program), or one that is not shipped,
#   - a shipped binding has half a program (a folder without a conf_ function,
#     or the other way round),
#   - a program fails: the ledger never excuses a program.
# A language that is not shipped and has no program is skipped.
#
# Each program is also that binding's memtest: libazul built with
# `--features alloc-stats` counts the bytes it holds, and every case runs once
# to warm up, then AZ_MEMTEST_N more times (default 1), and must leave that
# count unchanged. `--memtest` additionally runs each program under gdb/lldb,
# so a crash or double free is reported with its backtrace.
#
#   bash scripts/conformance.sh [--memtest] [lang ...]
#
# With no languages, every language that has a generated program runs. The
# ledger is checked for the whole shipped tier in every run.
# Prerequisites (this script builds neither):
#   cargo build --release -p azul-dll --features build-dll,alloc-stats -> target/release
#   cargo run -r -p azul-doc -- codegen all                  -> target/codegen
# Exit status: 0 iff the ledger is exact and every requested program built and
# passed (a pending or skipped language counts as neither).
# =============================================================================
set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEGEN_DIR="$REPO_ROOT/target/codegen"
CONF_DIR="$CODEGEN_DIR/conformance"
RELEASE_DIR="$REPO_ROOT/target/release"
WORK_DIR="$REPO_ROOT/target/conformance"
LEDGER_FILE="$REPO_ROOT/scripts/conformance_pending.txt"

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

# run <lang> <command...>: the program itself, or (--memtest) under a
# debugger that prints a backtrace on a crash.
run() {
  local lang="$1"; shift
  if [ "$MEMTEST" = 0 ]; then
    "$@"
    return
  fi
  local out
  # lldb on macOS (a Homebrew gdb there cannot run programs unsigned), gdb
  # elsewhere.
  if [ "$IS_MACOS" = 1 ] && command -v lldb >/dev/null 2>&1; then
    out="$(AZ_MEMTEST_N=2 lldb --batch -o run -o 'bt 12' -- "$@" 2>&1)"
  elif command -v gdb >/dev/null 2>&1; then
    out="$(AZ_MEMTEST_N=2 gdb -batch -return-child-result -ex run -ex 'bt 12' --args "$@" 2>&1)"
  elif command -v lldb >/dev/null 2>&1; then
    out="$(AZ_MEMTEST_N=2 lldb --batch -o run -o 'bt 12' -- "$@" 2>&1)"
  else
    echo "[$lang] FAIL: --memtest needs gdb or lldb" >&2
    return 1
  fi
  printf '%s\n' "$out" | grep -vE '^\[(New|Thread) ' | tail -40
  if printf '%s' "$out" | grep -qE 'SIGSEGV|SIGABRT|SIGBUS|EXC_BAD_ACCESS|stop reason = signal'; then
    echo "[$lang] FAIL: crashed under the debugger" >&2
    return 1
  fi
  printf '%s' "$out" | grep -qE 'failure\(s\)' && ! printf '%s' "$out" | grep -qE ' [1-9][0-9]* failure\(s\)'
}

# ---- C ----------------------------------------------------------------------
conf_c() {
  local CC; CC="$(command -v clang || command -v gcc || command -v cc || true)"
  [ -n "$CC" ] || { echo "[c] no C compiler" >&2; return 1; }
  local out="$WORK_DIR/c"; mkdir -p "$out"
  local bin="$out/conformance"
  if [ "$IS_MACOS" = 1 ]; then
    # An rpath, not DYLD_LIBRARY_PATH alone: macOS strips DYLD_* variables
    # from protected binaries (lldb), so under --memtest the program would
    # not find libazul.
    "$CC" -std=c11 -g -O0 -w -I"$CODEGEN_DIR" "$CONF_DIR/c/conformance.c" -L"$RELEASE_DIR" -lazul \
      -Wl,-rpath,"$RELEASE_DIR" -o "$bin" || return 1
  elif [ "$IS_WINDOWS" = 1 ]; then
    bin="$bin.exe"
    "$CC" -std=c11 -g -O0 -w -I"$CODEGEN_DIR" "$CONF_DIR/c/conformance.c" "$RELEASE_DIR/azul.dll.lib" -o "$bin" || return 1
  else
    "$CC" -std=c11 -g -O0 -w -I"$CODEGEN_DIR" "$CONF_DIR/c/conformance.c" -L"$RELEASE_DIR" -lazul -lpthread -lm -ldl \
      -Wl,-rpath,"$RELEASE_DIR" -o "$bin" || return 1
  fi
  run c "$bin"
}

# ---- the shipped tier and the ledger ----------------------------------------
# The shipped tier, from its one definition (e2e_language_matrix.sh).
SHIPPED=" $(sed -n '/^SHIPPED_LANGS=(/,/^)/p' "$REPO_ROOT/scripts/e2e_language_matrix.sh" \
  | sed 's/#.*//' | tr -d '()' | sed 's/SHIPPED_LANGS=//' | tr '\n' ' ') "
case "$SHIPPED" in
  *" c "*) ;;
  *)
    # Without the tier every language would pass as "not shipped".
    echo "error: SHIPPED_LANGS was not found in scripts/e2e_language_matrix.sh" >&2
    exit 2 ;;
esac

# The ledger: one lower-case language name per line, `#` starts a comment.
if [ ! -f "$LEDGER_FILE" ]; then
  echo "error: $LEDGER_FILE is missing" >&2
  exit 2
fi
PENDING=" "
while IFS= read -r line || [ -n "$line" ]; do
  line="${line%%#*}"
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"
  [ -n "$line" ] || continue
  case "$line" in
    *[!a-z0-9_]*)
      echo "error: $LEDGER_FILE: '$line' is not a language name (one lower-case name per line)" >&2
      exit 2 ;;
  esac
  PENDING="$PENDING$line "
done < "$LEDGER_FILE"

# in_list <word> <" w1 w2 ... ">
in_list() {
  case "$2" in
    *" $1 "*) return 0 ;;
  esac
  return 1
}

# program_state <lang>: what a language has of a program - "both" (the folder
# the generator rendered and a conf_<lang> that runs it), "runner" or
# "rendered" (half), "none".
program_state() {
  local runner=0 rendered=0
  declare -F "conf_$1" >/dev/null && runner=1
  [ -d "$CONF_DIR/$1" ] && rendered=1
  case "$runner$rendered" in
    11) echo both ;;
    10) echo runner ;;
    01) echo rendered ;;
    *) echo none ;;
  esac
}

pending_count=0
for l in $PENDING; do pending_count=$((pending_count + 1)); done
echo "conformance ledger: $pending_count shipped binding(s) have no generated conformance program yet (scripts/conformance_pending.txt)"

# Every language asked for, then every shipped or listed one that was not: the
# ledger is checked for the whole tier, a program only for the ones asked for.
ALL=" "
for lang in "${LANGS[@]}" $SHIPPED $PENDING; do
  in_list "$lang" "$ALL" || ALL="$ALL$lang "
done
ASKED=" ${LANGS[*]} "

status=0
for lang in $ALL; do
  state="$(program_state "$lang")"
  shipped=0; in_list "$lang" "$SHIPPED" && shipped=1
  listed=0;  in_list "$lang" "$PENDING" && listed=1
  asked=0;   in_list "$lang" "$ASKED" && asked=1

  verdict=skip
  if [ "$shipped" = 0 ] && [ "$listed" = 1 ]; then
    verdict=unlisted
  elif [ "$shipped" = 0 ]; then
    [ "$state" = both ] && verdict=run
  else
    case "$state:$listed" in
      both:0) verdict=run ;;
      both:1) verdict=stale ;;
      none:1) verdict=pending ;;
      none:0) verdict=missing ;;
      *) verdict=half ;;
    esac
  fi

  # What a run was not asked for is reported only when it is wrong.
  if [ "$asked" = 1 ]; then
    echo "=== conformance [$lang] ==="
  fi
  case "$verdict" in
    run)
      [ "$asked" = 1 ] || continue
      if "conf_$lang"; then
        echo "[$lang] OK"
      else
        echo "[$lang] FAIL" >&2
        status=1
      fi ;;
    pending)
      [ "$asked" = 1 ] && echo "[$lang] PENDING (no generated conformance program yet)" ;;
    skip)
      [ "$asked" = 1 ] && echo "[$lang] SKIP: not shipped, no conformance program" ;;
    missing)
      echo "[$lang] FAIL: shipped binding without a generated conformance program (target/codegen/conformance/$lang + conf_$lang) and without a line in scripts/conformance_pending.txt" >&2
      status=1 ;;
    stale)
      echo "[$lang] FAIL: scripts/conformance_pending.txt lists $lang, which has a generated conformance program now: delete the line (the ledger can only shrink)" >&2
      status=1 ;;
    unlisted)
      echo "[$lang] FAIL: scripts/conformance_pending.txt lists $lang, which is not a shipped binding (SHIPPED_LANGS of scripts/e2e_language_matrix.sh): delete the line" >&2
      status=1 ;;
    half)
      if [ "$state" = runner ]; then
        echo "[$lang] FAIL: half a conformance program: conf_$lang is here, the generator rendered no target/codegen/conformance/$lang" >&2
      else
        echo "[$lang] FAIL: half a conformance program: the generator rendered target/codegen/conformance/$lang, this script has no conf_$lang" >&2
      fi
      status=1 ;;
  esac
done
exit "$status"
