#!/usr/bin/env bash
# demo_lib.sh: build the ONE libazul that every demo app of a target links over the C ABI, and stage it.
#
#   scripts/demo_lib.sh <kind> <rust-target> <features> <out-dir>
#
#   kind      desktop | android | ios
#   target    the rust target ("" or "host" for the runner's own, desktop only)
#   features  desktop: the features of the shipped libazul (`dist`); mobile: the feature union of the
#             variant (scripts/demo_apps.toml [lib.*]), e.g. "std,logging,link-static,a11y,map-tiles"
#   out-dir   what the app jobs download:
#               desktop   libazul.a (azul.lib on Windows)
#               android   libazul.so
#               ios       libazul.a + libazul.a.native-libs (the system libraries rustc says the archive
#                         needs: dll/build_link.rs reads that file instead of guessing them)
#             and libazul.info.json {kind, target, features, git, seconds, size, sha256}
#
# Every app of the target links THIS file instead of compiling the engine again (the 2026-11-26 run
# compiled azul-dll once per app and target: 6-11 minutes each on a macOS runner).
set -euo pipefail

kind="${1:?kind: desktop | android | ios}"
target="${2:-}"
features="${3:?features}"
out="${4:?out-dir}"
[ "$target" = "host" ] && target=""
mkdir -p "$out"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
started=$(date +%s)
target_flag=()
[ -n "$target" ] && target_flag=(--target "$target")

# The directory cargo writes to, and the profile directory inside it.
profile_dir() {
  local p="$1"
  if [ -n "$target" ]; then echo "target/$target/$p"; else echo "target/$p"; fi
}

case "$kind" in
  desktop)
    # What ships (build_lib): prod-release, thin LTO, the static archive without the bitcode.
    cargo build --profile prod-release ${target_flag[@]+"${target_flag[@]}"} -p azul-dll --features "$features"
    dir="$(profile_dir prod-release)"
    for f in libazul.a azul.lib; do
      [ -f "$dir/$f" ] || continue
      cp "$dir/$f" "$out/$f"
      bash scripts/strip_staticlib.sh "$out/$f"
      case "$(uname -s)" in
        Darwin) strip -S -x "$out/$f" || true ;;
        Linux) strip --strip-debug --strip-unneeded "$out/$f" || true ;;
      esac
    done
    ;;
  android)
    : "${ANDROID_NDK_HOME:?ANDROID_NDK_HOME (NDK r27)}"
    case "$(uname -s)" in Darwin) host=darwin-x86_64 ;; *) host=linux-x86_64 ;; esac
    ndkbin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host/bin"
    upper="$(echo "${target//-/_}" | tr '[:lower:]' '[:upper:]')"
    lower="${target//-/_}"
    export "CARGO_TARGET_${upper}_LINKER=$ndkbin/${target}24-clang"
    export "CARGO_TARGET_${upper}_AR=$ndkbin/llvm-ar"
    export "CC_${lower}=$ndkbin/${target}24-clang" "CXX_${lower}=$ndkbin/${target}24-clang++" "AR_${lower}=$ndkbin/llvm-ar"
    cargo build --release ${target_flag[@]+"${target_flag[@]}"} -p azul-dll --no-default-features --features "$features"
    dir="$(profile_dir release)"
    cp "$dir/libazul.so" "$out/libazul.so"
    "$ndkbin/llvm-strip" --strip-unneeded "$out/libazul.so"
    # A device with 16 KiB pages (Android 15 and later) cannot load a library whose PT_LOAD segments are aligned to 4 KiB, NDK 27's
    # default: the rustflags of the Android targets in .cargo/config.toml link it with 16 KiB, and this fails the lib job, with the
    # file's name, when they did not take effect (RUSTFLAGS in the environment would replace them)
    python3 scripts/android_elf_check.py "$out/libazul.so"
    ;;
  ios)
    # The oldest iOS the apps' Info.plist names (build-ios.sh MIN_OS): rustc and the C parts of the engine (cc-rs) both read it
    export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"
    # Only the archive is built (no cdylib to link on an iPhone), and rustc prints, for the staticlib, the
    # system libraries and frameworks a consumer must link it with.
    log="$out/libazul.build.log"
    cargo rustc --release ${target_flag[@]+"${target_flag[@]}"} -p azul-dll --lib --crate-type staticlib \
      --no-default-features --features "$features" -- --print native-static-libs 2>&1 | tee "$log"
    dir="$(profile_dir release)"
    cp "$dir/libazul.a" "$out/libazul.a"
    bash scripts/strip_staticlib.sh "$out/libazul.a"
    strip -S -x "$out/libazul.a" || true
    sed -n 's/^note: native-static-libs: //p' "$log" | tail -n 1 > "$out/libazul.a.native-libs"
    if [ ! -s "$out/libazul.a.native-libs" ]; then
      echo "::error::rustc printed no native-static-libs line for the iOS archive ($log)"
      exit 1
    fi
    echo "native-static-libs: $(cat "$out/libazul.a.native-libs")"
    rm -f "$log"
    ;;
  *) echo "unknown kind: $kind" >&2; exit 2 ;;
esac

# (not `ls a b c | head`: ls exits 2 when one of the names is absent, and pipefail makes that the script's end)
lib=""
for f in libazul.so libazul.a azul.lib; do
  if [ -f "$out/$f" ]; then lib="$out/$f"; break; fi
done
[ -n "$lib" ] || { echo "::error::no library staged in $out"; exit 1; }
# A library built WITHOUT the cargo feature test-hooks must not hold its marker (nothing that ships reads stand-ins or failure
# injections from the environment), one built WITH it must (the negative control: a check that cannot fail checks nothing).
# scripts/check_no_test_hooks.py; the feature is only ever in an artifact-only variant (demo_apps.py check).
case ",$features," in
  *,test-hooks,*) python3 scripts/check_no_test_hooks.py --expect-marker=test-hooks "$lib" ;;
  *)              python3 scripts/check_no_test_hooks.py "$lib" ;;
esac
sum="$( (sha256sum "$lib" 2>/dev/null || shasum -a 256 "$lib") | cut -d' ' -f1)"
size="$(wc -c < "$lib" | tr -d ' ')"
python3 - "$out/libazul.info.json" "$kind" "${target:-host}" "$features" "$(git rev-parse HEAD)" "$(( $(date +%s) - started ))" "$size" "$sum" "$(basename "$lib")" <<'PY'
import json, sys
path, kind, target, features, git, seconds, size, sha, name = sys.argv[1:]
json.dump({"kind": kind, "target": target, "features": features, "git": git, "seconds": int(seconds),
           "size": int(size), "sha256": sha, "file": name}, open(path, "w"), indent=2, sort_keys=True)
PY
ls -la "$out"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  echo "- libazul \`$(basename "$lib")\` for \`${target:-host}\` (\`$features\`): $(( $(date +%s) - started )) s, $size bytes" >> "$GITHUB_STEP_SUMMARY"
fi
