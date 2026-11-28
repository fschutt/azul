#!/usr/bin/env bash
# The idle-redraw host tests of azul-dll (headless window), with their messages.
set -u
out="${RUNNER_TEMP:-/tmp}/idle_run.out"
cargo test -p azul-dll --lib -- an_idle_page_asks_for_no_frame idle_cpu > "$out" 2>&1
code=$?
grep -aE "^test .*(ok|FAILED)$|^test result|^error" "$out" | head -30
awk '/^---- .* stdout ----$/{p=10} p>0{print; p--}' "$out" | head -60
echo "IDLE_EXIT=$code"
