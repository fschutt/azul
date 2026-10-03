import json
import sys

# usage: gen.py <out.json> <width> <height>  -- writes switch checkpoints
out = sys.argv[1]
w = int(sys.argv[2]) if len(sys.argv) > 2 else 900
h = int(sys.argv[3]) if len(sys.argv) > 3 else 1300


def scen(name, extra):
    return {"name": name, "description": "probe",
            "setup": {"window_width": w, "window_height": h},
            "steps": [{"op": "wait_frame"}] + extra}


K = {"op": "get_node_layout", "selector": ".__azul-native-switch-knob"}
A = {"op": "get_animations"}
FAIL = {"op": "assert_response", "contains": "NEVER_THERE"}
click = [{"op": "click", "selector": ".__azul-native-switch"}, {"op": "wait_frame"}]


def tick(n):
    return [{"op": "tick_animations", "dt_micros": 16666, "steps": n}, {"op": "wait_frame"}]


tests = [
    scen("a_before", [K, FAIL]),
    scen("b_after_click_knob", click + [K, FAIL]),
    scen("c_after_click_anims", click + [A, FAIL]),
    scen("d_tick1_knob", click + tick(1) + [K, FAIL]),
    scen("e_tick1_anims", click + tick(1) + [A, FAIL]),
    scen("f_tick3_knob", click + tick(3) + [K, FAIL]),
    scen("g_tick30_knob", click + tick(30) + [K, FAIL]),
]
json.dump(tests, open(out, "w"), indent=1)
