#!/usr/bin/env bash
# api.json through the autofix flow, as the CI's autofix_check job runs it (azul-doc built here in release): the result is the artifact
# out/api.json (the file after autofix, explain, patch safe, patch, normalize) and out/api.diff (what it changed against the commit).
set +e
mkdir -p out
cargo build -r -p azul-doc > out/build.log 2>&1
echo "BUILD_EXIT=$?"
tail -3 out/build.log
D=target/release/azul-doc
$D autofix > out/autofix.log 2>&1; echo "AUTOFIX_EXIT=$?"
$D autofix explain >> out/autofix.log 2>&1; echo "EXPLAIN_EXIT=$?"
$D patch safe target/autofix/patches >> out/autofix.log 2>&1; echo "PATCH_SAFE_EXIT=$?"
$D patch target/autofix/patches >> out/autofix.log 2>&1; echo "PATCH_EXIT=$?"
$D normalize >> out/autofix.log 2>&1; echo "NORMALIZE_EXIT=$?"
grep -E "Types to add|Type modifications|Path fixes|^  [+~] |Total changes|Failed|error" out/autofix.log | head -40
git diff --stat | tail -5
git diff api.json > out/api.diff
cp api.json out/api.json
echo "DIFF_LINES=$(wc -l < out/api.diff)"
head -80 out/api.diff
