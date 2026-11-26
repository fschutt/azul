#!/usr/bin/env bash
# The companion's gate on a macOS runner (scripts/remote.sh run <sha> macos-15 'bash examples/azul-companion/scripts/gate_mac.sh' --codegen
# --artifacts 'target/azul-lib/*'): libazul built with the shipped feature set, the app and the link crate checked and tested, the transport
# tests over the real iroh engine, the wiring example run. target/azul-lib/ is uploaded: the library, the two generated files a local
# build needs (AZ_LINK_PATH = the folder with the library, AZ_CODEGEN_DIR = the folder with the two .rs files) and the two programs
# (AzCompanion, companion_sim: release builds, run with DYLD_LIBRARY_PATH = that folder).
set -euo pipefail
cargo build -r -p azul-dll --features build-dll
mkdir -p target/azul-lib
cp target/release/libazul.dylib target/azul-lib/
cp target/codegen/dll_api_external.rs target/codegen/reexports.rs target/azul-lib/
export AZ_LINK_PATH="$PWD/target/azul-lib"
export DYLD_LIBRARY_PATH="$PWD/target/azul-lib"
cargo check -p AzCompanion --all-targets
cargo test -p AzCompanion --lib
cargo test -p azul-review-link --lib
AZ_REQUIRE_IROH_ENGINE=1 cargo test -p azul-review-link --lib --features link-dynamic
cargo run -p azul-review-link --example host_wiring
# the two programs, built here so that nothing has to be built on a machine with little disk: the phone's own app (device mode) and
# the simulator, to run beside the library
cargo build -r -p AzCompanion --bin AzCompanion --bin companion_sim
cp target/release/AzCompanion target/release/companion_sim target/azul-lib/
