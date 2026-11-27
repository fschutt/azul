#!/usr/bin/env bash
# The landscape-band tests on the OLD code (RED proof).
set -u
out="${RUNNER_TEMP:-/tmp}/land_red.out"
cargo test -p azul-layout --test all -- a_rotated_window a_window_turned_back > "$out" 2>&1
code=$?
grep -aE "^test .*(ok|FAILED)$|^test result|^error" "$out" | head -20
awk '/^---- .* stdout ----$/{p=10} p>0{print; p--}' "$out" | head -60
echo "LAND_RED_EXIT=$code"
