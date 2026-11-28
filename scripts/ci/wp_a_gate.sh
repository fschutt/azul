#!/usr/bin/env bash
# wp_a_gate.sh <check|clippy|test> <Package...>: the compile, lint and test gates of the apps of WP-A (AzMeet, AzCalendar) on a GitHub
# runner (the bindings must be generated first, so --codegen):
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/wp_a_gate.sh check AzCalendar AzMeet' --codegen
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/wp_a_gate.sh clippy AzCalendar AzMeet' --codegen
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/wp_a_gate.sh test AzCalendar AzMeet' --codegen
#
#   check   `cargo check -p <App> --tests` over the generated bindings with a stub libazul (a check links nothing; azul-dll's build script
#           refuses a library in the cargo profile directory, so the stub lies in a folder of its own);
#   clippy  `cargo clippy -p <App> --tests --no-deps -- -D warnings` the same way (the app's own lints, other crates' are not asked);
#   test    the real libazul first (`build-dll`), then `cargo test -p <App> --lib --no-fail-fast`.
# The command text stays the same from run to run, so the runner's build cache (keyed by the command) is warm after the first run.
set +e
step="${1:?check, clippy or test}"; shift
[ $# -gt 0 ] || { echo "usage: wp_a_gate.sh check|clippy|test <Package...>"; exit 2; }
pkgs=(); for p in "$@"; do pkgs+=(-p "$p"); done
link=$(mktemp -d)
status=0
case "$step" in
  check|clippy)
    : > "$link/libazul.so"
    export AZ_LINK_PATH="$link"
    if [ "$step" = check ]; then
      cargo check "${pkgs[@]}" --tests > "$link/out.txt" 2>&1
    else
      cargo clippy "${pkgs[@]}" --tests --no-deps -- -D warnings > "$link/out.txt" 2>&1
    fi
    status=$?
    echo "=== $step exit $status"
    # what is wrong first (the errors), then every warning that points into the apps
    awk '/^error/ {p=1} p' "$link/out.txt" | head -600
    echo "=== diagnostics that point into the apps"
    grep -B2 -A10 -E '^\s+--> examples/azul-(meet|calendar|calendar-core|appkit)/' "$link/out.txt" | head -400
    ;;
  test)
    cargo build -p azul-dll --features build-dll 2>&1 | tail -3
    cp target/debug/libazul.so "$link/"
    export AZ_LINK_PATH="$link"
    export LD_LIBRARY_PATH="$link${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    cargo test "${pkgs[@]}" --lib --no-fail-fast > "$link/out.txt" 2>&1
    status=$?
    echo "=== test exit $status"
    awk '/^error/ {p=1} p' "$link/out.txt" | head -300
    grep -E '^test result|^running|FAILED|panicked|^failures:' "$link/out.txt" | head -200
    tail -60 "$link/out.txt"
    ;;
  *) echo "usage: wp_a_gate.sh check|clippy|test <Package...>"; exit 2 ;;
esac
exit "$status"
