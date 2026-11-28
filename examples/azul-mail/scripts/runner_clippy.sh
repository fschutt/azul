#!/usr/bin/env bash
# AzMail's lints on a GitHub runner (the bindings must be generated first):
#
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-mail/scripts/runner_clippy.sh' --codegen
#
# `cargo clippy -p AzMail --tests` with the stub libazul of runner_check.sh (a lint links nothing);
# prints every warning that points into AzMail's own files.
set +e
link=$(mktemp -d)
: > "$link/libazul.so"
export AZ_LINK_PATH="$link"
cargo clippy -p AzMail --tests > "$link/clippy.txt" 2>&1
code=$?
echo "=== clippy exit $code"
grep -c "^warning" "$link/clippy.txt"
grep -B1 -A12 -E '^\s+--> examples/azul-mail/' "$link/clippy.txt" | head -600
echo "=== the lint summary"
grep -E "^(warning|error): .*generated|could not compile" "$link/clippy.txt" | head -20
exit "$code"
