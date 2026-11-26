#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): the iOS Simulator leg of the demos pipeline on one runner: the plain phone libazul and
# the iroh variant for the companion, every iOS app against its own, and the boot test of each in a simulator with the manifest's
# buttons and texts (scripts/ci/ios_smoke.sh, the one pass line).
#   bash scripts/ci/exp_ios_apps.sh
set -euxo pipefail
target=aarch64-apple-ios-sim; col=ios-sim
rustup target add "$target"
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan | tee demo-out/plan.txt
python3 - "$col" <<'PY' > demo-out/leg.env
import json, sys
col = sys.argv[1]
plan = {}
for line in open("demo-out/plan.txt"):
    key, _, value = line.rstrip("\n").partition("=")
    plan[key] = value
libs = [e for e in json.loads(plan["libs_ios"])["include"] if e["leg"] == col]
apps = next(e["apps"] for e in json.loads(plan["apps_ios"])["include"] if e["leg"] == col and e["variant"] == "")
for e in libs:
    print("LIB_%s='%s'" % (e["variant"].replace("-", "_"), e["features"]))
print("APPS='%s'" % apps)
PY
cat demo-out/leg.env
# shellcheck disable=SC1091
source demo-out/leg.env
start=$(date +%s)
bash scripts/demo_lib.sh ios "$target" "$LIB_mobile" "demo-out/libs/demolib-$col-mobile"
echo "LIB mobile SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
bash scripts/demo_lib.sh ios "$target" "$LIB_mobile_iroh" "demo-out/libs/demolib-$col-mobile-iroh"
echo "LIB mobile-iroh SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
python3 scripts/demo_build.py ios --target "$target" --apps "$APPS" --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant ""
echo "APP SECONDS: $(( $(date +%s) - start ))"
ls -la demo-out/apps
for f in demo-out/results/*.json; do python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:200])' "$f"; done
# idb, to press the button a manifest names (best effort)
( brew tap facebook/fb && brew install idb-companion && python3 -m venv "$RUNNER_TEMP/idb" && "$RUNNER_TEMP/idb/bin/pip" install fb-idb ) \
  && export PATH="$RUNNER_TEMP/idb/bin:$PATH" || echo "idb is not available"
export AZ_SMOKE_TAPS="$(grep '^smoke_taps=' demo-out/plan.txt | cut -d= -f2-)"
export AZ_SMOKE_EXPECTS="$(grep '^smoke_expects=' demo-out/plan.txt | cut -d= -f2-)"
bash scripts/ci/ios_smoke.sh demo-out/smoke 12 demo-out/apps/*-ios-sim.app.zip || echo "SMOKE FAILED"
for f in demo-out/smoke/*/result.json; do python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], "PASS" if r["pass"] else "FAIL", "-", r["reason"])' "$f"; done
