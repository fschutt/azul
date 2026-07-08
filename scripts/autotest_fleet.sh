#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# autotest_fleet.sh — fan out a headless Claude fleet to WRITE Rust unit tests
# in parallel, WITHOUT compiling. You run + fix the red tests ONCE at the very
# end (single global pass), never per-agent. Runs unattended (bypass perms) so
# it can go overnight without stopping on a prompt.
#
# MODES (first positional arg)
#   (none) | full   Adversarial coverage over azul-doc's `autotest` task files
#                   (target/autotest/tasks/*.md — one per source file).
#                   Default model/effort: opus / xhigh.
#
#   fable-run       COVERAGE-AWARE gap fill (the Saturday Fable pass). Builds a
#                   fresh lcov over the CURRENT tree (so it sees what the first
#                   run already covered), keeps only files with uncovered lines,
#                   hands each agent the exact uncovered line numbers, and tells
#                   it to write tests reaching ONLY those — no overlap.
#                   Default: fable / xhigh.
#
#   css             CSS-SPEC CONFORMANCE. For every source file carrying
#                   `+spec:{prop}:{hash} - <desc>` comments, write, per tagged
#                   feature, (a) a test asserting the code implements the spec
#                   requirement and (b) a misuse test (invalid values / spec
#                   violations must be rejected or handled). This runs BEFORE
#                   Chrome reftesting so spec bugs surface without visual
#                   debugging. Combine with `--fable` to only fill uncovered
#                   spec features. Default: opus / xhigh.
#
# FLAGS (override the mode defaults)
#   --model <m>   opus|sonnet|haiku|fable      --effort <e>  low|medium|high|xhigh|max
#   --jobs <n>    parallel agents (default 6)  --lcov <p>    reuse an lcov.info
#   --fable       coverage-gap overlay for any mode (dedupe vs existing coverage)
#   --dry-run     print the work list, launch nothing
#
# THEN the ONE global verify + fix:
#   cargo test -p azul-core -p azul-css -p azul-layout --lib 2>&1 | tail -80
#   # fix or delete any red test (agents never compiled), then commit.
# ---------------------------------------------------------------------------
set -uo pipefail
cd "$(dirname "$0")/.."

MODE=full; MODEL=opus; EFFORT=xhigh; JOBS=6; LCOV=""; DRY=0; FABLE=0
case "${1:-}" in
  full)       shift;;
  fable-run)  MODE=full; FABLE=1; MODEL=fable; shift;;
  css)        MODE=css;  shift;;
  css-review) MODE=review; MODEL=fable; shift;;   # audit +spec: impls, WRITE A REPORT (no edits)
  --*|"")    ;;                       # no positional mode; flags follow
  *) echo "unknown mode: $1 (use: full | fable-run | css | css-review)"; exit 2;;
esac
while [ $# -gt 0 ]; do case "$1" in
  --model)  MODEL="$2"; shift 2;;
  --effort) EFFORT="$2"; shift 2;;
  --jobs)   JOBS="$2"; shift 2;;
  --lcov)   LCOV="$2"; shift 2;;
  --fable)  FABLE=1; MODEL=fable; shift;;
  --dry-run) DRY=1; shift;;
  *) echo "unknown arg: $1"; exit 2;;
esac; done

command -v claude >/dev/null || { echo "error: 'claude' CLI not on PATH"; exit 1; }
mkdir -p target/autotest

# --- Build the work list: SRC_FILES[] (the .rs files to hand out, 1 agent each).
declare -a SRC_FILES
if [ "$MODE" = css ] || [ "$MODE" = review ]; then
  mapfile -t SRC_FILES < <(grep -rlE '\+spec:' css/src layout/src core/src 2>/dev/null | sort -u)
  echo "[fleet] $MODE mode: ${#SRC_FILES[@]} files carry +spec: comments"
else
  echo "[fleet] regenerating autotest task files ..."
  cargo run -r -p azul-doc -- autotest >/dev/null || { echo "autotest failed"; exit 1; }
  mapfile -t SRC_FILES < <(for tf in target/autotest/tasks/*.md; do
      grep -m1 -oE '(core|css|layout)/src/[A-Za-z0-9_/]+\.rs' "$tf"; done | sort -u)
fi
[ "${#SRC_FILES[@]}" -gt 0 ] || { echo "empty work list"; exit 1; }

# --- Coverage overlay (fable / --fable): keep only files with uncovered lines,
#     and record those line numbers per file for the prompt.
: > target/autotest/.uncovered.env
if [ "$FABLE" = 1 ]; then
  if [ -z "$LCOV" ]; then
    echo "[fable] building coverage (grcov -> lcov) over the current tree ..."
    command -v grcov >/dev/null || cargo install grcov
    rustup component add llvm-tools-preview 2>/dev/null || true
    rm -rf target/cov-prof && mkdir -p target/cov-prof
    RUSTFLAGS="-Cinstrument-coverage" LLVM_PROFILE_FILE="target/cov-prof/az-%p-%m.profraw" \
      cargo test -p azul-core -p azul-css -p azul-layout --lib >/dev/null 2>&1 || true
    LCOV=target/cov-prof/lcov.info
    grcov target/cov-prof -s . -t lcov --branch --ignore-not-existing -o "$LCOV" \
      --ignore "target/*" --ignore "*/tests/*" >/dev/null 2>&1 || true
  fi
  [ -s "$LCOV" ] || { echo "no usable lcov at '$LCOV'"; exit 1; }
  KEEP=()
  for src in "${SRC_FILES[@]}"; do
    lines="$(awk -v f="$src" '/^SF:/{inf=(index($0,f)>0)} inf&&/^DA:/{split(substr($0,4),a,",");if(a[2]==0)printf"%s,",a[1]}' "$LCOV")"
    if [ -n "$lines" ]; then
      KEEP+=("$src"); printf 'UNC_%s=%q\n' "$(echo "$src"|tr '/.' '__')" "${lines%,}" >> target/autotest/.uncovered.env
    fi
  done
  SRC_FILES=("${KEEP[@]}")
  echo "[fable] ${#SRC_FILES[@]} files still have uncovered lines"
fi

echo "[fleet] mode=$MODE fable=$FABLE model=$MODEL effort=$EFFORT jobs=$JOBS files=${#SRC_FILES[@]}"

run_one() {
  local src="$1" key; key="UNC_$(echo "$src"|tr '/.' '__')"
  source target/autotest/.uncovered.env 2>/dev/null || true
  local unc="${!key:-}"

  # --- css-review: audit spec conformance, capture a markdown report (no edits).
  if [ "$MODE" = review ]; then
    mkdir -p target/autotest/spec-review
    local frag="target/autotest/spec-review/$(echo "$src"|tr '/.' '__').md"
    local rprompt="Read '$src' and its '+spec:{prop}:{hash} - <desc>' comments. \
For EACH, audit whether the code CORRECTLY implements that CSS-spec requirement \
(run 'cargo run -rq -p azul-doc -- spec show <prop>:<hash>' for the full \
paragraph). OUTPUT GitHub-flavored markdown only: a '## $src' heading, then one \
bullet per +spec id — 'STATUS(CORRECT|PARTIAL|INCORRECT|MISSING) — <spec id> — \
<finding + the exact spec deviation if any, with file:line>'. Be concrete about \
where the impl diverges. Do NOT edit any file; print the report."
    if [ "${DRY:-0}" = 1 ]; then echo "[dry-review] $src"; return 0; fi
    if claude -p "$rprompt" --model "$MODEL" --effort "$EFFORT" \
          --permission-mode bypassPermissions --output-format text > "$frag" 2>/dev/null; then
      echo "[ok]   $src"; else echo "[fail] $src"; fi
    return 0
  fi

  local prompt
  if [ "$MODE" = css ]; then
    prompt="You are testing CSS-spec conformance for the azul layout/style engine. \
Read '$src'. It contains '+spec:{prop}:{hash} - <description>' comments, each \
marking where a CSS-spec requirement is implemented (the description says what \
the spec requires; run 'cargo run -rq -p azul-doc -- spec show <prop>:<hash>' if \
you want the full paragraph). For EACH +spec comment, APPEND to an inline \
'#[cfg(test)] mod spec_conformance' in '$src': (1) a test asserting the code \
implements that requirement (parse the property, check the computed/used value \
matches the spec), and (2) a MISUSE test (invalid values, out-of-range, \
conflicting declarations, spec violations) asserting the impl rejects/handles \
them per spec. Goal: catch 'spec implemented wrong' BEFORE Chrome reftesting."
  else
    prompt="You are writing adversarial Rust unit tests for azul. Read the \
autotest task file 'target/autotest/tasks/' entry for '$src' (functions + \
per-category strategies) and read '$src'. APPEND #[test]s to an inline \
'#[cfg(test)] mod autotest_generated' in '$src': parsers -> \
malformed/huge/boundary/unicode; numeric -> overflow/NaN/saturation/limits; \
round-trip -> encode==decode; getters/predicates -> invariants."
  fi
  if [ -n "$unc" ]; then
    prompt="$prompt  COVERAGE-GAP: '$src' already has tests — write tests ONLY \
for currently-uncovered code and do NOT duplicate existing tests. Uncovered \
source lines: $unc."
  fi
  prompt="$prompt  HARD RULES: do NOT run cargo (a single global test pass runs \
later); edit ONLY '$src'; touch ONLY test code, never non-test code; skip a \
test rather than guess at an un-constructible type."

  if [ "${DRY:-0}" = 1 ]; then echo "[dry] $src ${unc:+(gap)}"; return 0; fi
  if claude -p "$prompt" --model "$MODEL" --effort "$EFFORT" \
        --permission-mode bypassPermissions >/dev/null 2>&1; then
    echo "[ok]   $src"; else echo "[fail] $src"; fi
}
export -f run_one; export MODE MODEL EFFORT DRY

printf '%s\n' "${SRC_FILES[@]}" | xargs -P "$JOBS" -I{} bash -c 'run_one "$@"' _ {}

echo
if [ "$MODE" = review ]; then
  REPORT="scripts/SPEC_CONFORMANCE_REVIEW.md"
  {
    echo "# CSS-spec conformance review (+spec: comments)"
    echo ""
    echo "Generated by \`autotest_fleet.sh css-review\` (model=$MODEL). Read INCORRECT/"
    echo "PARTIAL/MISSING items first — those are spec bugs to fix before Chrome reftests."
    echo ""
    cat target/autotest/spec-review/*.md 2>/dev/null
  } > "$REPORT"
  echo "[fleet] spec-review report -> $REPORT"
  echo "  grep -E 'INCORRECT|PARTIAL|MISSING' $REPORT   # the spec bugs"
else
  echo "[fleet] done. Single global verify + fix:"
  echo "  cargo test -p azul-core -p azul-css -p azul-layout --lib 2>&1 | tail -80"
fi
