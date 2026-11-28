#!/usr/bin/env bash
# gl_desktop_check.sh: the DESKTOP OpenGL path after a change to the vendored WebRender device layer (webrender/core/src/device/gl.rs):
# the GLES branches are gated by the GL type (`Device::is_gles`), and this shows the desktop branch is the code it was.
#
#   scripts/remote.sh run <full sha> macos-15 'bash scripts/ci/gl_desktop_check.sh' --codegen
#
# 1. WebRender's own unit tests (`cargo test -p webrender`): the workspace job runs them on every push.
# 2. The real GPU frame pipeline on desktop GL, offscreen: dll/src/desktop/shell2/macos/headless_gpu.rs draws pages with WebRender on a
#    CGL context and the CPU renderer's picture of the same display list, and the tests hold the two together (these call
#    Device::clear_target with a depth target on desktop OpenGL). Those tests return without checking when the machine cannot make a
#    CGL context (the file says so itself), so the script prints what the machine has (system_profiler) and the tests' own timings: a
#    suite that ran in milliseconds ran nothing.
set -uo pipefail

system_profiler SPDisplaysDataType 2>/dev/null | head -n 20

overall=0
echo "=== cargo test -p webrender ==="
cargo test -p webrender
status=$?
echo "gl desktop: webrender exit $status"
if [ "$status" -ne 0 ]; then overall=$status; fi

echo "=== cargo test -p azul-dll --lib headless_gpu ==="
cargo test -p azul-dll --lib headless_gpu -- --test-threads=1 --nocapture
status=$?
echo "gl desktop: headless_gpu exit $status"
if [ "$status" -ne 0 ]; then overall=$status; fi

echo "gl desktop: overall exit $overall"
if [ "$overall" -eq 0 ]; then echo "GL DESKTOP VERDICT: PASS"; else echo "GL DESKTOP VERDICT: FAIL ($overall)"; fi
# Exit 0 whatever the verdict, so that the runner keeps its build cache (it saves it only for a job that succeeded); the verdict is the line above.
exit 0
