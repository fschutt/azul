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
command -v node >/dev/null || { echo "ERROR: node not found (drives Chrome over CDP; Node 18+)." >&2; exit 1; }

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

# --- 3. plan the three-book structure -------------------------------------
# The planner buckets pages into the three trees the website uses (Getting
# Started / Advanced / Contributor — see guide.rs classify_tree), orders each,
# writes a cover + per-book title+index page into <deploy>/_pdf/, and prints
# the final ordered URL list (cover, book-1 index, book-1 pages, book-2 …).
TMP="$(mktemp -d)"
URLS=()
while IFS= read -r u; do [ -n "$u" ] && URLS+=("$u"); done < <(
  node "$ROOT/scripts/docs_pdf_book.mjs" "$DEPLOY" "http://localhost:$PORT" "$ROOT/doc/guide/en"
)
[ "${#URLS[@]}" -gt 0 ] || { echo "ERROR: planner produced no pages." >&2; exit 1; }
echo "==> Rendering ${#URLS[@]} pages (cover + 3 books) via one persistent Chrome (CDP)..."

# ONE persistent headless Chrome over CDP (like reftest's ChromeCdp) — per-page
# `--print-to-pdf` hangs on Chrome 148 headless=new. The driver writes
# <TMP>/NNNN.pdf in URL order.
node "$ROOT/scripts/docs_pdf_cdp.mjs" "$CHROME" "$TMP" "${URLS[@]}"

# collect produced PDFs in order
PDFS=()
for ((i = 0; i < ${#URLS[@]}; i++)); do
  f="$(printf '%s/%04d.pdf' "$TMP" "$i")"
  [ -s "$f" ] && PDFS+=("$f")
done

# --- 4. merge -------------------------------------------------------------
[ "${#PDFS[@]}" -gt 0 ] || { echo "ERROR: no pages rendered." >&2; exit 1; }
mkdir -p "$ROOT/target"
echo "==> Merging ${#PDFS[@]} pages -> $OUT"
pdfunite "${PDFS[@]}" "$OUT"
rm -rf "$TMP"

echo "==> Done: $OUT ($(du -h "$OUT" | cut -f1))"
[ "$OPEN" -eq 1 ] && command -v open >/dev/null && open "$OUT" || true
