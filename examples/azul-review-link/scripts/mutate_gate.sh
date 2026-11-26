#!/usr/bin/env bash
# The mutation check of the link crate on a runner (mutate.py says what it does):
#
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-review-link/scripts/mutate_gate.sh [iroh|client|both]' --codegen
#
# libazul is built first (the iroh table's tests run over the real engine; AZ_REQUIRE_IROH_ENGINE=1 makes a missing engine a failure), then
# each table is run; a table that has an escaped, stale or broken mutation fails the script at the end, the other table still runs.
set -uo pipefail
which="${1:-both}"
case "$(uname)" in
  Darwin) lib=libazul.dylib; var=DYLD_LIBRARY_PATH ;;
  *) lib=libazul.so; var=LD_LIBRARY_PATH ;;
esac
cargo build -r -p azul-dll --features build-dll || { echo "=== FAILED: libazul"; exit 1; }
mkdir -p target/azul-lib
cp "target/release/$lib" target/azul-lib/
export AZ_LINK_PATH="$PWD/target/azul-lib"
export "$var=$PWD/target/azul-lib"
status=0
if [ "$which" = both ] || [ "$which" = iroh ]; then
  echo "=== table: iroh"
  python3 -I examples/azul-review-link/scripts/mutate.py iroh || status=1
fi
if [ "$which" = both ] || [ "$which" = client ]; then
  echo "=== table: client"
  python3 -I examples/azul-review-link/scripts/mutate.py client || status=1
fi
[ "$status" -eq 0 ] && echo "=== mutation check ok" || echo "=== mutation check FAILED"
exit $status
