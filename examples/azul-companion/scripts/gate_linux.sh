#!/usr/bin/env bash
# The companion's gate on a Linux runner (scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-companion/scripts/gate_linux.sh' --codegen):
# the same steps as gate_mac.sh with the library libazul.so, which is what the CI's dll_tests job builds too. A step that fails does not stop
# the others: what failed is said at the end and the exit status is not zero.
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
cp target/release/libazul.so target/azul-lib/
export AZ_LINK_PATH="$PWD/target/azul-lib"
export LD_LIBRARY_PATH="$PWD/target/azul-lib"
step "check the app" cargo check -p AzCompanion --all-targets
# clippy over the two crates: only what it says about their own files counts (the engine's crates are the CI clippy job's business)
clippy_own() {
  local out code
  out="$(mktemp)"
  cargo clippy "$@" > "$out" 2>&1
  code=$?
  grep -E -B3 -A10 -e '--> examples/azul-(companion|review-link)/' "$out" | head -300
  if grep -q -E -e '--> examples/azul-(companion|review-link)/' "$out"; then return 1; fi
  if [ "$code" -ne 0 ]; then tail -40 "$out"; fi
  return "$code"
}
step "clippy (app)" clippy_own -p AzCompanion --all-targets
step "clippy (link crate with the engine adapter)" clippy_own -p azul-review-link --all-targets --features link-dynamic
step "app tests" cargo test -p AzCompanion --lib
step "link tests" cargo test -p azul-review-link --lib
step "transport tests (real iroh engine)" env AZ_REQUIRE_IROH_ENGINE=1 cargo test -p azul-review-link --lib --features link-dynamic
step "wiring example" cargo run -p azul-review-link --example host_wiring
if [ -n "$failed" ]; then
  echo "=== gate failed:$failed"
  exit 1
fi
echo "=== gate ok"
