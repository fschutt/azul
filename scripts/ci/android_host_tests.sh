#!/usr/bin/env bash
# android_host_tests.sh: the host tests of the Android work, on a runner (needs the bindings: remote.sh --codegen).
#
#   scripts/ci/android_host_tests.sh
#
# The pure parts of the Android shell (shell2/android_core, extra/permission/android_table) are compiled on every host,
# so their tests run here, where the engine is built (they also run alone with `rustc --test`, std only, except the
# ones that name an engine type). The dll/tests/android_*.rs files read the Java glue and the Rust sources as text and
# need nothing built, but cargo builds the crate they belong to first.
set -euo pipefail
cargo test -p azul-dll --features build-dll --lib -- android_core android_table touch_selection create_callback
cargo test -p azul-dll --features build-dll \
  --test android_jni_symbols_match_the_java_glue \
  --test android_jni_scopes_release_their_local_references \
  --test android_ui_thread_entry_points_never_touch_the_window \
  --test android_activity_lifecycle_contract \
  --test android_extras_java_glue_contract \
  --test shells_reach_the_create_callback \
  --test android_build_script_names_its_linker \
  --test android_libraries_are_linked_for_16_kb_pages
echo "android host tests: ok"
