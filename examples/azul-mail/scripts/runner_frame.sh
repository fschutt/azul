#!/usr/bin/env bash
# The frame's end-to-end run and its pictures on a GitHub runner (the bindings must be generated first):
#
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash examples/azul-mail/scripts/runner_frame.sh' --codegen \
#       --artifacts $'target/pictures/azmail/*'
#
# A libazul with the debug server and the test hooks (`build-dll,test-hooks`), AzMail with its
# test hooks, then `scripts/azmail_e2e.py --phase frame` for the desktop and for the phone preset
# (`--form phone`, a 411 x 760 window): its checks, and the PNGs of the run (flora, light and
# dark, the sidebar expanded and as the rail, the Preview with its (?), File's location, the
# phone's drawer and sheet) copied to target/pictures/azmail/.
set +e
out=target/pictures/azmail
mkdir -p "$out"
cargo build -p azul-dll --features build-dll,test-hooks 2>&1 | tail -3
link=$(mktemp -d)
cp target/debug/libazul.so "$link/"
export AZ_LINK_PATH="$link"
cargo build -p AzMail --features test-hooks 2>&1 | grep -E "^(error|warning: unused)|-->|Finished" | head -60
bin=target/debug/AzMail
status=0
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
ls -la "$out"
exit "$status"
