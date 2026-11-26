#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): the Apple legs of the demos pipeline on one macOS runner, as the evidence that ld still
# links after the ABI guard of the generated binding stopped naming az_abi_check_slow on Apple targets (doc/src/codegen/v2/abi_guard.rs).
#   bash scripts/ci/exp_apple_link.sh sim      the iOS Simulator leg: the plain phone lib and the iroh lib for aarch64-apple-ios-sim, the six
#                                              apps, `xcrun dyld_info -inits` of two of them, and the boot test of each (information)
#   bash scripts/ci/exp_apple_link.sh evidence   the formal link evidence, ONE run: AzWidgets linked for aarch64-apple-ios-sim, for
#                                              aarch64-apple-ios (unsigned device bundle) and for macOS arm64 (the shipped desktop lib, dist),
#                                              each with `xcrun dyld_info -inits` and the init sections of its binary
# Everything lands in demo-out/ (apps, results, dyld-inits-*.txt, otool-inits-*.txt, link-*.txt).
set -euxo pipefail
mode="${1:?evidence | sim}"
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan | tee demo-out/plan.txt

lib_features() {   # <leg> <variant>
  python3 - "$1" "$2" <<'PY'
import json, sys
leg, variant = sys.argv[1:]
plan = {}
for line in open("demo-out/plan.txt"):
    key, _, value = line.rstrip("\n").partition("=")
    plan[key] = value
for e in json.loads(plan["libs_ios"])["include"]:
    if e["leg"] == leg and e["variant"] == variant:
        print(e["features"])
PY
}

inits() {   # <label> <mach-o file>
  { echo "== xcrun dyld_info -inits $2"; xcrun dyld_info -inits "$2" 2>&1 || true; } | tee "demo-out/dyld-inits-$1.txt" | head -n 40
  echo "az_abi_check lines in the initializers of $1: $(grep -c az_abi_check "demo-out/dyld-inits-$1.txt" || true)"
  { echo "== otool -l (init sections) $2"; otool -l "$2" 2>&1 | grep -E "sectname __(mod_init_func|init_offsets)" || echo "no __mod_init_func and no __init_offsets section"; } | tee "demo-out/otool-inits-$1.txt"
}

case "$mode" in
  sim)
    target=aarch64-apple-ios-sim; col=ios-sim
    rustup target add "$target"
    for variant in mobile mobile-iroh; do
      start=$(date +%s)
      bash scripts/demo_lib.sh ios "$target" "$(lib_features "$col" "$variant")" "demo-out/libs/demolib-$col-$variant"
      echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"
    done
    start=$(date +%s)
    apps="$(python3 -c 'import json,sys; p=dict(l.rstrip("\n").partition("=")[::2] for l in open("demo-out/plan.txt")); print(next(e["apps"] for e in json.loads(p["apps_ios"])["include"] if e["leg"]=="ios-sim" and e["variant"]==""))')"
    python3 scripts/demo_build.py ios --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" | tee demo-out/link-ios-sim.txt
    echo "APP SECONDS: $(( $(date +%s) - start ))"
    ls -la demo-out/apps
    for f in demo-out/results/*.json; do python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:300])' "$f"; done
    for app in AzWidgets AzCompanion; do
      [ -f "target/ios-bundle/$target/$app.app/$app" ] && inits "ios-sim-$app" "target/ios-bundle/$target/$app.app/$app"
    done
    ( brew tap facebook/fb && brew install idb-companion && python3 -m venv "$RUNNER_TEMP/idb" && "$RUNNER_TEMP/idb/bin/pip" install fb-idb ) \
      && export PATH="$RUNNER_TEMP/idb/bin:$PATH" || echo "idb is not available"
    AZ_SMOKE_TAPS="$(grep '^smoke_taps=' demo-out/plan.txt | cut -d= -f2-)"
    AZ_SMOKE_EXPECTS="$(grep '^smoke_expects=' demo-out/plan.txt | cut -d= -f2-)"
    export AZ_SMOKE_TAPS AZ_SMOKE_EXPECTS
    bash scripts/ci/ios_smoke.sh demo-out/smoke 12 demo-out/apps/*-ios-sim.app.zip || echo "SMOKE FAILED"
    for f in demo-out/smoke/*/result.json; do python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], "PASS" if r["pass"] else "FAIL", "-", r["reason"])' "$f"; done
    ;;
  evidence)
    # (1) the iOS Simulator: the plain phone lib and AzWidgets linked against it
    target=aarch64-apple-ios-sim; col=ios-sim
    rustup target add "$target"
    start=$(date +%s)
    bash scripts/demo_lib.sh ios "$target" "$(lib_features "$col" mobile)" "demo-out/libs/demolib-$col-mobile"
    echo "LIB ios-sim SECONDS: $(( $(date +%s) - start ))"
    python3 scripts/demo_build.py ios --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" | tee demo-out/link-ios-sim.txt
    [ -f "target/ios-bundle/$target/AzWidgets.app/AzWidgets" ] && inits ios-sim-AzWidgets "target/ios-bundle/$target/AzWidgets.app/AzWidgets"
    # (2) the iOS device: unsigned
    target=aarch64-apple-ios; col=ios-device
    rustup target add "$target"
    start=$(date +%s)
    bash scripts/demo_lib.sh ios "$target" "$(lib_features "$col" mobile)" "demo-out/libs/demolib-$col-mobile"
    echo "LIB ios-device SECONDS: $(( $(date +%s) - start ))"
    python3 scripts/demo_build.py ios --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" | tee demo-out/link-ios-device.txt
    [ -f "target/ios-bundle/$target/AzWidgets.app/AzWidgets" ] && inits ios-device-AzWidgets "target/ios-bundle/$target/AzWidgets.app/AzWidgets"
    # the macOS desktop app: the shipped desktop lib (dist) and the app against it, as the macos-arm64 leg builds them
    brew install coreutils || true
    export MACOSX_DEPLOYMENT_TARGET=11.0
    start=$(date +%s)
    bash scripts/demo_lib.sh desktop "" dist demo-out/libs/demolib-macos-arm64
    echo "LIB macos-arm64 SECONDS: $(( $(date +%s) - start ))"
    python3 scripts/demo_build.py desktop --leg macos-arm64 --apps AzWidgets --lib demo-out/libs/demolib-macos-arm64/libazul.a --out demo-out/apps --results demo-out/results --deployment-target 11.0 | tee demo-out/link-macos-arm64.txt
    ls -la demo-out/apps
    [ -f demo-out/apps/AzWidgets-macos ] && inits macos-arm64-AzWidgets demo-out/apps/AzWidgets-macos
    for f in demo-out/results/*.json; do python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["column"], r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:300])' "$f"; done
    ;;
  *) echo "unknown mode $mode" >&2; exit 2 ;;
esac
