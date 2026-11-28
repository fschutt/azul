#!/usr/bin/env bash
# AzMail's unit tests on a GitHub runner (the bindings must be generated first):
#
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-mail/scripts/runner_test.sh' --codegen
#
# The real libazul is built first (`build-dll`) and copied to a folder of its own (azul-dll's
# build script refuses a library in the cargo profile directory, and a test binary needs a real
# one to link), then `cargo test -p AzMail --lib`.
set +e
cargo build -p azul-dll --features build-dll 2>&1 | tail -3
link=$(mktemp -d)
cp target/debug/libazul.so "$link/"
nm -D "$link/libazul.so" | grep -q " T AzU8Vec_delete" && echo CABI_OK
export AZ_LINK_PATH="$link"
cargo test -p AzMail --lib --no-fail-fast > "$link/test.txt" 2>&1
code=$?
echo "=== test exit $code"
awk '/^error/ {p=1} p' "$link/test.txt" | head -300
grep -E '^test result|^running|FAILED|panicked|^failures:' "$link/test.txt" | head -150
tail -40 "$link/test.txt"
exit "$code"
