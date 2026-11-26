#!/usr/bin/env bash
# The companion's gate on a macOS runner (scripts/remote.sh run <sha> macos-15 'bash examples/azul-companion/scripts/gate_mac.sh' --codegen
# --artifacts 'target/azul-lib/*'): libazul built with the shipped feature set, the two programs built (the phone's own app and the
# simulator, release), then the app and the link crate checked and tested, the transport tests over the real iroh engine and the wiring
# example run. A step that fails does not stop the others: what failed is said at the end and the exit status is not zero.
# target/azul-lib/ is uploaded: the library, the two generated files a local build needs (AZ_LINK_PATH = the folder with the library,
# AZ_CODEGEN_DIR = the folder with the two .rs files) and the two programs (run them with DYLD_LIBRARY_PATH = that folder).
set -uo pipefail
failed=""
step() {
  local name="$1"
  shift
  echo "=== step: $name"
  if "$@"; then echo "=== ok: $name"; else echo "=== FAILED: $name"; failed="$failed [$name]"; fi
}
cargo build -r -p azul-dll --features build-dll || { echo "=== FAILED: libazul"; exit 1; }
mkdir -p target/azul-lib
cp target/release/libazul.dylib target/azul-lib/
cp target/codegen/dll_api_external.rs target/codegen/reexports.rs target/azul-lib/
export AZ_LINK_PATH="$PWD/target/azul-lib"
export DYLD_LIBRARY_PATH="$PWD/target/azul-lib"
step "build the programs" cargo build -r -p AzCompanion --bin AzCompanion --bin companion_sim
cp target/release/AzCompanion target/release/companion_sim target/azul-lib/ 2>/dev/null || true
step "check the app" cargo check -p AzCompanion --all-targets
step "app tests" cargo test -p AzCompanion --lib
step "link tests" cargo test -p azul-review-link --lib
step "transport tests (real iroh engine)" env AZ_REQUIRE_IROH_ENGINE=1 cargo test -p azul-review-link --lib --features link-dynamic
step "wiring example" cargo run -p azul-review-link --example host_wiring
if [ -n "$failed" ]; then
  echo "=== gate failed:$failed"
  exit 1
fi
echo "=== gate ok"
