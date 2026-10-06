#!/bin/bash
# api.json changes go through autofix (never by hand): run `azul-doc autofix`,
# apply its patches, repeat until it generates none, then regenerate the code.
cd "$(git rev-parse --show-toplevel)" || exit 1
while true; do
  echo "Running azul-doc autofix..."
  output=$(cargo run --release -p azul-doc -- autofix 2>&1)

  if echo "$output" | grep -q "Generated 0 patches"; then
    echo "0 patches to apply. Breaking."
    break
  fi

  if echo "$output" | grep -q "Generated .* patches"; then
    echo "Found patches. Applying..."
    cargo run --release -p azul-doc -- patch target/autofix/patches 2>&1
  else
    echo "No patches mentioned, but also didn't say 0 patches. Something is wrong."
    echo "$output"
    break
  fi
done
echo "Running codegen all..."
cargo run --release -p azul-doc -- codegen all 2>&1
echo "Done."
