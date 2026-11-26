#!/usr/bin/env bash
# fetch_whistle.sh: the two files of the built-in speech recognizer, downloaded from the vendor and checked against
# the hashes pinned here, and the licence text that goes with them.
#
#   scripts/fetch_whistle.sh [<folder>] [--platform <wheel tag>]
#   scripts/fetch_whistle.sh --static [<folder>] [--platform <release folder>]
#
# <folder>   where the files go (default: speech); it is the folder `SpeechSession::set_builtin_files` is pointed at
#            (or a `speech` folder next to the executable, or a bundle's Resources: `builtin::default_places`)
# --platform the engine wheel's tag when it is not this machine's: macosx_11_0_arm64, macosx_11_0_x86_64,
#            manylinux2014_x86_64, manylinux2014_aarch64, musllinux_1_2_x86_64, musllinux_1_2_aarch64,
#            win_amd64, win_arm64
# --static   the engine as the vendor's STATIC library instead of the shared one: <folder>/static/libcactus.a, which
#            the cargo feature `speech-engine-static` of azul-dll links (a build of one's own only: no shipped build
#            has the feature; see dll/Cargo.toml). `--platform` then names the release's folder: macos-arm64,
#            ios-arm64, ios-sim-arm64, tvos-arm64, watchos-arm64, linux-x86_64, linux-arm64, linux-armv7,
#            linux-riscv64, linux-mipsel, android-arm64, android-armv7, android-riscv64, windows-x86_64,
#            windows-arm64 (no macOS on Intel, no musl, no Android on x86_64: the release has none; the Windows
#            ones are MinGW builds). The model and the licence text are fetched as for the shared library.
#
# What it fetches (Cactus Compute, Apache-2.0: the repository's LICENSE and the model card say so for the engine's
# prebuilt binaries and for the weights; the engine's source is not public, and no copyright holder is named in the
# LICENSE file - read dll/src/desktop/extra/speech/builtin/mod.rs before shipping):
#   libcactus.{dylib,so,dll}   the engine's shared library, out of the runtime wheel cactus_runtime-4.1.4 (13 C functions)
#                              (with --static: static/libcactus.a, the release's archive, instead)
#   whistle.cact               the model (16,919,407 bytes)
#   LICENSE-cactus.txt         the Apache-2.0 text of the engine's release, to be shipped beside the two files
# Nothing is fetched at build time and nothing from here is in the default build.
set -euo pipefail

ENGINE_REVISION="f4ab2741203a282c6e7b89983de2b5e5b2e805a8"   # Cactus-Compute/cactus, the 4.1.4 release
ENGINE_VERSION="4.1.4"
MODEL_REVISION="88cb00eeb8928ddbcd410b491584f938ba030b07"    # Cactus-Compute/whistle
MODEL_SHA256="b6e02f048568ac5d01a2042556c658061e699acbc0aa2a1439f52f3d461dffeb"
MODEL_BYTES=16919407
LICENSE_SHA256="cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"

wheel_sha256() {
  case "$1" in
    macosx_11_0_arm64)       echo "45eaae74b37291425bc8bedfc3fa0ff6984cf8ea1997d2ea1711387974d25e43" ;;
    macosx_11_0_x86_64)      echo "bb04050273486582ef8484bd5c95cba3a43cb2db4510970b1f97cc6432f464cd" ;;
    manylinux2014_aarch64)   echo "45b703ee71fd979d3155f945a733c1dd1f18637035314e9c96fdfd60ae96b787" ;;
    manylinux2014_x86_64)    echo "314cd027e95cae1cb8537c478ca6cddaec3fd02165f80d960f1934399871fc33" ;;
    musllinux_1_2_aarch64)   echo "f54163ee24baf7a5fef6e864bd440d3e6e87363932ed30ac56f676507e57d5bf" ;;
    musllinux_1_2_x86_64)    echo "78fd9d4ec5d1c091fd354afc6a0c4714b5fa1a3a2dda95019adc421374495e40" ;;
    win_amd64)               echo "76f21737d93a5246afe2bfee65a5a338df35217cb6258c7aa9cc747e6ea2bb73" ;;
    win_arm64)               echo "f8dae7aacb3912623814db25c2d336b5b2a1b00fb1eb96d630ef53899b156e07" ;;
    *) return 1 ;;
  esac
}

dest="speech"
tag=""
static_engine=0
while [ $# -gt 0 ]; do
  case "$1" in
    --platform) shift; tag="${1:?--platform needs a wheel tag (a release folder with --static)}" ;;
    --static) static_engine=1 ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option $1" >&2; exit 2 ;;
    *) dest="$1" ;;
  esac
  shift
done

static_sha256() {   # the release's folder -> the sha256 of its libcactus.a (the manifest of the 4.1.4 release)
  case "$1" in
    android-arm64)   echo "67dbd7b856a3e9d8b0fbb68f0c59592e7572cf859238ba7f3074f74b6fa9a5da" ;;
    android-armv7)   echo "6f7daade8587c714a2f46fdccf90535660b78d22e3c4a89518d53ea857568afa" ;;
    android-riscv64) echo "721e711b492a4dd7e3682ab0535f66f4731c20e42146ce840679e27d4c0be69e" ;;
    ios-arm64)       echo "18fab404a7ffefcea860af9aaeb39ee46b37800ddf2be82a72290ad8427ea3d4" ;;
    ios-sim-arm64)   echo "5b6cfc4c7280026541f84c93ce64d57ebd292abd2ea6f38f850ef53bb3b9f206" ;;
    linux-arm64)     echo "31ee6ce7b0b1390fc8ffa6d242d2aa3cdae18b405d7d885d483cd79258016dd0" ;;
    linux-armv7)     echo "4e82a8939596c20f7ac36a0f55624f98d334982adc2581c13793c84f6f5489a5" ;;
    linux-mipsel)    echo "568b090d630e6f7678d553c14f658ee277a47366ba8a99c01527d580df1eb6b9" ;;
    linux-riscv64)   echo "e399c408ac23e1879cce978f8d673b38ba7d8c57ae8d2f78d73a2c898d79a7d8" ;;
    linux-x86_64)    echo "a977d506ed9545369628cfe9fbf87747d73ab766ac376d8c1c098146485327c4" ;;
    macos-arm64)     echo "f00c3cf43e31db9399b7b1739146fa8ab293b6f3e71d7c7ab9c8a91042f8c5ef" ;;
    tvos-arm64)      echo "46670a390fca493d25bd5133c42b738d5c3eab6c344cd81d88672f0f86777d16" ;;
    watchos-arm64)   echo "198261b7f8e3ad001ba146c0b6367d146cee63113e3a133e38f0613cba05525a" ;;
    windows-arm64)   echo "d0a20fc405aa99e9b104b9ed4586781df06c2e01dfd51d35cb08731e9cb3939d" ;;
    windows-x86_64)  echo "013c73a2a68b0449be1a8e1a003f8d102da38e5460cccf0c62ea92fda37bc070" ;;
    *) return 1 ;;
  esac
}

if [ "$static_engine" = 1 ] && [ -z "$tag" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)  tag="macos-arm64" ;;
    Linux-x86_64)  tag="linux-x86_64" ;;
    Linux-aarch64) tag="linux-arm64" ;;
    MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) tag="windows-x86_64" ;;
    *) echo "the release has no static engine for $(uname -s)-$(uname -m): pass --platform" >&2; exit 2 ;;
  esac
fi
if [ "$static_engine" = 1 ]; then
  want="$(static_sha256 "$tag")" || { echo "the release has no static engine in a folder named $tag" >&2; exit 2; }
fi

if [ "$static_engine" = 0 ] && [ -z "$tag" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)  tag="macosx_11_0_arm64" ;;
    Darwin-x86_64) tag="macosx_11_0_x86_64" ;;
    Linux-x86_64)  tag="manylinux2014_x86_64" ;;
    Linux-aarch64) tag="manylinux2014_aarch64" ;;
    MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) tag="win_amd64" ;;
    *) echo "no engine wheel is known for $(uname -s)-$(uname -m): pass --platform" >&2; exit 2 ;;
  esac
fi
if [ "$static_engine" = 0 ]; then
  want="$(wheel_sha256 "$tag")" || { echo "unknown wheel tag $tag" >&2; exit 2; }
fi

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

fetch_checked() {   # url, file, sha256
  local url="$1" file="$2" sha="$3" got
  if [ -f "$file" ] && [ "$(sha256_of "$file")" = "$sha" ]; then
    echo "have   $file"
    return 0
  fi
  echo "fetch  $url"
  curl --fail --silent --show-error --location --output "$file.part" "$url"
  got="$(sha256_of "$file.part")"
  if [ "$got" != "$sha" ]; then
    echo "CHECKSUM MISMATCH for $url: expected $sha, got $got (the file is left as $file.part: delete it)" >&2
    exit 1
  fi
  mv "$file.part" "$file"
}

mkdir -p "$dest"

if [ "$static_engine" = 1 ]; then
  mkdir -p "$dest/static"
  fetch_checked "https://huggingface.co/Cactus-Compute/cactus/resolve/${ENGINE_REVISION}/releases/${ENGINE_VERSION}/${tag}/libcactus.a" \
    "$dest/static/libcactus.a" "$want"
  lib="static/libcactus.a"
  echo "wrote  $dest/$lib ($(wc -c < "$dest/$lib" | tr -d ' ') bytes)"
else
  # The wheel is unpacked in a temporary folder; nothing here deletes it (delete it when you like).
  work="$(mktemp -d)"
  trap 'echo "left: $work (the downloaded wheel; delete when you like)"' EXIT
  wheel="cactus_runtime-${ENGINE_VERSION}-py3-none-${tag}.whl"
  fetch_checked "https://huggingface.co/Cactus-Compute/cactus/resolve/${ENGINE_REVISION}/releases/${ENGINE_VERSION}/python/${wheel}" \
    "$work/$wheel" "$want"
  case "$tag" in
    macosx*) lib="libcactus.dylib" ;;
    win*)    lib="libcactus.dll" ;;
    *)       lib="libcactus.so" ;;
  esac
  unzip -o -q -j "$work/$wheel" "cactus/$lib" -d "$dest"
  chmod 755 "$dest/$lib"
  echo "wrote  $dest/$lib ($(wc -c < "$dest/$lib" | tr -d ' ') bytes)"
fi

fetch_checked "https://huggingface.co/Cactus-Compute/whistle/resolve/${MODEL_REVISION}/whistle.cact" "$dest/whistle.cact" "$MODEL_SHA256"
[ "$(wc -c < "$dest/whistle.cact" | tr -d ' ')" = "$MODEL_BYTES" ] || { echo "whistle.cact has the wrong size" >&2; exit 1; }
fetch_checked "https://huggingface.co/Cactus-Compute/cactus/resolve/${ENGINE_REVISION}/releases/${ENGINE_VERSION}/LICENSE" \
  "$dest/LICENSE-cactus.txt" "$LICENSE_SHA256"
if [ "$static_engine" = 1 ]; then
  echo "done: $dest holds $lib, whistle.cact and LICENSE-cactus.txt: for a build of one's own with the static engine (AZUL_CACTUS_LIB_DIR=$dest/static); no shipped build carries the engine, see dll/Cargo.toml"
else
  echo "done: $dest holds $lib, whistle.cact and LICENSE-cactus.txt (ship the three together)"
fi
