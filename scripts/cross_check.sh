#!/usr/bin/env bash
#
# cross_check.sh - Test cross-compilation of azul-dll across many Rust targets
#
# Usage:
#   ./scripts/cross_check.sh                  # check all targets
#   ./scripts/cross_check.sh i686 arm wasm    # check targets matching keywords
#   ./scripts/cross_check.sh --list           # list all targets
#   ./scripts/cross_check.sh --quick          # sub-crates only (fast)
#
# Requires: rustup (targets are installed automatically)
#
# Since azul is pure Rust (no ring, no cc-compiled C), cargo check works
# for any target that has a Rust std-lib (or core for no_std targets).
# We use cargo check (not build) so no cross-linker is needed.

set -euo pipefail

# ---------------------------------------------------------------------------
# Target definitions
#
# Format: TARGET:GROUP:BUILD_DLL:ENV
#   TARGET    = rustup target triple
#   GROUP     = category tag for filtering
#   BUILD_DLL = "yes" if build-dll feature should be tested
#               (only makes sense for targets whose OS matches the host,
#                or where the platform-gated windowing code resolves)
#   ENV       = extra env vars for the check (e.g. MACOSX_DEPLOYMENT_TARGET=10.13)
#               use "-" for none
# ---------------------------------------------------------------------------

HOST_OS="$(uname -s)"
HOST_ARCH="$(uname -m)"

define_targets() {
    local targets=()

    # -- Linux targets (build-dll works when running on Linux) --
    local linux_dll="no"
    [[ "$HOST_OS" == "Linux" ]] && linux_dll="yes"

    targets+=(
        "i686-unknown-linux-gnu:linux-32bit:$linux_dll:-"
        "x86_64-unknown-linux-gnu:linux-64bit:$linux_dll:-"
        "aarch64-unknown-linux-gnu:linux-arm64:$linux_dll:-"
        "armv7-unknown-linux-gnueabihf:linux-arm32:$linux_dll:-"
        "riscv64gc-unknown-linux-gnu:linux-riscv:$linux_dll:-"
        "powerpc64-unknown-linux-gnu:linux-ppc64:$linux_dll:-"
        "powerpc64le-unknown-linux-gnu:linux-ppc64le:$linux_dll:-"
        "s390x-unknown-linux-gnu:linux-s390x:$linux_dll:-"
        "mips64-unknown-linux-gnuabi64:linux-mips64:$linux_dll:-"
        "loongarch64-unknown-linux-gnu:linux-loongarch:$linux_dll:-"
    )

    # -- Windows targets (build-dll works when running on Windows / native) --
    local win_dll="no"
    [[ "$HOST_OS" == *"MINGW"* || "$HOST_OS" == *"MSYS"* || "$HOST_OS" == *"NT"* ]] && win_dll="yes"

    targets+=(
        "i686-pc-windows-msvc:windows-32bit:$win_dll:-"
        "x86_64-pc-windows-msvc:windows-64bit:$win_dll:-"
        "i686-pc-windows-gnu:windows-32bit-gnu:$win_dll:-"
        "x86_64-pc-windows-gnu:windows-64bit-gnu:$win_dll:-"
        "aarch64-pc-windows-msvc:windows-arm64:$win_dll:-"
    )

    # -- macOS targets with retro deployment targets --
    # build-dll only works when running on macOS
    local mac_dll="no"
    [[ "$HOST_OS" == "Darwin" ]] && mac_dll="yes"

    targets+=(
        # High Sierra (2017) - oldest reasonable Intel macOS
        "x86_64-apple-darwin:macos-10.13:$mac_dll:MACOSX_DEPLOYMENT_TARGET=10.13"
        # Catalina (2019) - last macOS without Rosetta
        "x86_64-apple-darwin:macos-10.15:$mac_dll:MACOSX_DEPLOYMENT_TARGET=10.15"
        # Big Sur (2020) - first Apple Silicon macOS
        "aarch64-apple-darwin:macos-11.0:$mac_dll:MACOSX_DEPLOYMENT_TARGET=11.0"
        # Current (no deployment target override)
        "x86_64-apple-darwin:macos-x86-current:$mac_dll:-"
        "aarch64-apple-darwin:macos-arm64-current:$mac_dll:-"
    )

    # -- WASM (no windowing, sub-crates only) --
    targets+=(
        "wasm32-unknown-unknown:wasm:no:-"
    )

    # -- FreeBSD (tier 2, no build-dll) --
    targets+=(
        "x86_64-unknown-freebsd:freebsd:no:-"
    )

    # -- Android (no build-dll for now) --
    targets+=(
        "aarch64-linux-android:android-arm64:no:-"
        "armv7-linux-androideabi:android-arm32:no:-"
        "i686-linux-android:android-x86:no:-"
    )

    # -- iOS (no build-dll for now) --
    targets+=(
        "aarch64-apple-ios:ios-arm64:no:-"
    )

    printf '%s\n' "${targets[@]}"
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

PASS=0
FAIL=0
SKIP=0
FAILED_TARGETS=()

log_pass() { echo -e "  ${GREEN}PASS${NC} $1"; PASS=$((PASS + 1)); }
log_fail() { echo -e "  ${RED}FAIL${NC} $1"; FAIL=$((FAIL + 1)); FAILED_TARGETS+=("$1"); }
log_skip() { echo -e "  ${YELLOW}SKIP${NC} $1"; SKIP=$((SKIP + 1)); }
log_info() { echo -e "${CYAN}::${NC} $1"; }

ensure_codegen() {
    if [[ ! -f target/codegen/dll_api_build.rs ]]; then
        log_info "Generating DLL API bindings (first run)..."
        cargo run -r -p azul-doc codegen all
    fi
}

install_target() {
    local target="$1"
    if ! rustup target list --installed | grep -q "^${target}$"; then
        log_info "Installing target: $target"
        if ! rustup target add "$target" 2>/dev/null; then
            return 1
        fi
    fi
    return 0
}

check_crate() {
    local target="$1"
    local label="$2"
    local envvar="$3"
    shift 3
    local cmd=(cargo check --target "$target" "$@")
    if [[ "$envvar" != "-" ]]; then
        cmd=(env "$envvar" "${cmd[@]}")
    fi
    if "${cmd[@]}" 2>/dev/null; then
        log_pass "$label"
        return 0
    else
        log_fail "$label"
        return 1
    fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

QUICK=false
LIST_ONLY=false
FILTER_ARGS=()

for arg in "$@"; do
    case "$arg" in
        --quick) QUICK=true ;;
        --list)  LIST_ONLY=true ;;
        --help|-h)
            echo "Usage: $0 [--quick] [--list] [keyword ...]"
            echo ""
            echo "  --quick    Only check sub-crates (skip azul-dll)"
            echo "  --list     List all targets and exit"
            echo "  keyword    Filter targets by keyword (e.g. 'i686', 'arm', 'wasm')"
            echo ""
            echo "Examples:"
            echo "  $0                      # check everything"
            echo "  $0 i686 arm             # only 32-bit x86 and ARM targets"
            echo "  $0 wasm --quick         # WASM sub-crates only"
            echo "  $0 --list               # show all targets"
            exit 0
            ;;
        *)       FILTER_ARGS+=("$arg") ;;
    esac
done

ALL_TARGETS="$(define_targets)"

# Apply keyword filter
if [[ ${#FILTER_ARGS[@]} -gt 0 ]]; then
    FILTERED=""
    for line in $ALL_TARGETS; do
        for kw in "${FILTER_ARGS[@]}"; do
            if [[ "$line" == *"$kw"* ]]; then
                FILTERED+="$line"$'\n'
                break
            fi
        done
    done
    ALL_TARGETS="$FILTERED"
fi

if [[ "$LIST_ONLY" == true ]]; then
    echo "Available cross-check targets:"
    echo ""
    printf "%-45s %-22s %-10s %s\n" "TARGET" "GROUP" "BUILD-DLL" "ENV"
    printf '%.0s-' {1..110}; echo
    while IFS=: read -r target group dll envvar; do
        [[ -z "$target" ]] && continue
        [[ "$envvar" == "-" ]] && envvar=""
        printf "%-45s %-22s %-10s %s\n" "$target" "$group" "$dll" "$envvar"
    done <<< "$ALL_TARGETS"
    exit 0
fi

echo ""
echo "=== Azul Cross-Compilation Check ==="
echo "Host: $HOST_OS / $HOST_ARCH"
echo "Rust: $(rustc --version)"
echo ""

ensure_codegen

while IFS=: read -r target group check_dll envvar; do
    [[ -z "$target" ]] && continue
    [[ -z "$envvar" ]] && envvar="-"

    echo ""
    if [[ "$envvar" != "-" ]]; then
        log_info "Target: $target ($group) [$envvar]"
    else
        log_info "Target: $target ($group)"
    fi

    if ! install_target "$target"; then
        log_skip "$target (target not available for this toolchain)"
        continue
    fi

    # Sub-crates (always checked)
    check_crate "$target" "$target / azul-css" "$envvar" \
        -p azul-css

    check_crate "$target" "$target / azul-core" "$envvar" \
        -p azul-core

    check_crate "$target" "$target / azul-layout (text+svg+xml)" "$envvar" \
        -p azul-layout --no-default-features --features "text_layout,svg,xml"

    if [[ "$QUICK" == true ]]; then
        continue
    fi

    # azul-dll with link-static (always, no windowing code)
    check_crate "$target" "$target / azul-dll link-static" "$envvar" \
        -p azul-dll --no-default-features --features "link-static,logging"

    # azul-dll with build-dll (only on same-OS targets)
    if [[ "$check_dll" == "yes" ]]; then
        check_crate "$target" "$target / azul-dll build-dll" "$envvar" \
            -p azul-dll --features "build-dll"
    fi

done <<< "$ALL_TARGETS"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo ""
echo "==========================================="
echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}  ${YELLOW}SKIP: $SKIP${NC}"
echo "==========================================="

if [[ $FAIL -gt 0 ]]; then
    echo ""
    echo "Failed targets:"
    for t in "${FAILED_TARGETS[@]}"; do
        echo "  - $t"
    done
    exit 1
fi

exit 0
