#!/usr/bin/env bash
# sounds_gate.sh: the gate of the system-sounds work on one Ubuntu runner, every step with its REAL exit code.
#
#   scripts/remote.sh run <ref or full sha> ubuntu-22.04 'bash scripts/ci/sounds_gate.sh' --codegen \
#     --artifacts 'autofix.diff,autofix2.diff,*-tests.txt'
#
# The steps to run are named in scripts/ci/sounds_gate.steps (one per line, # comments): the command string stays the
# same from run to run, so the runner's build cache is reused. No such file, or an empty one: every step runs.
#   COMPILE gate (before a push):  java_compile core_tests layout_check dll_check android_check ios_cross_check apps_check dll_tests_check autofix doc_check
#   TEST gate (on the pushed sha): layout_sound_tests dll_sound_tests clippy_features clippy_default workspace_tests dll_default_tests srcgate
# A step that fails does not stop the ones after it, so one run says everything that is wrong; the last lines are one
# `STEP <name> <exit>` each, and the exit code of the script is 0 only when every step passed.
set -uo pipefail
names=()
codes=()
steps_file="${SOUNDS_STEPS_FILE:-scripts/ci/sounds_gate.steps}"
selected=""
if [ -f "$steps_file" ]; then
  selected="$(grep -v '^[[:space:]]*#' "$steps_file" | tr -s ' \n' ',' | sed 's/^,//; s/,$//')"
fi

run() {
  local name="$1"
  shift
  case ",$selected," in
    ",,") ;;
    *",$name,"*) ;;
    *) return 0 ;;
  esac
  echo "=== STEP $name: $* ==="
  "$@"
  local code=$?
  names+=("$name")
  codes+=("$code")
  echo "=== STEP $name: exit $code ==="
}

features="azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater"

# 1. every Java file the APK builds compiles against the platform jar (a Java error breaks every Android app's build,
#    and no step of the Rust gates sees it). scripts/build-android.sh compiles all of scripts/android/*.java with
#    -source 11 -target 11 against android-34.
java_compile() {
  local sdk="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
  local jar="$sdk/platforms/android-34/android.jar"
  if [ ! -f "$jar" ]; then
    echo "no $jar on this runner"
    return 2
  fi
  local out
  out="$(mktemp -d)"
  javac -source 11 -target 11 -classpath "$jar" -d "$out" scripts/android/*.java
}
run java_compile java_compile

# 2. the module of azul-core that holds the decisions (route, gates, pacing, recording), by name, and the whole lib
run core_tests cargo test -p azul-core --lib sound

# 3. the layout crate compiles with the e2e ops (assert_sound), its tests included: a check, not a run
run layout_check cargo check -p azul-layout --lib --tests --features "$features" --message-format short

# 4. the shells compile: the dll with the desktop stack, and the two phone shells
run dll_check cargo check -p azul-dll --features build-dll --lib --message-format short
run android_check bash scripts/ci/android_check.sh
run ios_cross_check bash scripts/ci/ios_cross_check.sh
# the apps compile against the generated API (the Sounds section of AzWidgets names SystemSound and the new CallbackInfo methods).
# They link the dll dynamically, and the build script of the dll refuses a build into a target directory that holds a
# libazul.so (dll/build_link.rs, "Refusing to build azul-dll for 'link-dynamic'"): the test builds of the dll leave one in
# target/debug, and the runner's cache keeps it for the next run, so the apps build into a target directory of their own.
run apps_check env CARGO_TARGET_DIR=target/apps cargo check -p azul-appkit -p AzWidgets --lib --profile test --features azul-appkit/azul --message-format short

# 4b. every test of the dll compiles (the lib's tests and the integration tests): a check of the test targets, which the
#     CI's `cd dll && cargo test` runs; the lib's own check above is reused, so this is the test builds only
run dll_tests_check cargo check -p azul-dll --features build-dll --tests --message-format short

# 5. api.json against the source: autofix must find nothing more, and a SECOND round on the same tree nothing either
#    (autofix.diff is the tree after round 1, autofix2.diff after round 2: both must be empty). The first run on a tree with a
#    new API fails here ON PURPOSE: autofix.diff is what to apply to api.json.
autofix_round() {
  cargo run -r -p azul-doc -- autofix &&
    cargo run -r -p azul-doc -- autofix explain &&
    cargo run -r -p azul-doc -- patch safe target/autofix/patches &&
    cargo run -r -p azul-doc -- patch target/autofix/patches &&
    cargo run -r -p azul-doc -- normalize
}
autofix() {
  autofix_round || return 1
  git diff > autofix.diff
  autofix_round || return 1
  git diff > autofix2.diff
  if [ -s autofix.diff ]; then
    echo "autofix round 1 changed the tree (autofix.diff)"
    return 1
  fi
  if [ -s autofix2.diff ]; then
    echo "autofix round 2 changed the tree (autofix2.diff)"
    return 1
  fi
  return 0
}
run autofix autofix

# 6. the double-drop invariant and the guide links (the CI's azul-doc job), and the layout checks of the guide pages
run doc_check cargo run -r -p azul-doc -- check
run doc_check_layout cargo run -r -p azul-doc -- check layout

# 7. the TEST gate, on the pushed sha: the tests of the sound work by name (cheap: they compile what the CI's commands
#    compile and run only these), then the CI's own commands
run layout_sound_tests cargo test -p azul-layout --lib --features "$features" -- sound_tests
run dll_sound_tests bash -c 'cd dll && cargo test --lib -- shell_settings system_sounds sound_invariants && cargo test --test headless_system_sounds --test android_sounds_java_glue_contract'
workspace_tests() {
  sudo apt-get update -qq || true
  sudo apt-get install -y fonts-liberation fonts-dejavu-core || true
  AZ_REQUIRE_TEST_FONTS=1 cargo test -p azul-css -p azul-core -p azul-layout -p webrender --lib --tests \
    --features "$features" --no-fail-fast 2>&1 | tee workspace-tests.txt
  local code=${PIPESTATUS[0]}
  ./scripts/zero_test_guard.sh workspace-tests.txt || code=1
  return "$code"
}
run workspace_tests workspace_tests
run clippy_default cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings
run clippy_features cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --features "$features" -- -D warnings
dll_default_tests() {
  (cd dll && cargo test --no-fail-fast 2>&1 | tee ../dll-default-tests.txt)
  local code=${PIPESTATUS[0]}
  ./scripts/zero_test_guard.sh dll-default-tests.txt || code=1
  return "$code"
}
run dll_default_tests dll_default_tests
srcgate() {
  cargo test -p azul-srcgate --no-fail-fast 2>&1 | tee azul-srcgate-tests.txt
  local code=${PIPESTATUS[0]}
  ./scripts/zero_test_guard.sh --strict azul-srcgate-tests.txt || code=1
  return "$code"
}
run srcgate srcgate

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: $steps_file names none of the steps"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"
