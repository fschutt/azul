#!/usr/bin/env bash
# ios_phone_selftest_build.sh: AzSelfTest for YOUR iPhone, from the sources, on your Mac (Xcode installed): the QA library and the app for
# aarch64-apple-ios, UNSIGNED (the release's device slice is unsigned on purpose: the identity has to be yours), and, when you give it the
# two things it needs, signed, installed and run:
#
#   bash scripts/ci/ios_phone_selftest_build.sh                       # build only: demo-out/apps/AzSelfTest-e2e-ios-device.app.zip
#   IOS_SIGNING_IDENTITY="Apple Development: you@example.com (TEAMID)" IOS_DEVICE="<the phone's name or id>" \
#     bash scripts/ci/ios_phone_selftest_build.sh                     # build, sign, install, run, tell what to do, read the report
#
# <the phone's name or id> is what `xcrun devicectl list devices` shows: connected by cable, unlocked, trusted. The signing identity is
# `security find-identity -v -p codesigning`; a free Apple ID works (Xcode > Settings > Accounts makes the development certificate, the
# profile for the app's bundle id is made the first time Xcode signs it: if the install says "no valid provisioning profile", open any
# project in Xcode with that bundle id once, or sign with `rcodesign`, see doc/guide/en/deploying/mobile.md).
#
# What the Simulator cannot prove (it has no accelerometer, gyroscope, magnetometer, barometer, camera or Taptic Engine) is what this is
# for: scripts/ci/ios_phone_selftest.py starts the app, tells in plain words what a person has to do (shake it, cover the top with a hand,
# say a word) and prints one table; its exit code is 1 when a row that says it is available FAILED, or when a row a real phone must pass
# says UNAVAILABLE.
#
# The library build is ~15 to 25 minutes cold (the QA library of the app: debug server, E2E scripting and test hooks, an ARTIFACT, never
# shipped); the app takes a minute.
set -euo pipefail

app="${1:-AzSelfTest}"
target=aarch64-apple-ios
col=ios-device

if ! python3 scripts/demo_apps.py qa-only "$app"; then
  echo "$app is not a qa_only app of the manifest (scripts/demo_apps.toml): this builds only the QA library variant of one" >&2
  exit 2
fi

mkdir -p demo-out/libs demo-out/apps demo-out/results
rustup target add "$target"
python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt

lib="$(python3 scripts/demo_apps.py e2e-lib "$app")"
# The plan builds the QA library for the Simulator, not for a device (nobody ships it): the features are the same
features="$(python3 scripts/demo_apps.py features ios-sim "$lib")"
echo "== build $app on $lib for $target: $features"
bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-$col-$lib"
python3 scripts/demo_build.py ios --target "$target" --apps "$app" --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant "$lib"

zip="demo-out/apps/$app-e2e-$col.app.zip"
ls -la "$zip"
mkdir -p demo-out/phone-app
unzip -q -o "$zip" -d demo-out/phone-app
bundle="$(find demo-out/phone-app -maxdepth 2 -name '*.app' -type d | head -n 1)"
[ -n "$bundle" ] || { echo "the zip holds no .app" >&2; exit 1; }
echo "== the app (unsigned): $bundle"

if [ -z "${IOS_SIGNING_IDENTITY:-}" ] || [ -z "${IOS_DEVICE:-}" ]; then
  cat <<EOF

Built, not installed. To put it on your iPhone and run every row that needs no person (add --attended to be asked for the rest):

  codesign --force --sign "<your identity from: security find-identity -v -p codesigning>" --entitlements scripts/ios/entitlements.xcent "$bundle"
  python3 scripts/ci/ios_phone_selftest.py "<the phone's name or id from: xcrun devicectl list devices>" --app "$bundle"
  python3 scripts/ci/ios_phone_selftest.py "<the phone>" --attended        # the rows that need a person: shake it, cover the top, say a word

or set IOS_SIGNING_IDENTITY and IOS_DEVICE and run this script again (the library is built once).
EOF
  exit 0
fi

codesign --force --sign "$IOS_SIGNING_IDENTITY" --entitlements scripts/ios/entitlements.xcent "$bundle"
python3 scripts/ci/ios_phone_selftest.py "$IOS_DEVICE" --app "$bundle" --out demo-out/ios-phone-selftest
