#!/bin/zsh
# The parent's full local verification: every suite, the dylib, every app crate's lib tests.
# -j 4: the 8 GB Mac panicked (watchdog, low swap) under -j 6 plus agents (2026-09-30); on battery
# use AZ_JOBS=2 - at -j 4 it drains even on AC (power loss 2026-10-02 22:41).
# Logs + summary in $AZ_WORK/logs (durable - /tmp is wiped on reboot).
cd /Users/fschutt/Development/azul
J=${AZ_JOBS:-4}
L=${AZ_WORK:-$HOME/Development/azul-work}/logs
mkdir -p $L
: > $L/suites_summary.txt
run() { name=$1; shift; "$@" > $L/suite_$name.log 2>&1; rc=$?; echo "$name exit=$rc $(grep -E '^test result' $L/suite_$name.log | awk '{p+=$4; f+=$6} END {print "passed="p" failed="f}')" >> $L/suites_summary.txt; }
run core cargo test --release -j $J --no-fail-fast -p azul-core --features codegen
run css cargo test --release -j $J --no-fail-fast -p azul-css
run css_codegen cargo test --release -j $J --no-fail-fast -p azul-css --features codegen,parser --test codegen_structure --test codegen_goldens
run layout_lib cargo test --release -j $J --no-fail-fast -p azul-layout --lib
run layout_all cargo test --release -j $J --no-fail-fast -p azul-layout --test all
run dll_lib cargo test --release -j $J --no-fail-fast -p azul-dll --lib --features build-dll
run dll_tests cargo test --release -j $J --no-fail-fast -p azul-dll --features build-dll --test transient_window_layout --test headless_global_hotkeys --test app_shell_animations --test mode_headless --test theme_and_mode_are_two_names --test app_theme_headless
run layout_e2e cargo test --release -j $J --no-fail-fast -p azul-layout --features e2e-server --lib
run doc cargo test --release -j $J --no-fail-fast -p azul-doc --bin azul-doc
cargo build --release -j $J -p azul-dll --features build-dll > $L/suite_dylib.log 2>&1 && mkdir -p target/azul-lib && cp target/release/libazul.dylib target/azul-lib/libazul.dylib.new && mv target/azul-lib/libazul.dylib.new target/azul-lib/libazul.dylib
echo "dylib exit=$?" >> $L/suites_summary.txt
P=(); for d in examples/azul-*/; do n=$(grep -m1 '^name' $d/Cargo.toml | sed 's/.*= *"\(.*\)"/\1/'); P+=(-p $n); done
AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -j $J --keep-going "${P[@]}" > $L/suite_apps_build.log 2>&1; echo "apps_build exit=$? $(grep -c '^error: could not compile' $L/suite_apps_build.log) crates failed" >> $L/suites_summary.txt
run apps_lib env AZ_LINK_PATH=$PWD/target/azul-lib DYLD_LIBRARY_PATH=$PWD/target/azul-lib cargo test --release -j $J --no-fail-fast "${P[@]}" --lib
echo DONE >> $L/suites_summary.txt
