#!/usr/bin/env bash
# gles_host.sh: the OpenGL ES host test on an Ubuntu runner.
#
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/gles_host.sh' --codegen --artifacts 'target/gles-host/*.png'
#
# WebRender over an OpenGL ES 3 context that Mesa (llvmpipe) makes on EGL's surfaceless platform, compared with the CPU
# renderer's picture of the same display list (dll/src/desktop/shell2/common/gles_host_test.rs): the GLES path of the
# phone shells without a phone. Then the pure tests of the pieces (the probe, the EGL tables, the shader cache rules).
# The same command string every time, so the runner's build cache is reused.
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
for filter in gles_host gles_probe egl_ shader_cache; do
  echo "=== cargo test -p azul-dll --lib $filter ==="
  cargo test -p azul-dll --lib "$filter" -- --test-threads=1 --nocapture
  status=$?
  echo "gles host: filter $filter exit $status"
  if [ "$status" -ne 0 ]; then overall=$status; fi
done
echo "gles host: overall exit $overall"
exit "$overall"
