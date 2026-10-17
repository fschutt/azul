#!/usr/bin/env bash
#
# scripts/check_layout_features.sh - azul-layout builds in every feature set a
# consumer turns on.
#
# A default build compiles every `#[cfg(feature = ..)]` branch at once, so code
# that uses an item imported only under a feature it does not itself sit under
# compiles there and nowhere else. printpdf (and through it html2pdf) builds
# azul-layout with `default-features = false`, so such code breaks printpdf's
# build while every default build here passes.
#
# The sets: the bare crate, single features, and printpdf's own feature lists
# (printpdf/Cargo.toml `text_layout`, `text_layout_hyphenation`, `html`,
# `html_multithreaded`, `svg`). Keep the printpdf rows in sync with it. Every
# set is checked even after one fails, so one run names every broken set.
#
# Usage: scripts/check_layout_features.sh   (from the repository root)

set -uo pipefail

SETS=(
  ""
  "std,text_layout"
  "font_loading"
  "svg"
  "xml"
  "text_layout,font_loading,svg,xml"
  "std,text_layout,font_loading,text_layout_hyphenation"
  "std,text_layout,text_layout_hyphenation,font_loading,xml,pdf"
  "std,text_layout,text_layout_hyphenation,font_loading,xml,pdf,font_loading_multithreaded"
  "std,text_layout,text_layout_hyphenation,font_loading,xml,pdf,svg,cpurender"
)

failed=()
for set in "${SETS[@]}"; do
  echo "::group::azul-layout --no-default-features --features \"$set\""
  if [ -z "$set" ]; then
    cargo check -p azul-layout --no-default-features || failed+=("(none)")
  else
    cargo check -p azul-layout --no-default-features --features "$set" || failed+=("$set")
  fi
  echo "::endgroup::"
done

if [ ${#failed[@]} -ne 0 ]; then
  echo "azul-layout does not build with these feature sets:"
  printf '  --features "%s"\n' "${failed[@]}"
  exit 1
fi
echo "azul-layout builds with all ${#SETS[@]} feature sets."
