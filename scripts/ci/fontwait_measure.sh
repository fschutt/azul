#!/usr/bin/env bash
# fontwait_measure.sh <android|ios> <run id of build A> <run id of build B>: ONE runner job, no build: the font-wait A/B on ONE emulator boot / ONE Simulator.
# The builds come from two runs of scripts/ci/fontwait_build.sh (A = the master, B = the master plus the lever); per app variant (control, bundled) the blocks are
# miss A, hit A, miss B, hit B (scripts/ci/font_wait_series.py: `AZ_QA_FONTWAIT_STARTS` starts per block, default 10; every MISS start from an empty font cache).
#
#   scripts/remote.sh run <any sha with this file> ubuntu-22.04 'bash scripts/ci/fontwait_measure.sh android <A> <B>' --artifacts 'demo-out/qa-fontwait/**'
#   scripts/remote.sh run <any sha with this file> macos-15     'bash scripts/ci/fontwait_measure.sh ios <A> <B>'     --artifacts 'demo-out/qa-fontwait/**'
# Result: demo-out/qa-fontwait/**/fontwait/font-wait.md and .json (the table, every start) and starts/ (the timeline lines of every start).
set -uxo pipefail
platform="${1:?android or ios}"; run_a="${2:?run id of build A}"; run_b="${3:?run id of build B}"
artifact=remote-Linux; [ "$platform" = ios ] && artifact=remote-macOS
mkdir -p fw-a fw-b
gh run download "$run_a" -R "$GITHUB_REPOSITORY" -n "$artifact" -D fw-a || { echo "no artifact of run $run_a"; exit 3; }
gh run download "$run_b" -R "$GITHUB_REPOSITORY" -n "$artifact" -D fw-b || { echo "no artifact of run $run_b"; exit 3; }
a="$(find fw-a -type d -name fontwait | head -n 1)"; b="$(find fw-b -type d -name fontwait | head -n 1)"
[ -n "$a" ] && [ -n "$b" ] || { echo "no demo-out/fontwait in the artifacts"; find fw-a fw-b -maxdepth 4 | head -30; exit 3; }
echo "build A: $(cat "$a/commit.txt" 2>/dev/null)  build B: $(cat "$b/commit.txt" 2>/dev/null)"
case "$platform" in
  android)
    plan="control:A=$a/AzWidgets-control.apk;control:B=$b/AzWidgets-control.apk"
    [ -e "$a/AzWidgets-bundled.apk" ] && [ -e "$b/AzWidgets-bundled.apk" ] && plan="$plan;bundled:A=$a/AzWidgets-bundled.apk;bundled:B=$b/AzWidgets-bundled.apk"
    AZ_QA_FONTWAIT_PLAN="$plan" timeout 3000 bash scripts/ci/android_qa.sh demo-out/qa-fontwait "$a/AzWidgets-control.apk" swiftshader_indirect "font wait A/B" fontwait ;;
  ios)
    apps=()
    for side in a b; do
      dir="$a"; [ "$side" = b ] && dir="$b"
      for variant in control bundled; do
        z="$dir/AzWidgets-$variant.zip"; [ -e "$z" ] || continue
        mkdir -p "fw-unzip/$side-$variant"; unzip -q -o "$z" -d "fw-unzip/$side-$variant"
      done
    done
    pa="$(ls -d fw-unzip/a-control/*.app | head -n 1)"; pb="$(ls -d fw-unzip/b-control/*.app | head -n 1)"
    plan="control:A=$PWD/$pa;control:B=$PWD/$pb"
    if [ -d fw-unzip/a-bundled ] && [ -d fw-unzip/b-bundled ]; then
      plan="$plan;bundled:A=$PWD/$(ls -d fw-unzip/a-bundled/*.app | head -n 1);bundled:B=$PWD/$(ls -d fw-unzip/b-bundled/*.app | head -n 1)"
    fi
    AZ_QA_FONTWAIT_PLAN="$plan" bash scripts/ci/ios_qa.sh demo-out/qa-fontwait "$pa" "font wait A/B" fontwait ;;
  *) echo "usage: fontwait_measure.sh android|ios <A> <B>"; exit 2 ;;
esac
echo "SESSION fontwait $platform: exit $?"
find demo-out/qa-fontwait -name 'font-wait.md' -exec cat {} \;
exit 0
