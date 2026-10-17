#!/bin/zsh
# The whole PGO + order-file chain. Needs ~10 GB free (two extra target dirs) - check `df -h /` first.
set -u
ROOT=/Users/fschutt/Development/azul
T=$ROOT/scripts/waves/tools
W=${AZ_WORK:-$HOME/Development/azul-work}; mkdir -p $W/logs
P=~/.rustup/toolchains/1.91.0-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/llvm-profdata
J=${AZ_JOBS:-4}
cd $ROOT
rm -rf target/pgo/raw; mkdir -p target/pgo/raw
echo "== 1. instrumented build"
CARGO_TARGET_DIR=$ROOT/target/pgo-gen RUSTFLAGS="-Cprofile-generate=$ROOT/target/pgo/raw --cfg azul_pgo" cargo build --release -j $J -p azul-dll --features build-dll > $W/logs/pgo_gen.log 2>&1 || { echo "instrumented build FAILED"; exit 1; }
echo "== 2. training"; zsh $ROOT/scripts/pgo/train.sh $ROOT/target/pgo-gen/release
echo "== 3. merge"
$P merge -o target/pgo/azul.profdata target/pgo/raw/*.profraw 2>&1 | tail -3
[ -f target/pgo/azul.profdata ] || { echo "merge FAILED"; exit 1; }
echo "== 4. order"
python3 $ROOT/scripts/pgo/order_from_counts.py target/pgo/azul.profdata target/pgo/order_ir.txt $P
python3 $ROOT/scripts/pgo/to_order_file.py target/pgo/order_ir.txt target/pgo/order.txt target/azul-lib/libazul.dylib
echo "== 5. free the instrumented build"; rm -rf target/pgo-gen target/pgo/raw; df -h / | tail -1
echo "== 6. PGO + order-file build"
CARGO_TARGET_DIR=$ROOT/target/pgo-use RUSTFLAGS="-Cprofile-use=$ROOT/target/pgo/azul.profdata -Cllvm-args=-pgo-warn-mismatch=false -Clink-arg=-Wl,-order_file,$ROOT/target/pgo/order.txt" cargo build --release -j $J -p azul-dll --features build-dll > $W/logs/pgo_use.log 2>&1 || { echo "PGO build FAILED"; exit 1; }
echo "== 7. measure: e2e corpus timing x3, then memory alone/together"
H=$(mktemp -d /tmp/azopt-home.XXXX)
for name lib in baseline $ROOT/target/azul-lib pgo+order $ROOT/target/pgo-use/release; do
  for i in 1 2 3; do t0=$(python3 -c "import time; print(time.time())"); env HOME=$H AZ_BACKEND=headless AZ_E2E=$ROOT/e2e DYLD_LIBRARY_PATH=$lib $ROOT/target/release/AzPaint > /dev/null 2>&1; echo "$name run $i: $(python3 -c "import time; print(round(time.time()-$t0,2))")s"; done
done
bash $T/run_capped.sh --cap-mb 2500 --seconds 1500 --log $W/logs/pgo_opt_alone.log -- env AZ_MEASURE_LIBDIR=$ROOT/target/pgo-use/release AZ_MEASURE_OUT=$W/pgo_opt python3 $T/memprobe.py alone
bash $T/run_capped.sh --cap-mb 5000 --seconds 900 --log $W/logs/pgo_opt_together.log -- env AZ_MEASURE_LIBDIR=$ROOT/target/pgo-use/release AZ_MEASURE_OUT=$W/pgo_opt python3 $T/memprobe.py together
grep together: $W/logs/pgo_opt_together.log
echo CHAIN DONE
