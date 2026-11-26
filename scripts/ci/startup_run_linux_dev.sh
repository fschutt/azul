#!/usr/bin/env bash
# The DEV profile's start: AzWidgets built the way `cargo run` builds it (the dev profile: opt-level 0 for the engine and
# every dependency), the timeline of 5 starts under Xvfb with the CPU renderer, then the same with the dependencies built at
# opt-level 2 (`[profile.dev.package."*"] opt-level = 2`, here as `--config`: what a project that wants a fast `cargo run`
# puts into its Cargo.toml), and the numbers side by side. Run through scripts/remote.sh with --codegen:
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash scripts/ci/startup_run_linux_dev.sh' --codegen --artifacts 'out/*.json'
set +e
echo "=== runner: $(uname -a)"; grep -m1 'model name' /proc/cpuinfo; echo "cores: $(nproc)"; uptime
sudo apt-get install -y -qq xvfb xauth >/dev/null 2>&1
Xvfb :99 -screen 0 1280x800x24 >/dev/null 2>&1 &
sleep 2
export DISPLAY=:99
mkdir -p out
echo "=== dev profile: libazul (build-dll) and AzWidgets, opt-level 0"
cargo build -p azul-dll --features build-dll 2>&1 | tail -20
mkdir -p "$RUNNER_TEMP/cabi-dev"
cp target/debug/libazul.so "$RUNNER_TEMP/cabi-dev/"
AZ_LINK_PATH="$RUNNER_TEMP/cabi-dev" cargo build -p AzWidgets 2>&1 | tail -20
mkdir -p out/dev
cp target/debug/AzWidgets out/dev/ && cp target/debug/libazul.so out/dev/
echo "=== dev profile, opt-level 0: 5 starts"
LD_LIBRARY_PATH="$PWD/out/dev" python3 scripts/ci/startup_measure.py --exe "$PWD/out/dev/AzWidgets" --runs 5 --label "AzWidgets linux DEV profile (opt-level 0), Xvfb, CPU renderer" --json out/startup-linux-dev.json --timeout 120
echo "=== dev profile with the dependencies at opt-level 2: libazul (build-dll) and AzWidgets"
cargo build -p azul-dll --features build-dll --config 'profile.dev.package."*".opt-level=2' 2>&1 | tail -20
mkdir -p "$RUNNER_TEMP/cabi-dev2" out/dev2
cp target/debug/libazul.so "$RUNNER_TEMP/cabi-dev2/"
AZ_LINK_PATH="$RUNNER_TEMP/cabi-dev2" cargo build -p AzWidgets --config 'profile.dev.package."*".opt-level=2' 2>&1 | tail -20
cp target/debug/AzWidgets out/dev2/ && cp target/debug/libazul.so out/dev2/
echo "=== dev profile, dependencies at opt-level 2: 5 starts"
LD_LIBRARY_PATH="$PWD/out/dev2" python3 scripts/ci/startup_measure.py --exe "$PWD/out/dev2/AzWidgets" --runs 5 --label "AzWidgets linux DEV profile, dependencies at opt-level 2, Xvfb, CPU renderer" --json out/startup-linux-dev2.json --timeout 120
