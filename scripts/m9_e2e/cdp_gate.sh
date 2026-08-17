#!/usr/bin/env bash
# Rebuild dll (BigInt-safe azMemset/azMemcpy + intrinsics) → restart server → CDP click gate.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/cdp_gate.log; : > "$LOG"
export RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=abort" CARGO_BUILD_JOBS=6
echo "=== build dll $(date +%H:%M:%S) ===" | tee -a "$LOG"
cargo build -p azul-dll --release --no-default-features --features "build-dll web web-transpiler" \
  -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc 2>&1 | tail -3 | tee -a "$LOG"
grep -q "error\[" "$LOG" && { echo "COMPILE-ERROR" | tee -a "$LOG"; exit 1; }
[ -f target/x86_64-pc-windows-msvc/release/azul.dll ] || { echo "DLL-FAIL" | tee -a "$LOG"; exit 1; }
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
cp -f target/x86_64-pc-windows-msvc/release/azul.dll examples/c/azul.dll
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb examples/c/azul.pdb
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1
export AZ_MINI_MAX_DEPTH=16384 AZ_CB_MAX_DEPTH=8192
echo "=== relift $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_cdp.log 2>&1 &
for i in $(seq 1 400); do
  grep -qE "Listening on" /c/rb/server_cdp.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  a=$(ps -W 2>/dev/null | grep -icE 'hello-world')
  [ "$a" = "0" ] && { echo "SERVER DIED" | tee -a "$LOG"; tail -6 /c/rb/server_cdp.log | tee -a "$LOG"; exit 1; }
  sleep 10
done
# Timeout must ABORT, not fall through: without READY the click gate would run
# against whatever stale tab/server answers, producing convincing-looking
# results from the WRONG build (observed once: "counter=127" artifact).
grep -qE "Listening on" /c/rb/server_cdp.log 2>/dev/null || {
  echo "TIMEOUT: server never printed 'Listening on' — aborting gate" | tee -a "$LOG"; exit 1; }
echo "link-fails: $(grep -icE '0xc0000142|falling back to 8-byte' /c/rb/server_cdp.log)" | tee -a "$LOG"
grep -oE "azul-mini: lifted \+ linked [0-9]+ bytes \([0-9]+ exports" /c/rb/server_cdp.log | tail -1 | tee -a "$LOG"
# x86 tail-call discovery (2026-08-14): these two lines are the fix's verdict.
# `Formatter::pad` is reached ONLY by tail-jmp from the Display-for-str thunks, so
# a non-zero count proves the jmp-rel32 scan found what the arm64-only scan missed.
echo "tail-call deps found: $(grep -icE 'tail-call dep: ' /c/rb/server_cdp.log)" | tee -a "$LOG"
echo "Formatter::pad lifted: $(grep -icE 'Formatter::pad[^_]' /c/rb/server_cdp.log)" | tee -a "$LOG"
grep -oE "transitive lift complete: [0-9]+ functions" /c/rb/server_cdp.log | tail -1 | tee -a "$LOG"
sleep 3
# Edge headless does not survive reboots — relaunch if :9222 is down (otherwise
# the click gate dies with "fetch failed" while the lift results are fine).
if ! curl -s -o /dev/null --max-time 3 http://127.0.0.1:9222/json/version; then
  echo "(re)starting Edge headless on :9222" | tee -a "$LOG"
  nohup "/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe" \
    --headless=new --remote-debugging-port=9222 --disable-gpu --no-first-run \
    --user-data-dir="$(cygpath -w "${TEMP:-/tmp}/az-edge-headless" 2>/dev/null || echo /tmp/az-edge-headless)" \
    about:blank >/dev/null 2>&1 &
  for i in $(seq 1 20); do
    curl -s -o /dev/null --max-time 2 http://127.0.0.1:9222/json/version && break
    sleep 1
  done
fi
echo "=== CDP CLICK GATE $(date +%H:%M:%S) ===" | tee -a "$LOG"
"/c/Users/felix/tools/node/node.exe" --experimental-websocket scripts/cdp_click_hw.js 2>&1 | tail -8 | tee -a "$LOG"
echo "=== MARKERS (probeFmt + solve) $(date +%H:%M:%S) ===" | tee -a "$LOG"
# AZ_SKIP_FMT=1: a trapping export never restores the shadow-stack global, and one
# leaked 134592-byte frame (SP starts at 196608) makes EVERY later export trap OOB.
# Skipping probeFmt keeps the solve measurement honest. Run the fmt probe separately.
AZ_PORT=8800 AZ_HYDRATE=1 AZ_FONT=1 AZ_SKIP_FMT=1 "/c/Users/felix/tools/node/node.exe" --experimental-websocket \
  scripts/m9_e2e/full-cycle.js 2>&1 | grep -vE '^\[STUB-0\]' \
  | grep -E "2d-alloc|2d-cc\]|2d-ldri|2d-dispatch|2d\] solve|2d-fontparse|PASS|FAIL" -A3 | tee -a "$LOG"
echo "--- fmt probe (separate instance; may trap) ---" | tee -a "$LOG"
AZ_PORT=8800 AZ_HYDRATE=1 AZ_FONT=1 "/c/Users/felix/tools/node/node.exe" --experimental-websocket \
  scripts/m9_e2e/full-cycle.js 2>&1 | grep -vE '^\[STUB-0\]' \
  | grep -E "2d-fmt\]|2d-fmt-mark" | tee -a "$LOG"
echo "--- lift failures (remill/llc crashes → Leaf stubs) ---" | tee -a "$LOG"
grep -cE "FAILED to lift" /c/rb/server_cdp.log | tee -a "$LOG"
grep -oE "⚠ [0-9]+ function\(s\) Leaf-stubbed" /c/rb/server_cdp.log | tail -2 | tee -a "$LOG"
echo "=== GATE DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
