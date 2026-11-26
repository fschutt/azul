#!/usr/bin/env bash
# The clippy sweep's runner command: the CI's autofix_check job (api.json must not drift: the lint fixes
# narrowed visibility of items in azul-layout, which the autofix reads from the source).
set +e
cargo build -r -p azul-doc
export PATH="$PWD/target/release:$PATH"
azul-doc autofix
azul-doc autofix explain
azul-doc patch safe target/autofix/patches
azul-doc patch target/autofix/patches
azul-doc normalize
echo "== git diff --stat after the autofix =="
git diff --stat | tail -20
echo "== git status --porcelain =="
git status --porcelain | head -20
true
