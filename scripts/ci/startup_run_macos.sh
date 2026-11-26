#!/usr/bin/env bash
# The startup timeline on a macOS runner: a release libazul (build-dll) + AzWidgets, the binaries as artifacts
# (out/), a few starts on the runner itself (a virtual machine: the Mac's own numbers come from running the
# artifact there), and the tests of the timeline. Run through scripts/remote.sh with --codegen:
#   scripts/remote.sh run <sha> macos-15 'bash scripts/ci/startup_run_macos.sh' --codegen --artifacts 'out/*'
set +e
echo "=== runner: $(uname -a)"
echo "=== machine: $(sw_vers -productVersion), cpu $(sysctl -n machdep.cpu.brand_string), vmm: $(sysctl -n kern.hv_vmm_present 2>&1), cores $(sysctl -n hw.ncpu), mem $(sysctl -n hw.memsize)"
echo "=== release libazul (build-dll)"
cargo build --release -p azul-dll --features build-dll 2>&1 | tail -40
mkdir -p "$RUNNER_TEMP/azul-cabi" out
cp target/release/libazul.dylib "$RUNNER_TEMP/azul-cabi/" && cp target/release/libazul.dylib out/
echo "=== AzWidgets (release, linked dynamically)"
AZ_LINK_PATH="$RUNNER_TEMP/azul-cabi" cargo build --release -p AzWidgets 2>&1 | tail -40
cp target/release/AzWidgets out/
cp scripts/ci/startup_measure.py out/
ls -la out
echo "=== AzWidgets on the runner: 7 starts, the timeline of each, exit at the first frame"
python3 scripts/ci/startup_measure.py --exe "$PWD/out/AzWidgets" --runs 7 --label "AzWidgets macos release (runner VM)" --env "DYLD_LIBRARY_PATH=$PWD/out" --json out/startup-macos-runner.json
echo "=== the phases of each layout pass: one start with AZ_PROFILE=cpu (the profiler adds a little)"
( cd out && DYLD_LIBRARY_PATH="$PWD" AZ_STARTUP=1 AZ_PROFILE=cpu AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1 ./AzWidgets 2>&1 | grep -E "^\[CPU\]|^\[startup\]" | head -220 )
echo "=== the tests of the timeline, the settings and the app's shared state"
cargo test --release -p azul-dll --lib --features build-dll -- startup_timeline shell_settings app_shared first_frame 2>&1 | grep -E "^test |^test result|FAILED|panicked|^error|could not compile" | head -80
