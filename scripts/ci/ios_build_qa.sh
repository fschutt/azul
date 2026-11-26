#!/usr/bin/env bash
# ios_build_qa.sh: build the iOS Simulator app of ONE app from the `mobile` library and from the `mobile-e2e` library, and run the
# QA on each, in ONE job on ONE macOS runner (one macOS slot): library, app, boot test and lifecycle QA, for either variant or both.
#
#   bash scripts/ci/ios_build_qa.sh [mobile | mobile-e2e | both] [<App>]        (default: both AzWidgets)
#
# For each variant, in this order (the two libraries share one cargo target directory, so the second one only builds what differs):
#   1. the library for aarch64-apple-ios-sim with the features the plan names for it (python3 scripts/demo_apps.py features ios-sim
#      <variant>: `mobile` is what ships; `mobile-e2e` adds debug-server, e2e-scripting and test-hooks, an ARTIFACT, never shipped),
#   2. the .app of <App> linked against it (scripts/demo_build.py; the QA build is named <App>-e2e-ios-sim.app.zip),
#   3. the boot test (scripts/ci/ios_smoke.sh: the one pass line, information here) and the QA of scripts/ci/ios_qa.sh (session qa for
#      `mobile`, session both = qa then e2e for `mobile-e2e`; AZ_QA_SCENARIOS names the scenario files of the e2e session).
# Everything lands in demo-out/: libs/*/libazul.info.json, apps/*.app.zip, results/*.json, smoke/, qa-<variant>/.
#
# On a runner, from any ref that has this file (master and what merged it):
#   scripts/remote.sh run <ref> macos-15 'bash scripts/ci/ios_build_qa.sh both AzWidgets' --codegen --artifacts 'demo-out/results/*
#   demo-out/libs/*/libazul.info.json
#   demo-out/apps/*
#   demo-out/smoke/**
#   demo-out/qa-*/**'
# The library builds are ~15 minutes each cold (the QA variant after the plain one less), the QA about 15 minutes each.
set -euo pipefail

which="${1:-both}"
app="${2:-AzWidgets}"
target=aarch64-apple-ios-sim
col=ios-sim
case "$which" in
  both) variants="mobile mobile-e2e" ;;
  mobile | mobile-e2e) variants="$which" ;;
  *) echo "usage: ios_build_qa.sh [mobile | mobile-e2e | both] [<App>]" >&2; exit 2 ;;
esac

mkdir -p demo-out/libs demo-out/apps demo-out/results
rustup target add "$target"
python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt

for variant in $variants; do
  features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
  echo "== $variant: $features"
  start=$(date +%s)
  bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-$col-$variant"
  echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"

  flag=""
  zip="demo-out/apps/$app-$col.app.zip"
  session=qa
  case "$variant" in
    mobile-e2e) flag=mobile-e2e; zip="demo-out/apps/$app-e2e-$col.app.zip"; session=both ;;
  esac
  start=$(date +%s)
  python3 scripts/demo_build.py ios --target "$target" --apps "$app" --libs demo-out/libs --out demo-out/apps \
    --results demo-out/results --variant "$flag"
  echo "APP $variant SECONDS: $(( $(date +%s) - start ))"
  ls -la "$zip"

  # the boot test (information) and the lifecycle QA; a failing app is a result of this job, not a failure of it
  bash scripts/ci/ios_smoke.sh "demo-out/smoke/$variant" 12 "$zip" || echo "SMOKE $variant: exit $?"
  bash scripts/ci/ios_qa.sh "demo-out/qa-$variant" "$zip" "$app" "$session" || echo "QA $variant: exit $?"
done

for f in demo-out/results/*.json; do
  python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["column"], r.get("variant", ""), r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:300])' "$f"
done
