#!/usr/bin/env bash
# The compile gate of the startup work on one host: does everything type-check (the library, its bins, tests and examples
# under the features of the shipped library), and do the pure tests of the work pass? Cheaper than a release build and a
# measurement (the measurements run after the push). Run through scripts/remote.sh with --codegen on any runner:
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash scripts/ci/startup_gate_compile.sh' --codegen
#   scripts/remote.sh run <sha> macos-15 'bash scripts/ci/startup_gate_compile.sh' --codegen
set +e
echo "=== runner: $(uname -a)"
echo "=== cargo check -p azul-dll --all-targets (build-dll)"
cargo check -p azul-dll --all-targets --features build-dll --message-format short 2>&1 | tail -80
check_status="${PIPESTATUS[0]}"
echo "CHECK all-targets: exit $check_status"
echo "=== the tests of the startup work (the library's own unit tests: the timeline, the settings, the first frame, the style discovery, the GTK input method, the process-free environment reader)"
cargo test -p azul-dll --lib --features build-dll -- startup_timeline shell_settings app_shared first_frame gtk_im style_discovery system_style env:: 2>&1 \
    | grep -E "^test |^test result|FAILED|panicked|^error|could not compile|^failures:" | head -150
test_status="${PIPESTATUS[0]}"
echo "TESTS: exit $test_status"
[ "$check_status" = "0" ] && [ "$test_status" = "0" ]
