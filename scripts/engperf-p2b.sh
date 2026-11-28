#!/usr/bin/env bash
# Push 2 of the gesture series, the dll half only (the combined gate's dll step ran on a tree with the iOS Copy-lint incident): the whole
# azul-dll lib and the source gates on the tree with master >= 1fe1278d93. `GATE <step>: exit <n>` lines at the end.
set +e
mkdir -p out
results=()
step() {
    local name="$1"; shift
    echo "=== $name: $*"
    "$@" > "out/gate-$name.log" 2>&1
    local rc=$?
    grep -aE "^test result|^error(\[E[0-9]+\])?:|^ +--> |FAILED|panicked at|could not compile" "out/gate-$name.log" | head -60
    echo "GATE $name: exit $rc"
    results+=("$name:$rc")
}
sudo apt-get install -y -qq libv4l-dev fonts-dejavu-core fonts-liberation > /dev/null 2>&1
step dll-lib cargo test -p azul-dll --lib --no-fail-fast
step srcgate cargo test -p azul-srcgate --no-fail-fast
echo "=== the verdicts"
status=0
for r in "${results[@]}"; do echo "GATE ${r%%:*}: exit ${r##*:}"; [ "${r##*:}" = 0 ] || status=1; done
echo "GATE push2-dll: $([ $status = 0 ] && echo PASSED || echo FAILED)"
exit $status
