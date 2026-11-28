#!/usr/bin/env python3
"""Writes AzMaps' scenario files, examples/azul-maps/e2e/*.json, from one description of the steps.

    python3 scripts/wp_e2_maps_scenarios.py [--check]

The files are what QA re-runs unchanged on every platform (scripts/ci/e2e_matrix.py: the desktop host, the Android emulator, the
iOS Simulator): the steps are written once here, and each file holds a `host` test (a window the test resizes to the acceptance
size of the brief: the desktop 1280x800, a Pixel 8, an iPhone 16 Pro, a small phone, a phone on its side) and a `device` test
(the phone is the window: no resize, no display preset), `only_on` says where each runs. `--check` fails when a file on disk is
not what this script writes (the generator is the source, the files are its output).

Every test is one picture set; the pictures are `take_screenshot` steps, numbered in capture order from 1 in each run of the app
(AZ_E2E_SHOT_DIR), and each test's `description` names them in that order.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "examples", "azul-maps", "e2e")

DESKTOPS = ["linux", "macos", "windows"]
PHONES = ["android", "ios"]


def op(name, **params):
    step = {"op": name}
    step.update(params)
    return step


def frames(n=2):
    return [op("wait_frame") for _ in range(n)]


def settle():
    return frames(2) + [op("wait_settled")]


def shot():
    return settle() + [op("take_screenshot")]


def click(selector):
    return [op("click", selector=selector)] + frames(2)


def type_search(text):
    """Focus the search field and type: the app searches once the typing has rested (the engine clock moves, then the geocoder worker
    answers in real time, then the window is rebuilt for the answer)."""
    return (
        click("#__azmaps_search")
        + [op("text_input", text=text), op("wait", ms=600, real=True), op("tick_ms", ms=300), op("wait", ms=900, real=True)]
        + frames(3)
        + [op("tick_ms", ms=200)]
        + frames(2)
    )


def tiles():
    """The world's tiles ship in the binary; a network brings the finer ones: a real wait, short."""
    return [op("wait", ms=3000, real=True)] + frames(3)


def window(name, width, height, preset):
    return [op("set_display_preset", preset=preset), op("resize", width=float(width), height=float(height))] + frames(3) + [op("wait_settled")]


def grip_keys(*keys):
    """The keys on the focused grip of the sheet: the same detents a drag settles on, on any device."""
    out = [op("focus_node", selector=".__azul-native-bottom-sheet-grip")] + frames(1)
    for key in keys:
        out += [op("key_down", key=key), op("key_up", key=key)] + frames(2)
    return out


# ---- the desktop ----

def desktop_steps():
    s = [op("set_mode", mode="light")] + tiles()
    s += [op("assert_exists", selector="#__azmaps_map"), op("assert_exists", selector="#__azmaps_panel"),
          op("assert_exists", selector="#__azmaps_search"), op("assert_exists", selector="#__azmaps_chip-0"),
          op("assert_exists", selector="#__azmaps_zoom-in"), op("assert_exists", selector="#__azmaps_zoom-out"),
          op("assert_exists", selector="#__azmaps_locate"), op("assert_exists", selector="#__azmaps_layers"),
          op("assert_not_exists", selector="#__azmaps_sheet")]
    s += shot()                                                   # 1 first look: the panel, the chips, the keys
    s += type_search("Stutt") + [op("assert_exists", selector="#__azmaps_results")] + shot()          # 2 results
    s += click("#__azmaps_suggestion-0") + [op("assert_exists", selector="#__azmaps_place")] + shot()  # 3 the place
    s += click("#__azmaps_place-route") + [op("assert_exists", selector="#__azmaps_travel")] + shot()  # 4 the way
    s += click("#__azmaps_back") + click("#__azmaps_back") + [op("assert_exists", selector="#__azmaps_chips")]
    s += click("#__azmaps_chip-0") + shot()                       # 5 a chip's search
    s += [op("set_mode", mode="dark")] + shot()                   # 6 dark
    s += [op("set_mode", mode="light")] + click("#__azmaps_panel-toggle") + [op("assert_exists", selector="#__azmaps_panel-show")] + shot()  # 7 folded
    s += click("#__azmaps_panel-show") + [op("assert_exists", selector="#__azmaps_panel")]
    return s


def desktop_file():
    desc = ("AzMaps on a desktop, 1280 x 800: the map fills the window, the panel at the left (search, chips, then the results, the place, "
            "the way), the zoom, locate and map-style keys at the right. Pictures, in order: 1 first look, 2 the results of 'Stutt', 3 the "
            "place, 4 the way (Route), 5 a chip's search, 6 dark, 7 the panel folded away to its search key. (The map styles' popover is a "
            "window of its own: the host's picture is of the main window, so no step opens it.)")
    return [{
        "name": "azul_maps_desktop_panel_search_place_way",
        "description": desc,
        "only_on": DESKTOPS,
        "steps": window("desktop", 1280, 800, "off") + desktop_steps(),
    }]


# ---- the phone ----

def phone_steps():
    s = [op("set_mode", mode="light")] + tiles()
    s += [op("assert_exists", selector="#__azmaps_map"), op("assert_exists", selector="#__azmaps_sheet"),
          op("assert_exists", selector="#__azmaps_search-bar"), op("assert_exists", selector="#__azmaps_chips"),
          op("assert_exists", selector="#__azmaps_layers"), op("assert_exists", selector="#__azmaps_locate"),
          op("assert_not_exists", selector="#__azmaps_panel")]
    s += shot()                                                   # 1 first look: the bar, the chips, the sheet at its peek
    s += type_search("Stutt") + [op("assert_exists", selector="#__azmaps_results")] + shot()          # 2 results
    s += click("#__azmaps_suggestion-0") + [op("assert_exists", selector="#__azmaps_place")] + shot()  # 3 the place at its peek
    s += grip_keys("Up") + shot()                                 # 4 half
    s += grip_keys("Up") + shot()                                 # 5 full
    s += grip_keys("Down", "Down") + [op("assert_exists", selector="#__azmaps_place-route")]
    s += click("#__azmaps_place-route") + [op("assert_exists", selector="#__azmaps_travel")] + shot()  # 6 the way
    s += click("#__azmaps_back") + click("#__azmaps_back") + click("#__azmaps_chip-0") + shot()       # 7 a chip's search
    s += [op("set_mode", mode="dark")] + shot()                                                      # 8 dark
    s += [op("set_mode", mode="light")]
    return s


def phone_file():
    names = "1 first look, 2 the results of 'Stutt', 3 the place at the sheet's peek, 4 half, 5 full, 6 the way, 7 a chip's search, 8 dark"
    sizes = [("pixel8", 411, 914, "pixel8"), ("iphone", 402, 874, "iphone_island"), ("small", 360, 640, "off"),
             ("landscape", 914, 411, "pixel8")]
    tests = []
    for tag, w, h, preset in sizes:
        tests.append({
            "name": f"azul_maps_phone_{tag}_search_place_way_host",
            "description": f"AzMaps in a {w} x {h} window ({preset}), as a phone shows it: the search bar and the chips on top of the map, the "
                           f"sheet from the bottom ({'the panel at the left: a phone on its side is wide' if w > h else 'peek, half, full'}). "
                           f"Pictures, in order: {names}.",
            "only_on": DESKTOPS,
            "steps": window(tag, w, h, preset) + phone_steps() if w < h else window(tag, w, h, preset) + desktop_steps(),
        })
    # The ghosts of the Android audit (the buttons of the title drawn twice on the device, once at the place the first layout gave
    # them): the engine's own soundness check - every pixel that differs from a full repaint lies inside the frame's damage.
    tests.append({
        "name": "azul_maps_phone_insets_arriving_leave_no_stale_pixels_host",
        "description": "The system's insets arrive after the first frame (a display preset stands for them): everything that moved is "
                       "repainted where it was and where it is, and nothing is left of the first layout (no pictures).",
        "only_on": DESKTOPS,
        "steps": [op("resize", width=411.0, height=914.0), op("set_display_preset", preset="off")] + settle()
                 + [op("snapshot_frame", **{"as": "first_layout"}), op("set_display_preset", preset="pixel8")] + settle()
                 + [op("assert_damage_covers_changes", vs="first_layout")],
    })
    tests.append({
        "name": "azul_maps_phone_search_place_way_device",
        "description": "AzMaps on the phone itself (the window is the phone's, upright: no resize, no display preset). Pictures, in order: "
                       + names + ".",
        "only_on": PHONES,
        "steps": phone_steps(),
    })
    return tests


FILES = {"maps_desktop.json": desktop_file, "maps_phone.json": phone_file}


def main():
    check = "--check" in sys.argv
    bad = 0
    os.makedirs(OUT, exist_ok=True)
    for name, make in FILES.items():
        text = json.dumps(make(), indent=2, ensure_ascii=False) + "\n"
        path = os.path.join(OUT, name)
        if check:
            have = open(path, encoding="utf-8").read() if os.path.exists(path) else None
            if have != text:
                print("%s differs from what the generator writes" % path)
                bad = 1
        else:
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
            print("wrote %s (%d tests)" % (path, len(json.loads(text))))
    return bad


if __name__ == "__main__":
    sys.exit(main())
