#!/usr/bin/env bash
# relift-one.sh — reproduce ONE function's remill lift locally, WITH the same
# `--extra_data` the transpiler passes, in seconds instead of a 40-minute relift.
#
# Why this exists: every lifter crash / `__remill_error` found so far is reachable
# ONLY through jump-table devirtualization, which only happens when `--extra_data`
# supplies the .rdata offset tables. Lifting the raw function bytes alone always
# succeeds, so the bytes are NOT a reproducer — the bytes PLUS extra_data are.
#
# Usage: relift-one.sh <native_addr_hex> <size_dec> [out.ll]
#   e.g. relift-one.sh 0x00007ffa2cb6b9c0 12384
#
# Address model (re-derive MODULE_BASE per build — see the memory note):
#   RVA   = native - MODULE_BASE
#   VMA   = 0x180000000 + RVA                (what llvm-objdump prints)
#   file  = RVA - 0xC00   (.text)  |  RVA - 0x1000 (.rdata)
#   synth = RVA + SYNTH_BIAS                 (what remill lifts at)
set -o pipefail
cd /c/Users/felix/Development/azul

DLL=examples/c/azul.dll
OBJ=third_party/remill/dependencies/install/bin/llvm-objdump.exe
LIFT=/c/rb/remill/bin/lift/remill-lift-17.exe
MODULE_BASE=0x7FFD0BFB0000
SYNTH_BIAS=0x10F000
TEXT_LO=0x1000; TEXT_HI=0x147AA9A       # RVA range of .text
RDATA_LO=0x147A000; RDATA_HI=0x1E807D0  # RVA range of .rdata

NATIVE=$1; SIZE=$2; OUT=${3:-/c/rb/repro/one.ll}
[ -z "$SIZE" ] && { echo "usage: relift-one.sh <native_hex> <size> [out.ll]"; exit 2; }
mkdir -p /c/rb/repro

RVA=$(( NATIVE - MODULE_BASE ))
VMA=$(( 0x180000000 + RVA ))
FOFF=$(( RVA - 0xC00 ))
SYNTH=$(( RVA + SYNTH_BIAS ))
printf "native=0x%x RVA=0x%x VMA=0x%x file=0x%x synth=0x%x size=%d\n" \
  "$NATIVE" "$RVA" "$VMA" "$FOFF" "$SYNTH" "$SIZE"

dd if="$DLL" of=/c/rb/repro/one.bin bs=1 skip=$FOFF count=$SIZE 2>/dev/null

# Collect every RIP-relative target llvm-objdump resolves ("# 0x...") and mirror a
# 128-byte window at each (the transpiler's LEA_MIRROR_WINDOW), at its synth address.
"$OBJ" -d --start-address=$VMA --stop-address=$(( VMA + SIZE )) "$DLL" 2>/dev/null > /c/rb/repro/one.asm
/c/Users/felix/tools/node/node.exe scripts/m9_e2e/extradata.mjs \
  "$DLL" /c/rb/repro/one.asm /c/rb/repro/one.extra || exit 1
EXTRA=$(cat /c/rb/repro/one.extra)
BYTES=$(xxd -p /c/rb/repro/one.bin | tr -d '\n')

# gflags --flagfile: one --flag=value per line (args are far past the cmdline limit).
# Paths inside the flagfile are read LITERALLY by remill — MSYS does not rewrite
# them the way it rewrites argv — so they must be Windows-style.
WOUT=$(cygpath -w "$OUT" 2>/dev/null | sed 's#\\#/#g'); WOUT=${WOUT:-$OUT}
FF=/c/rb/repro/one.flags
{ echo "--arch=amd64"; echo "--os=windows";
  printf -- "--address=0x%x\n" "$SYNTH"; printf -- "--entry_address=0x%x\n" "$SYNTH";
  echo "--bytes=$BYTES"; echo "--extra_data=$EXTRA"; echo "--ir_out=$WOUT"; } > "$FF"

"$LIFT" --flagfile="$FF" 2>&1 | head -20
rc=${PIPESTATUS[0]}
echo "LIFT_EXIT=$rc"
[ -f "$OUT" ] && echo "errors=$(grep -c '__remill_error' "$OUT") missing=$(grep -c '__remill_missing_block' "$OUT") switches=$(grep -c 'switch i64' "$OUT")"
