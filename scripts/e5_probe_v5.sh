#!/usr/bin/env bash
# Engine ledger E5, probe v5. v3: the job's full build fails deterministically at wr_azul_glyph_rasterizer with E0463 (can't find crate
# for webrender_api, azul_core, azul_layout, smallvec, euclid); v4: the same crate built alone (a small closure) compiles. The
# difference is the closure: with the new build-dir layout cargo gives rustc one `-L dependency=<dir>` per dependency unit, so a crate
# that depends on azul-layout with the features of the whole dll has a command line of tens of thousands of characters.
# This runs the job's own build (azul-dll, feature dist) with a rustc wrapper that records the length of every rustc command line
# (and whether cargo moved it into an @argfile) and the arguments and environment of the failing crate, then reruns that command by hand.
# Not part of the CI. Run through scripts/remote.sh on windows-2022:
#   scripts/remote.sh run <ref> windows-2022 'bash scripts/e5_probe_v5.sh' --codegen --artifacts 'e5/*'
set -u
export TZ=UTC
mkdir -p e5
stamp() { printf '%(%H:%M:%S)T' -1; }
echo "== e5 probe v5, $(date -u +%FT%TZ)"

echo "== the toolchain (the job's steps: download the asset, unpack, link)"
mkdir -p dist rust9x/build/x86_64-pc-windows-msvc
curl -fsSL -o dist/rust9x.tar.gz https://github.com/fschutt/azul/releases/download/rust9x-toolchain/rust9x-stage1-x86_64-pc-windows-msvc.tar.gz || { echo "asset download failed"; exit 3; }
tar -xzf dist/rust9x.tar.gz -C rust9x/build/x86_64-pc-windows-msvc --exclude=stage1/lib/rustlib/rustc-src/rust --exclude=stage1/lib/rustlib/src/rust
rustup toolchain link rust9x "$(pwd)/rust9x/build/x86_64-pc-windows-msvc/stage1"
rustc +rust9x -vV | head -5
cargo --version 2>&1 | head -2; ls "$HOME/.rustup/fallback" 2>/dev/null | head -3
"$HOME/.rustup/fallback/cargo.exe" --version 2>&1 | head -2

echo "== the rustc wrapper"
cat > e5/wrap.rs <<'RS'
use std::fs::OpenOptions;
use std::io::Write;
use std::process::{exit, Command};

/// The arguments with every `@file` replaced by the lines of the file (how rustc reads it).
fn expand(args: &[String]) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    let mut used = false;
    for a in args {
        if let Some(path) = a.strip_prefix('@') {
            if let Ok(text) = std::fs::read_to_string(path) {
                used = true;
                out.extend(text.lines().map(|l| l.to_string()));
                continue;
            }
        }
        out.push(a.clone());
    }
    (out, used)
}

fn main() {
    let all: Vec<String> = std::env::args().skip(1).collect();
    if all.is_empty() {
        eprintln!("wrap: no rustc given");
        exit(2);
    }
    let rustc = &all[0];
    let rest = &all[1..];
    let dir = std::env::var("E5_DIR").unwrap_or_else(|_| "e5".to_string());
    let (expanded, argfile) = expand(rest);
    let name = expanded
        .windows(2)
        .find(|w| w[0] == "--crate-name")
        .map(|w| w[1].clone())
        .unwrap_or_default();
    let len: usize = expanded.iter().map(|a| a.len() + 1).sum();
    let direct: usize = rest.iter().map(|a| a.len() + 1).sum();
    let ls = expanded.iter().filter(|a| *a == "-L").count();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(format!("{dir}/cmdlens.txt")) {
        let _ = writeln!(f, "{len:7} chars {:4} args {ls:4} -L direct={direct:6} argfile={argfile} crate={name}", expanded.len());
    }
    if name == "wr_azul_glyph_rasterizer" {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(format!("{dir}/rustc_args.txt")) {
            let _ = writeln!(f, "=== ARGS");
            let _ = writeln!(f, "{rustc}");
            for a in &expanded {
                let _ = writeln!(f, "{a}");
            }
            let _ = writeln!(f, "=== END");
            let _ = writeln!(f, "=== RAW (what cargo gave the wrapper after the rustc path)");
            for a in rest.iter().take(8) {
                let _ = writeln!(f, "{a}");
            }
            let _ = writeln!(f, "=== ENV");
            for (k, v) in std::env::vars_os() {
                let _ = writeln!(f, "{}={}", k.to_string_lossy(), v.to_string_lossy());
            }
            let _ = writeln!(f, "=== ENVEND");
        }
    }
    match Command::new(rustc).args(rest).status() {
        Ok(s) => {
            if name == "wr_azul_glyph_rasterizer" {
                if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(format!("{dir}/cmdlens.txt")) {
                    let _ = writeln!(f, "glyph rustc exit {:?}", s.code());
                }
            }
            exit(s.code().unwrap_or(1))
        }
        Err(e) => {
            eprintln!("wrap: cannot run {rustc}: {e}");
            exit(1)
        }
    }
}
RS
rustc -O e5/wrap.rs -o e5/wrap.exe || { echo "the wrapper did not compile"; exit 4; }
export E5_DIR="$(cygpath -w "$(pwd)/e5")"
export RUSTC_WRAPPER="$(cygpath -w "$(pwd)/e5/wrap.exe")"

echo "== the job's build, $(stamp)"
( cargo +rust9x build --release --target i686-rust9x-windows-msvc -p azul-dll --features=dist 2>&1; echo "CARGO_EXIT=$?" ) \
  | while IFS= read -r line; do printf '%s %s\n' "$(stamp)" "$line"; done > e5/build.log &
bpid=$!
( while [ ! -f e5/done.flag ]; do sleep 90; echo "[heartbeat $(stamp)] crates: $(grep -c 'Compiling ' e5/build.log 2>/dev/null); longest command line so far: $(sort -rn e5/cmdlens.txt 2>/dev/null | head -n 1 | cut -c1-120)"; done ) &
t0=$SECONDS
while kill -0 "$bpid" 2>/dev/null; do
  sleep 15
  if [ $((SECONDS - t0)) -ge 3000 ]; then echo "== TIME BOX: stopping the build"; taskkill //F //IM cargo.exe >/dev/null 2>&1; taskkill //F //IM rustc.exe >/dev/null 2>&1; break; fi
done
wait "$bpid" 2>/dev/null
echo "== the build ended $(stamp) after $((SECONDS - t0)) s: $(grep -a 'CARGO_EXIT=' e5/build.log | tail -n 1)"
unset RUSTC_WRAPPER

echo "== the longest rustc command lines of the build (chars, args, -L flags, bytes given to the wrapper, argfile?)"
sort -rn e5/cmdlens.txt | grep -v '^glyph' | head -12
echo "-- how many invocations needed an argfile: $(grep -c 'argfile=true' e5/cmdlens.txt) of $(grep -c ' chars ' e5/cmdlens.txt)"
echo "-- the glyph crate: $(grep 'crate=wr_azul_glyph_rasterizer' e5/cmdlens.txt | head -n 3 | tr '\n' '|')"
grep '^glyph' e5/cmdlens.txt

echo "== errors of the build"
grep -aE "LoadLibraryExW|^[0-9:]+ error(\[E[0-9]+\])?: " e5/build.log | cut -c1-200 | head -20

if [ -s e5/rustc_args.txt ]; then
  awk '/^=== ARGS$/{n++; next} /^=== END$/{if (n == 1) exit} n == 1 {print}' e5/rustc_args.txt > e5/args1.txt
  awk '/^=== ENV$/{n++; next} /^=== ENVEND$/{if (n == 1) exit} n == 1 {print}' e5/rustc_args.txt > e5/env1.txt
  rustc_exe="$(head -n 1 e5/args1.txt)"
  tail -n +2 e5/args1.txt > e5/glyph.rsp
  echo "== the failing crate: $(wc -l < e5/glyph.rsp) args, $(awk '{ n += length($0) + 1 } END { print n }' e5/glyph.rsp) chars, $(grep -c '^-L$' e5/glyph.rsp) -L flags, $(grep -c '^--extern$' e5/glyph.rsp) externs"
  echo "== V0: that command by hand, as an @argfile (rustc reads it the way cargo's argfile is read)"
  "$rustc_exe" @e5/glyph.rsp > e5/v0.out 2>&1; code=$?
  echo "V0 exit $code; E0463 lines: $(grep -c 'E0463' e5/v0.out)"
  grep -o '"message":"[^"]*"' e5/v0.out | sort | uniq -c | sort -rn | head -10

  echo "== V1: the externs first, then the search paths (the order of the flags)"
  awk 'BEGIN{i=0} {a[i++]=$0} END{ for (j=0;j<i;j++) if (a[j]=="--extern") { print a[j]; print a[j+1] } ; for (j=0;j<i;j++) { if (a[j]=="--extern") { j++; continue } print a[j] } }' e5/glyph.rsp > e5/glyph_ext_first.rsp
  "$rustc_exe" @e5/glyph_ext_first.rsp > e5/v1.out 2>&1; echo "V1 exit $?; E0463 lines: $(grep -c 'E0463' e5/v1.out)"

  echo "== V2: only the search paths the failing crate's externs live in (a short command line)"
  awk 'BEGIN{i=0} {a[i++]=$0} END{ for (j=0;j<i;j++) { if (a[j]=="-L") { j++; continue } print a[j] } }' e5/glyph.rsp > e5/glyph_noL.rsp
  "$rustc_exe" @e5/glyph_noL.rsp > e5/v2.out 2>&1; echo "V2 (no -L at all) exit $?; E0463 lines: $(grep -c 'E0463' e5/v2.out); $(grep -o '"message":"[^"]*"' e5/v2.out | sort | uniq -c | sort -rn | head -3 | tr '\n' '|')"

  echo "== the same crate again through cargo, with nothing changed (cargo's own argfile path), without the wrapper"
  ( cargo +rust9x build --release --target i686-rust9x-windows-msvc -p azul-dll --features=dist 2>&1; echo "CARGO_EXIT=$?" ) \
    | while IFS= read -r line; do printf '%s %s\n' "$(stamp)" "$line"; done > e5/build2.log
  grep -aE "CARGO_EXIT=|^[0-9:]+ error(\[E[0-9]+\])?: " e5/build2.log | cut -c1-200 | head -10
fi
touch e5/done.flag
echo "== end $(stamp)"
