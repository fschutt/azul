#!/usr/bin/env bash
# The companion's gate on a Linux runner (scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-companion/scripts/gate_linux.sh' --codegen):
# the same as gate_mac.sh with the library libazul.so, which is what the CI's dll_tests job builds too.
set -euo pipefail
cargo build -r -p azul-dll --features build-dll
mkdir -p target/azul-lib
cp target/release/libazul.so target/azul-lib/
export AZ_LINK_PATH="$PWD/target/azul-lib"
export LD_LIBRARY_PATH="$PWD/target/azul-lib"
cargo check -p AzCompanion --all-targets
cargo test -p AzCompanion --lib
cargo test -p azul-review-link --lib
AZ_REQUIRE_IROH_ENGINE=1 cargo test -p azul-review-link --lib --features link-dynamic
cargo run -p azul-review-link --example host_wiring
