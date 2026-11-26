#!/usr/bin/env bash
# The mutation check of the link crate on a runner (mutate.py says what it does):
#
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-review-link/scripts/mutate_gate.sh iroh' --codegen
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-review-link/scripts/mutate_gate.sh client'
#
# One table per run: a runner job ends after two hours, and each row is a build and a test run. The iroh table's tests run over the real
# engine, so libazul is built first (--codegen on the run; AZ_REQUIRE_IROH_ENGINE=1 makes a missing engine a failure, not a skipped test);
# the client table needs no engine and no --codegen.
set -uo pipefail
which="${1:?usage: mutate_gate.sh iroh|client}"
case "$which" in
  iroh)
    case "$(uname)" in
      Darwin) lib=libazul.dylib; var=DYLD_LIBRARY_PATH ;;
      *) lib=libazul.so; var=LD_LIBRARY_PATH ;;
    esac
    cargo build -r -p azul-dll --features build-dll || { echo "=== FAILED: libazul"; exit 1; }
    mkdir -p target/azul-lib
    cp "target/release/$lib" target/azul-lib/
    export AZ_LINK_PATH="$PWD/target/azul-lib"
    export "$var=$PWD/target/azul-lib"
    ;;
  client) ;;
  *) echo "usage: mutate_gate.sh iroh|client"; exit 2 ;;
esac
if python3 -I examples/azul-review-link/scripts/mutate.py "$which"; then
  echo "=== mutation check ok: $which"
else
  echo "=== mutation check FAILED: $which"
  exit 1
fi
