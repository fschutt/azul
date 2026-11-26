#!/usr/bin/env bash
# Engine ledger E5, probe v4. Probe v3 found no LoadLibraryExW error but a deterministic E0463 ("can't find crate for webrender_api,
# azul_core, azul_layout, smallvec, euclid") at wr_azul_glyph_rasterizer, on the first and on two later attempts (the second with one
# job on an idle machine), after 565 crates had compiled. This builds just that crate with a rustc wrapper that records the command line
# and environment cargo gives rustc, reruns that command outside cargo, and compiles one-line crates against each missing dependency.
# Not part of the CI. Run through scripts/remote.sh on windows-2022:
#   scripts/remote.sh run <ref> windows-2022 'bash scripts/e5_probe_v4.sh' --artifacts 'e5/*'
set -u
export TZ=UTC
mkdir -p e5
stamp() { printf '%(%H:%M:%S)T' -1; }
echo "== e5 probe v4, $(date -u +%FT%TZ)"

echo "== the toolchain (the job's steps: download the asset, unpack, link)"
mkdir -p dist rust9x/build/x86_64-pc-windows-msvc
curl -fsSL -o dist/rust9x.tar.gz https://github.com/fschutt/azul/releases/download/rust9x-toolchain/rust9x-stage1-x86_64-pc-windows-msvc.tar.gz || { echo "asset download failed"; exit 3; }
tar -xzf dist/rust9x.tar.gz -C rust9x/build/x86_64-pc-windows-msvc --exclude=stage1/lib/rustlib/rustc-src/rust --exclude=stage1/lib/rustlib/src/rust
rustup toolchain link rust9x "$(pwd)/rust9x/build/x86_64-pc-windows-msvc/stage1"
rustc +rust9x -vV | head -5

echo "== the rustc wrapper"
cat > e5/wrap.rs <<'RS'
use std::io::Write;
use std::process::{exit, Command};

fn main() {
    let all: Vec<String> = std::env::args().skip(1).collect();
    if all.is_empty() {
        eprintln!("wrap: no rustc given");
        exit(2);
    }
    let rustc = &all[0];
    let rest = &all[1..];
    let dir = std::env::var("E5_DIR").unwrap_or_else(|_| "e5".to_string());
    let target = rest.windows(2).any(|w| w[0] == "--crate-name" && w[1] == "wr_azul_glyph_rasterizer");
    if target {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(format!("{dir}/rustc_args.txt")) {
            let _ = writeln!(f, "=== ARGS");
            for a in &all {
                let _ = writeln!(f, "{a}");
            }
            let _ = writeln!(f, "=== END");
            let _ = writeln!(f, "=== ENV");
            for (k, v) in std::env::vars_os() {
                let _ = writeln!(f, "{}={}", k.to_string_lossy(), v.to_string_lossy());
            }
            let _ = writeln!(f, "=== ENVEND");
        }
    }
    match Command::new(rustc).args(rest).status() {
        Ok(s) => exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("wrap: cannot run {rustc}: {e}");
            exit(1)
        }
    }
}
RS
rustc -O e5/wrap.rs -o e5/wrap.exe || { echo "the wrapper did not compile"; exit 4; }
export E5_DIR="$(cygpath -w "$(pwd)/e5")"

build() { # name, wrapper(1/0)
  local name="$1" wrapped="$2"
  echo "== build $name begins $(stamp)"
  echo "BUILD $name" >> e5/build.log
  if [ "$wrapped" = 1 ]; then export RUSTC_WRAPPER="$(cygpath -w "$(pwd)/e5/wrap.exe")"; else unset RUSTC_WRAPPER; fi
  ( cargo +rust9x build --release --target i686-rust9x-windows-msvc -p wr_azul_glyph_rasterizer 2>&1; echo "CARGO_EXIT=$?" ) \
    | while IFS= read -r line; do printf '%s %s\n' "$(stamp)" "$line"; done >> e5/build.log
  echo "== build $name ended $(stamp): $(grep -a 'CARGO_EXIT=' e5/build.log | tail -n 1)"
}

build wrapped 1

echo "== the failing command, as cargo gave it to rustc"
if [ ! -s e5/rustc_args.txt ]; then echo "(the wrapper saw no wr_azul_glyph_rasterizer compile: the build did not get there)"; fi
awk '/^=== ARGS$/{n++; next} /^=== END$/{if (n == 1) exit} n == 1 {print}' e5/rustc_args.txt > e5/args1.txt
awk '/^=== ENV$/{n++; next} /^=== ENVEND$/{if (n == 1) exit} n == 1 {print}' e5/rustc_args.txt > e5/env1.txt
mapfile -t args < e5/args1.txt
echo "args: ${#args[@]}"; cut -c1-300 e5/args1.txt | head -60
echo "-- env names (values are in e5/env1.txt):"; cut -d= -f1 e5/env1.txt | grep -iE '^(RUST|CARGO|PATH|TEMP|TMP|LIB|INCLUDE)' | tr '\n' ' '; echo

if [ "${#args[@]}" -gt 2 ]; then
  echo "== V0: the same command, run by hand (not by cargo)"
  "${args[@]}" > e5/v0.out 2>&1; code=$?
  echo "V0 exit $code; E0463 lines: $(grep -c 'E0463' e5/v0.out); first lines:"
  grep -o '"message":"[^"]*"' e5/v0.out | head -8

  echo "== the externs and the search paths of that command"
  declare -A ext
  libs=()
  for ((i = 0; i < ${#args[@]}; i++)); do
    case "${args[i]}" in
      --extern) v="${args[i+1]}"; ext["${v%%=*}"]="${v#*=}";;
      -L) libs+=(-L "${args[i+1]}");;
    esac
  done
  for n in "${!ext[@]}"; do
    p="${ext[$n]}"
    echo "extern $n -> $p"
    ls -la "$(cygpath -u "$p")" 2>&1 | head -2
  done
  echo "search paths: ${libs[*]}"

  echo "== V1: a one-line crate against each extern of the failing crate, by itself"
  tgtflag=(--target i686-rust9x-windows-msvc)
  for n in "${!ext[@]}"; do
    printf 'extern crate %s as dep;\n' "$n" > "e5/t_$n.rs"
    rustc +rust9x --edition 2021 --crate-type lib --crate-name "t_$n" "e5/t_$n.rs" --emit=metadata -o "e5/t_$n.rmeta" "${tgtflag[@]}" "${libs[@]}" --extern "$n=${ext[$n]}" > "e5/t_$n.out" 2>&1
    echo "V1 $n: exit $? ; E0463 lines: $(grep -c 'E0463' "e5/t_$n.out"); $(head -c 200 "e5/t_$n.out" | tr '\n' ' ')"
  done
  echo "== V2: the same, with no search path at all (just the extern)"
  for n in smallvec euclid; do
    if [ -n "${ext[$n]:-}" ]; then
      rustc +rust9x --edition 2021 --crate-type lib --crate-name "u_$n" "e5/t_$n.rs" --emit=metadata -o "e5/u_$n.rmeta" "${tgtflag[@]}" --extern "$n=${ext[$n]}" > "e5/u_$n.out" 2>&1
      echo "V2 $n: exit $? ; E0463 lines: $(grep -c 'E0463' "e5/u_$n.out"); $(head -c 200 "e5/u_$n.out" | tr '\n' ' ')"
    fi
  done
fi

echo "== the files of the failing crate's dependencies"
ls -la target/i686-rust9x-windows-msvc/release/deps 2>/dev/null | grep -E "libsmallvec|libeuclid|libwebrender_api|libazul_core|libazul_layout|librayon-|liblog-" | cut -c1-200

echo "== build again, without the wrapper (only the failing crate is left to compile)"
build plain 0

echo "== SUMMARY"
grep -aE "CARGO_EXIT=|^[0-9:]+ error(\[E[0-9]+\])?: " e5/build.log | cut -c1-200 | head -40
touch e5/done.flag
echo "== end $(stamp)"
