#!/usr/bin/env bash
# gpu_lib_size.sh: what the Android GPU path costs libazul.so, measured as a pair in ONE runner job (same runner, same cache state,
# same feature strings): the library of this checkout and the library of the commit of master the branch was made from.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/gpu_lib_size.sh <full sha of the base commit>' --codegen \
#       --artifacts 'demo-out/size/**/libazul.info.json'
#
# For the phone variants `mobile` and `mobile-e2e` (scripts/demo_apps.toml), android x86_64, built by scripts/demo_lib.sh exactly as the demos
# pipeline builds the library the apps link: release, stripped, the 16 KiB page flags. The feature string of each variant is read ONCE, from
# this checkout, and given to both builds, so a difference is the code and not the features. `seconds` is the wall clock of demo_lib.sh on a
# shared runner; the second pair of builds finds the dependencies built, as the first did.
set -uo pipefail

base="${1:?the full sha of the commit of master the branch was made from}"
target=x86_64-linux-android
col=android-x86_64
here_sha="$(git rev-parse HEAD)"

if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  python3 -m pip install --quiet --user tomli || true   # the runner's Python 3.10 has no tomllib
fi
export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
rustup target add "$target"

mkdir -p demo-out/size
features_mobile="$(python3 scripts/demo_apps.py features "$col" mobile)"
features_e2e="$(python3 scripts/demo_apps.py features "$col" mobile-e2e)"
echo "mobile:     $features_mobile"
echo "mobile-e2e: $features_e2e"

overall=0
build() {   # label, variant, features
  local label="$1" variant="$2" features="$3"
  echo "=== $label $variant ==="
  bash scripts/demo_lib.sh android "$target" "$features" "demo-out/size/$label-$variant"
  local status=$?
  echo "gpu lib size: $label $variant exit $status"
  if [ "$status" -ne 0 ]; then overall=$status; fi
}

echo "=== this checkout: $here_sha ==="
build after mobile "$features_mobile"
build after mobile-e2e "$features_e2e"

echo "=== the base: $base ==="
git fetch --depth 1 origin "$base" || { echo "the base commit could not be fetched"; overall=1; }
if git checkout --detach "$base"; then
  # the bindings of the base's API, in case the two differ
  cargo run -r -p azul-doc codegen all >target/gpu-size-codegen.log 2>&1 || { tail -n 20 target/gpu-size-codegen.log; overall=1; }
  build before mobile "$features_mobile"
  build before mobile-e2e "$features_e2e"
else
  echo "the base commit could not be checked out"; overall=1
fi

echo "=== GPU LIB SIZE ==="
python3 - <<'PY'
import json, pathlib
rows = {}
for p in sorted(pathlib.Path("demo-out/size").glob("*/libazul.info.json")):
    label, variant = p.parent.name.split("-", 1)
    rows[(label, variant)] = json.loads(p.read_text())
for variant in ("mobile", "mobile-e2e"):
    a, b = rows.get(("after", variant)), rows.get(("before", variant))
    if not (a and b):
        print(f"{variant}: a build is missing (after: {bool(a)}, before: {bool(b)})")
        continue
    d = a["size"] - b["size"]
    print(f"{variant}: before {b['size']} bytes ({b['seconds']} s), after {a['size']} bytes ({a['seconds']} s): "
          f"{d:+d} bytes ({100.0 * d / b['size']:+.2f} %), {a['seconds'] - b['seconds']:+d} s; features {a['features']}")
PY
echo "gpu lib size: overall exit $overall"
if [ "$overall" -eq 0 ]; then echo "GPU LIB SIZE VERDICT: PASS"; else echo "GPU LIB SIZE VERDICT: FAIL ($overall)"; fi
# Exit 0 whatever the verdict, so that the runner keeps its build cache; the verdict is the line above.
exit 0
