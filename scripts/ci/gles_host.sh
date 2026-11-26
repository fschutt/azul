#!/usr/bin/env bash
# gles_host.sh: the OpenGL ES host tests on an Ubuntu runner.
#
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/gles_host.sh' --codegen --artifacts 'target/gles-host/*.png'
#
# WebRender over an OpenGL ES 3 context that Mesa (llvmpipe) makes on EGL's surfaceless platform, compared with the CPU renderer's
# picture of the same display list (dll/src/desktop/shell2/common/gles_host_test.rs: the pictures; gles_gpu_host_test.rs: the
# lifecycle executor with real EGL and WebRender): the GLES path of the phone shells without a phone. Then the pure tests of the pieces
# (the probe, the EGL tables, the executor's tables, the shader cache rules).
#
# EVERY GLES test runs in a process of its own (`--exact`): a driver or WebRender crash in one is that test's failure and not the end
# of the others. The same command string every time, so the runner's build cache is reused.
#
# AZ_REQUIRE_GLES=1: a runner without OpenGL ES fails the test instead of skipping it, a skip is not a pass here.
set -uo pipefail

sudo apt-get install -y libegl1 libgles2 libegl-mesa0 libgl1-mesa-dri libglapi-mesa >/dev/null 2>&1 || true
echo "EGL and GLES libraries on this runner:"
ls /usr/lib/x86_64-linux-gnu | grep -E 'libEGL|libGLESv2|libgbm' | head -20

export EGL_PLATFORM=surfaceless
export AZ_REQUIRE_GLES=1
export AZ_GPU_PIXELS_DIR="$PWD/target/gles-host"
mkdir -p "$AZ_GPU_PIXELS_DIR"

overall=0
summary=""

echo "=== the GLES tests, one process each ==="
names="$(cargo test -p azul-dll --lib -- --list 2>/dev/null \
  | grep -E '^desktop::shell2::common::(gles_host_test|gles_gpu_host_test)::.*: test$' | sed 's/: test$//')"
echo "$names"
for name in $names; do
  echo "--- $name"
  cargo test -p azul-dll --lib "$name" -- --exact --test-threads=1 --nocapture
  status=$?
  if [ "$status" -eq 0 ]; then summary="$summary"$'\n'"PASS $name"; else summary="$summary"$'\n'"FAIL($status) $name"; overall=$status; fi
done

for filter in gles_probe egl_ gles_gpu::tests shader_cache; do
  echo "=== cargo test -p azul-dll --lib $filter ==="
  cargo test -p azul-dll --lib "$filter" -- --test-threads=1 --nocapture
  status=$?
  echo "gles host: filter $filter exit $status"
  if [ "$status" -ne 0 ]; then summary="$summary"$'\n'"FAIL($status) filter $filter"; overall=$status; else summary="$summary"$'\n'"PASS filter $filter"; fi
done

echo "=== SUMMARY ==="
echo "$summary"
echo "gles host: overall exit $overall"
if [ "$overall" -eq 0 ]; then echo "GLES HOST VERDICT: PASS"; else echo "GLES HOST VERDICT: FAIL ($overall)"; fi
# Exit 0 whatever the verdict: the runner saves its build cache only when the job succeeds, and a failed test run would throw away the
# 25 minutes of compiling the next run needs. The verdict is the line above and the exit codes printed per test.
exit 0
