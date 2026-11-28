#!/usr/bin/env bash
# What the tree of a push must pass that the measurement runs do not look at:
#   1. the doc checker. `azul-doc check layout` reads every code sample of the guide and the examples for calls api.json does not
#      define; in a guide a dotted name followed by a parenthesis (`kern.proc.pid (resolution ...)`) is read as a call. The
#      whole `azul-doc check` adds the guide's links, the orphaned wiring and the other lints.
#   2. the cross-target type checks of what the startup work touches: Windows, iOS, Android and macOS (nothing is linked, one
#      Ubuntu runner does all four: scripts/ci/startup_check_targets.sh).
# Run through scripts/remote.sh with --codegen (the examples lint reads target/codegen):
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash scripts/ci/startup_prepush.sh' --codegen
set +e
mkdir -p out
echo "=== runner: $(uname -a), cores $(nproc)"
echo "=== azul-doc check layout"
cargo run -r -p azul-doc -- check layout > out/check-layout.log 2>&1
check_layout=$?
grep -E "^\[(ok|FAIL|skip)\]|^    " out/check-layout.log | head -60
echo "CHECK layout: exit $check_layout"
echo "=== azul-doc check (the whole checker: informational, a failure that master has too is not this tree's)"
cargo run -r -p azul-doc -- check > out/check-all.log 2>&1
check_all=$?
grep -E "^\[(ok|FAIL|skip)\]|^    " out/check-all.log | head -80
echo "CHECK all: exit $check_all"
echo "=== the cross-target type checks"
bash scripts/ci/startup_check_targets.sh
check_targets=$?
echo "CHECK targets: exit $check_targets"
[ "$check_layout" = "0" ] && [ "$check_targets" = "0" ]
