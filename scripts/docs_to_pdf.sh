#!/usr/bin/env bash
# Render the entire azul guide (including the internal docs) to a single PDF
# for offline / paper review.
#
# Pipeline: azul-doc generates the HTML site (with the @media print CSS from
# docgen/guide.rs) -> a local HTTP server serves it so relative asset URLs
# resolve -> headless Chrome prints each guide page to a PDF -> pdfunite merges
# them into target/azul-documentation.pdf.
#
# Usage:  ./scripts/docs_to_pdf.sh [--no-open] [--port N]
# Output: <repo>/target/azul-documentation.pdf
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEPLOY="$ROOT/doc/target/deploy"
OUT="$ROOT/target/azul-documentation.pdf"
PORT=8799
OPEN=1
SKIP_DEPLOY=0
for a in "$@"; do
  case "$a" in
    --no-open) OPEN=0 ;;
    --skip-deploy) SKIP_DEPLOY=1 ;;   # reuse the existing doc/target/deploy
    --port) shift; PORT="${1:-8799}" ;;
    --port=*) PORT="${a#*=}" ;;
  esac
done

# --- locate Chrome ---------------------------------------------------------
CHROME=""
for c in \
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  "/Applications/Chromium.app/Contents/MacOS/Chromium" \
  "$(command -v google-chrome || true)" \
  "$(command -v chromium || true)" \
  "$(command -v chromium-browser || true)"; do
  if [ -n "$c" ] && [ -x "$c" ]; then CHROME="$c"; break; fi
done
[ -z "$CHROME" ] && { echo "ERROR: Google Chrome / Chromium not found." >&2; exit 1; }
command -v pdfunite >/dev/null || { echo "ERROR: pdfunite not found (brew install poppler)." >&2; exit 1; }

# --- 1. generate the HTML site (debug = root-relative URLs for local serving) ---
if [ "$SKIP_DEPLOY" -eq 1 ] && [ -d "$DEPLOY/guide" ]; then
  echo "==> Reusing existing deploy at $DEPLOY (--skip-deploy)"
else
  echo "==> Generating docs (azul-doc deploy debug)..."
  ( cd "$ROOT" && cargo run -r -p azul-doc deploy debug )
fi
[ -d "$DEPLOY/guide" ] || { echo "ERROR: no guide pages at $DEPLOY/guide" >&2; exit 1; }

# --- 2. serve the deploy dir so /images, /main.css, etc. resolve -----------
echo "==> Serving $DEPLOY on :$PORT ..."
python3 -m http.server "$PORT" --directory "$DEPLOY" >/dev/null 2>&1 &
SERVER_PID=$!
cleanup() { kill "$SERVER_PID" 2>/dev/null || true; }
trap cleanup EXIT
# wait for the server to accept connections
for _ in $(seq 1 50); do
  curl -fsS "http://localhost:$PORT/" >/dev/null 2>&1 && break
  sleep 0.2
done

# --- 3. ordered page list: top-level guides first, then nested (internals/…) ---
TMP="$(mktemp -d)"
# Top-level guides first, then nested (internals/…). Portable to bash 3.2
# (macOS default) — no mapfile.
PAGES=()
while IFS= read -r line; do [ -n "$line" ] && PAGES+=("$line"); done < <( { ls "$DEPLOY"/guide/*.html 2>/dev/null; find "$DEPLOY/guide" -mindepth 2 -name '*.html' | sort; } )
echo "==> Rendering ${#PAGES[@]} guide pages to PDF via headless Chrome..."

# Render in parallel (each Chrome needs its own --user-data-dir). The doc pages
# are static (server-side markdown + syntect highlight), so a short virtual-time
# budget is plenty — it only needs to cover web-font load.
render_one() {
  "$CHROME" --headless=new --disable-gpu --no-sandbox \
    --no-pdf-header-footer \
    --virtual-time-budget=3000 \
    --run-all-compositor-stages-before-draw \
    --user-data-dir="$(mktemp -d)" \
    --print-to-pdf="$1" "$2" >/dev/null 2>&1 || true
}
MAXJOBS="${MAXJOBS:-6}"
i=0
PDFS=()
for page in "${PAGES[@]}"; do
  rel="${page#$DEPLOY/}"               # e.g. guide/internals/dom.html
  out="$(printf '%s/%04d.pdf' "$TMP" "$i")"
  PDFS+=("$out")                        # keep page order for the merge
  render_one "$out" "http://localhost:$PORT/$rel" &
  printf '  [%3d/%d] %s\n' "$((i+1))" "${#PAGES[@]}" "$rel"
  i=$((i+1))
  while [ "$(jobs -rp | wc -l)" -ge "$MAXJOBS" ]; do sleep 0.1; done
done
wait
# keep only pages that actually produced a non-empty PDF, in order
_kept=()
for p in "${PDFS[@]}"; do [ -s "$p" ] && _kept+=("$p"); done
PDFS=("${_kept[@]}")

# --- 4. merge -------------------------------------------------------------
[ "${#PDFS[@]}" -gt 0 ] || { echo "ERROR: no pages rendered." >&2; exit 1; }
mkdir -p "$ROOT/target"
echo "==> Merging ${#PDFS[@]} pages -> $OUT"
pdfunite "${PDFS[@]}" "$OUT"
rm -rf "$TMP" "$PROFILE"

echo "==> Done: $OUT ($(du -h "$OUT" | cut -f1))"
[ "$OPEN" -eq 1 ] && command -v open >/dev/null && open "$OUT" || true
