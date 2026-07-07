#!/usr/bin/env bash
# coretext_regression.sh — drive the CoreText-vs-azul hinting autoregression harness.
#
# Make executable once:  chmod +x scripts/coretext_regression.sh
# Run:                    scripts/coretext_regression.sh
# With overrides:         AZ_CT_PPEMS=10,12,16 AZ_CT_CHARS=agm AZ_CT_WORDS=minimum scripts/coretext_regression.sh
# Regression gate (fail): AZ_CT_MAX_RMS=40 scripts/coretext_regression.sh
# Any trailing args are forwarded to the test binary, e.g. `... -- --test-threads=1`.
#
# ============================================================================
# THE AUTOREGRESSION LOOP (protocol)
# ============================================================================
#   1. Run this script. It renders every (ppem,char) and (ppem,word) case BOTH
#      through our hinted CPU rasterizer (agg + build_hinted_path) AND through
#      macOS CoreText (same font file, face 0, identical outlines), writing:
#        - <out>/NNpx_<case>_<bucket>_rmsX.png   (4 panels, 16x, pixel grid)
#        - <out>/metrics.jsonl                    (one ranked JSON per case)
#        - <out>/SUMMARY.md                       (worst-first table + histogram)
#   2. Open the WORST PNGs (top of SUMMARY.md). Each PNG is 4 panels:
#        [ ours-hinted (green) | coretext (blue) | diff-heatmap (magenta) | ours-unhinted (gray) ]
#      In the diff panel: RED = we put down MORE ink than CoreText (over-ink),
#      BLUE = LESS ink (under-ink), GRAY = agreement.
#   3. Classify the dominant divergence (map below) -> edit the suspect code.
#   4. Re-run. `metrics.prev.jsonl` is diffed against the new run and per-case
#      REGRESSED/improved deltas are printed. Iterate until MATCH/CLOSE dominate.
#   5. To lock in progress, set AZ_CT_MAX_RMS to just above the current worst so
#      CI fails if any case regresses past it.
#
# ============================================================================
# DIVERGENCE CLASS  ->  SUSPECT CODE  (keep in sync with the .rs module doc)
# ============================================================================
#   over-ink EVERYWHERE (whole glyph redder/heavier than CoreText)
#       -> gamma / coverage in  layout/src/cpurender/raster.rs
#          (agg fills linear coverage; CoreText applies a text gamma even with
#           font-smoothing OFF. A global coverage/gamma curve lives here.)
#
#   1px VERTICAL SHIFT (whole glyph one row up/down; rms_aligned << rms_raw)
#       -> phantom-point / baseline rounding in  layout/src/glyph_cache.rs
#          (build_hinted_path advance/phantom handling) OR the Y-flip in
#          build_path_from_contours (font Y-up -> screen Y-down negation).
#
#   STEMS 1px TOO FAR at small ppem (vertical strokes off by a column; big
#   max_col_diff, worse as ppem drops)
#       -> CVT cut-in / rounding state in
#          third_party/allsorts/src/hinting/interpreter.rs
#          (SROUND / cut-in / RTG round-state applied to control-value stems).
#
#   IDENTICAL TO UNHINTED (hinted panel == unhinted panel; metrics all_hinted=false)
#       -> hinting is NOT running. Check the `gasp` table gate, ParsedFont
#          `hint_instance` being Some, and build_hinted_path returning None
#          (missing raw_points / instructions / set_ppem failure).
#
# No network. Output stays under target/ (gitignored).
# ============================================================================

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Output dir: default under target/ (already gitignored). Exported so the test
# and this script agree on the location.
export AZ_CT_OUT="${AZ_CT_OUT:-target/coretext_autoregression}"
mkdir -p "$AZ_CT_OUT"

PREV="$AZ_CT_OUT/metrics.prev.jsonl"
NOW="$AZ_CT_OUT/metrics.jsonl"

# Rotate the previous run's metrics BEFORE the test overwrites metrics.jsonl.
if [ -f "$NOW" ]; then
  cp "$NOW" "$PREV"
fi

echo "[coretext_regression] out dir: $AZ_CT_OUT"
echo "[coretext_regression] AZ_CT_FONT=${AZ_CT_FONT:-<auto>} AZ_CT_PPEMS=${AZ_CT_PPEMS:-<default>} AZ_CT_MAX_RMS=${AZ_CT_MAX_RMS:-<none>}"

# --release: hinting/raster at optimized speed for the full matrix. --nocapture:
# stream the top-10 worst to the terminal. AZ_CT_* env is inherited by the test.
set +e
cargo test -p azul-layout --features coretext_tests \
  --test coretext_autoregression --release -- --nocapture "$@"
STATUS=$?
set -e

echo "[coretext_regression] out dir: $AZ_CT_OUT   (SUMMARY.md, metrics.jsonl, *.png)"

# Prev-vs-now per-case delta report (inner join on case@ppem).
if [ -f "$PREV" ] && [ -f "$NOW" ]; then
  echo ""
  echo "[coretext_regression] prev-vs-now (rms_raw) deltas:"

  extract() {
    awk '
    {
      cv=""; rv=""; pp="";
      if (match($0, /"case":"[^"]*"/)) cv=substr($0, RSTART+8, RLENGTH-9);
      if (match($0, /"ppem":[0-9]+/))  pp=substr($0, RSTART+7, RLENGTH-7);
      if (match($0, /"rms_raw":[0-9.]+/)) rv=substr($0, RSTART+10, RLENGTH-10);
      if (cv != "" && rv != "") print cv "@" pp "\t" rv;
    }' "$1" | sort
  }

  join -t "$(printf '\t')" <(extract "$PREV") <(extract "$NOW") | awk -F'\t' '
  {
    prev=$2+0; now=$3+0; d=now-prev;
    tag = (now<2 ? "MATCH" : (now<8 ? "CLOSE" : "DIVERGENT"));
    if (d > 0.5)      { reg++; if (reg<=25) printf "  REGRESSED %-24s %8.2f -> %8.2f  (%+7.2f)  [%s]\n", $1, prev, now, d, tag; }
    else if (d < -0.5){ imp++; if (imp<=25) printf "  improved  %-24s %8.2f -> %8.2f  (%+7.2f)  [%s]\n", $1, prev, now, d, tag; }
    else same++;
  }
  END { printf "  --- %d regressed, %d improved, %d unchanged (|Δ|<=0.5) ---\n", reg+0, imp+0, same+0; }'
else
  echo "[coretext_regression] (no previous metrics.jsonl — baseline run, nothing to diff)"
fi

exit $STATUS
