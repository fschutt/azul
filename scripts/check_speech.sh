#!/usr/bin/env bash
# The checks of the speech work, for a runner or a machine of one's own:
#
#   bash scripts/check_speech.sh gate    the tests, the gates and the compile checks of the speech module, the
#                                        model download, the updater it shares its downloader with, and the
#                                        generated bindings (Linux; the bindings must be generated first:
#                                        scripts/remote.sh run <sha> ubuntu-22.04 'bash scripts/check_speech.sh gate' --codegen)
#   bash scripts/check_speech.sh static  the vendor's static engine linked on Linux (libc++ from packages), its
#                                        tests, the real engine on the real model, the real model from the server
#   bash scripts/check_speech.sh macos   the same on macOS arm64, then the compile checks for iOS and Android with
#                                        the engine linked, then the library of one's own with the engine inside
#
# Nothing here is uploaded, and the engine is only ever linked into what these run (the cargo feature of the
# static engine is in no workflow: see dll/Cargo.toml). Every part prints a line "=== <what>" before its output and
# ends with exit 0 so that a runner keeps its log: the decisive lines are the `exit` lines and the test results.
set -u

part="${1:-gate}"

section() {
  echo "=== $1"
}

# The lines of a log that say how it went.
verdict() {
  grep -E "test result|FAILED|panicked|^error" -A4 "$1" | head -"${2:-16}"
}

speech_tests() {
  section "SPEECH TESTS (3 runs)"
  for run in 1 2 3; do
    (cd dll && cargo test --lib speech > /tmp/speech.log 2>&1)
    echo "speech tests run $run exit $?"
    verdict /tmp/speech.log 6
  done
  (cd dll && cargo test --lib extra_settings > /tmp/settings.log 2>&1)
  echo "extra settings tests exit $?"
  verdict /tmp/settings.log 6
}

layout_tests() {
  section "LAYOUT: the ranged download and the updater that downloads with it"
  cargo test -p azul-layout --features updater --lib -- ranged_download updater > /tmp/layout.log 2>&1
  echo "layout tests exit $?"
  verdict /tmp/layout.log 30
}

doc_tests() {
  section "DOC TESTS: the generator's Send and Sync, autofix's reading of them, the bug classes"
  cargo test -r -p azul-doc -- thread_safe_test type_index_custom_impls_test bug_classes > /tmp/doc.log 2>&1
  echo "doc tests exit $?"
  grep -E "^test .*(thread_safe|type_index_custom|FAILED)|test result" /tmp/doc.log | head -20
  grep -E "^---- " -A14 /tmp/doc.log | head -60
}

autofix_agrees() {
  section "AUTOFIX AT ITS FIXPOINT"
  for round in 1 2; do
    cargo run -r -p azul-doc -- autofix 2>&1 | grep -E "Types to add|Type modifications|Path fixes|^  [+~] " | head -20
    cargo run -r -p azul-doc -- autofix apply 2>&1 | grep -E "Total changes|Failed" | head -3
  done
  git diff --stat | tail -4
  section "CHECK LAYOUT"
  cargo run -r -p azul-doc -- check layout 2>&1 | tail -8
}

source_gates() {
  section "SRCGATE"
  cargo test -p azul-srcgate 2>&1 | grep -E "^test result|FAILED|failed|panicked" | sort | uniq -c
  section "LEDGER OF IGNORED TESTS"
  (cd layout && rustc --edition 2021 --test tests/ignored_tests_are_on_the_ledger.rs -o /tmp/ledger_test 2>&1 | tail -3)
  (cd layout && CARGO_MANIFEST_DIR="$PWD" /tmp/ledger_test 2>&1 | tail -4)
}

compile_checks() {
  section "CHECK ALL TARGETS (azul-dll)"
  cargo check -p azul-dll --all-targets > /tmp/dllcheck.log 2>&1
  echo "dll check exit $?"
  grep -E "^error" -A8 /tmp/dllcheck.log | head -40
  section "LINK-DYNAMIC (azul-dll)"
  cargo check -p azul-dll --no-default-features --features "link-dynamic" > /tmp/dyncheck.log 2>&1
  echo "link-dynamic check exit $?"
  grep -E "^error" -A8 /tmp/dyncheck.log | head -30
  section "CLIPPY (azul-layout with the updater, the lint set CI holds the library to)"
  rustup component add clippy > /dev/null 2>&1
  cargo clippy -p azul-layout --features updater --lib -- -D warnings > /tmp/clippy.log 2>&1
  echo "clippy exit $?"
  grep -E "^(error|warning)" -A6 /tmp/clippy.log | head -40
}

# The two spoken sentences the real-engine test listens to, made by the machine's own voice.
sentences_linux() {
  espeak-ng -v en-us -w target/speech-engine/en.wav "Please move the label of the river to the left and delete the duplicate street name."
  espeak-ng -v de -w target/speech-engine/de.wav "Bitte verschiebe die Beschriftung des Flusses nach links und loesche den doppelten Strassennamen."
}

sentences_macos() {
  say -o /tmp/en.aiff "Please move the label of the river to the left and delete the duplicate street name."
  afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/en.aiff target/speech-engine/en.wav
  if say -v Anna -o /tmp/de.aiff "Bitte verschiebe die Beschriftung des Flusses nach links und loesche den doppelten Strassennamen."; then
    afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/de.aiff target/speech-engine/de.wav
  else
    echo "no German voice here: the English sentence stands in"
    cp target/speech-engine/en.wav target/speech-engine/de.wav
  fi
}

real_engine() {
  section "REAL ENGINE, linked, on the real model"
  cargo test -p azul-dll --test speech_builtin_engine --features speech-engine-static -- --ignored --nocapture > /tmp/real.log 2>&1
  echo "real engine exit $?"
  verdict /tmp/real.log 12
  grep -E "english|german|text:" /tmp/real.log | head -6
}

real_download() {
  section "THE REAL MODEL FROM THE REAL SERVER"
  cargo test -p azul-dll --test speech_model_download -- --ignored --nocapture > /tmp/download.log 2>&1
  echo "download test exit $?"
  grep -E "test result|FAILED|panicked|stopped with|the model is" /tmp/download.log | head -8
}

static_linux() {
  section "STATIC ENGINE ON LINUX: packages and files"
  sudo apt-get install -y libc++-14-dev libc++abi-14-dev espeak-ng > /tmp/apt.log 2>&1
  echo "apt exit $?"
  scripts/fetch_whistle.sh --static target/speech-engine > /tmp/fetch.log 2>&1
  echo "fetch exit $?"
  tail -2 /tmp/fetch.log | cut -c1-200
  sentences_linux
  export AZUL_CACTUS_CXX_DIR=/usr/lib/llvm-14/lib
  section "LINKED ENGINE ON LINUX: the engine seam and the built-in recognizer (libc++ as archives, else as libraries)"
  for archives in 1 0; do
    export AZUL_CACTUS_CXX_STATIC=$archives
    cargo test -p azul-dll --lib --features speech-engine-static builtin > /tmp/static.log 2>&1
    code=$?
    echo "linked-engine builtin tests (libc++ as archives=$archives) exit $code"
    verdict /tmp/static.log 12
    if [ "$code" = 0 ]; then
      break
    fi
  done
  real_engine
  real_download
}

mobile_check() {
  # $1 the target, $2 the folder of the vendor's release that has its archive
  scripts/fetch_whistle.sh --static "target/speech-engine-$2" --platform "$2" > "/tmp/fetch-$2.log" 2>&1
  echo "fetch $2 exit $?"
  AZUL_CACTUS_LIB_DIR="$PWD/target/speech-engine-$2/static" cargo check -p azul-dll --target "$1" \
    --no-default-features --features "std,logging,link-static,a11y,speech-engine-static" > "/tmp/check-$2.log" 2>&1
  echo "cargo check $1 exit $?"
  grep -E "^error" -A8 "/tmp/check-$2.log" | head -30
  grep -E "speech/" "/tmp/check-$2.log" | head -10
}

macos() {
  section "PLATFORM"
  sw_vers | head -3
  uname -m
  section "FETCH: the static engine for macOS arm64, the model"
  scripts/fetch_whistle.sh --static target/speech-engine > /tmp/fetch.log 2>&1
  echo "fetch exit $?"
  tail -2 /tmp/fetch.log | cut -c1-200
  section "THE SYSTEM VOICE SAYS TWO SENTENCES"
  sentences_macos
  ls -la target/speech-engine
  section "LINKED ENGINE ON macOS: the engine seam and the built-in recognizer"
  cargo test -p azul-dll --lib --features speech-engine-static builtin > /tmp/static.log 2>&1
  echo "linked-engine builtin tests exit $?"
  verdict /tmp/static.log 10
  grep -E "built for newer" /tmp/static.log | head -2
  real_engine
  section "iOS: the code with the engine linked compiles (device and simulator)"
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim > /dev/null 2>&1
  mobile_check aarch64-apple-ios ios-arm64
  mobile_check aarch64-apple-ios-sim ios-sim-arm64
  section "ANDROID arm64: the same"
  rustup target add aarch64-linux-android > /dev/null 2>&1
  export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-}}"
  echo "NDK: $ANDROID_NDK_HOME"
  cargo install cargo-ndk --version 4.1.2 --locked > /dev/null 2>&1
  scripts/fetch_whistle.sh --static target/speech-engine-android --platform android-arm64 > /tmp/fetch-android.log 2>&1
  echo "fetch android exit $?"
  AZUL_CACTUS_LIB_DIR="$PWD/target/speech-engine-android/static" cargo ndk -t arm64-v8a --platform 24 check -p azul-dll \
    --no-default-features --features "std,logging,link-static,a11y,android-activity,speech-engine-static" > /tmp/android.log 2>&1
  echo "cargo ndk check exit $?"
  grep -E "^error" -A8 /tmp/android.log | head -30
  grep -E "speech/" /tmp/android.log | head -10
  section "THE LIBRARY OF ONE'S OWN: libazul.dylib with the engine inside, and what it exports"
  cargo build -p azul-dll --features "build-dll,speech-engine-static" > /tmp/dylib.log 2>&1
  echo "cargo build build-dll with the static engine exit $?"
  grep -E "^error" -A8 /tmp/dylib.log | head -30
  ls -la target/debug/libazul.dylib | awk '{print "libazul.dylib bytes:", $5}'
  echo "exported names with cactus in them:"
  nm -gU target/debug/libazul.dylib | grep -ci cactus
  echo "exported Az names:"
  nm -gU target/debug/libazul.dylib | grep -c " _Az"
  otool -L target/debug/libazul.dylib | grep -E "c\+\+|cactus"
}

case "$part" in
  gate)
    speech_tests
    layout_tests
    doc_tests
    autofix_agrees
    source_gates
    compile_checks
    ;;
  static)
    static_linux
    ;;
  macos)
    macos
    ;;
  *)
    echo "usage: bash scripts/check_speech.sh gate | static | macos"
    ;;
esac
section "END"
exit 0
