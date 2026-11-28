#!/usr/bin/env bash
# Push 2 of the gesture series on the tree after the merge of origin/master (73 commits of other agents' work since the gated tree): the
# part of the gate the merge can change. api.json through the autofix flow (it must change nothing), the layout check, and the source gates
# (the first run of them failed on a line of master, since fixed). `GATE <step>: exit <n>` lines at the end, real exit codes.
set +e
mkdir -p out
results=()
# the source gates in their own target directory, beside the azul-doc build
( CARGO_TARGET_DIR=target-srcgate cargo test -p azul-srcgate --no-fail-fast > out/gate-srcgate.log 2>&1; echo $? > out/srcgate.rc ) &
SRC_PID=$!
cargo build -r -p azul-doc > out/build-doc.log 2>&1; echo "azul-doc build exit $?"
D=target/release/azul-doc
$D autofix > out/autofix.log 2>&1; $D autofix explain >> out/autofix.log 2>&1
$D patch safe target/autofix/patches >> out/autofix.log 2>&1; $D patch target/autofix/patches >> out/autofix.log 2>&1; $D normalize >> out/autofix.log 2>&1
if git diff --quiet; then echo "GATE autofix-no-drift: exit 0"; results+=("autofix-no-drift:0"); else echo "GATE autofix-no-drift: exit 1"; results+=("autofix-no-drift:1"); git diff --stat | tail -5; git diff | head -80; fi
$D check layout > out/gate-check-layout.log 2>&1
rc=$?
tail -5 out/gate-check-layout.log
echo "GATE check-layout: exit $rc"
results+=("check-layout:$rc")
wait $SRC_PID
grep -aE "^test result|FAILED|panicked at|finding\(s\)|^layout/|^dll/" out/gate-srcgate.log | head -60
rc=$(cat out/srcgate.rc)
echo "GATE srcgate: exit $rc"
results+=("srcgate:$rc")
echo "=== the verdicts"
status=0
for r in "${results[@]}"; do echo "GATE ${r%%:*}: exit ${r##*:}"; [ "${r##*:}" = 0 ] || status=1; done
echo "GATE push2-merged: $([ $status = 0 ] && echo PASSED || echo FAILED)"
exit $status
