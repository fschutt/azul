#!/usr/bin/env python3
"""LAYOUTPERF8 probe: what does ONE frame of the AzWidgets switch slide cost?

Writes an AZ_E2E scenario that clicks the first Switch, then advances the
scripted animation clock one 16.7 ms frame at a time (each tick re-lays out
the window through the css-dirty channel: the knob's `margin-left` tween),
and ends in a failing assert so the runner prints the last response.

Run it with the cpu profile on, ONE app at a time, through the capped runner:

    python3 scripts/layoutperf8_tick_scenario_gen.py /tmp/lp8/tick.json
    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 150 --log /tmp/lp8/tick.log -- \\
      env AZ_BACKEND=headless AZ_PROFILE=cpu AZ_E2E=/tmp/lp8/tick.json \\
      /Users/fschutt/Development/azul/target/release/AzWidgets

Then read the `[CPU]` blocks of the tick relayouts in the log: the counts of
`text_layout_flow` (text re-flows), `taffy_cache_get_miss` (flex items laid
out again), `root_layout_pass` (the layout pass itself). Before LAYOUTPERF8 a
tick re-laid out almost the whole page (ANIM8: 2853 text re-flows, 12181 taffy
misses, root_layout_pass ~200 ms); after it, a tick re-lays out the knob's
ancestor chain only (0 text re-flows, tens of taffy misses).

usage: layoutperf8_tick_scenario_gen.py <out.json> [width] [height] [ticks]
"""

import json
import sys

out = sys.argv[1]
width = int(sys.argv[2]) if len(sys.argv) > 2 else 900
height = int(sys.argv[3]) if len(sys.argv) > 3 else 1300
ticks = int(sys.argv[4]) if len(sys.argv) > 4 else 3

steps = [
    {"op": "wait_frame"},
    {"op": "click", "selector": ".__azul-native-switch"},
    {"op": "wait_frame"},
]
for _ in range(ticks):
    steps.append({"op": "tick_animations", "dt_micros": 16666, "steps": 1})
    steps.append({"op": "wait_frame"})
steps.append({"op": "get_node_layout", "selector": ".__azul-native-switch-knob"})
steps.append({"op": "assert_response", "contains": "NEVER_THERE"})

scenario = [{
    "name": "layoutperf8_switch_tick",
    "description": "one switch click, then single-frame animation ticks (each a css-dirty relayout)",
    "setup": {"window_width": width, "window_height": height},
    "steps": steps,
}]
with open(out, "w") as f:
    json.dump(scenario, f, indent=1)
print("wrote", out, "with", ticks, "ticks")
