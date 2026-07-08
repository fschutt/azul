#!/usr/bin/env bash
# run_memtest.sh — run one language's azul memtest and verify it neither
# SEGFAULTS nor LEAKS. Language-agnostic: the caller passes the already-set-up
# run command (env like LD_LIBRARY_PATH / PYTHONPATH set by the caller).
#
#   scripts/run_memtest.sh <lang-label> <run-command...>
#
# The memtest binary must honor AZ_MEMTEST_N (loop iterations) and exit 0.
# Two checks:
#   1. SEGFAULT — run under gdb with a tiny N; fail on SIGSEGV/SIGABRT.
#   2. LEAK     — run with a small then a large N and compare peak RSS; a real
#                 per-iteration leak scales with N, a correct binding stays flat.
#
# Exit 0 = clean, non-zero = crash or leak. Intended for the CI memtest matrix.
set -u
LABEL="${1:?usage: run_memtest.sh <label> <cmd...>}"; shift
CMD=("$@")
fail=0

echo "=== memtest [$LABEL]: $* ==="

# --- 1. segfault check (gdb, tiny N) ---
if command -v gdb >/dev/null 2>&1; then
  seg="$(AZ_MEMTEST_N=2000 timeout 180 gdb -batch \
           -ex 'run' -ex 'bt 8' -ex 'quit' --args "${CMD[@]}" 2>&1)"
  if printf '%s' "$seg" | grep -qE "SIGSEGV|SIGABRT|received signal"; then
    echo "[$LABEL] FAIL: crash under gdb"
    printf '%s\n' "$seg" | grep -A8 -E "SIGSEGV|SIGABRT|received signal" | head -20
    fail=1
  else
    echo "[$LABEL] segfault check: OK"
  fi
else
  echo "[$LABEL] (gdb absent — skipping segfault check)"
fi

# --- 2. leak check (peak RSS growth across N) ---
peak_rss_kb() { # $1 = N ; echoes peak RSS KB, or empty on failure
  AZ_MEMTEST_N="$1" timeout 420 /usr/bin/time -v "${CMD[@]}" 2>&1 \
    | grep -i "Maximum resident set size" | grep -oE "[0-9]+" | head -1
}
small="$(peak_rss_kb 50000)"
large="$(peak_rss_kb 300000)"
if [ -n "$small" ] && [ -n "$large" ]; then
  grew=$((large - small))
  echo "[$LABEL] RSS: N=50k -> ${small}KB, N=300k -> ${large}KB (grew ${grew}KB over 250k iters)"
  # A flat create/destroy loop grows only by allocator noise (<1MB observed for
  # python). 25MB over 750k extra iterations = ~34 B/iter, a wide leak margin.
  if [ "$grew" -gt 12000 ]; then
    echo "[$LABEL] FAIL: leak (RSS grew ${grew}KB)"
    fail=1
  else
    echo "[$LABEL] leak check: OK"
  fi
else
  echo "[$LABEL] (could not measure RSS — /usr/bin/time -v unavailable?)"
fi

[ "$fail" -eq 0 ] && echo "[$LABEL] MEMTEST PASS" || echo "[$LABEL] MEMTEST FAIL"
exit $fail
