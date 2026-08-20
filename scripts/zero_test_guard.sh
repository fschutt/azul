#!/usr/bin/env bash
#
# scripts/zero_test_guard.sh — fail a CI test step whose test BINARY ran nothing.
#
# WHY
# ---
# `running 0 tests` is printed in the same green as `running 940 tests`, and
# `cargo test` exits 0 for it. Four independent mechanisms produce it, all of
# them silent:
#
#   1. A whole-file `#![cfg(feature = "…")]` whose feature is off. The target
#      still COMPILES (to an empty harness) and still reports success.
#      (layout/tests/test_hint_vs_freetype.rs: 36 tests, feature enabled
#      nowhere.)
#   2. Every `#[test]` in the file individually `#[cfg]`-gated off.
#      (css/tests/test_system_style.rs was exactly this, until it got a
#      `required-features` entry.)
#   3. A filter argument that matches nothing — a renamed module makes
#      `-- some::path` quietly select zero tests.
#   4. A `mod` that stopped being declared, taking its tests with it.
#
# CI had this check hand-rolled in three places and nowhere else. This is the
# one implementation; every test step pipes through `tee` and calls it.
#
# USAGE
#   set -o pipefail
#   cargo test … 2>&1 | tee out.txt
#   scripts/zero_test_guard.sh out.txt
#
#   scripts/zero_test_guard.sh --strict out.txt
#     ignore the allowlist entirely. Use this in any step that names its targets
#     explicitly (`--test foo`, `--lib`, `--bins`): there, an empty harness is
#     ALWAYS a bug, and an allowlist entry written for a different feature set
#     must not excuse it. Concretely: `tests/icu_parity.rs` is allowlisted
#     because it is legitimately empty in the DEFAULT feature set that
#     `test_lib` uses — but in the `icu_parity` job, which selects it by name on
#     `--features icu`, empty means the backend cfg broke, so that step runs
#     --strict.
#
# It parses cargo's own output, so it works for any invocation (`--lib`,
# `--tests`, `-p a -p b`, `--bins`, a named `--test`) and reports the exact
# binary that was empty.
#
# WHAT IT DELIBERATELY DOES NOT FLAG
#   * `Doc-tests <crate>` — a crate with no runnable doc examples legitimately has zero
#     doc tests, and that is a documentation question, not a cfg'd-out-tests one.
#   * a target the invocation never mentioned — see
#     scripts/workspace_test_coverage.sh for the "nobody runs this crate at all"
#     direction. Pick your `cargo test` targets precisely (`--lib`, `--bins`, a
#     named `--test`) so an empty harness here means something.
#
# ALLOWLIST: scripts/zero_test_targets.txt — one substring per line, `#`
# comments. A binary whose `Running …` line contains an allowlisted substring is
# permitted to be empty. That file is the honest, reviewable home for a target
# kept dead on purpose; adding to it is a decision someone signs off on in a
# diff, which is the whole difference from the status quo.
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ALLOWLIST="${ZERO_TEST_ALLOWLIST:-$REPO_ROOT/scripts/zero_test_targets.txt}"

STRICT=0
if [ "${1:-}" = "--strict" ]; then
  STRICT=1
  shift
fi

if [ $# -lt 1 ]; then
  echo "usage: $0 [--strict] <cargo-test-output.txt> [more.txt …]" >&2
  exit 2
fi

for log in "$@"; do
  if [ ! -f "$log" ]; then
    echo "zero-test-guard: FAIL — no such log file: $log" >&2
    echo "  (the step's \`cargo test … | tee $log\` did not run, or wrote elsewhere)" >&2
    exit 1
  fi
done

# awk does the pairing: remember the most recent `Running <target> (<binary>)`
# line, and when the following `running N tests` says N == 0, emit it.
empty="$(
  awk '
    # Windows runners tee through git-bash; strip a stray CR so the anchored
    # patterns below still match.
    { sub(/\r$/, "") }
    # `Doc-tests` is EXEMPT, not merely untracked: zero doc tests is a normal
    # state and says nothing about compiled-out #[test]s.
    /^[[:space:]]*Doc-tests / { target = ""; exempt = 1; next }
    /^[[:space:]]*Running / {
      target = $0
      sub(/^[[:space:]]+/, "", target)
      exempt = 0
      next
    }
    /^running 0 tests$/ {
      if (!exempt) print (target == "" ? "<unknown target>" : target)
      target = ""; exempt = 0
      next
    }
    /^running [0-9]+ tests?$/ { target = ""; exempt = 0 }
  ' "$@"
)"

# Nothing empty at all: also guard against a step that ran NO test binary.
if ! grep -qE '^running [0-9]+ tests?' "$@"; then
  echo "zero-test-guard: FAIL — the step produced no \`running N tests\` line at all." >&2
  echo "  Either every target was skipped for unmet required-features (cargo does" >&2
  echo "  that silently, exit 0), or the build never got as far as a test binary." >&2
  exit 1
fi

if [ -z "$empty" ]; then
  echo "zero-test-guard: OK — every test binary in $* ran at least one test."
  exit 0
fi

allowed_patterns=()
if [ "$STRICT" -eq 1 ]; then
  echo "zero-test-guard: --strict, the allowlist is not consulted."
elif [ -f "$ALLOWLIST" ]; then
  while IFS= read -r line; do
    line="${line%%#*}"
    # trim
    line="$(printf '%s' "$line" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
    [ -n "$line" ] && allowed_patterns+=("$line")
  done < "$ALLOWLIST"
fi

violations=0
while IFS= read -r target; do
  [ -z "$target" ] && continue
  ok=0
  for pat in ${allowed_patterns[@]+"${allowed_patterns[@]}"}; do
    case "$target" in
      *"$pat"*) ok=1; break ;;
    esac
  done
  if [ "$ok" -eq 1 ]; then
    echo "zero-test-guard: allowlisted empty target — $target"
  else
    echo "zero-test-guard: EMPTY TEST BINARY — $target" >&2
    violations=$((violations + 1))
  fi
done <<< "$empty"

if [ "$violations" -gt 0 ]; then
  cat >&2 <<EOF

zero-test-guard: FAIL — $violations test binary/binaries ran 0 tests.

A binary that runs 0 tests is not a pass; it is a target whose tests were
compiled out. Fix one of:

  * the target is feature-gated  -> give it a \`required-features\` entry in its
    Cargo.toml so cargo SKIPS it loudly instead of building an empty harness,
    and enable the feature in the job that should run it;
  * the filter matched nothing   -> the module was renamed; fix the filter;
  * the tests are gone           -> delete the target;
  * it is dead ON PURPOSE        -> add a substring to
    $ALLOWLIST
    together with the reason. That file is reviewed like any other diff.
    (Not available under --strict: a step that names its own targets has no
    business running an empty one.)
EOF
  exit 1
fi

echo "zero-test-guard: OK — the only empty binaries are allowlisted."
