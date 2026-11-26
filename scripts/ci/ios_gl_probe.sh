#!/usr/bin/env bash
# ios_gl_probe.sh: what OpenGL ES really is on the iOS Simulator of this machine (the evidence behind the iOS GPU
# path's choice of OpenGL ES over Metal), and what the SDK and the runner say about it.
#
#   scripts/ci/ios_gl_probe.sh <out-dir> [<udid>]
#
# For a macOS host with Xcode (xcrun simctl). Builds scripts/ci/ios_gl_probe.m for the simulator SDK, boots a
# simulator (or uses the booted <udid>) and runs the tool INSIDE it. Writes into <out-dir>:
#   host.txt   the host: macOS, Xcode, the CPU, system_profiler's GPU
#   sdk.txt    the iOS SDK's OpenGLES.framework: files, the headers' deprecation lines, the runtime's own copy
#   probe.txt  what the tool printed (EAGL context, GL strings, limits, render targets, layer storage, frame cost,
#              Metal's default device, the engine's GL loader names that resolve on the framework)
# Exit 0 when the probe ran (the facts are in the files), 2 when it could not (no simulator, no compiler).
set -uo pipefail

out="${1:?out-dir}"; udid="${2:-}"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
mkdir -p "$out"

{
  echo "== sw_vers"; sw_vers
  echo "== uname"; uname -a
  echo "== cpu"; sysctl -n machdep.cpu.brand_string 2>/dev/null || sysctl -n hw.model
  echo "== memory (bytes)"; sysctl -n hw.memsize
  echo "== xcodebuild"; xcodebuild -version 2>&1
  echo "== runtimes"; xcrun simctl list runtimes 2>&1
  echo "== system_profiler SPDisplaysDataType"; system_profiler SPDisplaysDataType 2>&1 | head -60
} >"$out/host.txt" 2>&1

sdk="$(xcrun --sdk iphonesimulator --show-sdk-path 2>/dev/null)" || { echo "no iphonesimulator SDK"; exit 2; }
fw="$sdk/System/Library/Frameworks/OpenGLES.framework"
{
  echo "sdk: $sdk ($(xcrun --sdk iphonesimulator --show-sdk-version))"
  echo "== OpenGLES.framework in the SDK"; ls -la "$fw" 2>&1
  echo "== EAGL.h and EAGLDrawable.h: the lines that say deprecated"
  grep -n -i "deprecat\|API_DEPRECATED\|OPENGLES_DEPRECATED\|OPENGLES_ENABLE\|__IOS_PROHIBITED" "$fw/Headers/EAGL.h" "$fw/Headers/EAGLDrawable.h" 2>&1 | head -40
  echo "== the umbrella gl.h headers"
  ls "$fw/Headers" 2>&1 | head -40
  echo "== OpenGLES.tbd (targets, install name, the EAGL and gl symbols counted)"
  tbd="$fw/OpenGLES.tbd"
  if [ -f "$tbd" ]; then
    head -24 "$tbd"
    echo "symbols mentioning EAGL: $(grep -c 'EAGL' "$tbd")"
    echo "symbols starting with _gl or _egl: $(grep -o '_gl[A-Za-z0-9_]*' "$tbd" | sort -u | wc -l | tr -d ' ')"
  else
    echo "no OpenGLES.tbd in the SDK"
  fi
  echo "== Metal framework in the SDK"; ls "$sdk/System/Library/Frameworks/Metal.framework" 2>&1 | head -5
  echo "== CAMetalLayer / CAEAGLLayer headers"
  ls "$sdk/System/Library/Frameworks/QuartzCore.framework/Headers" 2>&1 | grep -i "metal\|eagl"
} >"$out/sdk.txt" 2>&1

# the engine's GL loader names: what shell2/common/gl_loader.rs asks a platform's resolver for
grep -o 'get_func("[A-Za-z0-9_]*")' "$root/dll/src/desktop/shell2/common/gl_loader.rs" | sed 's/get_func("\(.*\)")/\1/' | sort -u >"$out/loader_names.txt"
echo "loader names: $(wc -l <"$out/loader_names.txt" | tr -d ' ')" | tee -a "$out/sdk.txt"

arch="$(uname -m)"; [ "$arch" = "arm64" ] || arch="x86_64"
if ! xcrun --sdk iphonesimulator clang -arch "$arch" -mios-simulator-version-min=16.0 -fobjc-arc -O1 \
     -Wno-deprecated-declarations -framework Foundation -framework QuartzCore -framework OpenGLES -framework Metal \
     "$here/ios_gl_probe.m" -o "$out/ios_gl_probe" 2>"$out/compile.log"; then
  echo "the probe did not compile:"; cat "$out/compile.log"; exit 2
fi
echo "probe compiled for $arch"

own=""
if [ -z "$udid" ]; then
  pick="$(python3 - <<'PY'
import json, re, subprocess
def sh(*a): return json.loads(subprocess.check_output(a))
runtimes = [r for r in sh("xcrun", "simctl", "list", "runtimes", "-j")["runtimes"]
            if r.get("isAvailable", True) and r["identifier"].startswith("com.apple.CoreSimulator.SimRuntime.iOS")]
if not runtimes:
    raise SystemExit("no iOS simulator runtime is installed")
runtimes.sort(key=lambda r: [int(x) for x in r["version"].split(".")])
rt = runtimes[-1]
supported = {d["identifier"] for d in rt.get("supportedDeviceTypes", [])}
types = [d for d in sh("xcrun", "simctl", "list", "devicetypes", "-j")["devicetypes"]
         if d["name"].startswith("iPhone") and (not supported or d["identifier"] in supported)]
def rank(d):
    m = re.search(r"iPhone (\d+)", d["name"])
    plain = not re.search(r"(Pro|Plus|Max|mini|SE|e\b)", d["name"])
    return (int(m.group(1)) if m else 0, plain)
types.sort(key=rank)
print(rt["identifier"], types[-1]["identifier"], rt.get("runtimeRoot") or rt.get("bundlePath", ""))
PY
)" || { echo "$pick"; exit 2; }
  read -r runtime devtype runtimeroot <<<"$pick"
  echo "simulator: $runtime / $devtype"
  udid="$(xcrun simctl create azul-glprobe "$devtype" "$runtime")" || exit 2
  own=1
  trap 'xcrun simctl shutdown "$udid" >/dev/null 2>&1; echo "left the simulator $udid in place (the runner is thrown away)"' EXIT
  t0=$(date +%s)
  xcrun simctl boot "$udid" || exit 2
  xcrun simctl bootstatus "$udid" -b >"$out/boot.log" 2>&1 || { tail -n 20 "$out/boot.log"; exit 2; }
  echo "simulator booted in $(( $(date +%s) - t0 )) s: $udid"
  sleep 3
  {
    echo "== the runtime's own OpenGLES.framework ($runtimeroot)"
    rr="$runtimeroot"; [ -d "$rr/Contents/Resources/RuntimeRoot" ] && rr="$rr/Contents/Resources/RuntimeRoot"
    ls -la "$rr/System/Library/Frameworks/OpenGLES.framework" 2>&1 | head -20
    echo "== GL-related files in the runtime"
    find "$rr/System/Library" -maxdepth 4 \( -iname '*GLEngine*' -o -iname '*AppleMetalOpenGL*' -o -iname '*OpenGLES*' -o -iname '*libGL*' \) 2>/dev/null | head -40
    echo "== the simulator's GL-related processes"
    xcrun simctl spawn "$udid" launchctl list 2>/dev/null | grep -i "gl\|gpu\|metal\|render" | head -20
  } >>"$out/sdk.txt" 2>&1
fi

echo "running the probe inside the simulator $udid"
xcrun simctl spawn "$udid" "$out/ios_gl_probe" "$out/loader_names.txt" >"$out/probe.txt" 2>&1
rc=$?
echo "probe exit $rc; $(grep -c '^probe:' "$out/probe.txt") lines"
grep -E '^probe: (uname|env|Metal|EAGLContext|GL_|renderbufferStorage|layer storage|cleared|presentRenderbuffer|120 x|the engine|of [0-9]+ functions|dlopen)' "$out/probe.txt" | head -60
exit 0
