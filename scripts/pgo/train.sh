#!/bin/zsh
# Train an instrumented libazul.dylib: the /e2e corpus through an app host, then every app's startup
# (memprobe.py alone dumps each profile via the `dump_profile` op before closing the app).
# Usage: train.sh <dir of the instrumented libazul.dylib>
set -u
ROOT=/Users/fschutt/Development/azul
T=$ROOT/scripts/waves/tools
W=${AZ_WORK:-$HOME/Development/azul-work}; mkdir -p $W/logs
LIBDIR=${1:-$ROOT/target/pgo-gen/release}
export LLVM_PROFILE_FILE="$ROOT/target/pgo/raw/azul-%p.profraw"
cd $ROOT
H=$(mktemp -d /tmp/azpgo-home.XXXX)
echo "== e2e corpus ($(ls e2e/*.json | wc -l | tr -d ' ') scenarios)"
t0=$(date +%s)
bash $T/run_capped.sh --cap-mb 3000 --seconds 1200 --log $W/logs/pgo_train_e2e.log -- env HOME=$H AZ_BACKEND=headless AZ_E2E=$ROOT/e2e DYLD_LIBRARY_PATH=$LIBDIR LLVM_PROFILE_FILE="$LLVM_PROFILE_FILE" $ROOT/target/release/AzPaint
echo "e2e corpus exit $? in $(( $(date +%s) - t0 ))s; $(grep -c 'test .* ok' $W/logs/pgo_train_e2e.log) ok"
echo "== every app's startup"
bash $T/run_capped.sh --cap-mb 2500 --seconds 1500 --log $W/logs/pgo_train_apps.log -- env LLVM_PROFILE_FILE="$LLVM_PROFILE_FILE" AZ_MEASURE_LIBDIR=$LIBDIR AZ_MEASURE_OUT=$W/pgo_train_apps python3 $T/memprobe.py alone
echo "apps exit $?"
ls target/pgo/raw | wc -l | xargs echo "profraw files:"
du -sh target/pgo/raw
