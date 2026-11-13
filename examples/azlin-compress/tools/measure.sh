#!/bin/sh
# The measurement table of azlin-compress: examples/bench.rs over each set of the fixed corpus
# (tools/corpus.tsv, fetched by tools/fetch_corpus.py), one process per set so that the peak
# memory is the set's. Every file is decoded and compared; a mismatch stops the run.
#
#   cargo build --release -p azlin-compress --example bench      # first (under the build lock)
#   sh tools/measure.sh <corpus folder> [upload|idle] [threads] > table.md
#
# Prints the README's table: set, files, MB, today's AZL1 saving, this crate's saving, per-file
# p10 / median / p90, encode and decode MB/s, files transformed, peak memory.
set -eu
CORPUS=${1:?usage: measure.sh <corpus folder> [upload|idle] [threads]}
MODE=${2:-idle}
THREADS=${3:-4}
HERE=$(cd "$(dirname "$0")" && pwd)
BENCH=${BENCH:-$(cd "$HERE/../../.." && pwd)/target/release/examples/bench}
[ -x "$BENCH" ] || { echo "no bench binary at $BENCH (build it first, or set BENCH)" >&2; exit 2; }
TIME_OUT=$(mktemp)
trap 'rm -f "$TIME_OUT"' EXIT

peak_mib() {
  # macOS: "<bytes>  maximum resident set size"; Linux (GNU time -v): "Maximum resident set size (kbytes): <n>".
  awk '/maximum resident set size/ {print int($1 / 1048576)} /Maximum resident set size/ {print int($NF / 1024)}' "$TIME_OUT"
}

run() {
  label=$1
  shift
  if [ "$(uname)" = Darwin ]; then
    row=$(/usr/bin/time -l "$BENCH" "$MODE" "$THREADS" "$label" "$@" 2>"$TIME_OUT")
  else
    row=$(/usr/bin/time -v "$BENCH" "$MODE" "$THREADS" "$label" "$@" 2>"$TIME_OUT")
  fi
  echo "$row $(peak_mib) MiB |"
}

echo "| set | files | MB | AZL1 today | azlin-compress | per file p10 / median / p90 | encode MB/s | decode MB/s | transformed | peak |"
echo "|---|---|---|---|---|---|---|---|---|---|"
S=$CORPUS/silesia
run "Silesia text" "$S/dickens" "$S/nci" "$S/reymont" "$S/samba" "$S/webster" "$S/xml"
run "Silesia binary" "$S/mozilla" "$S/mr" "$S/ooffice" "$S/osdb" "$S/sao" "$S/x-ray"
run "JPEG (Galaxy S10)" "$CORPUS"/jpeg/*.jpg
run "PDF (GOV.UK)" "$CORPUS"/docs/pdf-*.pdf
run "DOCX (GOV.UK)" "$CORPUS"/docs/docx-*.docx
run "XLSX (GOV.UK)" "$CORPUS"/docs/xlsx-*.xlsx
run "PPTX (GOV.UK)" "$CORPUS"/docs/pptx-*.pptx
run "All" "$S"/* "$CORPUS"/jpeg/*.jpg "$CORPUS"/docs/*.pdf "$CORPUS"/docs/*.docx "$CORPUS"/docs/*.xlsx "$CORPUS"/docs/*.pptx
