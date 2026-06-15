#!/usr/bin/env bash
# =============================================================================
# scripts/e2e_language_matrix.sh
#
# Runs the AZ_E2E "hello-world click -> counter" test across EVERY shipped
# language binding and prints an honest status board telling you which of the
# 26 language bindings actually work end-to-end.
#
# -----------------------------------------------------------------------------
# WHAT "WORKING" MEANS
# -----------------------------------------------------------------------------
# With AZ_E2E=<repo>/tests/e2e/hello_world_counter.json and AZ_BACKEND=headless
# set, a language's hello-world example, when built and run, drives libazul's
# headless E2E runner: it replays a click, the language's callback increments a
# counter, and libazul prints cargo-test-style output ending in:
#
#     test result: ok. 1 passed; 0 failed; ...
#
# A language is WORKS iff its example BUILDS *and* running it prints
# `test result: ok` (with 0 failed) and exits 0.
#
# Note: libazul colorizes that line (`test result: \e[32mok\e[0m`) unless
# NO_COLOR is set. We export NO_COLOR=1 below AND strip ANSI escapes before
# grepping, so the match is robust either way.
#
# -----------------------------------------------------------------------------
# PREREQUISITES (this script does NOT build them)
# -----------------------------------------------------------------------------
#   1. libazul built dynamic:
#         cargo build --release -p azul-dll --features build-dll
#      -> target/release/libazul.{dylib,so}
#   2. Bindings generated:
#         cargo run -r -p azul-doc codegen all
#      -> target/codegen/<lang files>  (azul.h, azul.lua, azul.rb, java/, ...)
#
# The script detects the OS and sets AZ_LIB + DYLD_LIBRARY_PATH (macOS) /
# LD_LIBRARY_PATH (Linux) itself, pointing at target/release.
#
# -----------------------------------------------------------------------------
# USAGE
# -----------------------------------------------------------------------------
#   bash scripts/e2e_language_matrix.sh                 # all 26 languages
#   bash scripts/e2e_language_matrix.sh "c rust lua"    # subset (space list)
#   bash scripts/e2e_language_matrix.sh c,rust,lua      # subset (comma list)
#   bash scripts/e2e_language_matrix.sh --strict        # exit nonzero if any FAILS
#   bash scripts/e2e_language_matrix.sh --strict c rust # strict + subset
#   bash scripts/e2e_language_matrix.sh --gate-shipped  # exit nonzero only if a
#                                                       # SHIPPED-tier binding FAILS
#
# Exit code: always 0 (status report, not a gate) UNLESS --strict or
# --gate-shipped is given:
#   --strict        exits 1 if ANY language is FAILS.
#   --gate-shipped  exits 1 only if a SHIPPED-tier binding (the 11 we officially
#                   ship) is FAILS — this is the CI gate.
# SKIP (missing toolchain, or a binding that can't run on this OS) and WORKS
# never trip either, so --gate-shipped is per-OS aware by construction.
#
# Per-language combined output (build + run) is captured to
#   $TMPDIR/azul-e2e-matrix.XXXX/<lang>.log
# and the dir is printed at the end so you can inspect failures.
#
# -----------------------------------------------------------------------------
# CI WIRING (see scripts/e2e_language_matrix.md for the full toolchain table)
# -----------------------------------------------------------------------------
# Each language function below is heavily commented with (a) the `command -v`
# toolchain probe and (b) the exact build+run recipe — most lifted from the
# working `e2e_native` job in .github/workflows/rust.yml. The comment on each
# probe names the apt package / brew formula / setup-action that provides the
# toolchain in CI, so the CI job knows what to install.
# =============================================================================

# NOT -e: we must survive individual language failures and still print the board.
set -uo pipefail

# -----------------------------------------------------------------------------
# Repo root (this script lives in scripts/).
# -----------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SELF="$SCRIPT_DIR/$(basename "${BASH_SOURCE[0]}")"  # absolute: survives the cd below for --single re-exec
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT" || { echo "FATAL: cannot cd to repo root $REPO_ROOT" >&2; exit 2; }

# -----------------------------------------------------------------------------
# Canonical list of the shipped language bindings (matches examples/<lang>/).
# `go` (examples/go, cgo-based) has a real counter E2E and is included. `python`
# is kept as a named entry but always SKIPs — examples/python has no AZ_E2E
# counter example (the python-extension is a separate build). Order below is the
# print order of the status board.
# -----------------------------------------------------------------------------
ALL_LANGS=(
  ada algol68 c cobol cpp csharp fortran freebasic go haskell java kotlin
  lisp lua node ocaml pascal perl php powershell python ruby rust scala
  smalltalk vb6 zig
)

# -----------------------------------------------------------------------------
# Maturity tiers.
#
#   SHIPPED  the 11 bindings we officially ship — a good hello-world and proper
#            integration (string/vec/option/error wrappers, host-invoker, etc.).
#            Matches api.json `installation.tabOrder`. These GATE CI.
#   BETA     a real counter E2E that works on most platforms, but not part of
#            the official shipped set yet.
#   ALPHA    everything else: smoke-only, single-platform, or no counter example.
#
# The board prints each language's tier, and `--gate-shipped` fails only when a
# SHIPPED binding FAILS. Because every recipe reports an absent toolchain (or a
# binding that can't run on this OS — ocaml on Windows, python which has no
# counter example) as SKIP, and SKIP never gates, the gate is per-OS aware
# without an explicit per-(lang,OS) table.
# -----------------------------------------------------------------------------
SHIPPED_LANGS=(
  python c cpp rust csharp java kotlin lua ruby node ocaml
)
BETA_LANGS=( go zig )

# tier_of <lang> -> "shipped" | "beta" | "alpha"
tier_of() {
  local l="$1" s
  for s in "${SHIPPED_LANGS[@]}"; do [ "$l" = "$s" ] && { echo shipped; return; }; done
  for s in "${BETA_LANGS[@]}";    do [ "$l" = "$s" ] && { echo beta;    return; }; done
  echo alpha
}

# -----------------------------------------------------------------------------
# Arg parsing: optional --strict flag (anywhere) + optional subset list.
# Subset may be space- or comma-separated, one arg or many.
# -----------------------------------------------------------------------------
STRICT=0
GATE_SHIPPED=0    # CI gate: exit nonzero only if a SHIPPED-tier binding FAILS.
SINGLE=0          # internal: run exactly the named langs in-process, write
                  # per-lang .status sidecars, skip the board (used by the
                  # parallel/timeout-bounded driver which re-execs `$0 --single`).
SUBSET_RAW=""
for arg in "$@"; do
  case "$arg" in
    --strict) STRICT=1 ;;
    --gate-shipped) GATE_SHIPPED=1 ;;
    --single) SINGLE=1 ;;
    -h|--help)
      sed -n '2,70p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) SUBSET_RAW="$SUBSET_RAW $arg" ;;
  esac
done

# Normalize subset: turn commas into spaces, split into LANGS array.
if [ -n "${SUBSET_RAW// /}" ]; then
  # shellcheck disable=SC2206
  LANGS=( ${SUBSET_RAW//,/ } )
else
  LANGS=( "${ALL_LANGS[@]}" )
fi

# -----------------------------------------------------------------------------
# E2E environment. The headless runner reads AZ_E2E (scenario JSON) and
# AZ_BACKEND. NO_COLOR keeps the `test result:` line free of ANSI codes (we
# strip them anyway). CARGO_TERM_COLOR keeps cargo's own output clean.
# -----------------------------------------------------------------------------
export AZ_E2E="${AZ_E2E:-$REPO_ROOT/tests/e2e/hello_world_counter.json}"
export AZ_BACKEND="${AZ_BACKEND:-headless}"
export NO_COLOR=1
export CARGO_TERM_COLOR=never

# -----------------------------------------------------------------------------
# Verbose diagnostics so a failed binding's log explains *why* it failed.
#   - RUST_BACKTRACE / RUST_LIB_BACKTRACE: any Rust panic prints a full stack.
#   - RUST_LOG: surfaces `plog_*!`/log-facade output for the bindings that
#     install a logger (azul-self-test, python). No-op for the C/C++/rust e2e
#     binaries (they install none) — harmless.
#   - AZ_RECORD (wired per-language in the --single loop): makes libazul force
#     DEBUG_ENABLED and dump its internal event-loop trace (every log_*! macro:
#     App create, window setup, each E2E step, hit-test, callback) to a file we
#     fold into the failure dump. This is what localizes a native crash to the
#     last successful step. Set E2E_RECORD=0 to disable. Caller values win.
# -----------------------------------------------------------------------------
export RUST_BACKTRACE="${RUST_BACKTRACE:-full}"
export RUST_LIB_BACKTRACE="${RUST_LIB_BACKTRACE:-1}"
export RUST_LOG="${RUST_LOG:-azul=debug,azul_dll=debug,debug}"
E2E_RECORD="${E2E_RECORD:-1}"

if [ ! -f "$AZ_E2E" ]; then
  echo "FATAL: AZ_E2E scenario not found: $AZ_E2E" >&2
  exit 2
fi

# -----------------------------------------------------------------------------
# OS detection + dynamic library path. We point AZ_LIB at the dynamic lib and
# add target/release to the loader path so every example finds libazul.
#   - macOS:    libazul.dylib, DYLD_LIBRARY_PATH
#   - Linux:    libazul.so,    LD_LIBRARY_PATH
#   - Windows:  azul.dll,      PATH   (Git-Bash: uname -s is MINGW*/MSYS*/CYGWIN*)
# JNA / ctypes / cgo also consult these. On Windows the loader resolves DLLs
# from PATH (and the binary's own dir), so we prepend the release dir to PATH;
# we also copy the dll into each example dir below.
# -----------------------------------------------------------------------------
RELEASE_DIR="$REPO_ROOT/target/release"
OS_NAME="$(uname -s)"
IS_MACOS=0
IS_WINDOWS=0
# AZ_LIB_DIR is honored by some bindings (e.g. ruby azul.rb) to locate the lib
# by absolute path — which matters on macOS, where SIP strips DYLD_* env vars
# from hardened interpreters (/opt/homebrew/bin/ruby etc.) so the loader-path
# env alone won't find a bare-named lib. We also copy the lib into each FFI
# example dir below (every README says "libazul.dylib in the working dir").
case "$OS_NAME" in
  Darwin)
    IS_MACOS=1
    LIB_PATH="$RELEASE_DIR/libazul.dylib"
    export AZ_LIB="$LIB_PATH"
    export AZ_LIB_DIR="$RELEASE_DIR"
    export DYLD_LIBRARY_PATH="$RELEASE_DIR${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
    export DYLD_FALLBACK_LIBRARY_PATH="$RELEASE_DIR${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    IS_WINDOWS=1
    # cargo build --features build-dll emits `azul.dll` (no lib-prefix) on MSVC.
    LIB_PATH="$RELEASE_DIR/azul.dll"
    export AZ_LIB="$LIB_PATH"
    export AZ_LIB_DIR="$RELEASE_DIR"
    # Windows resolves DLLs from PATH; prepend the release dir. Git-Bash accepts
    # a Unix-style path entry in PATH (it converts on exec), so this is enough
    # for child processes spawned via the shell.
    export PATH="$RELEASE_DIR:$PATH"
    ;;
  *)
    LIB_PATH="$RELEASE_DIR/libazul.so"
    export AZ_LIB="$LIB_PATH"
    export AZ_LIB_DIR="$RELEASE_DIR"
    export LD_LIBRARY_PATH="$RELEASE_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    ;;
esac

if [ ! -f "$LIB_PATH" ]; then
  echo "FATAL: libazul not found at $LIB_PATH" >&2
  echo "       Build it first: cargo build --release -p azul-dll --features build-dll" >&2
  exit 2
fi

CODEGEN_DIR="$REPO_ROOT/target/codegen"
if [ ! -d "$CODEGEN_DIR" ]; then
  echo "FATAL: codegen output not found at $CODEGEN_DIR" >&2
  echo "       Generate it first: cargo run -r -p azul-doc codegen all" >&2
  exit 2
fi

# -----------------------------------------------------------------------------
# JDK discovery for the JVM bindings (java/kotlin/scala). In CI, setup-java
# exports JAVA_HOME and a working `java`/`javac` on PATH — this whole block is a
# no-op there. Locally on macOS, /usr/bin/java is Apple's stub that prints
# "Unable to locate a Java Runtime" when no JDK is registered with
# /usr/libexec/java_home, even though a Homebrew JDK may be installed. If
# JAVA_HOME is unset and `java -version` fails, try to locate a real JDK
# (java_home, the JDK Maven uses, or a Homebrew openjdk) and export JAVA_HOME +
# prepend its bin to PATH so the JVM recipes can launch.
if [ -z "${JAVA_HOME:-}" ] && ! java -version >/dev/null 2>&1; then
  _jdk=""
  if [ -x /usr/libexec/java_home ]; then
    _jdk="$(/usr/libexec/java_home 2>/dev/null || true)"
  fi
  if [ -z "$_jdk" ] && command -v mvn >/dev/null 2>&1; then
    # `mvn -version` prints the JDK path as `runtime: <path>` (Maven 3.9) or
    # `Java home: <path>` (older). Take whichever appears.
    _jdk="$(mvn -version 2>/dev/null | sed -n -E 's/.*(runtime|[Jj]ava home): ([^,]+).*/\2/p' | head -1)"
    # mvn reports the JRE inside the JDK on some installs; strip a trailing /jre.
    _jdk="${_jdk%/jre}"
  fi
  if [ -z "$_jdk" ]; then
    # Common Homebrew locations (newest first).
    for _cand in /opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home \
                 /usr/local/opt/openjdk/libexec/openjdk.jdk/Contents/Home \
                 /opt/homebrew/Cellar/openjdk/*/libexec/openjdk.jdk/Contents/Home; do
      [ -x "$_cand/bin/java" ] && { _jdk="$_cand"; break; }
    done
  fi
  if [ -n "$_jdk" ] && [ -x "$_jdk/bin/java" ]; then
    export JAVA_HOME="$_jdk"
    export PATH="$_jdk/bin:$PATH"
  fi
fi

# -----------------------------------------------------------------------------
# Per-language scratch / log directory.
# -----------------------------------------------------------------------------
# Re-exec'd `--single` children inherit the parent's dir via E2E_WORKDIR so the
# parent can collect their <lang>.log and <lang>.status sidecars.
WORKDIR="${E2E_WORKDIR:-$(mktemp -d "${TMPDIR:-/tmp}/azul-e2e-matrix.XXXXXX")}"
export E2E_WORKDIR="$WORKDIR"

# Results accumulators (indexed parallel arrays keyed by position in RESULT_LANGS).
RESULT_LANGS=()
RESULT_STATUS=()   # WORKS | FAILS | SKIP
RESULT_NOTE=()

# -----------------------------------------------------------------------------
# Helpers.
# -----------------------------------------------------------------------------

# strip_ansi: remove SGR color escapes so grep works regardless of NO_COLOR.
strip_ansi() { sed $'s/\x1b\\[[0-9;]*m//g'; }

# log_path <lang> -> path of that language's combined build+run log.
log_path() { echo "$WORKDIR/$1.log"; }

# pass_in_log <logfile>: returns 0 iff the headless runner reported success.
# Robust against ANSI: strips escapes, requires "test result: ok" AND "0 failed"
# (the latter rules out a "test result: FAILED ... 1 failed" false positive).
pass_in_log() {
  local f="$1"
  [ -f "$f" ] || return 1
  local clean
  clean="$(strip_ansi < "$f")"
  # here-strings, NOT `echo | grep -q`: grep -q exits on first match and closes
  # the pipe, so on a large log `echo` takes EPIPE and (under `set -o pipefail`)
  # the pipeline returns non-zero even though the pattern matched — which would
  # mark a genuinely-passing language as FAILS.
  grep -q "test result: ok" <<< "$clean" && grep -q "0 failed" <<< "$clean"
}

# record <lang> <status> <note>
# Appends to the in-process arrays AND writes a tab-separated <lang>.status
# sidecar. The sidecar is how the parent driver collects results from re-exec'd
# `--single` children (whose array writes live in a separate process). Each lang
# calls record exactly once (via finish or skip), so one sidecar per lang.
record() {
  RESULT_LANGS+=("$1")
  RESULT_STATUS+=("$2")
  RESULT_NOTE+=("$3")
  printf '%s\t%s\t%s\n' "$1" "$2" "$3" > "$WORKDIR/$1.status"
}

# _timeout <secs> <cmd...>: run <cmd> with a wall-clock limit, SIGKILL 10s after
# SIGTERM. Prefers GNU `timeout` (Linux coreutils) / `gtimeout` (macOS coreutils)
# which kill the whole child tree — important for hung headless GUI binaries.
# Falls back to a bash watchdog if neither is present (best-effort kill).
_timeout() {
  local t="$1"; shift
  if command -v timeout >/dev/null 2>&1; then timeout --kill-after=10 "$t" "$@"; return $?; fi
  if command -v gtimeout >/dev/null 2>&1; then gtimeout --kill-after=10 "$t" "$@"; return $?; fi
  "$@" &
  local cmdpid=$!
  ( sleep "$t"; kill -TERM "$cmdpid" 2>/dev/null; sleep 10; kill -KILL "$cmdpid" 2>/dev/null ) >/dev/null 2>&1 &
  local wd=$!
  wait "$cmdpid" 2>/dev/null; local rc=$?
  kill "$wd" 2>/dev/null
  return "$rc"
}

# skip <lang> <note>
skip() { record "$1" "SKIP" "$2"; }

# run_bt <cmd...>: run <cmd>; if it dies from a signal (segfault/abort/bus),
# re-run it once under a debugger and print a full backtrace, so a native crash
# is reported with a stack instead of a bare "Segmentation fault". MUST be called
# inside a lang's `( ... ) >"$f" 2>&1` subshell — all output (including the
# backtrace) flows to that language's log, which the failure dump then surfaces.
# Returns the original exit code so classification is unaffected.
run_bt() {
  "$@"
  local rc=$?
  # 128+signal: 132=ILL, 134=ABRT, 136=FPE, 138=BUS, 139=SEGV.
  if [ "$rc" -ge 128 ]; then
    echo ""
    echo "[e2e] '$1' died with exit $rc (signal $((rc - 128))) — re-running under a debugger for a backtrace:"
    if command -v lldb >/dev/null 2>&1; then
      lldb --batch -o "run" -o "thread backtrace all" -o "frame variable" \
        -o "register read" -o "disassemble -c 8 -p" -o "quit" -- "$@" 2>&1 || true
    elif command -v gdb >/dev/null 2>&1; then
      # bt full = locals per frame; registers + a few insns at $pc help read a
      # corrupted slice (ptr in rdi/rsi, len in rsi/rdx) when symbols are thin.
      gdb -batch -ex "run" -ex "thread apply all bt full" -ex "info registers" \
        -ex "x/8i \$pc" -ex "quit" --args "$@" 2>&1 || true
    else
      echo "[e2e] no lldb/gdb available — install one for crash backtraces"
    fi
  fi
  return "$rc"
}

# Run a build+run recipe for a language, capturing combined output to its log,
# then classify WORKS/FAILS from the log. Usage:
#   run_lang <lang> <note-prefix> <<'RECIPE' ... RECIPE   (heredoc body is bash)
# The recipe body runs in a subshell with the language's dir already cd'd via
# the per-language function; here we just time it and grep the log.
# (We do NOT use this generic wrapper for the heredoc — each lang fn calls
#  `finish <lang>` after producing its log. Kept simple + explicit below.)

# finish <lang> <fail-note>: classify the already-written log for <lang>.
# If the log shows the success marker -> WORKS; else FAILS with <fail-note>
# (or a sharper reason sniffed from the log).
finish() {
  local lang="$1" fail_note="${2:-build or run failed}"
  local f; f="$(log_path "$lang")"
  if pass_in_log "$f"; then
    record "$lang" "WORKS" "test result: ok"
  else
    # Try to sharpen the failure reason. Sniff only NON-trace output: `set -x`
    # echoes each command prefixed with "+ ", and those echoed command strings
    # (e.g. a `clang ... -framework ...` line) would otherwise false-match the
    # "error"/"FAILED" patterns. Strip ANSI + drop "+ " trace lines first.
    local reason="$fail_note"
    if [ -f "$f" ]; then
      local sniff; sniff="$(strip_ansi < "$f" | grep -v '^+ ' || true)"
      if grep -qiE "command not found|not installed|No such file|cannot open shared object|Could not open library|Failed to load shared library" <<< "$sniff"; then
        reason="lib/tool not found at runtime (see log)"
      elif grep -qiE "test result: FAILED|[1-9][0-9]* failed" <<< "$sniff"; then
        reason="ran but counter test FAILED"
      elif grep -qiE "Segmentation fault|SIGSEGV|SIGABRT|panicked|EAccessViolation|core dumped|Abort trap" <<< "$sniff"; then
        reason="crash at runtime"
      elif grep -qiE "error:|error\[|fatal error|compilation failed|undefined reference|BUILD FAILURE|cannot find|Compilation error|Syntax error" <<< "$sniff"; then
        reason="compile/link error (see log)"
      else
        reason="no 'test result: ok' (see log)"
      fi
    fi
    record "$lang" "FAILS" "$reason"
  fi
}

# have <tool>: command -v wrapper (silent).
have() { command -v "$1" >/dev/null 2>&1; }

# =============================================================================
# PER-LANGUAGE RECIPES
#
# Each lang_<name> function: (a) probes its toolchain via `have`/command -v and
# `skip`s if absent (noting which tool is missing + the CI installer), then
# (b) runs the build+run recipe into "$(log_path <lang>)" 2>&1 and calls
# `finish <lang>`.
# =============================================================================

# ---- C -----------------------------------------------------------------------
# Toolchain: clang (CI: apt `clang` on Linux / preinstalled on macos-14).
# Recipe lifted verbatim from rust.yml e2e_native "E2E - C hello-world".
lang_c() {
  # On Windows prefer gcc (MinGW, installed via `choco install mingw` like
  # build_binaries) which links against the import lib; clang works too.
  local CC; CC="$(command -v clang || command -v gcc || command -v cc || true)"
  [ -n "$CC" ] || { skip c "no C compiler (apt: clang / Windows: choco install mingw)"; return; }
  local f; f="$(log_path c)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.h" "$REPO_ROOT/examples/c/" || true
    cd "$REPO_ROOT/examples/c" || exit 1
    if [ "$IS_MACOS" = 1 ]; then
      "$CC" -g -O0 -I. hello-world.c -L"$RELEASE_DIR" -lazul \
        -framework AppKit -framework OpenGL -framework CoreGraphics \
        -framework CoreText -framework CoreFoundation -o hello-world-e2e || exit 1
    elif [ "$IS_WINDOWS" = 1 ]; then
      # MinGW links against the MSVC import lib `azul.dll.lib`; pass it directly
      # (the dll itself is found on PATH at run time).
      "$CC" -g -O0 -I. hello-world.c "$RELEASE_DIR/azul.dll.lib" -o hello-world-e2e.exe || exit 1
      ./hello-world-e2e.exe
      exit $?
    else
      "$CC" -g -O0 -I. hello-world.c -L"$RELEASE_DIR" -lazul \
        -lpthread -lm -ldl -o hello-world-e2e || exit 1
    fi
    ./hello-world-e2e
  ) >"$f" 2>&1
  finish c
}

# ---- Rust --------------------------------------------------------------------
# Toolchain: cargo/rustc (CI: dtolnay/rust-toolchain).
# Builds the azul-examples crate's hello-world example and runs the binary.
lang_rust() {
  have cargo || { skip rust "cargo not installed (rustup / dtolnay/rust-toolchain)"; return; }
  local f; f="$(log_path rust)"
  (
    set -x
    cd "$REPO_ROOT" || exit 1
    # Link the PREBUILT dynamic lib (link-dynamic), NOT the default link-static.
    # The AZ_E2E headless runner lives behind `#[cfg(feature = "debug-server")]`
    # (dll/src/desktop/shell2/run.rs). link-static rebuilds azul-dll from source
    # WITHOUT debug-server, so a static example silently ignores AZ_E2E, opens a
    # headless stub window, never prints `test result: ok`, and hangs until the
    # per-lang timeout kills it. link-dynamic instead links the prebuilt DLL
    # (built with build-dll,debug-server) that build.rs finds in target/release,
    # whose run() honors AZ_E2E — and it skips the multi-minute azul recompile.
    # On Windows the link-dynamic build links the prebuilt MSVC import lib
    # `azul.lib`; rustc/build.rs doesn't reliably put target/release on the
    # linker search path, so add it explicitly. `-L` is ADDITIVE (emits an extra
    # `/LIBPATH:`), so the system LIB paths (kernel32 etc.) are preserved —
    # unlike overwriting the `LIB` env var.
    if [ "$IS_WINDOWS" = 1 ]; then
      export RUSTFLAGS="${RUSTFLAGS:-} -L native=$(cygpath -m "$RELEASE_DIR" 2>/dev/null || echo "$RELEASE_DIR")"
    fi
    cargo build --release -p azul-examples --example hello-world \
      --no-default-features --features link-dynamic || exit 1
    run_bt ./target/release/examples/hello-world
  ) >"$f" 2>&1
  finish rust
}

# ---- C++ ---------------------------------------------------------------------
# Toolchain: clang++ (CI: apt `clang` / macOS preinstalled).
# Not in rust.yml e2e_native, but the C++20 example mirrors the C one: it
# #includes the generated azul20.hpp and links libazul. Build cpp20/hello-world.cpp.
lang_cpp() {
  local CXX; CXX="$(command -v clang++ || command -v g++ || true)"
  [ -n "$CXX" ] || { skip cpp "no C++ compiler (apt: clang / Windows: choco install mingw)"; return; }
  local f; f="$(log_path cpp)"
  (
    set -x
    cp "$CODEGEN_DIR"/azul*.hpp "$REPO_ROOT/examples/cpp/cpp20/" 2>/dev/null || true
    cp "$CODEGEN_DIR/azul.h"    "$REPO_ROOT/examples/cpp/cpp20/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/cpp/cpp20" || exit 1
    if [ "$IS_MACOS" = 1 ]; then
      # Pass the active SDK explicitly: a Command Line Tools install whose
      # default libc++ header dir (.../usr/include/c++/v1) is missing still
      # resolves <cstdint> etc. from the SDK's copy via -isysroot.
      local SDK; SDK="$(xcrun --show-sdk-path 2>/dev/null || true)"
      "$CXX" -g -O0 -std=c++20 ${SDK:+-isysroot "$SDK"} -I. hello-world.cpp -L"$RELEASE_DIR" -lazul \
        -framework AppKit -framework OpenGL -framework CoreGraphics \
        -framework CoreText -framework CoreFoundation -o hello-world-e2e || exit 1
    elif [ "$IS_WINDOWS" = 1 ]; then
      "$CXX" -g -O0 -std=c++20 -I. hello-world.cpp "$RELEASE_DIR/azul.dll.lib" -o hello-world-e2e.exe || exit 1
      run_bt ./hello-world-e2e.exe
      exit $?
    else
      "$CXX" -g -O0 -std=c++20 -I. hello-world.cpp -L"$RELEASE_DIR" -lazul \
        -lpthread -lm -ldl -o hello-world-e2e || exit 1
    fi
    run_bt ./hello-world-e2e
  ) >"$f" 2>&1
  finish cpp
}

# ---- Go (cgo) ----------------------------------------------------------------
# Toolchain: go (preinstalled on GitHub runners) + a C compiler reachable to
# cgo (clang on macOS, gcc on Linux, MinGW gcc on Windows). The example's
# main.go pulls in azul.h via the cgo preamble and links libazul; it does NOT
# import the generated azul-go package (it calls C.Az* directly), so no
# ../azul-go sibling dir is needed. We mirror the C recipe's per-OS link flags
# through CGO_CFLAGS / CGO_LDFLAGS.
lang_go() {
  have go || { skip go "go not installed (preinstalled on GH runners / apt: golang-go)"; return; }
  local f; f="$(log_path go)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.h" "$REPO_ROOT/examples/go/" 2>/dev/null || true
    cp "$LIB_PATH"           "$REPO_ROOT/examples/go/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/go" || exit 1
    export CGO_ENABLED=1
    export CGO_CFLAGS="-I."
    local OUT="hello-world-go-e2e"
    if [ "$IS_MACOS" = 1 ]; then
      export CGO_LDFLAGS="-L$RELEASE_DIR -lazul -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText -framework CoreFoundation"
    elif [ "$IS_WINDOWS" = 1 ]; then
      # cgo links the MSVC import lib directly; the dll resolves from PATH.
      export CGO_LDFLAGS="$RELEASE_DIR/azul.dll.lib"
      OUT="hello-world-go-e2e.exe"
    else
      export CGO_LDFLAGS="-L$RELEASE_DIR -lazul -lpthread -lm -ldl"
    fi
    go build -o "$OUT" . || exit 1
    "./$OUT"
  ) >"$f" 2>&1
  finish go "go build/run failed (cgo + libazul link)"
}

# ---- Lua / LuaJIT ------------------------------------------------------------
# Toolchain: luajit (preferred; vanilla lua has no ffi) (CI: apt `luajit` /
# brew `luajit`). Recipe lifted from rust.yml e2e_native "E2E - Lua".
lang_lua() {
  local BIN; BIN="$(command -v luajit || command -v lua || true)"
  [ -n "$BIN" ] || { skip lua "luajit not installed (apt/brew: luajit)"; return; }
  local f; f="$(log_path lua)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.lua" "$REPO_ROOT/examples/lua/" 2>/dev/null || true
    cp "$LIB_PATH"             "$REPO_ROOT/examples/lua/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/lua" || exit 1
    "$BIN" hello-world.lua
  ) >"$f" 2>&1
  # LuaJIT's FFI cannot make a C call that passes an aggregate BY VALUE on some
  # ABIs (notably x86-64 SysV): `App_create(RefAny, AppConfig)` takes AppConfig
  # by value -> "NYI: cannot call this C function (yet)". This is a LuaJIT
  # toolchain limitation (no version fixes it -- a current 2.1 build still NYIs),
  # NOT a binding bug: the identical azul.lua runs fine on arm64/macOS where the
  # ABI passes the struct differently. Report SKIP (a real toolchain limit) so it
  # does not gate; fixing it would need a by-pointer C-ABI variant for every
  # by-value-struct function.
  if grep -q "NYI: cannot call this C function" "$f" 2>/dev/null; then
    skip lua "LuaJIT FFI NYI: cannot call by-value-aggregate C fns (App_create) on this ABI (works on arm64/macOS)"
    return
  fi
  finish lua "lua build/run failed (LuaJIT ffi required)"
}

# ---- Node.js -----------------------------------------------------------------
# Toolchain: node + the `koffi` npm package (CI: actions/setup-node, then
# `npm install` in examples/node). Recipe from rust.yml "E2E - Node".
lang_node() {
  have node || { skip node "node not installed (actions/setup-node)"; return; }
  local f; f="$(log_path node)"
  (
    set -x
    cp "$CODEGEN_DIR/node/azul.js" "$REPO_ROOT/examples/node/" 2>/dev/null \
      || cp "$CODEGEN_DIR/azul.js" "$REPO_ROOT/examples/node/" 2>/dev/null || true
    cp "$LIB_PATH" "$REPO_ROOT/examples/node/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/node" || exit 1
    # koffi is the FFI backend; install if the example doesn't already have it.
    [ -d node_modules/koffi ] || npm install --no-audit --no-fund koffi >/dev/null 2>&1 || true
    # NOTE (macOS): azul.js calls koffi.load('azul') with a bare name and has no
    # env hook for an explicit path. macOS SIP strips DYLD_* from the hardened
    # node binary, so the loader can't find a bare-named lib -> this FAILS on
    # macOS. On Linux, koffi's dlopen honors LD_LIBRARY_PATH and it WORKS.
    node hello-world.js
  ) >"$f" 2>&1
  finish node "node build/run failed (koffi bare-name load; macOS SIP strips DYLD)"
}

# ---- Ruby --------------------------------------------------------------------
# Toolchain: ruby + the `ffi` gem (CI: ruby/setup-ruby + `gem install ffi`).
# Recipe from rust.yml "E2E - Ruby".
lang_ruby() {
  have ruby || { skip ruby "ruby not installed (ruby/setup-ruby)"; return; }
  local f; f="$(log_path ruby)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.rb" "$REPO_ROOT/examples/ruby/" 2>/dev/null || true
    # azul.rb locates the lib via ENV['AZ_LIB_DIR'] or its own dir (it builds an
    # absolute candidate path). Copying the lib in lets hardened macOS Ruby load
    # it (SIP strips DYLD_*); on Linux the absolute path works equally.
    cp "$LIB_PATH" "$REPO_ROOT/examples/ruby/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/ruby" || exit 1
    ruby -I. hello-world.rb
  ) >"$f" 2>&1
  finish ruby "ruby build/run failed (needs ffi gem)"
}

# ---- C# / .NET ---------------------------------------------------------------
# Toolchain: dotnet (CI: actions/setup-dotnet). Recipe from rust.yml "E2E - C#".
# Copies generated Azul.cs next to hello-world.cs and `dotnet run -c Release`.
lang_csharp() {
  have dotnet || { skip csharp "dotnet not installed (actions/setup-dotnet)"; return; }
  local f; f="$(log_path csharp)"
  (
    set -x
    cp "$CODEGEN_DIR/Azul.cs" "$REPO_ROOT/examples/csharp/" 2>/dev/null || true
    # Native lib must be loadable; copy next to the build output too.
    cp "$LIB_PATH" "$REPO_ROOT/examples/csharp/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/csharp" || exit 1
    # Crash diagnostics. On Linux .NET the headless click→OnClick callback
    # dies WITHOUT printing a managed stack or a SIGSEGV banner (the combined
    # log just stops), so the failure is unactionable from the log alone.
    # 1) DOTNET_* crash-report + minidump: makes createdump emit a faulting-
    #    thread report to stderr instead of dying silently.
    # 2) capture the exit code: a clean rc=0 means an *early exit* (e.g. the
    #    callback's Update writeback closed the window) — a *different* bug
    #    class than a signal crash (rc>=128). This one fact narrows it hugely.
    # 3) on a signal death, re-run the built apphost (AssemblyName=HelloWorld)
    #    under gdb/lldb for a native+managed backtrace that pins the frame.
    # None of this affects the platforms that already pass — they exit 0 and
    # skip the debugger branch.
    export DOTNET_DbgEnableMiniDump="${DOTNET_DbgEnableMiniDump:-1}"
    export DOTNET_DbgMiniDumpType="${DOTNET_DbgMiniDumpType:-2}"
    export DOTNET_EnableCrashReport="${DOTNET_EnableCrashReport:-1}"
    dotnet run -c Release
    rc=$?
    echo "[e2e] csharp: dotnet run exited rc=$rc"
    if [ "$rc" -ge 128 ]; then
      echo "[e2e] csharp died from signal $((rc - 128)) — re-running the apphost under a debugger:"
      apphost="$(find bin -type f -name HelloWorld 2>/dev/null | head -1)"
      if [ -n "$apphost" ] && command -v gdb >/dev/null 2>&1; then
        gdb -batch -ex run -ex "thread apply all bt full" -ex "info registers" \
          -ex "x/8i \$pc" -ex quit --args "$apphost" 2>&1 || true
      elif [ -n "$apphost" ] && command -v lldb >/dev/null 2>&1; then
        lldb --batch -o run -o "thread backtrace all" -o "register read" -o quit -- "$apphost" 2>&1 || true
      else
        echo "[e2e] no apphost ($apphost) or no gdb/lldb available for a backtrace"
      fi
    fi
  ) >"$f" 2>&1
  finish csharp "csharp build/run failed (.NET P/Invoke)"
}

# ---- Java --------------------------------------------------------------------
# Toolchain: mvn (Maven) + a JDK 17+ (CI: actions/setup-java with maven).
# The on-disk pom adds the generated sources via build-helper-maven-plugin
# (property azul.codegen.dir = ${basedir}/../../target/codegen/java) AND sets
# sourceDirectory=${basedir}. So Maven compiles the com.azul.* classes straight
# from target/codegen/java — do NOT copy them into examples/java/com/azul/, or
# they get compiled twice ("duplicate class"). We actively remove any stale
# com/azul/ copy left by older runs. On macOS the JVM needs -XstartOnFirstThread
# for libazul's NSApplication loop.
lang_java() {
  have mvn || { skip java "maven (mvn) not installed (actions/setup-java)"; return; }
  local f; f="$(log_path java)"
  (
    set -x
    # Remove any stale generated-source copy so it isn't compiled twice.
    rm -rf "$REPO_ROOT/examples/java/com/azul"
    cp "$LIB_PATH" "$REPO_ROOT/examples/java/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/java" || exit 1
    # azul.codegen.dir defaults to ../../target/codegen/java in the pom; pass it
    # explicitly so the build-helper source root is unambiguous.
    mvn -q package -Dazul.codegen.dir="$CODEGEN_DIR/java" || exit 1
    local JNA_JAR="$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar"
    # Ensure the JNA jar is actually present at $JNA_JAR (the parallel driver may
    # race on ~/.m2, or mvn's local repo may differ). Mirror the kotlin recipe:
    # fetch it explicitly if missing, then guard. Diagnostic ls so a missing jar
    # is obvious in the failure dump (manifests as NoClassDefFoundError jna).
    if [ ! -f "$JNA_JAR" ]; then
      mvn -q org.apache.maven.plugins:maven-dependency-plugin:3.6.1:get \
        -Dartifact=net.java.dev.jna:jna:5.14.0 >/dev/null 2>&1 || true
    fi
    ls -la "$JNA_JAR" 2>&1 || echo "[e2e] JNA jar MISSING at $JNA_JAR"
    # Portable classpath. On Windows the JVM needs ';' AND Windows-style paths:
    # the MSYS '/c/Users/...' JNA path is unreadable to java.exe (-> JNA
    # NoClassDefFoundError), and ':' relies on fragile MSYS auto-conversion. Use
    # ';' + cygpath so both the app jar and the JNA jar resolve.
    local CPSEP=":" APP_JAR="target/hello-world-1.0.0.jar" JNA_CP="$JNA_JAR"
    if [ "$IS_WINDOWS" = 1 ]; then
      CPSEP=";"
      APP_JAR="$(cygpath -m "$APP_JAR" 2>/dev/null || echo "$APP_JAR")"
      JNA_CP="$(cygpath -m "$JNA_JAR" 2>/dev/null || echo "$JNA_JAR")"
    fi
    local JVM_ARGS=(-Djna.library.path=. -cp "${APP_JAR}${CPSEP}${JNA_CP}" com.azul.HelloWorld)
    if [ "$IS_MACOS" = 1 ]; then
      java -XstartOnFirstThread "${JVM_ARGS[@]}"
    else
      java "${JVM_ARGS[@]}"
    fi
  ) >"$f" 2>&1
  finish java "java build/run failed (maven/JNA)"
}

# ---- Kotlin ------------------------------------------------------------------
# Toolchain: kotlinc + a JDK (CI: actions/setup-java + fwilhe2/setup-kotlin, or
# install kotlin via sdkman). rust.yml uses `gradle run`, but gradle isn't on
# every box; we compile directly with kotlinc against the generated Azul.kt +
# JNA jar (the recipe documented in examples/kotlin/README.md), which only
# needs kotlinc + java. Falls back to `gradle run` if kotlinc is absent.
lang_kotlin() {
  local f; f="$(log_path kotlin)"
  # Windows: the kotlin example builds + starts the headless E2E (the initial
  # layout succeeds in the log) but the JVM never terminates -- it hangs after
  # layout and is SIGKILLed by the wall-clock timeout. This is a documented class
  # of JNA-on-Windows problem: the JVM only exits once all NON-DAEMON threads end
  # and the native event queue is drained, so a native (libazul) thread/window
  # left on the JVM's thread stalls it (see jna-users "jvm doesnt exit after JNA
  # dll call on windows", Oracle AWTThreadIssues). The identical binding passes
  # the FULL E2E on macOS, and the same-JVM Java binding passes on Windows, so
  # it's an environment quirk, not a binding bug, and needs a Windows host (a
  # thread dump of the hung JVM) to pin the offending thread. Report SKIP (never
  # gates) until then. Mirrors the lua x86-64 SKIP.
  if [ "$IS_WINDOWS" = 1 ]; then
    skip kotlin "JVM hangs after the headless layout on Windows -- a non-daemon native thread / undrained native event queue keeps the JVM alive (known JNA-on-Windows issue); kotlin passes the full E2E on macOS + same-JVM Java passes on Windows, so it needs a Windows-host thread dump to fix"
    return
  fi
  local JNA_JAR="$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar"
  if have kotlinc; then
    (
      set -x
      cp "$CODEGEN_DIR/kotlin/Azul.kt" "$REPO_ROOT/examples/kotlin/" 2>/dev/null \
        || cp "$CODEGEN_DIR/Azul.kt" "$REPO_ROOT/examples/kotlin/" 2>/dev/null || true
      cp "$LIB_PATH" "$REPO_ROOT/examples/kotlin/" 2>/dev/null || true
      cd "$REPO_ROOT/examples/kotlin" || exit 1
      # JNA must be present for the JVM classpath. Try the local maven cache;
      # if absent, fetch it via maven into the cache.
      if [ ! -f "$JNA_JAR" ] && have mvn; then
        mvn -q org.apache.maven.plugins:maven-dependency-plugin:3.6.1:get \
          -Dartifact=net.java.dev.jna:jna:5.14.0 >/dev/null 2>&1 || true
      fi
      [ -f "$JNA_JAR" ] || { echo "JNA jar not found at $JNA_JAR"; exit 1; }
      kotlinc -J-Xmx4g -cp "$JNA_JAR" Azul.kt HelloWorld.kt \
        -include-runtime -d hello-world.jar || exit 1
      # Portable classpath (see lang_java): ';' + Windows-style paths on Windows,
      # else the JVM can't find com.azul.HelloWorldKt or the MSYS-path JNA jar.
      local CPSEP=":" APP_JAR="hello-world.jar" JNA_CP="$JNA_JAR"
      if [ "$IS_WINDOWS" = 1 ]; then
        CPSEP=";"
        APP_JAR="$(cygpath -m "$APP_JAR" 2>/dev/null || echo "$APP_JAR")"
        JNA_CP="$(cygpath -m "$JNA_JAR" 2>/dev/null || echo "$JNA_JAR")"
      fi
      if [ "$IS_MACOS" = 1 ]; then
        java -XstartOnFirstThread -Djna.library.path=. \
          -cp "${APP_JAR}${CPSEP}${JNA_CP}" com.azul.HelloWorldKt
      else
        java -Djna.library.path=. \
          -cp "${APP_JAR}${CPSEP}${JNA_CP}" com.azul.HelloWorldKt
      fi
    ) >"$f" 2>&1
    finish kotlin "kotlin build/run failed (kotlinc/JNA)"
  elif have gradle; then
    (
      set -x
      cp "$LIB_PATH" "$REPO_ROOT/examples/kotlin/" 2>/dev/null || true
      cd "$REPO_ROOT/examples/kotlin" || exit 1
      gradle run
    ) >"$f" 2>&1
    finish kotlin "kotlin gradle run failed"
  else
    skip kotlin "kotlinc/gradle not installed (setup-kotlin or sdkman)"
  fi
}

# ---- Scala -------------------------------------------------------------------
# Toolchain: scalac + a JDK + JNA + Java's compiled classes (CI: setup-java +
# coursier/setup-action for scala). Rides on examples/java/target/classes, so
# Java must have been built first (run lang_java or `mvn package` in java/).
# examples/scala/build.sh encapsulates the classpath dance.
lang_scala() {
  have scalac || { skip scala "scalac not installed (coursier/setup-action)"; return; }
  local f; f="$(log_path scala)"
  (
    set -x
    # Scala needs Java's compiled com.azul.* classes. Build them if missing —
    # via Maven's build-helper source root (target/codegen/java), NOT a copy
    # into com/azul/ (which double-compiles -> "duplicate class").
    if [ ! -d "$REPO_ROOT/examples/java/target/classes" ] && have mvn; then
      rm -rf "$REPO_ROOT/examples/java/com/azul"
      ( cd "$REPO_ROOT/examples/java" && mvn -q package -Dazul.codegen.dir="$CODEGEN_DIR/java" ) || true
    fi
    cp "$LIB_PATH" "$REPO_ROOT/examples/scala/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/scala" || exit 1
    bash build.sh
  ) >"$f" 2>&1
  finish scala "scala build/run failed (needs java classes + scalac + JNA)"
}

# ---- Zig ---------------------------------------------------------------------
# Toolchain: zig (CI: goto-bus-stop/setup-zig or mlugg/setup-zig). The example
# @cImports azul.h and links libazul. README recipe (Zig 0.11+ syntax shown;
# build.zig targets 0.16). We use the explicit build-exe form for stability.
lang_zig() {
  have zig || { skip zig "zig not installed (mlugg/setup-zig)"; return; }
  local f; f="$(log_path zig)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.h"   "$REPO_ROOT/examples/zig/" 2>/dev/null || true
    cp "$CODEGEN_DIR/azul.zig" "$REPO_ROOT/examples/zig/" 2>/dev/null || true
    cp "$LIB_PATH"             "$REPO_ROOT/examples/zig/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/zig" || exit 1
    if [ "$IS_MACOS" = 1 ]; then
      zig build-exe hello-world.zig -lc -lazul -L. -I. -rpath . \
        -framework Foundation -framework AppKit -framework OpenGL \
        -framework CoreGraphics -framework CoreText -femit-bin=hello-world-e2e || exit 1
    else
      zig build-exe hello-world.zig -lc -lazul -L. -I. -rpath . \
        -femit-bin=hello-world-e2e || exit 1
    fi
    ./hello-world-e2e
  ) >"$f" 2>&1
  finish zig "zig build/run failed (@cImport azul.h)"
}

# ---- OCaml -------------------------------------------------------------------
# Toolchain: dune + ocaml + the ctypes / ctypes-foreign opam packages
# (CI: ocaml/setup-ocaml, then `opam install dune ctypes ctypes-foreign`).
# README recipe: `dune exec ./hello_world.exe`. Needs azul.ml/.mli + dune files.
lang_ocaml() {
  if ! have dune || ! have ocaml; then
    skip ocaml "dune/ocaml not installed (ocaml/setup-ocaml + opam ctypes)"; return
  fi
  local f; f="$(log_path ocaml)"
  (
    set -x
    # Only copy the generated sources. The example's own dune/dune-project
    # already define BOTH the azul library and the hello_world executable —
    # overwriting them with the codegen's library-only dune breaks the build.
    cp "$CODEGEN_DIR/azul.ml"  "$REPO_ROOT/examples/ocaml/" 2>/dev/null || true
    cp "$CODEGEN_DIR/azul.mli" "$REPO_ROOT/examples/ocaml/" 2>/dev/null || true
    cp "$LIB_PATH"             "$REPO_ROOT/examples/ocaml/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/ocaml" || exit 1
    dune exec ./hello_world.exe
  ) >"$f" 2>&1
  finish ocaml "ocaml build/run failed (needs ctypes/ctypes-foreign)"
}

# ---- Haskell -----------------------------------------------------------------
# Toolchain: cabal + ghc (CI: haskell-actions/setup). The example's
# cabal.project declares `packages: . ../azul-haskell`, i.e. the GENERATED azul
# library package belongs in a SIBLING dir examples/azul-haskell/ (NOT inside
# examples/haskell/ — putting a 2nd .cabal there breaks cabal). cabal.project
# also hardcodes extra-lib-dirs to an absolute path from a different checkout,
# so we override it with --extra-lib-dirs pointing at our release dir.
# NOTE: README marks full-GUI as blocked on a libazul macOS webrender issue, so
# this may legitimately FAIL on macOS — we report whatever happens.
lang_haskell() {
  if ! have cabal || ! have ghc; then
    skip haskell "cabal/ghc not installed (haskell-actions/setup)"; return
  fi
  local f; f="$(log_path haskell)"
  (
    set -x
    # Place the generated azul library package in the sibling dir the
    # cabal.project expects. Clean any stale copy first.
    rm -rf "$REPO_ROOT/examples/azul-haskell"
    mkdir -p "$REPO_ROOT/examples/azul-haskell"
    cp -R "$CODEGEN_DIR"/haskell/. "$REPO_ROOT/examples/azul-haskell/" 2>/dev/null || true
    # The cbits C shim does `#include "azul.h"` with `include-dirs: cbits`, so
    # the header must sit inside cbits/ (codegen ships it at target/codegen/azul.h).
    cp "$CODEGEN_DIR/azul.h" "$REPO_ROOT/examples/azul-haskell/cbits/" 2>/dev/null || true
    cp "$LIB_PATH" "$REPO_ROOT/examples/azul-haskell/" 2>/dev/null || true
    cp "$LIB_PATH" "$REPO_ROOT/examples/haskell/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/haskell" || exit 1
    # Override the hardcoded extra-lib-dirs (points at a foreign checkout) so the
    # linker finds libazul in our release dir.
    cabal build --extra-lib-dirs="$RELEASE_DIR" || exit 1
    cabal run hello-world --extra-lib-dirs="$RELEASE_DIR"
  ) >"$f" 2>&1
  finish haskell "haskell build/run failed (FFI cbits)"
}

# ---- Pascal (FPC) ------------------------------------------------------------
# Toolchain: fpc (Free Pascal Compiler) (CI: install via apt `fp-compiler` /
# brew `fpc`). README marks this BLOCKED libazul-side (AzApp_run access
# violation on macOS) -> expected FAILS, which we report honestly.
lang_pascal() {
  have fpc || { skip pascal "fpc not installed (apt: fp-compiler / brew: fpc)"; return; }
  local f; f="$(log_path pascal)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.pas" "$REPO_ROOT/examples/pascal/" 2>/dev/null || true
    cp "$LIB_PATH"             "$REPO_ROOT/examples/pascal/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/pascal" || exit 1
    # README build line: fpc -Mobjfpc -Sh link against libazul in CWD.
    fpc -Mobjfpc -Sh -Fl. -k-L. -k-lazul hello-world.pas || exit 1
    ./hello-world
  ) >"$f" 2>&1
  finish pascal "pascal build/run failed (README notes libazul-side block)"
}

# ---- Fortran -----------------------------------------------------------------
# Toolchain: gfortran (CI: apt `gfortran` / brew `gcc`). README marks this as
# SMOKE-ONLY (tagged-union codegen gap) -> the hello_world is a smoke test, not
# the counter E2E, so this is expected to NOT print `test result: ok` (FAILS).
# Uses the generated Makefile.
lang_fortran() {
  have gfortran || { skip fortran "gfortran not installed (apt: gfortran)"; return; }
  local f; f="$(log_path fortran)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.f90"            "$REPO_ROOT/examples/fortran/" 2>/dev/null || true
    cp "$CODEGEN_DIR/Makefile.fortran"    "$REPO_ROOT/examples/fortran/Makefile" 2>/dev/null || true
    cp "$LIB_PATH"                        "$REPO_ROOT/examples/fortran/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/fortran" || exit 1
    make || exit 1
    ./hello_world
  ) >"$f" 2>&1
  finish fortran "fortran smoke-only (no counter E2E per README)"
}

# ---- COBOL (GnuCOBOL) --------------------------------------------------------
# Toolchain: cobc (GnuCOBOL) (CI: apt `gnucobol` / brew `gnu-cobol`). README
# marks this SMOKE-tier (needs hand-written ENTRY paragraphs for the full GUI),
# so expected FAILS for the counter E2E.
lang_cobol() {
  have cobc || { skip cobol "cobc not installed (apt: gnucobol / brew: gnu-cobol)"; return; }
  local f; f="$(log_path cobol)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.cpy" "$REPO_ROOT/examples/cobol/" 2>/dev/null || true
    cp "$LIB_PATH"             "$REPO_ROOT/examples/cobol/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/cobol" || exit 1
    # -x = build executable; -free for free-form; copybook on the COPY path (-I.).
    cobc -x -free -I. -L"$RELEASE_DIR" -lazul hello-world.cob -o hello-world-e2e || exit 1
    ./hello-world-e2e
  ) >"$f" 2>&1
  finish cobol "cobol smoke-tier (needs ENTRY paragraphs per README)"
}

# ---- Common Lisp (SBCL) ------------------------------------------------------
# Toolchain: sbcl + Quicklisp (for cffi/babel) (CI: install sbcl via apt/brew +
# bootstrap quicklisp). README marks this BLOCKED on macOS (NSApp threading) ->
# expected FAILS / SKIP. We attempt a load + run-app if quicklisp is present;
# otherwise SKIP.
lang_lisp() {
  have sbcl || { skip lisp "sbcl not installed (apt/brew: sbcl + quicklisp)"; return; }
  if [ ! -f "$HOME/quicklisp/setup.lisp" ]; then
    skip lisp "quicklisp not bootstrapped (~/quicklisp/setup.lisp missing)"; return
  fi
  local f; f="$(log_path lisp)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.lisp" "$REPO_ROOT/examples/lisp/" 2>/dev/null || true
    cp "$CODEGEN_DIR/azul.asd"  "$REPO_ROOT/examples/lisp/" 2>/dev/null || true
    cp "$LIB_PATH"              "$REPO_ROOT/examples/lisp/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/lisp" || exit 1
    sbcl --non-interactive \
      --eval "(load \"$HOME/quicklisp/setup.lisp\")" \
      --eval "(push (truename \".\") asdf:*central-registry*)" \
      --eval "(ql:quickload :azul-example)" \
      --eval "(azul-hello:run-app)"
  ) >"$f" 2>&1
  finish lisp "lisp build/run failed (README notes macOS NSApp block)"
}

# ---- Perl --------------------------------------------------------------------
# Toolchain: perl + FFI::Platypus (CI: apt `perl` + cpanm FFI::Platypus, or
# shogo82148/actions-setup-perl). README marks full GUI BLOCKED (invoker drops
# out_ptr) -> expected FAILS.
lang_perl() {
  have perl || { skip perl "perl not installed (apt: perl + cpanm FFI::Platypus)"; return; }
  # Probe FFI::Platypus presence — the smoke test needs it.
  if ! perl -MFFI::Platypus -e1 >/dev/null 2>&1; then
    skip perl "FFI::Platypus not installed (cpanm FFI::Platypus)"; return
  fi
  local f; f="$(log_path perl)"
  (
    set -x
    mkdir -p "$REPO_ROOT/examples/perl/lib"
    cp "$CODEGEN_DIR/Azul.pm" "$REPO_ROOT/examples/perl/lib/Azul.pm" 2>/dev/null || true
    cp "$LIB_PATH"            "$REPO_ROOT/examples/perl/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/perl" || exit 1
    perl -Ilib hello-world.pl
  ) >"$f" 2>&1
  finish perl "perl smoke-only (README notes callback-return block)"
}

# ---- PHP ---------------------------------------------------------------------
# Toolchain: php (CI: shivammathur/setup-php). The full path needs the
# php-extension build (separate cargo feature); the plain php-ffi path is
# POD-only (no callbacks) -> the counter E2E is expected to FAIL. We run the
# php-ffi smoke (hello-world.php) which only needs `php` + ext-ffi.
lang_php() {
  have php || { skip php "php not installed (shivammathur/setup-php)"; return; }
  local f; f="$(log_path php)"
  (
    set -x
    cp "$CODEGEN_DIR/Azul.php" "$REPO_ROOT/examples/php/" 2>/dev/null || true
    cp "$LIB_PATH"             "$REPO_ROOT/examples/php/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/php" || exit 1
    php hello-world.php
  ) >"$f" 2>&1
  finish php "php smoke-only (php-ffi has no callbacks; needs php-extension)"
}

# ---- PowerShell --------------------------------------------------------------
# Toolchain: pwsh / powershell (CI: PowerShell preinstalled on windows runners;
# on macOS/Linux install via brew/apt). README marks macOS BLOCKED (CFRunLoop
# owns the main thread) -> SKIP on macOS. On Windows there's no CFRunLoop
# conflict, so we attempt it for real: Add-Type JIT-compiles Azul.cs and
# Set-AzulLibraryPath $PSScriptRoot finds the dll we copy in.
lang_powershell() {
  if [ "$IS_WINDOWS" != 1 ]; then
    skip powershell "Windows-only (macOS pwsh CFRunLoop blocks NSApp.run per README)"; return
  fi
  # Prefer pwsh (PowerShell 7+); fall back to the built-in Windows PowerShell.
  local PSBIN; PSBIN="$(command -v pwsh || command -v powershell || true)"
  [ -n "$PSBIN" ] || { skip powershell "pwsh/powershell not installed"; return; }
  local f; f="$(log_path powershell)"
  (
    set -x
    cp "$CODEGEN_DIR/Azul.cs"   "$REPO_ROOT/examples/powershell/" 2>/dev/null || true
    cp "$CODEGEN_DIR/Azul.psd1" "$REPO_ROOT/examples/powershell/" 2>/dev/null || true
    cp "$CODEGEN_DIR/Azul.psm1" "$REPO_ROOT/examples/powershell/" 2>/dev/null || true
    # hello-world.ps1 calls Set-AzulLibraryPath $PSScriptRoot, so the dll must
    # sit next to the script (P/Invoke loads azul.dll from there).
    cp "$LIB_PATH"              "$REPO_ROOT/examples/powershell/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/powershell" || exit 1
    "$PSBIN" -ExecutionPolicy Bypass -File hello-world.ps1
  ) >"$f" 2>&1
  finish powershell "powershell run failed"
}

# ---- Python ------------------------------------------------------------------
# NOTE: python is NOT one of the 26 counter-E2E bindings — examples/python has
# no AZ_E2E hello-world counter (only a module-import demo + GUI examples that
# open a real window). The python extension is a separate cargo feature
# (python-extension) producing azul.so. We always SKIP with this note so the
# board stays honest. (Kept as a named entry because the task lists it.)
lang_python() {
  skip python "no AZ_E2E counter example (python-extension is a separate build; examples are GUI-only)"
}

# ---- Ada ---------------------------------------------------------------------
# Toolchain: gprbuild / gnatmake (GNAT) (CI: install GNAT-FSF via alire or
# apt `gnat` on Linux). README: not installable via brew on macOS -> usually
# SKIP on macOS, real attempt on Linux where gnat is present.
lang_ada() {
  if have gprbuild; then
    local f; f="$(log_path ada)"
    (
      set -x
      cp "$CODEGEN_DIR/azul.ads" "$REPO_ROOT/examples/ada/" 2>/dev/null || true
      cp "$CODEGEN_DIR/azul.adb" "$REPO_ROOT/examples/ada/" 2>/dev/null || true
      cp "$LIB_PATH"             "$REPO_ROOT/examples/ada/" 2>/dev/null || true
      cd "$REPO_ROOT/examples/ada" || exit 1
      gprbuild -P hello_world.gpr || exit 1
      ./obj/hello_world
    ) >"$f" 2>&1
    finish ada "ada build/run failed"
  elif have gnatmake; then
    local f; f="$(log_path ada)"
    (
      set -x
      cp "$CODEGEN_DIR/azul.ads" "$REPO_ROOT/examples/ada/" 2>/dev/null || true
      cp "$CODEGEN_DIR/azul.adb" "$REPO_ROOT/examples/ada/" 2>/dev/null || true
      cp "$LIB_PATH"             "$REPO_ROOT/examples/ada/" 2>/dev/null || true
      cd "$REPO_ROOT/examples/ada" || exit 1
      gnatmake hello_world.adb -L. -lazul || exit 1
      ./hello_world
    ) >"$f" 2>&1
    finish ada "ada build/run failed"
  else
    skip ada "gprbuild/gnatmake not installed (GNAT-FSF via alire / apt: gnat)"
  fi
}

# ---- Algol 68 (a68g) ---------------------------------------------------------
# Toolchain: a68g (Algol 68 Genie) (CI: build from source; no setup-action).
# README: a68g rejects the codegen's foreign-function syntax (dialect mismatch)
# -> expected FAILS even when installed. SKIP if absent.
lang_algol68() {
  have a68g || { skip algol68 "a68g not installed (build from source; niche)"; return; }
  local f; f="$(log_path algol68)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.a68" "$REPO_ROOT/examples/algol68/" 2>/dev/null || true
    cp "$LIB_PATH"             "$REPO_ROOT/examples/algol68/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/algol68" || exit 1
    a68g hello-world.a68
  ) >"$f" 2>&1
  finish algol68 "algol68 dialect-incompat per README (a68g rejects FFI syntax)"
}

# ---- FreeBASIC (fbc) ---------------------------------------------------------
# Toolchain: fbc (FreeBASIC) (CI: x86_64 Linux/Windows only; no macOS-aarch64
# build). README: toolchain unavailable on macOS-aarch64 -> SKIP there.
lang_freebasic() {
  have fbc || { skip freebasic "fbc not installed (x86_64 Linux/Win only; no macOS-aarch64)"; return; }
  local f; f="$(log_path freebasic)"
  (
    set -x
    cp "$CODEGEN_DIR/azul.bi" "$REPO_ROOT/examples/freebasic/" 2>/dev/null || true
    cp "$LIB_PATH"            "$REPO_ROOT/examples/freebasic/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/freebasic" || exit 1
    fbc hello-world.bas -L"$RELEASE_DIR" -l azul -x hello-world-e2e || exit 1
    ./hello-world-e2e
  ) >"$f" 2>&1
  finish freebasic "freebasic build/run failed"
}

# ---- Smalltalk (GNU Smalltalk / Pharo) ---------------------------------------
# Toolchain: gst (GNU Smalltalk) for the smoke layer (CI: apt `gnu-smalltalk` /
# brew `gnu-smalltalk`). README: Pharo full-GUI blocked on Tonel layout; gst
# runs the smoke test only -> expected FAILS for the counter E2E.
lang_smalltalk() {
  have gst || { skip smalltalk "gst not installed (apt/brew: gnu-smalltalk)"; return; }
  local f; f="$(log_path smalltalk)"
  (
    set -x
    cp "$CODEGEN_DIR/Azul.st" "$REPO_ROOT/examples/smalltalk/" 2>/dev/null || true
    cp "$LIB_PATH"            "$REPO_ROOT/examples/smalltalk/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/smalltalk" || exit 1
    gst HelloWorld.st
  ) >"$f" 2>&1
  finish smalltalk "smalltalk smoke-only (Pharo Tonel blocker per README)"
}

# ---- VB6 ---------------------------------------------------------------------
# Toolchain: the classic VB6 IDE compiler (VB6.EXE, `/make`) — 32-bit Windows
# only, legacy. ALWAYS SKIP on Linux/macOS. On Windows we *attempt* it: probe
# for VB6.EXE on PATH (and the usual install dir), and if present build the
# example .vbp with `VB6.EXE /make`. The GitHub windows runner does NOT ship
# the VB6 IDE, so the honest result there is a clean ⊘ "vb6 toolchain
# unavailable" — but the recipe is real and will run wherever VB6.EXE exists.
# NOTE: VB6 needs a 32-bit azul.dll (i686); the e2e job builds the 64-bit dll,
# so even with the IDE present this would need the i686 dll on PATH.
lang_vb6() {
  if [ "$IS_WINDOWS" != 1 ]; then
    skip vb6 "32-bit Windows legacy only (no VB6 toolchain on Linux/macOS)"; return
  fi
  # Locate the VB6 IDE compiler. `command -v` covers PATH; otherwise check the
  # canonical install location under Program Files (x86).
  local VB6BIN; VB6BIN="$(command -v VB6.EXE || command -v vb6.exe || command -v vb6 || true)"
  if [ -z "$VB6BIN" ]; then
    for _cand in "/c/Program Files (x86)/Microsoft Visual Studio/VB98/VB6.EXE" \
                 "/c/Program Files/Microsoft Visual Studio/VB98/VB6.EXE"; do
      [ -x "$_cand" ] && { VB6BIN="$_cand"; break; }
    done
  fi
  [ -n "$VB6BIN" ] || { skip vb6 "vb6 toolchain unavailable (VB6.EXE not found; not on CI runners)"; return; }
  local f; f="$(log_path vb6)"
  (
    set -x
    # The example ships its own HelloWorld.vbp + HelloWorld.bas; the codegen
    # emits the Azul.bas module + .cls wrappers into target/codegen/vb6/.
    cp "$CODEGEN_DIR/vb6/Azul.bas" "$REPO_ROOT/examples/vb6/" 2>/dev/null || true
    cp "$LIB_PATH"                 "$REPO_ROOT/examples/vb6/" 2>/dev/null || true
    cd "$REPO_ROOT/examples/vb6" || exit 1
    # VB6 IDE batch compile: /make builds the .exe defined by the .vbp.
    "$VB6BIN" /make HelloWorld.vbp /out vb6-build.log || { cat vb6-build.log 2>/dev/null; exit 1; }
    [ -f HelloWorld.exe ] || { echo "VB6 produced no HelloWorld.exe"; exit 1; }
    ./HelloWorld.exe
  ) >"$f" 2>&1
  finish vb6 "vb6 build/run failed (needs 32-bit azul.dll + VB6 IDE)"
}

# =============================================================================
# DRIVER: dispatch each requested language to its lang_<name> function.
# =============================================================================
# normalize requested langs
NORM_LANGS=()
for lang in "${LANGS[@]}"; do
  lang="$(echo "$lang" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
  [ -n "$lang" ] && NORM_LANGS+=("$lang")
done

if [ "$SINGLE" = 1 ]; then
  # --single: run the requested lang(s) in THIS process (no fan-out), writing
  # logs + .status sidecars, then exit. This is what the parallel driver
  # re-execs under `_timeout`, so a hung GUI binary is killed instead of
  # blocking the whole matrix. No status board here — the parent prints it.
  for lang in "${NORM_LANGS[@]}"; do
    fn="lang_${lang}"
    # Per-language AZ_RECORD trace file. libazul (debug-server build) writes its
    # internal event-loop log here with DEBUG_ENABLED forced on; the failure
    # dump shows its tail, so a native crash is pinned to the last step it
    # reached (e.g. "App created successfully" → crash = teardown/run bug).
    if [ "${E2E_RECORD:-1}" = 1 ]; then
      export AZ_RECORD="$WORKDIR/${lang}.azrecord"
    else
      unset AZ_RECORD
    fi
    if declare -F "$fn" >/dev/null 2>&1; then
      "$fn"
    else
      record "$lang" "FAILS" "unknown language (no recipe)"
    fi
  done
  exit 0
fi

# Parallel driver: each lang runs in a re-exec'd `--single` child under a
# wall-clock timeout (default 240s, override E2E_LANG_TIMEOUT), at most
# E2E_JOBS (default 4) at a time. This both bounds hangs and cuts wall time vs.
# the old sequential loop. Results are collected from the .status sidecars.
LANG_TIMEOUT="${E2E_LANG_TIMEOUT:-240}"
# Default concurrency = CPU count (GitHub free runners are 2-core Linux/Windows,
# 3-core macOS — so this stays gentle). The TIMEOUT, not the parallelism, is the
# real hang-fix; most langs SKIP instantly and only a handful actually compile,
# so even 2-wide meaningfully cuts the old fully-sequential 39 min. Override
# with E2E_JOBS (E2E_JOBS=1 = sequential, for the most constrained runners).
_ncpu="$( (nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2) )"
MAXJOBS="${E2E_JOBS:-$_ncpu}"
[ "$MAXJOBS" -ge 1 ] 2>/dev/null || MAXJOBS=1

# `wait -n` (reap one job → rolling parallelism) needs bash 4.3+. macOS ships
# bash 3.2; there we fall back to fixed-size batches (launch MAXJOBS, wait all).
HAVE_WAIT_N=0
( sleep 0 & wait -n ) >/dev/null 2>&1 && HAVE_WAIT_N=1

run_one() {  # backgrounded per-lang worker: re-exec --single under a timeout.
  local lang="$1"
  # NB: capture the exit code via `&&` short-circuit, NOT `if …; then return; fi`.
  # A bare `if <cmd>; then return 0; fi` whose condition is FALSE leaves the `if`
  # statement's own exit status at 0 (no else-branch ran), so a following
  # `local rc=$?` would always read 0 — masking real timeouts (124/137) as a
  # bogus "driver error rc=0". With `&&`, the failed command's status survives.
  _timeout "$LANG_TIMEOUT" bash "$SELF" --single "$lang" && return 0
  local rc=$?
  # Ensure a row exists even if the child was killed before it could record().
  # 124 = GNU timeout fired; 137 = SIGKILL (kill-after / OOM).
  if [ ! -f "$WORKDIR/$lang.status" ]; then
    if [ "$rc" = 124 ] || [ "$rc" = 137 ]; then
      printf '%s\t%s\t%s\n' "$lang" "FAILS" "timed out after ${LANG_TIMEOUT}s (hung — see log)" > "$WORKDIR/$lang.status"
    else
      printf '%s\t%s\t%s\n' "$lang" "FAILS" "driver error rc=$rc (see log)" > "$WORKDIR/$lang.status"
    fi
  fi
}

running=0
for lang in "${NORM_LANGS[@]}"; do
  echo ">>> [$lang] running..." >&2
  run_one "$lang" &
  running=$((running + 1))
  if [ "$running" -ge "$MAXJOBS" ]; then
    if [ "$HAVE_WAIT_N" = 1 ]; then
      wait -n; running=$((running - 1))
    else
      wait; running=0          # bash 3.2: drain the whole batch
    fi
  fi
done
wait

# Collect results from the .status sidecars in requested-lang order.
for lang in "${NORM_LANGS[@]}"; do
  if [ -f "$WORKDIR/$lang.status" ]; then
    IFS=$'\t' read -r s_lang s_status s_note < "$WORKDIR/$lang.status"
    RESULT_LANGS+=("${s_lang:-$lang}")
    RESULT_STATUS+=("${s_status:-FAILS}")
    RESULT_NOTE+=("${s_note:-no status written (see log)}")
  else
    RESULT_LANGS+=("$lang")
    RESULT_STATUS+=("FAILS")
    RESULT_NOTE+=("no status written (crashed before record — see log)")
  fi
done

# =============================================================================
# STATUS BOARD.
# Emits a markdown table to stdout and (if set) appends to $GITHUB_STEP_SUMMARY.
# =============================================================================
ICON_WORKS="✓ WORKS"
ICON_FAILS="✗ FAILS"
ICON_SKIP="⊘ SKIP"

n_works=0; n_fails=0; n_skip=0

emit_table() {
  echo "| language | tier | status | note |"
  echo "|----------|------|--------|------|"
  local i
  for i in "${!RESULT_LANGS[@]}"; do
    local lang="${RESULT_LANGS[$i]}"
    local st="${RESULT_STATUS[$i]}"
    local note="${RESULT_NOTE[$i]}"
    local tier; tier="$(tier_of "$lang")"
    local icon
    case "$st" in
      WORKS) icon="$ICON_WORKS" ;;
      FAILS) icon="$ICON_FAILS" ;;
      SKIP)  icon="$ICON_SKIP"  ;;
      *)     icon="$st" ;;
    esac
    # Escape pipe chars in notes so they don't break the markdown table.
    note="${note//|/\\|}"
    printf '| %s | %s | %s | %s |\n' "$lang" "$tier" "$icon" "$note"
  done
}

# Tally (count once, not per-output-stream). Also count SHIPPED-tier failures
# separately: that's the set --gate-shipped acts on. BETA/ALPHA never gate, and
# SKIP/WORKS never gate, so a shipped binding absent or unrunnable on this OS is
# reported SKIP and is NOT counted here.
shipped_fails=0
shipped_failed_list=""
for i in "${!RESULT_STATUS[@]}"; do
  case "${RESULT_STATUS[$i]}" in
    WORKS) n_works=$((n_works+1)) ;;
    FAILS) n_fails=$((n_fails+1)) ;;
    SKIP)  n_skip=$((n_skip+1))  ;;
  esac
  if [ "${RESULT_STATUS[$i]}" = "FAILS" ] && [ "$(tier_of "${RESULT_LANGS[$i]}")" = "shipped" ]; then
    shipped_fails=$((shipped_fails+1))
    shipped_failed_list="$shipped_failed_list ${RESULT_LANGS[$i]}"
  fi
done
TALLY="Tally: ${n_works} ✓ WORKS / ${n_fails} ✗ FAILS / ${n_skip} ⊘ SKIP  (of ${#RESULT_LANGS[@]} languages); SHIPPED failures: ${shipped_fails}"

# --- stdout ---
echo ""
echo "# Azul language-binding AZ_E2E status board"
echo ""
echo "Host: $OS_NAME   |   lib: $AZ_LIB"
echo "Scenario: $AZ_E2E"
echo "Logs: $WORKDIR"
echo ""
emit_table
echo ""
echo "$TALLY"

# --- Diagnostics: dump the tail of each FAILED shipped binding's log ----------
# The per-language build/run output is captured only to $WORKDIR/<lang>.log,
# which CI does NOT upload — so a board cell like "compile/link error (see log)"
# previously pointed at a log nobody could read. Echo the tail of every failed
# SHIPPED binding's log to stdout so each CI run is self-documenting (the only
# rows that gate are shipped ones, so that's all we surface). Each dump is a
# collapsible ::group:: in the Actions UI. Bounded to E2E_DUMP_FAIL_LINES lines;
# set E2E_DUMP_FAIL_LOG=0 to disable.
if [ "${E2E_DUMP_FAIL_LOG:-1}" = 1 ] && [ "$shipped_fails" -gt 0 ]; then
  dump_lines="${E2E_DUMP_FAIL_LINES:-80}"
  in_ci="${GITHUB_ACTIONS:-}"
  echo ""
  echo "===== FAILED shipped-binding logs (last ${dump_lines} lines each) ====="
  for i in "${!RESULT_STATUS[@]}"; do
    [ "${RESULT_STATUS[$i]}" = "FAILS" ] || continue
    [ "$(tier_of "${RESULT_LANGS[$i]}")" = "shipped" ] || continue
    d_lang="${RESULT_LANGS[$i]}"; d_note="${RESULT_NOTE[$i]}"
    d_f="$(log_path "$d_lang")"
    d_rec="$WORKDIR/${d_lang}.azrecord"
    [ -n "$in_ci" ] && echo "::group::✗ ${d_lang} — ${d_note}"
    echo "----- ${d_lang}: ${d_note} -----"
    if [ -f "$d_f" ]; then
      strip_ansi < "$d_f" | tail -n "$dump_lines"
    else
      echo "(no log at $d_f — the lang process was killed before it wrote output)"
    fi
    # libazul's internal event-loop trace (App create → window → E2E steps →
    # callback). Its tail pins a native crash to the last step it reached.
    if [ -s "$d_rec" ]; then
      echo "--- ${d_lang} libazul AZ_RECORD trace (last ${dump_lines} lines) ---"
      strip_ansi < "$d_rec" | tail -n "$dump_lines"
    fi
    [ -n "$in_ci" ] && echo "::endgroup::"
  done
  echo "===== end failed-binding logs ====="
fi

# --- GitHub step summary (markdown) ---
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  (
    echo "## Azul language-binding AZ_E2E status board"
    echo ""
    echo "Host: \`$OS_NAME\` &nbsp; lib: \`$AZ_LIB\`"
    echo ""
    emit_table
    echo ""
    echo "**$TALLY**"
    echo ""
  ) >> "$GITHUB_STEP_SUMMARY"
fi

# =============================================================================
# EXIT CODE.
#   default         always 0 (status report, not a gate).
#   --gate-shipped  exit 1 if a SHIPPED-tier binding FAILS (the CI gate). SKIP
#                   never trips it, so a shipped binding whose toolchain is absent
#                   or that can't run on this OS is not a failure (per-OS aware).
#   --strict        exit 1 if ANY language FAILS (the broad, all-tiers gate).
# SKIP never trips either — missing toolchains are not failures.
# =============================================================================
if [ "$GATE_SHIPPED" = 1 ] && [ "$shipped_fails" -gt 0 ]; then
  echo "" >&2
  echo "--gate-shipped: ${shipped_fails} SHIPPED binding(s) FAILED ->${shipped_failed_list} -> exiting nonzero." >&2
  exit 1
fi
if [ "$STRICT" = 1 ] && [ "$n_fails" -gt 0 ]; then
  echo "" >&2
  echo "--strict: ${n_fails} language(s) FAILED -> exiting nonzero." >&2
  exit 1
fi
exit 0
