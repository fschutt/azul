#!/usr/bin/env bash
#
# scripts/workspace_test_coverage.sh — no workspace member may join untested.
#
# WHY
# ---
# `cargo test -p a -p b -p c` names its packages. Add a member to `[workspace]
# members` — or a path dependency of one, which cargo makes a member implicitly,
# and that is how the four `webrender*` crates got in — and nothing anywhere
# notices that its tests run in no job. On 2026-08-20 that was 115 real tests
# running nowhere: azul-paint 3, azul-writer 16, webrender 96. (webrender also
# needed an unused `mozangle` dev-dependency removed before its tests could be
# built at all: it is an ANGLE C++ build, referenced from no Rust source here.)
#
# This script makes membership a decision: every member must appear in exactly
# one of the two lists in scripts/workspace_test_members.txt — `tested:` (some
# CI job runs its tests) or `no-tests:` (it genuinely has none, and the script
# VERIFIES that claim by counting `#[test]` in its sources). A new member in
# neither list fails the build.
#
# USAGE: scripts/workspace_test_coverage.sh
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
LIST="scripts/workspace_test_members.txt"

members="$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; [print(p["name"], p["manifest_path"]) for p in json.load(sys.stdin)["packages"]]' \
  | sort)"

tested="$(sed -n 's/^tested:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$LIST" | sort -u)"
notests="$(sed -n 's/^no-tests:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$LIST" | sort -u)"

fail=0

while read -r name manifest; do
  [ -z "$name" ] && continue
  in_tested=0; in_notests=0
  if grep -qx -- "$name" <<< "$tested";  then in_tested=1;  fi
  if grep -qx -- "$name" <<< "$notests"; then in_notests=1; fi

  if [ "$in_tested" -eq 1 ] && [ "$in_notests" -eq 1 ]; then
    echo "workspace-test-coverage: '$name' is in BOTH lists in $LIST" >&2
    fail=1
    continue
  fi

  if [ "$in_tested" -eq 0 ] && [ "$in_notests" -eq 0 ]; then
    echo "workspace-test-coverage: NEW UNTESTED MEMBER — '$name' ($manifest)" >&2
    echo "  It is in no CI test job and in neither list of $LIST." >&2
    fail=1
    continue
  fi

  if [ "$in_notests" -eq 1 ]; then
    dir="$(dirname "$manifest")"
    n="$(grep -rl --include='*.rs' -e '#\[test\]' -e '#\[tokio::test\]' "$dir" 2>/dev/null | wc -l | tr -d ' ' || true)"
    if [ "$n" -ne 0 ]; then
      echo "workspace-test-coverage: '$name' is listed 'no-tests:' but $n file(s) under" >&2
      echo "  $dir contain #[test]. Move it to 'tested:' and add it to a CI job." >&2
      fail=1
    fi
  fi
done <<< "$members"

# The reverse direction: a list entry for a package that no longer exists is
# rot, and would hide the next real gap behind a stale name.
for name in $tested $notests; do
  if ! grep -qE "^$name " <<< "$members"; then
    echo "workspace-test-coverage: '$name' is listed in $LIST but is not a workspace member" >&2
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  cat >&2 <<EOF

workspace-test-coverage: FAIL.

Every workspace member must be accounted for in $LIST:
  tested:   <name>   # <which CI job runs it>
  no-tests: <name>   # <why it has none> — the script verifies the claim
EOF
  exit 1
fi

echo "workspace-test-coverage: OK — all $(wc -l <<< "$members" | tr -d ' ') workspace members accounted for."
