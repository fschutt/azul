#!/usr/bin/env bash
# The gate of a push of the startup work, in two halves that run side by side on two runners (each is a debug build of its own):
#
#   dll     the doc checker (`azul-doc check layout`), the WHOLE lib tests of azul-dll, the integration test of the transient
#           windows (the docking of an inline panel), the source gates, and (last: their target directories sit in the tree) the
#           type checks of the targets no runner builds natively (Windows, iOS, Android, macOS: scripts/ci/startup_check_targets.sh)
#   srcgate the source gates alone (a clean tree)
#   layout  the CI's workspace test command for azul-css / azul-core / azul-layout / webrender (the same feature list, the
#           fonts the layout tests measure), its three required-features steps, and both clippy lines with `-D warnings`
#
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash scripts/ci/startup_push_gate.sh dll'    --codegen --artifacts 'out/gate-*.log'
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash scripts/ci/startup_push_gate.sh layout' --codegen --artifacts 'out/gate-*.log'
#
# The exit status of the script is the verdict: 0 only when every step of the half exited 0 (a step that ran no test at all is
# no pass: scripts/zero_test_guard.sh). The last lines are `GATE <step>: exit <n>` for each step, so a log is read once.
set +e
part="${1:?the half to run: dll or layout}"
mkdir -p out
results=()

# step <name> <command...>: runs the command with its output in out/gate-<name>.log, shows the lines that decide, keeps the exit status
step() {
    local name="$1"
    shift
    echo "=== $name: $*"
    "$@" > "out/gate-$name.log" 2>&1
    local rc=$?
    grep -E "^test result|^error(\[E[0-9]+\])?:|^ +--> |FAILED|panicked at|could not compile|^\[(ok|FAIL|skip)\]|^CHECK " "out/gate-$name.log" | head -80
    echo "GATE $name: exit $rc"
    results+=("$name:$rc")
}

# guard <name>: the log of a test step ran tests (a `running 0 tests` everywhere is a red step, not a green one)
guard() {
    local name="$1"
    ./scripts/zero_test_guard.sh "out/gate-$name.log" > "out/gate-$name-guard.log" 2>&1
    local rc=$?
    tail -3 "out/gate-$name-guard.log"
    echo "GATE $name-guard: exit $rc"
    results+=("$name-guard:$rc")
}

echo "=== runner: $(uname -a), cores $(nproc), disk $(df -h . | tail -1 | awk '{print $4" free"}')"
case "$part" in
dll)
    sudo apt-get install -y -qq libv4l-dev fonts-dejavu-core > /dev/null 2>&1
    step check-layout cargo run -r -p azul-doc -- check layout
    step dll-lib cargo test -p azul-dll --lib --no-fail-fast
    guard dll-lib
    step dll-transient cargo test -p azul-dll --test transient_window_layout
    guard dll-transient
    # The source gates scan the whole tree, and the speech-engine gate reads the `.fingerprint` files of any `target*` directory
    # it finds for the name of a feature no shipped build may name: they run BEFORE the cross-target checks, whose target
    # directories (target-windows, target-ios, target-android, target-macos) sit in the repository root.
    step srcgate cargo test -p azul-srcgate --no-fail-fast
    guard srcgate
    step targets bash scripts/ci/startup_check_targets.sh
    ;;
srcgate)
    step srcgate cargo test -p azul-srcgate --no-fail-fast
    guard srcgate
    ;;
firstlayout)
    # The pieces of the first layout's font wait: the rule and the resolver in azul-layout (and the tests that run the
    # resolver with a live registry), the shell's decision with a registry that never scans, the font cache's tests
    # (compile errors and the new tests first, before the whole halves).
    step layout-rule cargo test -p azul-layout --test all -- a_dom_drawn_by_faces_in_memory_needs_no_system_font a_stack_drawn_by_faces_in_memory mock_font_metrics typed_script_font_fallback a_variable_font_never_reaches_shaping
    guard layout-rule
    step dll-wait cargo test -p azul-dll --lib -- layout_font_wait app_font_cache font_cache_guard
    guard dll-wait
    ;;
fonts)
    # The font cache's tests and the source gates: for a change of those tests alone, after the whole `dll` half passed
    # on the same production code.
    step dll-fonts cargo test -p azul-dll --lib -- app_font_cache font_cache_guard font_instance_store
    guard dll-fonts
    step srcgate cargo test -p azul-srcgate --no-fail-fast
    guard srcgate
    ;;
layout)
    sudo apt-get install -y -qq fonts-liberation fonts-dejavu-core > /dev/null 2>&1
    export AZ_REQUIRE_TEST_FONTS=1
    features="azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater"
    # --keep-going: every target is linted and every finding reported in ONE run (the lib failing first hides the test targets)
    step clippy-features cargo clippy --keep-going -p azul-core -p azul-css -p azul-layout --all-targets --features "$features" -- -D warnings
    step clippy cargo clippy --keep-going -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings
    step workspace cargo test -p azul-css -p azul-core -p azul-layout -p webrender --lib --tests --features "$features" --no-fail-fast
    guard workspace
    step css-io cargo test -p azul-css --features io --test test_system_style
    step e2e-json cargo test -p azul-layout --test e2e_json --features e2e-server
    step core-hooks cargo test -p azul-core --features test-hooks --test run_mode_hooks
    ;;
*)
    echo "unknown half: $part (dll or layout)"
    exit 2
    ;;
esac

echo "=== the verdicts of the half '$part'"
status=0
for r in "${results[@]}"; do
    echo "GATE ${r%%:*}: exit ${r##*:}"
    [ "${r##*:}" = "0" ] || status=1
done
echo "GATE $part: $([ "$status" = "0" ] && echo PASSED || echo FAILED)"
exit "$status"
