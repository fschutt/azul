#!/usr/bin/env bash
# The new E41 tests on the OLD code (RED proof): the interpreter tests of azul-core, then the headless-window tests of azul-dll.
set -u
out="${RUNNER_TEMP:-/tmp}/e41_red.out"
cargo test -p azul-core --lib -- press_pass_drag_tests > "$out" 2>&1
code=$?
grep -aE "^test .*(ok|FAILED)$|^test result|^error" "$out" | head -20
awk '/^---- .* stdout ----$/{p=8} p>0{print; p--}' "$out" | head -40
echo "CORE_RED_EXIT=$code"
cargo test -p azul-dll --lib -- a_press_on_text_at_the_edge > "$out" 2>&1
code=$?
grep -aE "^test .*(ok|FAILED)$|^test result|^error" "$out" | head -20
awk '/^---- .* stdout ----$/{p=12} p>0{print; p--}' "$out" | head -60
echo "DLL_RED_EXIT=$code"
