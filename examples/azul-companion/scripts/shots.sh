#!/usr/bin/env bash
# The layout study: every scene of the phone app on every phone, in both ways round and both languages, headless (the
# CPU renderer, 2x, the phone alone), each with the layout check. One process at a time.
#
#   examples/azul-companion/scripts/shots.sh <out-dir> [path to companion_sim] [scenes: all | a | b1-remote,...]
#
# Pictures: <out-dir>/<en|de>/<device>-<portrait|landscape>-<lang>-<scene>.png. The check's words go to <out-dir>/check.log.
# The exit code is 1 when any scene has a fault.
set -u
out="${1:?usage: shots.sh <out-dir> [companion_sim] [scenes]}"
sim="${2:-target/debug/companion_sim}"
scenes="${3:-all}"
mkdir -p "$out"
: > "$out/check.log"
status=0
for lang in en de; do
  for device in iphone android-s android-l; do
    for way in portrait landscape; do
      flags=(--device "$device" --lang "$lang")
      [ "$way" = landscape ] && flags+=(--landscape)
      echo "== $device $way $lang" | tee -a "$out/check.log"
      AZ_BACKEND=headless AZ_MODE=dark "$sim" "${flags[@]}" --shots "$out/$lang" --scenes "$scenes" 2>&1 \
        | grep -E '^\[shots\] (CHECK|  |[0-9])' | grep -v ' ok ' | tee -a "$out/check.log"
      [ "${PIPESTATUS[0]}" -eq 0 ] || status=1
    done
  done
done
echo "check.log: $(grep -c 'faults' "$out/check.log") lines about faults"
exit $status
