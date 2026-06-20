#!/usr/bin/env bash
# =============================================================================
# test_export_code.sh — E2E test for AZ_DEBUG "Export → Code"
#
# Builds a C host app, runs it under AZ_DEBUG, and for each of rust/c/cpp/python
# asks the debug server to export the LIVE page as a compilable Azul project,
# then compiles/typechecks each exported project. Fails on any compile error.
#
# Requires: the DLL built with `--features build-dll,debug-server`, generated
# headers in target/codegen (azul.h / azul20.hpp), curl, python3, a C/C++
# compiler, and cargo.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PORT="${AZ_DEBUG_PORT:-8791}"
OUT_DIR="$ROOT/target/export_e2e"
HOST_BIN="$ROOT/target/export_e2e/host"
CODEGEN="$ROOT/target/codegen"
RELEASE="$ROOT/target/release"

CC="${CC:-cc}"
CXX="${CXX:-c++}"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

# ── Locate the prebuilt libazul (debug-server build) ────────────────────────
case "$(uname -s)" in
  Darwin) LIBNAME="libazul.dylib"; RPATH_FLAGS="-Wl,-rpath,$RELEASE";;
  *)      LIBNAME="libazul.so";    RPATH_FLAGS="-Wl,-rpath,$RELEASE";;
esac
if [ ! -f "$RELEASE/$LIBNAME" ]; then
  echo "FATAL: $RELEASE/$LIBNAME not found. Build with:"
  echo "  cargo build --release -p azul-dll --features build-dll,debug-server"
  exit 1
fi

# ── Step 1: build a C host that renders a small page ────────────────────────
# Reuse the existing hello-world.c host (a label + button) as the live UI.
echo "=== Build C host ==="
$CC -o "$HOST_BIN" "$ROOT/examples/c/hello-world.c" \
  -I "$CODEGEN" -L "$RELEASE" -lazul $RPATH_FLAGS -lpthread -lm -ldl 2>/dev/null \
  || $CC -o "$HOST_BIN" "$ROOT/examples/c/hello-world.c" -I "$CODEGEN" -L "$RELEASE" -lazul $RPATH_FLAGS
echo "  -> $HOST_BIN"

# ── Step 2: launch under AZ_DEBUG (headless) ────────────────────────────────
echo "=== Launch under AZ_DEBUG=$PORT ==="
AZ_BACKEND=headless AZ_DEBUG=$PORT "$HOST_BIN" > "$OUT_DIR/app.log" 2>&1 &
APP_PID=$!
cleanup() { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; }
trap cleanup EXIT

for i in $(seq 1 60); do
  if curl -s -o /dev/null -X POST "http://localhost:$PORT/" \
       -H 'Content-Type: application/json' -d '{"op":"get_state"}' 2>/dev/null; then
    echo "  -> server ready (attempt $i)"; break
  fi
  sleep 0.5
  if [ "$i" = 60 ]; then echo "FATAL: debug server never came up"; cat "$OUT_DIR/app.log"; exit 1; fi
done

# ── Step 3: export + compile each language ──────────────────────────────────
FAILED=0
for LANG in rust c cpp python; do
  echo ""
  echo "=== Export + compile: $LANG ==="
  LDIR="$OUT_DIR/$LANG"; mkdir -p "$LDIR"
  RESP="$LDIR/response.json"
  curl -s -X POST "http://localhost:$PORT/" -H 'Content-Type: application/json' \
    -d "{\"op\":\"export_code\",\"language\":\"$LANG\"}" -o "$RESP"

  # write the files from data.value.files
  python3 - "$RESP" "$LDIR" <<'PY'
import json, os, sys
resp, out = sys.argv[1], sys.argv[2]
data = json.load(open(resp))
files = data.get('data', {}).get('value', {}).get('files', {})
if not files:
    print("  -> NO FILES in export response:", json.dumps(data)[:300]); sys.exit(2)
for fname, content in files.items():
    fp = os.path.join(out, fname)
    os.makedirs(os.path.dirname(fp) or out, exist_ok=True)
    open(fp, 'w').write(content)
    print("  -> wrote", fname)
PY

  case "$LANG" in
    rust)
      # emit a Cargo.toml pointing at the LOCAL azul, then `cargo check`
      cat > "$LDIR/Cargo.toml" <<EOF
[workspace]
[package]
name = "azexport"
version = "0.0.0"
edition = "2021"
[dependencies]
azul = { path = "$ROOT/dll", package = "azul-dll", default-features = false, features = ["link-dynamic"] }
[[bin]]
name = "azexport"
path = "src/main.rs"
EOF
      ( cd "$LDIR" && cargo check --quiet ) || { echo "  !! RUST FAILED"; FAILED=1; }
      ;;
    c)
      clang -fsyntax-only -I "$CODEGEN" "$LDIR/main.c" || { echo "  !! C FAILED"; FAILED=1; }
      ;;
    cpp)
      # Probe the C++ toolchain first; if the local libc++ headers are missing
      # (some dev machines), skip rather than false-fail. CI has a full toolchain.
      if ! printf '#include <cstdint>\nint main(){return 0;}\n' | clang++ -std=c++20 -fsyntax-only -x c++ - 2>/dev/null; then
        echo "  -> C++ toolchain unavailable (no libc++ headers) — SKIPPED (CI tests this)"
      else
        clang++ -fsyntax-only -std=c++20 -I "$CODEGEN" "$LDIR/main.cpp" || { echo "  !! C++ FAILED"; FAILED=1; }
      fi
      ;;
    python)
      python3 -m py_compile "$LDIR/main.py" || { echo "  !! PYTHON FAILED"; FAILED=1; }
      ;;
  esac
  [ "$FAILED" = 0 ] && echo "  -> $LANG OK"
done

echo ""
if [ "$FAILED" = 0 ]; then
  echo "=== ALL EXPORT LANGUAGES COMPILED ==="
else
  echo "=== EXPORT TEST FAILED ==="; exit 1
fi
