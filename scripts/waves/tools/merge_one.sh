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
for f in $(git diff --name-only --diff-filter=U); do
  case "$f" in
    layout/tests/all.rs|Cargo.toml|scripts/workspace_test_members.txt|.github/workflows/rust.yml|scripts/dependency-justifications.toml|CHANGELOG.md|tests/wpt/reftest_expectations.txt)
      python3 $T/keep_both.py "$f" && git add "$f" ;;
    layout/src/widgets/themes/flat.rs|layout/src/widgets/themes/flora.rs)
      echo "CHECK BY HAND (appended functions share a closing brace - keep_both.py --join-functions if both sides append): $f" ;;
    Cargo.lock)
      git checkout --ours Cargo.lock && git add Cargo.lock && echo "Cargo.lock: ours (the next build regenerates it)" ;;
    *) echo "REAL CONFLICT: $f" ;;
  esac
done
left=$(git diff --name-only --diff-filter=U)
if [ -n "$left" ]; then echo "STOPPED - resolve: $left"; exit 1; fi
git commit -q --no-edit && echo "MERGED $b (append conflicts resolved)"
