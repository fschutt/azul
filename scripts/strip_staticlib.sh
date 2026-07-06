#!/usr/bin/env bash
# Strip embedded LLVM bitcode (.llvmbc/.llvmcmd, __LLVM,__bitcode) + debug info
# from shipped static libraries, then ASSERT none survived.
#
# Why: [profile.prod-release] lto="thin" embeds ThinLTO bitcode in every object
# — 56-89% of every shipped .a/.lib (azul.lib 295 MB -> ~110 MB, whole release
# ~1.3-1.5 GB). Consumers of a prebuilt staticlib never run Rust ThinLTO, so
# the bitcode is 100% dead weight. Stripping post-build keeps the
# ThinLTO-optimized machine code bit-identical (switching to embed-bitcode=no
# would change codegen). See scripts/RELEASE_SIZE_MEMORY_AUDIT_2026_07_04.md §2.4.
#
# Handles:
#   *.a        ELF (any arch — cross too, unlike host binutils strip, which
#              silently skipped the Android archives) and Mach-O, processed
#              whole-archive by llvm-objcopy.
#   *.lib      MSVC-style archives, which llvm-objcopy rejects at the archive
#              level -> member-wise via scripts/strip_coff_lib.py.
#   *.dll.lib  import libraries: skipped (no bitcode, objcopy can't parse them).
#
# Usage: strip_staticlib.sh <lib> [<lib> ...]
# Env:   STRIP_OBJCOPY=/path/to/llvm-objcopy to override tool discovery.
set -euo pipefail

find_objcopy() {
    if [ -n "${STRIP_OBJCOPY:-}" ]; then echo "$STRIP_OBJCOPY"; return; fi
    local sysroot
    sysroot="$(rustc --print sysroot 2>/dev/null || true)"
    local c
    for c in "$sysroot"/lib/rustlib/*/bin/rust-objcopy \
             "$sysroot"/lib/rustlib/*/bin/llvm-objcopy; do
        [ -x "$c" ] && { echo "$c"; return; }
    done
    if command -v llvm-objcopy >/dev/null 2>&1; then command -v llvm-objcopy; return; fi
    # Android NDK ships one (any arch's llvm-objcopy handles all targets)
    if [ -n "${ANDROID_NDK_HOME:-}" ]; then
        c="$(ls "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-objcopy 2>/dev/null | head -1)"
        [ -n "$c" ] && { echo "$c"; return; }
    fi
    # Last resort: the rustup component that ships llvm-objcopy
    if command -v rustup >/dev/null 2>&1; then
        rustup component add llvm-tools >/dev/null 2>&1 || true
        for c in "$sysroot"/lib/rustlib/*/bin/llvm-objcopy; do
            [ -x "$c" ] && { echo "$c"; return; }
        done
    fi
    return 1
}

OBJCOPY="$(find_objcopy)" || { echo "::error::strip_staticlib.sh: no llvm-objcopy found" >&2; exit 1; }
# rust-objcopy (llvm-tools) is dynamically linked against the toolchain's
# libLLVM.so (e.g. libLLVM.so.20.1-rust-1.88.0-stable), which lives in
# `$sysroot/lib` but isn't on the loader path — without this it dies with
# "error while loading shared libraries: libLLVM.so...: cannot open shared
# object file". Prepend the toolchain lib dir so the strip step can run.
_azul_sysroot="$(rustc --print sysroot 2>/dev/null || true)"
if [ -n "$_azul_sysroot" ] && [ -d "$_azul_sysroot/lib" ]; then
    export LD_LIBRARY_PATH="$_azul_sysroot/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
PYTHON="$(command -v python3 || command -v python)" || true
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Bitcode + debug sections across the three object formats. llvm-objcopy
# ignores --remove-section names that don't exist in a given format.
FLAGS=(
    --remove-section=.llvmbc
    --remove-section=.llvmcmd
    --remove-section=__LLVM,__bitcode
    --remove-section=__LLVM,__cmdline
    --strip-debug
)

assert_no_bitcode() {
    # Belt: the section NAME only appears in section headers / load commands,
    # so a raw byte scan is a sufficient (and format-agnostic) assert.
    local f="$1" pat
    for pat in ".llvmbc" "__bitcode"; do
        if grep -qF -- "$pat" "$f" 2>/dev/null; then
            echo "::error::$f still contains a '$pat' section after stripping" >&2
            return 1
        fi
    done
}

rc=0
for f in "$@"; do
    [ -f "$f" ] || { echo "skip (missing): $f"; continue; }
    case "$f" in
    *.dll.lib)
        echo "skip (import lib): $f"
        continue
        ;;
    *.lib)
        if [ -z "$PYTHON" ]; then echo "::error::python needed for $f" >&2; rc=1; continue; fi
        "$PYTHON" "$SCRIPT_DIR/strip_coff_lib.py" "$OBJCOPY" "$f" || rc=1
        continue
        ;;
    esac
    before=$(wc -c < "$f")
    if ! "$OBJCOPY" "${FLAGS[@]}" "$f"; then
        echo "::error::llvm-objcopy failed on $f" >&2
        rc=1
        continue
    fi
    after=$(wc -c < "$f")
    [ "$before" -gt 0 ] || before=1
    echo "$f: $before -> $after bytes ($(( 100 - after * 100 / before ))% smaller)"
    assert_no_bitcode "$f" || rc=1
done
exit $rc
