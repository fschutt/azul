#!/usr/bin/env bash
# The frame's end-to-end runs and its pictures on a GitHub runner (the bindings must be generated first):
#
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-mail/scripts/runner_frame.sh' --codegen \
#       --artifacts $'target/pictures/azmail/*\ntarget/pictures/azmail/e2e/*'
#
# A libazul with the debug server, the E2E scripting and the test hooks (`build-dll,test-hooks`:
# `build-dll` already holds the first two), AzMail with its test hooks, then
#   1. the scenario files examples/azul-mail/e2e/frame.json (the desktop), frame_phone.json (the
#      phone preset) and text_size.json (the phone frame at 360 x 640; QA runs it under the system's
#      text size) through the engine's own executor (AZ_E2E=<file>: the app runs the file and exits);
#   2. `scripts/azmail_e2e.py --phase frame` for the desktop and for the phone preset
#      (`--form phone`, a 411 x 760 window): its checks, and the PNGs of the run (flora, light and
#      dark, the sidebar expanded and as the rail, the Preview with its (?), File's location, the
#      phone's drawer and sheet).
# Everything lands in target/pictures/azmail/.
set +e
out=target/pictures/azmail
mkdir -p "$out/e2e"
cargo build -p azul-dll --features build-dll,test-hooks 2>&1 | tail -3
link=$(mktemp -d)
cp target/debug/libazul.so "$link/"
export AZ_LINK_PATH="$link"
cargo build -p AzMail --features test-hooks 2>&1 | grep -E "^(error|warning: unused)|-->|Finished" | head -60
bin=$PWD/target/debug/AzMail
status=0

# 1. the scenario files: <file>:<form>:<window size>
for scenario in frame:desktop:1280x860 frame_phone:phone:411x760 text_size:phone:360x640; do
  IFS=: read -r file form size <<< "$scenario"
  data=$(mktemp -d)
  AZ_E2E="$PWD/examples/azul-mail/e2e/$file.json" AZ_E2E_SHOT_DIR="$PWD/$out/e2e" AZ_BACKEND=headless AZ_LOG=off \
    AZMAIL_DATA="$data/AzMail" AZLIN_DATA="$data" AZLIN_CONFIG=off AZMAIL_FORM="$form" \
    timeout 300 "$bin" --sample --size "$size" --mode light --language en > "$out/e2e-$file.txt" 2>&1
  code=$?
  echo "=== scenario $file ($form, $size) exit $code"
  grep -E "^test |^---- |test result|step [0-9]+: " "$out/e2e-$file.txt" | head -80
  [ "$code" = 0 ] || status=1
done

# 2. the harness
for form in desktop phone; do
  python3 scripts/azmail_e2e.py --bin "$bin" --phase frame --form "$form" --keep-logs --timeout 240 \
    > "$out/frame-$form.log" 2>&1
  code=$?
  echo "=== frame $form exit $code"
  grep -E "^(ok|FAILED|picture|logs and data)|Traceback|Failure|Error" "$out/frame-$form.log" | head -60
  [ "$code" = 0 ] || status=1
  dir=$(grep -o 'logs and data: .*' "$out/frame-$form.log" | head -1 | sed 's/logs and data: //')
  if [ -n "$dir" ] && [ -d "$dir/screenshots" ]; then
    cp "$dir"/screenshots/*.png "$out/"
  fi
done
ls -la "$out" "$out/e2e"
exit "$status"
