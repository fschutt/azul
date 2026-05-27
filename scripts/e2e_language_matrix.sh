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
#
# Exit code: always 0 (status report, not a gate) UNLESS --strict is given, in
# which case it exits 1 if any language is FAILS. SKIP (missing toolchain) and
# WORKS never trip --strict.
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
# Arg parsing: optional --strict flag (anywhere) + optional subset list.
# Subset may be space- or comma-separated, one arg or many.
# -----------------------------------------------------------------------------
STRICT=0
SUBSET_RAW=""
for arg in "$@"; do
  case "$arg" in
    --strict) STRICT=1 ;;
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
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/azul-e2e-matrix.XXXXXX")"

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
record() {
  RESULT_LANGS+=("$1")
  RESULT_STATUS+=("$2")
  RESULT_NOTE+=("$3")
}

# skip <lang> <note>
skip() { record "$1" "SKIP" "$2"; }

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
    cargo build --release -p azul-examples --example hello-world || exit 1
    ./target/release/examples/hello-world
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
      ./hello-world-e2e.exe
      exit $?
    else
      "$CXX" -g -O0 -std=c++20 -I. hello-world.cpp -L"$RELEASE_DIR" -lazul \
        -lpthread -lm -ldl -o hello-world-e2e || exit 1
    fi
    ./hello-world-e2e
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
    dotnet run -c Release
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
    local JVM_ARGS=(-Djna.library.path=. -cp "target/hello-world-1.0.0.jar:$JNA_JAR" com.azul.HelloWorld)
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
      if [ "$IS_MACOS" = 1 ]; then
        java -XstartOnFirstThread -Djna.library.path=. \
          -cp "hello-world.jar:$JNA_JAR" com.azul.HelloWorldKt
      else
        java -Djna.library.path=. \
          -cp "hello-world.jar:$JNA_JAR" com.azul.HelloWorldKt
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
for lang in "${LANGS[@]}"; do
  lang="$(echo "$lang" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
  [ -n "$lang" ] || continue
  fn="lang_${lang}"
  if declare -F "$fn" >/dev/null 2>&1; then
    echo ">>> [$lang] running..." >&2
    "$fn"
  else
    record "$lang" "FAILS" "unknown language (no recipe)"
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
  echo "| language | status | note |"
  echo "|----------|--------|------|"
  local i
  for i in "${!RESULT_LANGS[@]}"; do
    local lang="${RESULT_LANGS[$i]}"
    local st="${RESULT_STATUS[$i]}"
    local note="${RESULT_NOTE[$i]}"
    local icon
    case "$st" in
      WORKS) icon="$ICON_WORKS" ;;
      FAILS) icon="$ICON_FAILS" ;;
      SKIP)  icon="$ICON_SKIP"  ;;
      *)     icon="$st" ;;
    esac
    # Escape pipe chars in notes so they don't break the markdown table.
    note="${note//|/\\|}"
    printf '| %s | %s | %s |\n' "$lang" "$icon" "$note"
  done
}

# Tally (count once, not per-output-stream).
for i in "${!RESULT_STATUS[@]}"; do
  case "${RESULT_STATUS[$i]}" in
    WORKS) n_works=$((n_works+1)) ;;
    FAILS) n_fails=$((n_fails+1)) ;;
    SKIP)  n_skip=$((n_skip+1))  ;;
  esac
done
TALLY="Tally: ${n_works} ✓ WORKS / ${n_fails} ✗ FAILS / ${n_skip} ⊘ SKIP  (of ${#RESULT_LANGS[@]} languages)"

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
# EXIT CODE: always 0, unless --strict and at least one FAILS.
# (SKIP never trips strict — missing toolchains are not failures.)
# =============================================================================
if [ "$STRICT" = 1 ] && [ "$n_fails" -gt 0 ]; then
  echo "" >&2
  echo "--strict: ${n_fails} language(s) FAILED -> exiting nonzero." >&2
  exit 1
fi
exit 0
