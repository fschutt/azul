#!/usr/bin/env python3
"""ANIMFRAME8 probe: what does a frame of the AzWidgets switch glide cost?

Writes an AZ_E2E scenario that clicks the first Switch, resets the frame
counters, advances the scripted animation clock one 16.7 ms frame at a time,
then reads the frame report, and ends in a failing assert so the runner prints
the last response (the report).

Since ANIMFRAME8 the knob slides by `transform: translateX` - a GPU value - and
the track's colour is patched into the display list in place, so a frame of
the glide owes a repaint only. Expected in the printed `get_frame_report`:
`layout_passes` 0 and `dl_rebuilds` 0 for the ticks (the click's own rebuild
happens before the reset), and NO `[incremental_relayout] START` line in the
log after the click's `regenerate_layout`. Before ANIMFRAME8 every tick was an
`incremental_relayout` of 19-22 ms (unprofiled).

Run it ONE app at a time, the AZ_* variables BEFORE the runner (SIP strips
DYLD_* through /usr/bin/env, see run_capped.sh):

    python3 scripts/animframe8_tick_probe_gen.py /tmp/af8/tick.json 900 1300 6
    AZ_BACKEND=headless AZ_E2E=/tmp/af8/tick.json \\
      scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 150 --log /tmp/af8/tick.log -- \\
      /Users/fschutt/Development/azul/target/release/AzWidgets
    grep -n "incremental_relayout\\] START\\|regenerate_layout (\\|layout_passes" /tmp/af8/tick.log

usage: animframe8_tick_probe_gen.py <out.json> [width] [height] [ticks]
"""

import json
import sys

out = sys.argv[1]
width = int(sys.argv[2]) if len(sys.argv) > 2 else 900
height = int(sys.argv[3]) if len(sys.argv) > 3 else 1300
ticks = int(sys.argv[4]) if len(sys.argv) > 4 else 6

steps = [
    {"op": "wait_frame"},
    {"op": "click", "selector": ".__azul-native-switch"},
    {"op": "wait_frame"},
    {"op": "reset_frame_counters"},
]
for _ in range(ticks):
    steps.append({"op": "tick_animations", "dt_micros": 16666, "steps": 1})
    steps.append({"op": "wait_frame"})
steps.append({"op": "get_frame_report"})
steps.append({"op": "assert_response", "contains": "NEVER_THERE"})

scenario = [{
    "name": "animframe8_switch_tick",
    "description": "one switch click, then single-frame animation ticks (each a GPU-value repaint)",
    "setup": {"window_width": width, "window_height": height},
    "steps": steps,
}]
with open(out, "w") as f:
    json.dump(scenario, f, indent=1)
print("wrote", out, "with", ticks, "ticks")
