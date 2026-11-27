#!/usr/bin/env bash
# E41 gate B (ubuntu, with the bindings): the whole `cargo test -p azul-dll --lib`, then the source gates. Real exit codes; failures print
# their assertion messages.
set -u
out="${RUNNER_TEMP:-/tmp}/e41_gate_b.out"
cargo test -p azul-dll --lib > "$out" 2>&1
code=$?
echo "dll tests: $(grep -aE '^test result' "$out" | head -3)"
grep -aE "^test .* FAILED|^error" "$out" | head -40
awk '/^---- .* stdout ----$/{p=14} p>0{print; p--}' "$out" | head -150
echo "DLL_LIB_EXIT=$code"
cargo test -p azul-srcgate --no-fail-fast > "$out" 2>&1
code=$?
grep -aE "^test result|^test .* FAILED|^error" "$out" | head -40
awk '/^---- .* stdout ----$/{p=14} p>0{print; p--}' "$out" | head -80
echo "SRCGATE_EXIT=$code"
