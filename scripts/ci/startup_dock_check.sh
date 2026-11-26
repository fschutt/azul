#!/usr/bin/env bash
# The docking fix of the first layout (layout/src/transient.rs: an inline transient window that opens as its config said is no
# docking change): RED against the source without it, GREEN with it, and the whole transient-window suite with it.
#   scripts/remote.sh run <sha of the fix commit or a descendant> ubuntu-22.04 'bash scripts/ci/startup_dock_check.sh <sha of the fix commit>' --codegen
# The tests are in layout/src/transient_dock_test.rs (the manager's unit tests) and dll/tests/transient_window_layout.rs
# (`an_inline_panel_the_first_dom_opens_costs_one_layout_pass`: the headless window counts the layout callback's calls).
set +e
fix="${1:?the sha of the fix commit}"
echo "=== runner: $(uname -a)"
git fetch -q --depth=2 origin "$fix" 2>&1 | tail -2
if ! git show "$fix~1:layout/src/transient.rs" > "$RUNNER_TEMP/transient_without_fix.rs"; then
    echo "the parent of $fix is not available"
    exit 2
fi
cp layout/src/transient.rs "$RUNNER_TEMP/transient_with_fix.rs"
if cmp -s "$RUNNER_TEMP/transient_without_fix.rs" "$RUNNER_TEMP/transient_with_fix.rs"; then
    echo "the checked out transient.rs is the one without the fix: nothing to compare"
    exit 2
fi
filter() { grep -E "^test |^test result|FAILED|panicked|^error|could not compile|assertion|left:|right:"; }
# One cargo call: its output in a file, its exit status kept, the lines that matter shown.
run() {
    log="$RUNNER_TEMP/dock-check-$1.log"
    shift
    "$@" > "$log" 2>&1
    status=$?
    filter < "$log" | head -160
    return "$status"
}

echo "=== RED: layout/src/transient.rs WITHOUT the fix - the manager's unit tests of the docking (failures expected)"
cp "$RUNNER_TEMP/transient_without_fix.rs" layout/src/transient.rs
run red-layout cargo test -p azul-layout --lib -- transient_dock_test
echo "RED layout tests: exit $? (non-zero expected)"
echo "=== RED: the same source - the window's layout passes (failure expected)"
run red-dll cargo test -p azul-dll --features build-dll --test transient_window_layout -- an_inline_panel_the_first_dom_opens_costs_one_layout_pass
echo "RED layout passes: exit $? (non-zero expected)"

echo "=== GREEN: layout/src/transient.rs WITH the fix - the manager's unit tests (all of the transient module)"
cp "$RUNNER_TEMP/transient_with_fix.rs" layout/src/transient.rs
run green-layout cargo test -p azul-layout --lib -- transient
layout_status=$?
echo "LAYOUT transient tests: exit $layout_status"
echo "=== GREEN: the whole transient-window suite of the headless shell (popups, tear-off, docking, dialogs, pickers)"
run green-dll cargo test -p azul-dll --features build-dll --test transient_window_layout
dll_status=$?
echo "DLL transient_window_layout: exit $dll_status"
[ "$layout_status" = "0" ] && [ "$dll_status" = "0" ]
