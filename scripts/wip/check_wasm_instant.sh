#!/usr/bin/env bash
# Reject std::time::Instant::now() reaching a WASM-compatible crate ungated.
#
# std's Instant::now() PANICS on wasm32-unknown-unknown. azul-css, azul-core and
# azul-layout all compile to wasm, so any call must sit behind a cfg that
# actually excludes wasm.
#
# This replaces an inline grep that had two holes, both verified against the tree:
#
#   1. It matched only the fully-qualified `std::time::Instant::now()` — 15 hits.
#      The idiomatic `use std::time::Instant;` + `Instant::now()`, and the alias
#      `use std::time::Instant as StdInstant;` + `StdInstant::now()` (core/src/
#      task.rs:32, used at :264/:266/:1190), were INVISIBLE to it. A new
#      un-gated `use std::time::Instant;` would have passed silently.
#
#   2. It accepted ANY `#[cfg` within 30 lines above — `#[cfg(test)]`,
#      `#[cfg(target_os = "windows")]`, `#[cfg(feature = "svg")]` all satisfied
#      it, and none of those excludes wasm.
#
# The naive widening (`\bInstant::now\(\)`) is wrong in the other direction: it
# matches 75 sites, most of which are azul_core::task::Instant — the INJECTABLE
# clock that exists precisely so azul does not assume a real one. Flagging those
# is what would push someone to "fix" the good API. So this is file-aware:
# it resolves what `Instant` means in each file first, and only then looks at
# calls.
set -uo pipefail

DIRS=("$@")
[ ${#DIRS[@]} -eq 0 ] && DIRS=(css/src core/src layout/src)

# A cfg that genuinely keeps code off wasm. `feature = "std"` counts because the
# wasm builds do not enable it; that is the convention the tree already uses.
CFG_OK='target_family[[:space:]]*=[[:space:]]*"wasm"|target_arch[[:space:]]*=[[:space:]]*"wasm|feature[[:space:]]*=[[:space:]]*"std"'

found=0
scanned=0

for dir in "${DIRS[@]}"; do
  [ -d "$dir" ] || { echo "ERROR: no such directory: $dir" >&2; exit 2; }
  while IFS= read -r file; do
    # What does `Instant` refer to in THIS file? Only std's counts.
    names=""
    # `use std::time::Instant as X;` binds X, NOT Instant — in such a file a bare
    # `Instant` is something else entirely (in core/src/task.rs it is azul's OWN
    # injectable Instant, which is the API we WANT people using). Flagging it
    # would push someone to "fix" the good type.
    alias=$(sed -nE 's/^[[:space:]]*use[[:space:]]+std::time::Instant[[:space:]]+as[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*;.*/\1/p' "$file" | head -1)
    if [ -n "$alias" ]; then
      names="$alias"
    elif grep -qE '^[[:space:]]*use[[:space:]]+std::time::(\{[^}]*\b)?Instant\b[[:space:]]*(,[^;]*)?;' "$file"; then
      names="Instant"
    fi
    # The fully-qualified form needs no import.
    pattern='std::time::Instant::now\(\)'
    [ -n "$names" ] && pattern="$pattern|\\b($names)::now\\(\\)"

    while IFS= read -r hit; do
      lno=${hit%%:*}
      code=${hit#*:}
      trimmed=$(printf '%s' "$code" | sed 's/^[[:space:]]*//')
      case "$trimmed" in
        //*|/\**|\**) continue ;;   # comment
      esac
      scanned=$((scanned + 1))
      start=$(( lno > 120 ? lno - 120 : 1 ))
      # Module-level `#![cfg(...)]` at the top of the file gates everything in it.
      if head -20 "$file" | grep -qE "#!\[cfg[^]]*($CFG_OK)"; then continue; fi
      if ! sed -n "${start},${lno}p" "$file" | grep -qE "#\[cfg[^]]*($CFG_OK)"; then
        echo "ERROR: std Instant::now() not gated away from wasm at ${file}:${lno}"
        echo "    $trimmed"
        found=1
      fi
    done < <(grep -nE "$pattern" "$file" | sed 's/:/ /; s/ /:/')
  done < <(find "$dir" -name '*.rs' -type f)
done

if [ "$found" -eq 1 ]; then
  cat >&2 <<'MSG'

std::time::Instant::now() PANICS on wasm32-unknown-unknown.

Either gate the call with a cfg that excludes wasm — #[cfg(feature = "std")] or
#[cfg(not(target_family = "wasm"))] — or, better, use azul_core::task::Instant,
which exists exactly so azul does not assume a real clock is present.
MSG
  exit 1
fi

echo "OK: $scanned std-Instant call site(s) checked, all gated away from wasm."
