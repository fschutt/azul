#!/usr/bin/env bash
#
# Fast compile check of the feature combinations that CI builds but the
# everyday test battery never compiles.
#
# WHY THIS EXISTS: `cargo test -p azul-dll --lib` uses DEFAULT features.
# Whole subsystems (pdf, a11y, video, …) are feature-gated and therefore
# never type-checked by that run — code inside them can be broken for days
# while every local gate stays green. That happened: K30c's
# AZ_PAGINATION_ENGINE=tokens branch in dll/src/desktop/extra/pdf/mod.rs
# `return Ok(..)`-ed from a `-> Vec<u8>` function, breaking CI's
# `build-dll` check (and every downstream app enabling `pdf`) from the
# moment it landed. Nothing local noticed until an app turned the feature
# on.
#
# This is `cargo check` only (no codegen, no linking), so it is cheap
# enough to run before a push. build_all.sh remains the heavyweight
# verification; this is the fast pre-push tripwire.
#
# Usage: ./scripts/check_feature_matrix.sh
# Exit:  0 = every combination compiles, 1 = at least one failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "${SCRIPT_DIR}")"
cd "${PROJECT_ROOT}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Cap parallelism: this box has hard RAM/disk locks (see
# memory/azul-machine-build-safety.md).
JOBS="${AZ_CHECK_JOBS:-2}"
TIMEOUT="${AZ_CHECK_TIMEOUT:-570}"

FAILED=0
PASSED=0

check() {
    local name="$1"
    shift
    echo -e "${YELLOW}[CHECK]${NC} ${name}"
    if timeout "${TIMEOUT}" cargo check -j "${JOBS}" "$@" >/tmp/az_feature_check.log 2>&1; then
        echo -e "${GREEN}[PASS]${NC}  ${name}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}[FAIL]${NC}  ${name}"
        grep -E '^error' -A6 /tmp/az_feature_check.log | head -30
        FAILED=$((FAILED + 1))
    fi
}

# The full functional surface CI ships as libazul — the combination that
# regressed. Covers pdf, a11y, icons, svg, xml, icu, fluent, http,
# map-tiles, db-sqlite, video-native.
check "azul-dll --features build-dll" -p azul-dll --features build-dll

# What an APP links (examples, miniword): the static-link surface.
check "azul-dll --features link-static" \
    -p azul-dll --no-default-features --features link-static

# Apps that additionally want DOM->PDF export.
check "azul-dll --features link-static,pdf" \
    -p azul-dll --no-default-features --features link-static,pdf

# azul-layout feature islands: each must compile ALONE (a feature that
# silently depends on another one's modules is a latent break for any
# consumer that enables only the first — cpurender/xml was exactly that).
check "azul-layout --features text_layout" \
    -p azul-layout --no-default-features --features std,text_layout
check "azul-layout --features font_loading,text_layout" \
    -p azul-layout --no-default-features --features std,font_loading,text_layout
check "azul-layout --features cpurender" \
    -p azul-layout --no-default-features --features std,font_loading,text_layout,cpurender
check "azul-layout --features widgets" \
    -p azul-layout --no-default-features --features std,font_loading,text_layout,widgets
check "azul-layout --features xml" \
    -p azul-layout --no-default-features --features std,text_layout,xml

echo ""
echo -e "${GREEN}Passed:${NC} ${PASSED}   ${RED}Failed:${NC} ${FAILED}"
if [[ ${FAILED} -gt 0 ]]; then
    echo -e "${RED}Feature matrix BROKEN — a gated subsystem does not compile.${NC}"
    exit 1
fi
echo -e "${GREEN}Feature matrix clean.${NC}"
exit 0
