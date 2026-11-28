#!/usr/bin/env bash
# e2e_matrix_desktop.sh [<dir of the desktop QA build's apps>]: the desktop column of the e2e matrix (scripts/ci/e2e_matrix.py).
#
#   - the files that mount their own DOM run in process, headless, in `azul-doc e2e <file> --keep-going` (the executor of an app without an app);
#   - the files that assert an app's DOM run in that app's binary of the desktop QA build (scripts/ci/desktop_e2e_build.sh: the shipped
#     libazul plus the debug server and the e2e executor, the apps AzWidgets-e2e-linux and AzWriter-e2e-linux), AZ_E2E=<file> on the headless
#     backend, when the directory is given;
#   - the rest is marked with the reason it has no desktop host.
# The scenarios take Chrome's numbers for 16px Arial, which azul draws in Liberation Sans on Linux: the fonts of the CI's layout tests are
# installed first, or a font-dependent line count is a verdict about the runner's fonts.
#
#   scripts/remote.sh run <ref> ubuntu-22.04 'gh run download <desktop qa build run id> -n remote-Linux -D desktop-qa; bash scripts/ci/e2e_matrix_desktop.sh desktop-qa/demo-out/apps' \
#       --codegen --artifacts 'demo-out/e2e-matrix-desktop/**'
set -uo pipefail
apps_dir="${1:-}"
if command -v apt-get >/dev/null 2>&1; then
  { sudo apt-get update -qq && sudo apt-get install -y -qq fonts-liberation fonts-dejavu-core; } >/dev/null 2>&1 \
    && echo "fonts of the CI's layout tests installed" || echo "the fonts of the CI's layout tests could not be installed"
fi
doc=target/release/azul-doc
[ -x "$doc" ] || cargo build -r -p azul-doc || { echo "azul-doc did not build"; exit 3; }
bins=()
if [ -n "$apps_dir" ] && [ -d "$apps_dir" ]; then
  for app in "$apps_dir"/*-e2e-linux; do
    [ -f "$app" ] || continue
    chmod +x "$app"
    bins+=(--bin "$(basename "$app" | cut -d- -f1)=$app")
    echo "desktop host: $app"
  done
else
  echo "no desktop QA build given: the files that assert an app's DOM stay 'no desktop host'"
fi
python3 scripts/ci/e2e_matrix.py desktop --repo . --out demo-out/e2e-matrix-desktop --azul-doc "$doc" ${bins[@]+"${bins[@]}"}

# AZ_MATRIX_DIAG=1: the numbers behind a red scenario of the matrix (the rects of a list of inline list items, the computed padding after a Tab):
# every assertion of this file is meant to FAIL and its message carries the value it found. Not part of the verdicts.
if [ -n "${AZ_MATRIX_DIAG:-}" ]; then
  mkdir -p demo-out/e2e-matrix-desktop
  cat > demo-out/e2e-matrix-desktop/diag.json <<'DIAGEND'
[
 {"name":"diag_list_original","setup":{"window_width":400,"window_height":160,"dpi":96},"steps":[
  {"op":"mount","html":["<ol id=\"numbered\"><li id=\"a1\" class=\"inline\">apples</li> <li id=\"a2\" class=\"inline\">pears</li></ol>","<ul id=\"bulleted\"><li id=\"b1\" class=\"inline\">plums</li> <li id=\"b2\" class=\"inline\">figs</li></ul>"],"css":["body { margin: 0; }","ol, ul { margin: 10px; padding: 0; font-size: 16px; line-height: 20px; }",".inline { display: inline list-item; list-style-position: inside; margin-right: 12px; }"]},
  {"op":"wait_frame"},
  {"op":"assert_layout","selector":"#numbered","property":"width","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#numbered","property":"height","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a1","property":"x","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a1","property":"y","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a1","property":"width","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a1","property":"height","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a2","property":"x","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a2","property":"y","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a2","property":"width","expected":-1,"tolerance":0}
 ]},
 {"name":"diag_list_inline_block","setup":{"window_width":400,"window_height":160,"dpi":96},"steps":[
  {"op":"mount","html":["<ol id=\"numbered\"><li id=\"a1\" class=\"inline\">apples</li> <li id=\"a2\" class=\"inline\">pears</li></ol>"],"css":["body { margin: 0; }","ol, ul { margin: 10px; padding: 0; font-size: 16px; line-height: 20px; }",".inline { display: inline-block; margin-right: 12px; }"]},
  {"op":"wait_frame"},
  {"op":"assert_layout","selector":"#numbered","property":"height","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a1","property":"width","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a2","property":"x","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a2","property":"y","expected":-1,"tolerance":0}
 ]},
 {"name":"diag_list_inline","setup":{"window_width":400,"window_height":160,"dpi":96},"steps":[
  {"op":"mount","html":["<ol id=\"numbered\"><li id=\"a1\" class=\"inline\">apples</li> <li id=\"a2\" class=\"inline\">pears</li></ol>"],"css":["body { margin: 0; }","ol, ul { margin: 10px; padding: 0; font-size: 16px; line-height: 20px; }",".inline { display: inline; margin-right: 12px; }"]},
  {"op":"wait_frame"},
  {"op":"assert_layout","selector":"#numbered","property":"height","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a2","property":"x","expected":-1,"tolerance":0},
  {"op":"assert_layout","selector":"#a2","property":"y","expected":-1,"tolerance":0}
 ]},
 {"name":"diag_focus_within","setup":{"window_width":400,"window_height":300,"dpi":96},"steps":[
  {"op":"mount","html":["<div class=\"frame\" id=\"frame\">","  <div class=\"field\" id=\"field\" tabindex=\"0\"></div>","</div>"],"css":["body { margin: 0; }",".frame { width: 100px; padding: 2px; }",".field { height: 10px; }",".frame:focus-within { padding: 8px; }"]},
  {"op":"wait_frame"},
  {"op":"key_down","key":"Tab"},
  {"op":"key_up","key":"Tab"},
  {"op":"wait_frame"},
  {"op":"assert_css","selector":"#frame","property":"padding-top","expected":"IMPOSSIBLE"},
  {"op":"assert_layout","selector":"#frame","property":"height","expected":-1,"tolerance":0},
  {"op":"wait","ms":200},
  {"op":"assert_layout","selector":"#frame","property":"height","expected":-2,"tolerance":0}
 ]}
]
DIAGEND
  for jobs in 1 4; do
    echo "=== diagnostic scenarios, --jobs $jobs"
    "$doc" e2e demo-out/e2e-matrix-desktop/diag.json --keep-going --jobs "$jobs" 2>&1 | tail -90
  done
fi
