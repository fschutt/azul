#!/usr/bin/env bash
# Build remill as a static lib via its CMakePresets + Trail of Bits'
# cxx-common pre-built dependency bundle.
#
# Output: $REMILL_INSTALL_DIR/install — point experiments/transpile-blueprint
# at this dir via `REMILL_INSTALL_DIR=... cargo build --features remill`.
#
# Idempotent: the cxx-common download is cached by URL, the cmake build
# tree is reused if already configured.
#
# Required:
#   - cmake, ninja, git (third_party/remill is a submodule)
#   - macOS: brew install cmake ninja
#   - Linux: apt-get install cmake ninja-build
#
# Env vars (optional):
#   REMILL_INSTALL_DIR  default: third_party/remill-install
#   CXX_COMMON_DIR      default: third_party/cxx-common
#   CXX_COMMON_VERSION  default: v0.6.10
#
# macOS gotcha: Apple's /Library/Developer/CommandLineTools has libc++
# headers in $(xcrun --show-sdk-path)/usr/include/c++/v1/ but only
# cxxabi.h in /Library/Developer/CommandLineTools/usr/include/c++/v1/.
# The embedded Ghidra/sleigh subbuild uses /usr/bin/c++ which only
# searches the latter and fails on `#include <cstdint>`. We patch
# CXXFLAGS with an explicit -isystem to the SDK headers.

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
REMILL_SRC="${REPO_ROOT}/third_party/remill"
REMILL_INSTALL_DIR="${REMILL_INSTALL_DIR:-${REPO_ROOT}/third_party/remill-install}"
CXX_COMMON_DIR="${CXX_COMMON_DIR:-${REPO_ROOT}/third_party/cxx-common}"
CXX_COMMON_VERSION="${CXX_COMMON_VERSION:-v0.6.10}"

UNAME_S=$(uname -s)
UNAME_M=$(uname -m)

case "${UNAME_S}-${UNAME_M}" in
    Darwin-arm64)
        CXX_COMMON_ASSET="vcpkg_macos-13_llvm-17-liftingbits-llvm_xcode-15.0_arm64.tar.xz"
        CXX_COMMON_SUBDIR="vcpkg_macos-13_llvm-17-liftingbits-llvm_xcode-15.0_arm64"
        VCPKG_TARGET_TRIPLET="arm64-osx-rel"
        CMAKE_PRESET="vcpkg-arm64-rel"
        BUILD_PRESET="arm64-rel"
        VCPKG_ARCH="arm64"
        ;;
    Darwin-x86_64)
        CXX_COMMON_ASSET="vcpkg_macos-13_llvm-17-liftingbits-llvm_xcode-15.0_amd64.tar.xz"
        CXX_COMMON_SUBDIR="vcpkg_macos-13_llvm-17-liftingbits-llvm_xcode-15.0_amd64"
        VCPKG_TARGET_TRIPLET="x64-osx-rel"
        CMAKE_PRESET="vcpkg-x64-rel"
        BUILD_PRESET="x64-rel"
        VCPKG_ARCH="x64"
        ;;
    Linux-x86_64)
        CXX_COMMON_ASSET="vcpkg_ubuntu-22.04_llvm-17-liftingbits-llvm_amd64.tar.xz"
        CXX_COMMON_SUBDIR="vcpkg_ubuntu-22.04_llvm-17-liftingbits-llvm_amd64"
        VCPKG_TARGET_TRIPLET="x64-linux-rel"
        CMAKE_PRESET="vcpkg-x64-rel"
        BUILD_PRESET="x64-rel"
        VCPKG_ARCH="x64"
        ;;
    Linux-aarch64)
        CXX_COMMON_ASSET="vcpkg_ubuntu-22.04_llvm-17-liftingbits-llvm_arm64.tar.xz"
        CXX_COMMON_SUBDIR="vcpkg_ubuntu-22.04_llvm-17-liftingbits-llvm_arm64"
        VCPKG_TARGET_TRIPLET="arm64-linux-rel"
        CMAKE_PRESET="vcpkg-arm64-rel"
        BUILD_PRESET="arm64-rel"
        VCPKG_ARCH="arm64"
        ;;
    *)
        echo "build_remill.sh: unsupported host ${UNAME_S}-${UNAME_M}" >&2
        echo "Supported: Darwin-arm64, Darwin-x86_64, Linux-x86_64, Linux-aarch64" >&2
        exit 1
        ;;
esac

CXX_COMMON_URL="https://github.com/lifting-bits/cxx-common/releases/download/${CXX_COMMON_VERSION}/${CXX_COMMON_ASSET}"

if [ ! -d "${REMILL_SRC}/.git" ] && [ ! -f "${REMILL_SRC}/CMakeLists.txt" ]; then
    echo "build_remill.sh: third_party/remill not initialized. Run:" >&2
    echo "    git submodule update --init --recursive third_party/remill" >&2
    exit 1
fi

# ── patches: applied to the submodule before configure ──
PATCH_DIR="${REPO_ROOT}/scripts/remill-patches"
if [ -d "${PATCH_DIR}" ]; then
    for patch in "${PATCH_DIR}"/*.patch; do
        [ -f "${patch}" ] || continue
        # `git apply --check` is idempotent — skip if already applied.
        if (cd "${REMILL_SRC}" && git apply --check --reverse "${patch}" 2>/dev/null); then
            echo "[remill] patch already applied: $(basename "${patch}")"
        else
            echo "[remill] applying: $(basename "${patch}")"
            (cd "${REMILL_SRC}" && git apply "${patch}")
        fi
    done
fi

# ── cxx-common (Trail of Bits pre-built dependency bundle) ──
if [ ! -d "${CXX_COMMON_DIR}/${CXX_COMMON_SUBDIR}" ]; then
    echo "[remill] downloading cxx-common ${CXX_COMMON_VERSION} (${CXX_COMMON_ASSET})"
    mkdir -p "${CXX_COMMON_DIR}"
    cd "${CXX_COMMON_DIR}"
    if [ ! -f "${CXX_COMMON_ASSET}" ]; then
        curl -fL -o "${CXX_COMMON_ASSET}" "${CXX_COMMON_URL}"
    fi
    echo "[remill] extracting cxx-common (~1 GB → ~6 GB)"
    tar -xJf "${CXX_COMMON_ASSET}"
    rm -f "${CXX_COMMON_ASSET}"
fi

VCPKG_ROOT="${CXX_COMMON_DIR}/${CXX_COMMON_SUBDIR}"
CMAKE_TOOLCHAIN_FILE="${VCPKG_ROOT}/scripts/buildsystems/vcpkg.cmake"

# ── CXXFLAGS gymnastics for macOS CommandLineTools libc++ headers ──
EXTRA_CXXFLAGS=""
if [ "${UNAME_S}" = "Darwin" ]; then
    SDKROOT=$(xcrun --show-sdk-path)
    if [ ! -f "/Library/Developer/CommandLineTools/usr/include/c++/v1/cstdint" ] \
            && [ -f "${SDKROOT}/usr/include/c++/v1/cstdint" ]; then
        echo "[remill] working around macOS CLT missing libc++ headers"
        echo "[remill]   pointing at ${SDKROOT}/usr/include/c++/v1"
        EXTRA_CXXFLAGS="-isystem ${SDKROOT}/usr/include/c++/v1"
        export SDKROOT
    fi
fi

# ── configure + build ──
cd "${REMILL_SRC}"
mkdir -p "${REMILL_INSTALL_DIR}"

echo "[remill] configure (preset ${CMAKE_PRESET})"
CXXFLAGS="${CXXFLAGS:-} ${EXTRA_CXXFLAGS}" \
CFLAGS="${CFLAGS:-}" \
VCPKG_ROOT="${VCPKG_ROOT}" \
CMAKE_TOOLCHAIN_FILE="${CMAKE_TOOLCHAIN_FILE}" \
VCPKG_TARGET_TRIPLET="${VCPKG_TARGET_TRIPLET}" \
INSTALL_DIR="${REMILL_INSTALL_DIR}" \
VCPKG_ARCH="${VCPKG_ARCH}" \
cmake --preset "${CMAKE_PRESET}"

echo "[remill] build (preset ${BUILD_PRESET}) — this takes 30-60 min"
CXXFLAGS="${CXXFLAGS:-} ${EXTRA_CXXFLAGS}" \
CFLAGS="${CFLAGS:-}" \
VCPKG_ROOT="${VCPKG_ROOT}" \
CMAKE_TOOLCHAIN_FILE="${CMAKE_TOOLCHAIN_FILE}" \
VCPKG_TARGET_TRIPLET="${VCPKG_TARGET_TRIPLET}" \
INSTALL_DIR="${REMILL_INSTALL_DIR}" \
VCPKG_ARCH="${VCPKG_ARCH}" \
cmake --build --preset "${BUILD_PRESET}"

echo "[remill] install → ${REMILL_INSTALL_DIR}/install"
cmake --install "${REMILL_INSTALL_DIR}/build/remill"

echo ""
echo "[remill] done. Point the blueprint at it:"
echo "    cd experiments/transpile-blueprint"
echo "    REMILL_INSTALL_DIR=${REMILL_INSTALL_DIR}/install \\"
echo "        cargo run --release --features remill"
