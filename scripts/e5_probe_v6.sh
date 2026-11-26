#!/usr/bin/env bash
# Engine ledger E5, probe v6: the job's own build (azul-dll, feature dist, the job's cargo and toolchain) with ONE change: a short
# target directory (CARGO_TARGET_DIR=D:\t). With the new build-dir layout cargo passes rustc one `-L dependency=<target>\<triple>\
# release\build\<crate>\<hash>\out` per dependency unit, so the longest rustc command line grows with the length of the target path.
# Not part of the CI. Run through scripts/remote.sh on windows-2022:
#   scripts/remote.sh run <ref> windows-2022 'bash scripts/e5_probe_v6.sh' --codegen --artifacts 'e5/*'
set -u
export TZ=UTC
mkdir -p e5
stamp() { printf '%(%H:%M:%S)T' -1; }
echo "== e5 probe v6, $(date -u +%FT%TZ)"
mkdir -p dist rust9x/build/x86_64-pc-windows-msvc
curl -fsSL -o dist/rust9x.tar.gz https://github.com/fschutt/azul/releases/download/rust9x-toolchain/rust9x-stage1-x86_64-pc-windows-msvc.tar.gz || { echo "asset download failed"; exit 3; }
tar -xzf dist/rust9x.tar.gz -C rust9x/build/x86_64-pc-windows-msvc --exclude=stage1/lib/rustlib/rustc-src/rust --exclude=stage1/lib/rustlib/src/rust
rustup toolchain link rust9x "$(pwd)/rust9x/build/x86_64-pc-windows-msvc/stage1"
rustc +rust9x -vV | head -3

export CARGO_TARGET_DIR='D:\t'
echo "== the job's build with CARGO_TARGET_DIR=$CARGO_TARGET_DIR, $(stamp)"
( cargo +rust9x build --release --target i686-rust9x-windows-msvc -p azul-dll --features=dist 2>&1; echo "CARGO_EXIT=$?" ) \
  | while IFS= read -r line; do printf '%s %s\n' "$(stamp)" "$line"; done > e5/build.log &
bpid=$!
( while [ ! -f e5/done.flag ]; do sleep 90; echo "[heartbeat $(stamp)] crates: $(grep -c 'Compiling ' e5/build.log 2>/dev/null); last: $(grep 'Compiling ' e5/build.log 2>/dev/null | tail -n 1 | cut -c1-80)"; done ) &
t0=$SECONDS
while kill -0 "$bpid" 2>/dev/null; do
  sleep 15
  if [ $((SECONDS - t0)) -ge 3300 ]; then echo "== TIME BOX: stopping the build"; taskkill //F //IM cargo.exe >/dev/null 2>&1; taskkill //F //IM rustc.exe >/dev/null 2>&1; echo TIMEBOX >> e5/build.log; break; fi
done
wait "$bpid" 2>/dev/null
echo "== the build ended $(stamp) after $((SECONDS - t0)) s: $(grep -a 'CARGO_EXIT=' e5/build.log | tail -n 1)"
echo "== errors"
grep -aE "LoadLibraryExW|^[0-9:]+ error(\[E[0-9]+\])?: " e5/build.log | cut -c1-200 | head -20
echo "== the artifact"
ls -la 'D:/t/i686-rust9x-windows-msvc/release/' 2>/dev/null | grep -E "azul\.(dll|lib)" | head
grep -a "Finished" e5/build.log | tail -n 1
touch e5/done.flag
echo "== end $(stamp)"
