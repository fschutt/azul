#!/usr/bin/env bash
# gpu_swiftshader_host.sh: EXPERIMENT (the Android GPU root-cause hunt, never part of a gate): the GLES host tests of the GPU path against
# the GL libraries the Android emulator itself uses (its SwiftShader, and its Mesa and ANGLE when the package has them) instead of the
# runner's Mesa. The emulator's GL stack is the guest encoder + a host translator + THESE libraries; here the tests talk to the libraries
# directly: if WebRender draws the page on them, the emulator's black window is the guest side (encoder, translator, how the app talks
# to them), if it does not, it is a property of the library and the test reproduces it without an emulator.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/gpu_swiftshader_host.sh' --codegen --artifacts 'target/gles-host/*'
#
#   AZ_SW_STACKS   the stacks to try, `name=directory under the emulator package's lib64` separated by blanks;
#                  default "swiftshader=gles_swiftshader"
#   AZ_SW_TESTS    the tests, default "gles_control the_first_surface_brings_the_path_up_and_draws_the_cpu_page
#                  webrender_over_gles_draws_the_page_the_cpu_renderer_draws webrender_over_gles_draws_the_widget_showcase_the_cpu_renderer_draws
#                  a_second_frame_shows_its_own_page_and_nothing_of_the_first"
set -uo pipefail

export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "emulator" >/tmp/sdkmanager-emulator.log 2>&1 || { echo "sdkmanager failed"; tail -n 20 /tmp/sdkmanager-emulator.log; }
emu="$ANDROID_HOME/emulator"
out="target/gles-host"
mkdir -p "$out"
echo "=== the emulator package: $(ls "$emu" 2>/dev/null | head -n 30 | tr '\n' ' ')"
echo "=== its GL and Vulkan libraries"
find "$emu" \( -name '*.so' -o -name '*.so.*' \) \( -path '*gles*' -o -path '*vulkan*' -o -name '*swiftshader*' -o -name '*angle*' -o -name '*mesa*' -o -name 'libEGL*' -o -name 'libGLES*' \) 2>/dev/null | sort | head -n 80 \
  | while read -r f; do printf '%s %s\n' "$(stat -c %s "$f")" "${f#"$emu"/}"; done

echo "=== the test binary"
bin="$(cargo test -p azul-dll --lib --no-run --message-format=json 2>"$out/build.err" \
  | python3 -c '
import json, sys
best = ""
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m.get("profile", {}).get("test") and "azul_dll" in m.get("target", {}).get("name", "").replace("-", "_"):
        best = m["executable"]
print(best)
')"
if [ -z "$bin" ] || [ ! -x "$bin" ]; then
  echo "the test binary was not built:"; grep -E "^error" -A 12 "$out/build.err" | head -n 80; tail -n 5 "$out/build.err"
  exit 1
fi
echo "binary: $bin"

stacks="${AZ_SW_STACKS:-swiftshader=gles_swiftshader}"
tests="${AZ_SW_TESTS:-gles_control the_first_surface_brings_the_path_up_and_draws_the_cpu_page webrender_over_gles_draws_the_page_the_cpu_renderer_draws webrender_over_gles_draws_the_widget_showcase_the_cpu_renderer_draws a_second_frame_shows_its_own_page_and_nothing_of_the_first}"

for entry in $stacks; do
  name="${entry%%=*}"; sub="${entry#*=}"
  dir="$emu/lib64/$sub"
  echo "================ stack $name: $dir"
  if [ ! -d "$dir" ]; then echo "no such directory"; ls "$emu/lib64" 2>/dev/null | head -n 40; continue; fi
  ls -la "$dir" | head -n 30
  shim="$PWD/target/gl-shim/$name"
  rm -rf "$shim"; mkdir -p "$shim"
  # the names the tests open: libEGL.so.1 / libEGL.so, libGLESv2.so.2 / libGLESv2.so
  for pair in "libEGL.so.1:libEGL.so" "libEGL.so:libEGL.so" "libGLESv2.so.2:libGLESv2.so" "libGLESv2.so:libGLESv2.so" "libGLESv3.so:libGLESv2.so"; do
    link="${pair%%:*}"; target="${pair#*:}"
    if [ -e "$dir/$target" ]; then ln -sf "$dir/$target" "$shim/$link"; fi
  done
  ls -la "$shim"
  ldd "$dir/libEGL.so" 2>&1 | head -n 12
  export LD_LIBRARY_PATH="$shim:$dir:$emu/lib64:$emu/lib64/vulkan:${LD_LIBRARY_PATH:-}"
  for t in $tests; do
    echo "=== $name / $t"
    case "$t" in
      gles_control) filter="gles_control" ;;
      the_first_surface_brings_the_path_up_and_draws_the_cpu_page) filter="the_first_surface_brings_the_path_up_and_draws_the_cpu_page" ;;
      *) filter="desktop::shell2::common::gles_host_test::$t" ;;
    esac
    mkdir -p "$PWD/$out/pixels-$name"
    env -u EGL_PLATFORM AZ_REQUIRE_GLES=1 AZ_GLES_HOST_TESTS=1 AZ_GPU_PIXELS_DIR="$PWD/$out/pixels-$name" AZ_GL_TRACE="2:$PWD/$out/gltrace-$name-$t.txt" \
      timeout 900 "$bin" "$filter" --test-threads=1 --nocapture >"$out/$name-$t.log" 2>&1
    status=$?
    echo "exit $status"
    grep -E "control \(split|test result|panicked|FAILED|SKIPPED|differ|% of|mean |GL_RENDERER|renderer|llvmpipe|SwiftShader|Mesa|ES 3" "$out/$name-$t.log" | cut -c1-300 | head -n 60
    [ -s "$out/gltrace-$name-$t.txt" ] && gzip -f "$out/gltrace-$name-$t.txt"
  done
  unset LD_LIBRARY_PATH
done
ls -la "$out" | head -n 60
echo "gpu swiftshader host: done"
exit 0
