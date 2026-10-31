#!/bin/bash
# Merge one agent branch into the PR branch: `merge_one.sh <branch-suffix> "<message>"` (wt/<suffix>).
# Never rebase, never force. Append-only files keep both sides; anything else stops for a human.
# The other session's uncommitted layout/tests/all.rs lines must be taken out first
# (git apply -R $AZ_WORK/other_session_all_rs.patch) and put back after (git apply ...).
cd /Users/fschutt/Development/azul
T=$(dirname "$0"); L=${AZ_WORK:-$HOME/Development/azul-work}/logs; mkdir -p $L
b=$1; msg=$2
git merge --no-ff --no-edit -m "$msg

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>" wt/$b > $L/merge_$b.log 2>&1
if grep -q "Aborting\|fatal:" $L/merge_$b.log; then cat $L/merge_$b.log; echo "MERGE OF $b ABORTED (dirty tracked files? commit or take them out first)"; exit 1; fi
if [ -z "$(git diff --name-only --diff-filter=U)" ]; then tail -2 $L/merge_$b.log; echo "MERGED $b cleanly"; exit 0; fi
resolved=$(git diff --name-only --diff-filter=U)
for f in $resolved; do
  case "$f" in
    layout/tests/all.rs|Cargo.toml|scripts/workspace_test_members.txt|.github/workflows/rust.yml|scripts/dependency-justifications.toml|CHANGELOG.md|tests/wpt/reftest_expectations.txt)
      python3 $T/keep_both.py "$f" && git add "$f" ;;
    layout/src/widgets/themes/flat.rs|layout/src/widgets/themes/flora.rs)
      # both sides appended functions at the end: ours, then theirs (shared closing lines kept in both)
      python3 $T/resolve_tail_appends.py "$f" && git add "$f" ;;
    Cargo.lock)
      git checkout --ours Cargo.lock && git add Cargo.lock && echo "Cargo.lock: ours (the next build regenerates it)" ;;
    *) echo "REAL CONFLICT: $f" ;;
  esac
done
# keep_both / tail resolution can drop a shared closing line (`));`, `}`) - parse-check every resolved .rs file
bad=""
for f in $resolved; do
  case "$f" in *.rs) rustfmt --edition 2021 --check "$f" 2>&1 | grep -q '^error' && bad="$bad $f" ;; esac
done
left=$(git diff --name-only --diff-filter=U)
if [ -n "$left$bad" ]; then echo "STOPPED - resolve: $left; parse errors after resolving (fix, git add, git commit --no-edit):$bad"; exit 1; fi
git commit -q --no-edit && echo "MERGED $b (append conflicts resolved)"
