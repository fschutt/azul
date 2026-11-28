#!/usr/bin/env bash
# E45 on a Windows runner: the Windows shell compiles (cargo check of azul-dll with the generated bindings), real exit code printed.
set -u
out="${RUNNER_TEMP:-/tmp}/win_check.out"
cargo check -p azul-dll --lib > "$out" 2>&1
code=$?
grep -aE "^error" -A12 "$out" | head -80
tail -3 "$out"
echo "WIN_CHECK_EXIT=$code"
