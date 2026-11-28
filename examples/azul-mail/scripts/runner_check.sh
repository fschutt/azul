#!/usr/bin/env bash
# AzMail's compile gate on a GitHub runner (the bindings must be generated first):
#
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-mail/scripts/runner_check.sh' --codegen
#
# `cargo check -p AzMail --tests` over the generated bindings. azul-dll's build script refuses to
# link a libazul that lies in the cargo profile directory, and a check links nothing: an empty stub
# file in a folder of its own satisfies it. The command text stays the same from run to run, so the
# runner's build cache (keyed by the command) is warm after the first run.
set +e
link=$(mktemp -d)
: > "$link/libazul.so"
export AZ_LINK_PATH="$link"
cargo check -p AzMail --tests > "$link/check.txt" 2>&1
code=$?
# the generated code's own warnings come first; what is wrong with AzMail follows
awk '/^error/ {p=1} p' "$link/check.txt" | head -700
echo "=== warnings that point into AzMail"
grep -B2 -A8 -E '^\s+--> examples/azul-(mail|appkit)/' "$link/check.txt" | head -300
echo "=== check exit $code"
exit "$code"
