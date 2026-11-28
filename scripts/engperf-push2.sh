#!/usr/bin/env bash
# Push 2 of the gesture series on the combined tree, ONE run: api.json through the autofix flow a second time (it must change nothing), the
# doc checker, both clippy lines, then the whole azul-dll lib (the bindings are generated from api.json) and the source gates.
# Real exit codes; `GATE <step>: exit <n>` lines at the end.
set +e
mkdir -p out
results=()
step() {
    local name="$1"; shift
    echo "=== $name: $*"
    "$@" > "out/gate-$name.log" 2>&1
    local rc=$?
    grep -aE "^test result|^error(\[E[0-9]+\])?:|^ +--> |FAILED|panicked at|could not compile|^\[(ok|FAIL|skip)\]|^CHECK " "out/gate-$name.log" | head -60
    echo "GATE $name: exit $rc"
    results+=("$name:$rc")
}
sudo apt-get install -y -qq libv4l-dev fonts-dejavu-core fonts-liberation > /dev/null 2>&1
cargo build -r -p azul-doc > out/build-doc.log 2>&1; echo "azul-doc build exit $?"
D=target/release/azul-doc
$D autofix > out/autofix.log 2>&1; $D autofix explain >> out/autofix.log 2>&1
$D patch safe target/autofix/patches >> out/autofix.log 2>&1; $D patch target/autofix/patches >> out/autofix.log 2>&1; $D normalize >> out/autofix.log 2>&1
if git diff --quiet; then echo "GATE autofix-no-drift: exit 0"; results+=("autofix-no-drift:0"); else echo "GATE autofix-no-drift: exit 1"; results+=("autofix-no-drift:1"); git diff --stat | tail -5; git diff | head -60; fi
step check-layout $D check layout
F=azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater
step clippy-features cargo clippy --keep-going -p azul-core -p azul-css -p azul-layout --all-targets --features "$F" -- -D warnings
step clippy cargo clippy --keep-going -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings
step dll-lib cargo test -p azul-dll --lib --no-fail-fast
step srcgate cargo test -p azul-srcgate --no-fail-fast
echo "=== the verdicts"
status=0
for r in "${results[@]}"; do echo "GATE ${r%%:*}: exit ${r##*:}"; [ "${r##*:}" = 0 ] || status=1; done
echo "GATE push2: $([ $status = 0 ] && echo PASSED || echo FAILED)"
exit $status
