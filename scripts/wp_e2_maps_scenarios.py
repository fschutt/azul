#!/usr/bin/env python3
"""Writes AzMaps' scenario files, examples/azul-maps/e2e/*.json, from one description of the steps.

    python3 scripts/wp_e2_maps_scenarios.py [--check]

The files are what QA re-runs unchanged on every platform (scripts/ci/e2e_matrix.py: the desktop host, the Android emulator, the
iOS Simulator): the steps are written once here. ONE FILE PER TEST: the app of a file runs all its tests in one process, so a
test that follows another starts with the state the first left (a search typed, a sheet drawn up); a file of its own is a fresh
start. A `host` test resizes the window to an acceptance size of the brief (the desktop 1280x800, a Pixel 8, an iPhone 16 Pro, a
small phone, a phone on its side), the `device` test is the phone itself (no resize, no display preset); `only_on` says where each
runs. `--check` fails when a file on disk is not what this script writes (the generator is the source, the files are its output).

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


def results_in():
    """The search rested, the geocoder worker answered, the window was rebuilt for the answer (a chip is a search too)."""
    return [op("wait", ms=600, real=True), op("tick_ms", ms=300), op("wait", ms=900, real=True)] + frames(3) + [op("tick_ms", ms=200)] + frames(2)


def map_redrawn():
    """The map's tiles are drawn again in the other style a moment after the window is (the map's own render, not the window's)."""
    return [op("wait", ms=1500, real=True)] + frames(3)


def tiles():
    """The world's tiles ship in the binary; a network brings the finer ones: a real wait, short."""
    return [op("wait", ms=3000, real=True)] + frames(3)


def settings_page():
    """The gear opens azul-appkit's settings page in the window itself (a full-window view with an OK and a Cancel): a picture of it, then
    Cancel, which closes it on every device (a phone has no Escape)."""
    return (click("#__azmaps_settings") + [op("assert_exists", selector="#appkit-settings")] + shot()
            + click("#appkit-settings-cancel") + [op("assert_not_exists", selector="#appkit-settings"), op("assert_exists", selector="#__azmaps_map")])


def window(name, width, height, preset):
    return [op("set_display_preset", preset=preset), op("resize", width=float(width), height=float(height))] + frames(3) + [op("wait_settled")]


def press(key):
    """A key down and up on the window: with nothing focused the window's own key handler hears it (the system's Back arrives as an Escape)."""
    return [op("key_down", key=key), op("key_up", key=key)] + frames(2)


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
    s += type_search("Stuttgart") + [op("assert_exists", selector="#__azmaps_results")] + shot()          # 2 results
    s += click("#__azmaps_suggestion-0") + [op("assert_exists", selector="#__azmaps_place")] + tiles() + shot()  # 3 the place (the map flew there: its tiles)
    s += click("#__azmaps_place-share") + [op("assert_exists", selector="#__azmaps_link-copied")] + shot()   # 3b Share: the notice
    s += click("#__azmaps_place-route") + [op("assert_exists", selector="#__azmaps_travel")] + shot()  # 4 the way
    s += click("#__azmaps_back") + click("#__azmaps_back") + [op("assert_exists", selector="#__azmaps_chips")]
    s += click("#__azmaps_chip-0") + results_in() + [op("assert_exists", selector="#__azmaps_results")] + shot()   # 5 a chip's search
    s += [op("set_mode", mode="dark")] + map_redrawn() + shot()   # 6 dark
    # the map is drawn in the light style again (its tiles are fetched in it, a gray ground until they are in): wait, then fold the panel
    s += [op("set_mode", mode="light")] + tiles() + click("#__azmaps_panel-toggle") + [op("assert_exists", selector="#__azmaps_panel-show")] + shot()  # 7 folded
    s += click("#__azmaps_panel-show") + [op("assert_exists", selector="#__azmaps_panel")]
    s += settings_page()                                          # 8 the settings page
    return s


def desktop_file():
    desc = ("AzMaps on a desktop, 1280 x 800: the map fills the window, the panel at the left (search, chips, then the results, the place, "
            "the way), the zoom, locate and map-style keys at the right. Pictures, in order: 1 first look, 2 the results of 'Stuttgart', 3 the "
            "place, 3b Share (the notice that the link is on the clipboard), 4 the way (Route), 5 a chip's search, 6 dark, 7 the panel folded away to its search key, "
            "8 the settings page (the gear). (The map styles' popover is a window of its own: the host's picture is of the main window, so no step opens it.)")
    return [{
        "name": "azul_maps_desktop_panel_search_place_way",
        "description": desc,
        "only_on": DESKTOPS,
        "steps": window("desktop", 1280, 800, "off") + desktop_steps(),
    }]


# ---- the phone ----

def sheet_at(height, root_top, root_height, tolerance=1.5):
    """The sheet's own box when it is `height` tall in a root `root_height` tall that starts `root_top` under the window's top: it
    lies on the root's lower edge, so its top edge is `root_height - height` under the root's top."""
    return [op("assert_layout", selector="#__azmaps_sheet", property="y", expected=root_top + root_height - height, tolerance=tolerance),
            op("assert_layout", selector="#__azmaps_sheet", property="height", expected=height, tolerance=tolerance)]


def phone_steps(root_top=None, root_height=None):
    """The walk of a phone upright. `root_top` / `root_height`: where the window's root starts and how tall it is when a display
    preset stands for the bars (a host test), None on a device (its own bars): with them the boxes of the bar and the sheet are
    asserted, which is what keeps a full sheet from reaching over the search bar."""
    boxes = root_top is not None
    s = [op("set_mode", mode="light")] + tiles()
    s += [op("assert_exists", selector="#__azmaps_map"), op("assert_exists", selector="#__azmaps_sheet"),
          op("assert_exists", selector="#__azmaps_search-bar"), op("assert_exists", selector="#__azmaps_chips"),
          op("assert_exists", selector="#__azmaps_layers"), op("assert_exists", selector="#__azmaps_locate"),
          op("assert_not_exists", selector="#__azmaps_panel")]
    if boxes:
        # the bar starts 8 px under the root's top on a device and under the window controls (44 px) on a host window: both pass
        s += [op("assert_layout", selector="#__azmaps_search-bar", property="y", expected=root_top + 26.0, tolerance=20.0)]
        s += sheet_at(168.0, root_top, root_height)
    s += shot()                                                   # 1 first look: the bar, the chips, the sheet at its peek
    s += type_search("Stuttgart") + [op("assert_exists", selector="#__azmaps_results")] + shot()      # 2 results
    s += click("#__azmaps_suggestion-0") + [op("assert_exists", selector="#__azmaps_place")] + tiles() + shot()  # 3 the place at its peek
    s += grip_keys("Up") + shot()                                 # 4 half
    if boxes:
        s += sheet_at(root_height / 2.0, root_top, root_height)
    s += grip_keys("Up") + shot()                                 # 5 full
    if boxes:
        # the full sheet leaves the search bar in sight: the top (8 px on a device, 44 under a host window's controls), the bar's 56
        # and 8 px twice (ui::TOP_ROOM) under the root's top: 80 or 116 px
        s += sheet_at(root_height - 98.0, root_top, root_height, tolerance=20.0)
    s += grip_keys("Down", "Down") + [op("assert_exists", selector="#__azmaps_place-route")]
    s += click("#__azmaps_place-route") + [op("assert_exists", selector="#__azmaps_travel")] + shot()  # 6 the way
    s += click("#__azmaps_back") + click("#__azmaps_back") + click("#__azmaps_chip-0") + results_in()
    s += [op("assert_exists", selector="#__azmaps_results")] + shot()                                # 7 a chip's search
    s += [op("set_mode", mode="dark")] + map_redrawn() + shot()                                      # 8 dark
    s += [op("set_mode", mode="light")]
    s += settings_page()                                                                             # 9 the settings page
    return s


def back_steps(root_top=None, root_height=None):
    """Back (an Escape; the system's Back on a phone arrives as one) steps out one thing at a time: the way to its place, the place to the
    search, the search's text, then (a phone) the sheet drawn up; each step claims the key."""
    s = [op("set_mode", mode="light")] + tiles()
    s += type_search("Stuttgart") + click("#__azmaps_suggestion-0") + [op("assert_exists", selector="#__azmaps_place")]
    s += click("#__azmaps_place-route") + [op("assert_exists", selector="#__azmaps_travel")]
    s += press("Escape") + [op("assert_exists", selector="#__azmaps_place"), op("assert_not_exists", selector="#__azmaps_travel")]
    s += press("Escape") + [op("assert_exists", selector="#__azmaps_chips"), op("assert_not_exists", selector="#__azmaps_place"),
                            op("assert_exists", selector="#__azmaps_search-clear")]
    s += press("Escape") + [op("assert_not_exists", selector="#__azmaps_search-clear")]
    if root_top is not None:
        s += grip_keys("Up") + sheet_at(root_height / 2.0, root_top, root_height)
        s += press("Escape") + sheet_at(168.0, root_top, root_height)
    return s


PICTURES = "1 first look, 2 the results of 'Stuttgart', 3 the place at the sheet's peek, 4 half, 5 full, 6 the way, 7 a chip's search, 8 dark, 9 the settings page (the gear)"

# tag, width, height, display preset, (status bar, navigation bar) the preset stands for
UPRIGHT = [
    ("pixel8", 411, 914, "pixel8", 48.0, 24.0),
    ("iphone", 402, 874, "iphone_island", 59.0, 34.0),
    ("small", 360, 640, "off", 0.0, 0.0),
]


def phone_files():
    out = {}
    for tag, w, h, preset, top, bottom in UPRIGHT:
        out[f"maps_phone_{tag}.json"] = [{
            "name": f"azul_maps_phone_{tag}_search_place_way_host",
            "description": f"AzMaps in a {w} x {h} window ({preset}), as a phone shows it: the search bar and the chips on top of the map, "
                           f"the sheet from the bottom (peek, half, full). Pictures, in order: {PICTURES}.",
            "only_on": DESKTOPS,
            "steps": window(tag, w, h, preset) + phone_steps(root_top=top, root_height=h - top - bottom),
        }]
    out["maps_back_phone.json"] = [{
        "name": "azul_maps_back_steps_out_one_view_at_a_time_phone_host",
        "description": "Back (an Escape; the system's Back arrives as one) in a Pixel 8 sized window: the way goes to its place, the place to the "
                       "search, the search's text goes, the sheet drawn up comes down (no pictures).",
        "only_on": DESKTOPS,
        "steps": window("pixel8", 411, 914, "pixel8") + back_steps(root_top=48.0, root_height=914.0 - 48.0 - 24.0),
    }]
    out["maps_phone_landscape.json"] = [{
        "name": "azul_maps_phone_landscape_search_place_way_host",
        "description": "AzMaps in a 914 x 411 window (pixel8, turned on its side): wide, so the panel at the left, as on a desktop. Pictures, "
                       "in order: 1 first look, 2 the results of 'Stuttgart', 3 the place, 3b Share, 4 the way, 5 a chip's search, 6 dark, 7 the "
                       "panel folded away, 8 the settings page.",
        "only_on": DESKTOPS,
        "steps": window("landscape", 914, 411, "pixel8") + desktop_steps(),
    }]
    # The ghosts of the Android audit (the buttons of the title drawn twice on the device, once at the place the first layout gave
    # them): the engine's own soundness check - every pixel that differs from a full repaint lies inside the frame's damage.
    out["maps_phone_insets.json"] = [{
        "name": "azul_maps_phone_insets_arriving_leave_no_stale_pixels_host",
        "description": "The system's insets arrive after the first frame (a display preset stands for them): everything that moved is "
                       "repainted where it was and where it is, and nothing is left of the first layout (no pictures).",
        "only_on": DESKTOPS,
        "steps": [op("resize", width=411.0, height=914.0), op("set_display_preset", preset="off")] + settle()
                 + [op("snapshot_frame", **{"as": "first_layout"}), op("set_display_preset", preset="pixel8")] + settle()
                 + [op("assert_damage_covers_changes", vs="first_layout")],
    }]
    out["maps_phone_device.json"] = [{
        "name": "azul_maps_phone_search_place_way_device",
        "description": "AzMaps on the phone itself (the window is the phone's, upright: no resize, no display preset). Pictures, in order: "
                       + PICTURES + ".",
        "only_on": PHONES,
        "steps": phone_steps(),
    }]
    return out


def files():
    """File name -> the tests it holds."""
    out = {"maps_desktop.json": desktop_file()}
    out["maps_search_limit.json"] = [{
        "name": "azul_maps_search_takes_at_most_256_characters_desktop",
        "description": "A text longer than the search takes (300 characters, a paste) is refused with a notice, not searched (no pictures).",
        "only_on": DESKTOPS,
        "steps": window("desktop", 1280, 800, "off") + [op("set_mode", mode="light")] + click("#__azmaps_search")
                 + [op("text_input", text="x" * 300)] + frames(3) + [op("assert_exists", selector="#__azmaps_link-copied")],
    }]
    out["maps_back_desktop.json"] = [{
        "name": "azul_maps_back_steps_out_one_view_at_a_time_desktop",
        "description": "Back (an Escape) in a 1280 x 800 window: the way goes to its place, the place to the search, the search's text goes "
                       "(no pictures).",
        "only_on": DESKTOPS,
        "steps": window("desktop", 1280, 800, "off") + back_steps(),
    }]
    out.update(phone_files())
    return out


def main():
    check = "--check" in sys.argv
    bad = 0
    os.makedirs(OUT, exist_ok=True)
    made = files()
    for name, tests in made.items():
        text = json.dumps(tests, indent=2, ensure_ascii=False) + "\n"
        path = os.path.join(OUT, name)
        if check:
            have = open(path, encoding="utf-8").read() if os.path.exists(path) else None
            if have != text:
                print("%s differs from what the generator writes" % path)
                bad = 1
        else:
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
            print("wrote %s (%d tests)" % (path, len(tests)))
    # a file of the folder the generator does not write is a file nobody keeps up
    for name in sorted(os.listdir(OUT)):
        if name.endswith(".json") and name not in made:
            print("%s is not written by the generator (list it for the cleanup, or add it here)" % os.path.join(OUT, name))
            bad = 1
    return bad


if __name__ == "__main__":
    sys.exit(main())
