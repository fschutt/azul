#!/usr/bin/env python3
"""Writes AzCode's scenario files, examples/azul-code/e2e/*.json, from one description of the steps.

    python3 scripts/wp_e2_code_scenarios.py [--check]

The files are what QA re-runs unchanged on every platform (scripts/ci/e2e_matrix.py: the desktop host, the Android emulator, the iOS
Simulator). The steps are written once here; ONE FILE HOLDS ONE TEST, because the runner plays all the tests of a file in one process and a
second test would start with the state the first left (the drawer open, a file edited, the sheet raised). A `host` test resizes the window to a
size of the brief (the desktop 1280x800, a Pixel 8, an iPhone 16 Pro, a small phone, a phone on its side); the `device` test has the phone
itself for the window (no resize, no display preset: the first start of AzCode on a phone has opened the sample workspace and its
src/main.rs); `only_on` says where each runs. `--check` fails when a file on disk is not what this script writes, or when the folder holds a
file this script does not write (the generator is the source, the files are its output).

The ops are the engine's (layout/src/e2e/full.rs). Every picture is a `take_screenshot` step, numbered in capture order from 1 in each
run of the app (AZ_E2E_SHOT_DIR); each test's `description` names them in that order.

A host run starts AzCode with no arguments and no folder: the sample workspace is the welcome page's third link (it is written once into
the data folder by a worker thread, then listed: a real wait). The desktop's shortcuts take the primary key: Ctrl on Linux and Windows,
Cmd (`meta`) on macOS, so the desktop tests exist once per kind.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "examples", "azul-code", "e2e")

PHONES = ["android", "ios"]
# the desktops by the key their shortcuts take
PRIMARY = {"ctrl": ["linux", "windows"], "meta": ["macos"]}


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


def exists(*selectors):
    return [op("assert_exists", selector=s) for s in selectors]


def missing(*selectors):
    return [op("assert_not_exists", selector=s) for s in selectors]


def rest(ms):
    """A real wait: the worker threads (the sample being written, a folder listed, a file read, git asked) answer in real time, then the
    window is rebuilt for the answer."""
    return [op("wait", ms=ms, real=True)] + frames(3)


def window(width, height, preset):
    return [op("set_display_preset", preset=preset), op("resize", width=float(width), height=float(height))] + frames(3) + [op("wait_settled")]


def chord(key, primary, shift=False):
    """A shortcut: the primary key (and Shift) held, `key` pressed and let go."""
    mods = {primary: True}
    if shift:
        mods["shift"] = True
    return [op("key_down", key=key, modifiers=mods), op("key_up", key=key, modifiers={})] + frames(2)


def layout_is(selector, **expected):
    """The node's box (x, y, width, height in window px) is where the layout puts it."""
    return [op("assert_layout", selector=selector, property=prop, expected=float(value), tolerance=1.5) for prop, value in expected.items()]


# phone_model.rs: the top bar's and the status bar's heights, the sheet's top margin (a finger tall and the hairline), px
TOP_BAR = 49.0
STATUS_BAR = 49.0


def press(key):
    return [op("key_down", key=key), op("key_up", key=key)] + frames(2)


def grip_keys(*keys):
    """The keys on the focused grip of the sheet: the same detents a drag settles on, on any device."""
    out = [op("focus_node", selector=".__azul-native-bottom-sheet-grip")] + frames(1)
    for key in keys:
        out += [op("key_down", key=key), op("key_up", key=key)] + frames(2)
    return out


# the explorer's rows are ids of the keys: `__azcode_tree-` and the key, every character but a letter, a digit, `_` and `-` as `-`
SRC = "#__azcode_tree-src-"
MAIN = "#__azcode_tree-src-main-rs"


def open_sample():
    """The welcome page's link: the sample is written once, then listed."""
    return click("#__azcode_open-sample") + rest(3500) + exists(SRC)


def open_main():
    """src/ opened in the explorer, then src/main.rs (the file is read on a worker thread)."""
    return click(SRC) + rest(800) + exists(MAIN) + click(MAIN) + rest(900) + exists("#__azcode_tab-0")


# ---- the desktop ----

def desktop_steps(primary):
    s = [op("set_mode", mode="dark")]
    s += exists("#__azcode_activity-explorer", "#__azcode_activity-search", "#__azcode_activity-git", "#__azcode_activity-run",
                "#__azcode_activity-settings", "#__azcode_welcome", "#__azcode_open-sample")
    s += missing("#__azcode_panel", "#__azcode_drawer", "#__azcode_top-bar")
    s += shot()                                                        # 1 first look: the welcome page, the four views' icons
    s += open_sample() + shot()                                        # 2 the sample workspace in the explorer
    s += open_main() + exists("#__azcode_editor") + shot()             # 3 main.rs open: tabs, breadcrumbs, the code
    s += click("#__azcode_editor") + chord("f", primary) + click("#__azcode_find-input")
    s += [op("text_input", text="counts")] + frames(3) + exists("#__azcode_find-bar", "#__azcode_find-count") + shot()   # 4 find
    s += press("escape")
    s += click("#__azcode_activity-git") + rest(1200) + exists("#__azcode_git-panel") + shot()          # 5 source control (the sample is no repository)
    s += click("#__azcode_activity-run") + rest(600) + exists("#__azcode_run-panel", "#__azcode_task-0") + shot()   # 6 run: cargo's tasks
    s += click("#__azcode_activity-search") + exists("#__azcode_search-panel") + click("#__azcode_activity-explorer")
    # The panel's three views by the keys of the card's commands (azul's status bar has no ids for its segments to click; a keyboard
    # user has these chords, so they are the better test anyway): Mod+Shift+M problems, Mod+Shift+U output, Mod+J the terminal
    s += chord("m", primary, shift=True) + exists("#__azcode_panel", "#__azcode_panel-tab-problems") + shot()   # 7 the panel: problems
    s += chord("u", primary, shift=True) + exists("#__azcode_output") + shot()                           # 8 the panel: output
    s += chord("j", primary) + rest(1500) + exists("#__azcode_panel") + shot()                           # 9 the panel: terminal
    s += click("#__azcode_panel-close") + missing("#__azcode_panel")
    s += chord("p", primary, shift=True) + exists("#__azcode_command-palette") + shot()                # 10 the command palette
    s += press("escape")
    s += [op("set_mode", mode="light")] + shot()                       # 11 light
    s += [op("set_mode", mode="dark")]
    return s


def desktop_files():
    desc = ("AzCode on a desktop, 1280 x 800: the workbench of the card - the activity bar (explorer, search, source control, run), the "
            "side bar, the editor with its file tabs, the bottom panel (terminal, problems, output) and the status bar. Pictures, in "
            "order: 1 first look (the welcome page), 2 the sample workspace in the explorer, 3 src/main.rs open, 4 the find bar, "
            "5 source control (the sample is no repository), 6 run (the tasks of its Cargo.toml), 7 the panel's Problems, 8 Output, "
            "9 Terminal, 10 the command palette, 11 light.")
    out = {}
    for primary, desktops in PRIMARY.items():
        tag = "cmd" if primary == "meta" else "ctrl"
        out[f"code_desktop_{tag}.json"] = [{
            "name": f"azul_code_desktop_workbench_{tag}_host",
            "description": desc,
            "only_on": desktops,
            "steps": window(1280, 800, "off") + desktop_steps(primary),
        }]
    return out


# ---- the phone ----

def phone_steps(first_start, width=None, height=None, bars=None):
    """first_start: the device's first look is src/main.rs (the phone opened the sample workspace and its first file itself); a host
    window has the welcome page and opens the sample and the file through the UI. bars: (top, bottom, left, right) px the display
    preset keeps for the status bar, the home indicator and the cutout of a window `width` x `height`: the engine insets the ROOT by
    them, so the top bar starts under the status bar, the status bar ends over the home indicator, and the sheet drawn up
    reaches its top margin under the root's top and no further."""
    s = [op("set_mode", mode="dark")]
    s += exists("#__azcode_top-bar", "#__azcode_drawer-button", "#__azcode_palette-button", "#__azcode_status-bar")
    s += missing("#__azcode_drawer", "#__azcode_panel", "#__azcode_key-row", "#__azcode_activity-explorer")
    if bars:
        top, bottom, left, right = bars
        # the root the engine insets: the sheet's container (the box the sheet lies over) is exactly it
        s += layout_is("#__azcode_phone-root", x=left, y=top, width=width - left - right, height=height - top - bottom)
        s += layout_is("#__azcode_top-bar", x=left, y=top, width=width - left - right, height=TOP_BAR)
        s += layout_is("#__azcode_status-bar", y=height - bottom - STATUS_BAR, height=STATUS_BAR)
    if first_start:
        s += rest(4500) + exists("#__azcode_tab-0", "#__azcode_editor")
        s += shot()                                                    # 1 first look: the sample's main.rs, the editor is the screen
    else:
        s += exists("#__azcode_welcome")
        s += shot()                                                    # 1 first look: the welcome page, the top bar, the status bar
        # the sample is written and listed; its tree is in the drawer, which is closed
        s += click("#__azcode_open-sample") + rest(3500)
    s += click("#__azcode_drawer-button") + exists("#__azcode_drawer", "#__azcode_scrim", "#__azcode_activity-git", "#__azcode_activity-run", SRC)
    s += shot()                                                        # 2 the drawer: the explorer over the scrim
    if first_start:
        s += click(SRC) + rest(800) + exists(MAIN) + click(MAIN) + rest(900)
    else:
        s += open_main()
    s += missing("#__azcode_drawer") + exists("#__azcode_editor")
    s += shot()                                                        # 3 the file's tab: the drawer has given way to the editor
    s += click("#__azcode_editor") + exists("#__azcode_key-row", "#__azcode_key-0")
    s += shot()                                                        # 4 the key row (Tab, braces, arrows) over the keyboard's place
    # the Right arrow moves the caret and edits nothing: the file stays clean, so its tab can be closed at the end (a dirty one stays and
    # says why); the phone itself, last of its file, also types a pair of braces
    s += click("#__azcode_key-10") + frames(2)
    if first_start:
        s += click("#__azcode_key-2") + shot()                         # 4b braces typed by the key row
    s += click("#__azcode_status-problems") + exists("#__azcode_sheet", "#__azcode_panel-tab-problems")
    s += shot()                                                        # 5 the panel as a sheet, at its detent
    if bars:
        # Half: half the root (the container is the root, not the window)
        s += layout_is("#__azcode_sheet", y=top + (height - top - bottom) / 2.0, height=(height - top - bottom) / 2.0)
    s += grip_keys("Up") + shot()                                      # 6 the sheet drawn up
    if bars:
        # drawn up, the sheet stops under the top bar: its top is the root's top and the top bar's height under the window's top, its
        # foot the root's foot
        s += layout_is("#__azcode_sheet", y=top + TOP_BAR, height=height - top - bottom - TOP_BAR)
    s += grip_keys("Down", "Down") + shot()                            # 7 and pushed down to its grip
    # its header is out of reach at the grip: raised first, and let to settle (a click in the middle of the animation lands where the
    # button was)
    s += grip_keys("Up") + settle() + click("#__azcode_panel-close") + missing("#__azcode_sheet")
    s += click("#__azcode_drawer-button") + click("#__azcode_activity-git") + exists("#__azcode_git-panel") + rest(1000)
    s += shot()                                                        # 8 source control in the drawer
    s += click("#__azcode_activity-run") + exists("#__azcode_run-panel") + shot()                  # 9 run
    # the drawer closes on its scrim (a tap on the strip beside it: the middle of the scrim is under the drawer) or, where the
    # window's size is not known here, on Escape
    if width:
        s += [op("click", x=float(width) - 12.0, y=float(height) / 2.0)] + frames(2) + missing("#__azcode_drawer")
    else:
        s += press("escape") + missing("#__azcode_drawer")
    s += [op("set_mode", mode="light")] + shot()                       # 10 light
    s += [op("set_mode", mode="dark")]
    return s


def phone_files():
    names = ("1 first look, 2 the drawer (explorer), 3 the file in the editor, 4 the key row, 5 the panel's sheet, 6 drawn up, 7 pushed "
             "down, 8 source control, 9 run, 10 light (the phone itself has a picture of the braces typed by the key row after the 4th)")
    # what the display preset keeps (core/src/display_features.rs: the status bar, the navigation bar, the cutout at the left in
    # landscape): top, bottom, left, right
    sizes = [("pixel8", 411, 914, "pixel8", (48.0, 24.0, 0.0, 0.0)), ("iphone", 402, 874, "iphone_island", (59.0, 34.0, 0.0, 0.0)),
             ("small", 360, 640, "off", (0.0, 0.0, 0.0, 0.0)), ("landscape", 914, 411, "pixel8", (24.0, 24.0, 36.0, 0.0))]
    out = {}
    for tag, w, h, preset, bars in sizes:
        out[f"code_phone_{tag}.json"] = [{
            "name": f"azul_code_phone_{tag}_drawer_sheet_keys_host",
            "description": f"AzCode in a {w} x {h} window ({preset}), as a phone shows it: the editor is the screen, the explorer a drawer "
                           f"over a scrim, the panel a sheet from the bottom, a key row over the keyboard's place, one status bar "
                           f"line. Pictures, in order: {names}.",
            "only_on": ["linux", "macos", "windows"],
            "steps": window(w, h, preset) + phone_steps(False, w, h, bars),
        }]
    # The system's insets arrive after the first frame (a display preset stands for them): the top bar moves under the status bar.
    out["code_phone_insets.json"] = [{
        "name": "azul_code_phone_insets_arriving_leave_no_stale_pixels_host",
        "description": "The system's insets arrive after the first frame (a display preset stands for them): everything that moved is "
                       "repainted where it was and where it is, and nothing is left of the first layout (no pictures).",
        "only_on": ["linux", "macos", "windows"],
        "steps": [op("resize", width=411.0, height=914.0), op("set_display_preset", preset="off")] + settle()
                 + [op("snapshot_frame", **{"as": "first_layout"}), op("set_display_preset", preset="pixel8")] + settle()
                 + [op("assert_damage_covers_changes", vs="first_layout")],
    }]
    out["code_phone_device.json"] = [{
        "name": "azul_code_phone_first_start_drawer_sheet_keys_device",
        "description": "AzCode on the phone itself (the window is the phone's, upright: no resize, no display preset): the first start has "
                       "opened the sample workspace and its src/main.rs. Pictures, in order: " + names + ".",
        "only_on": PHONES,
        "steps": phone_steps(True),
    }]
    return out


def files():
    """File name -> the tests it holds (one each)."""
    out = desktop_files()
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
            print("wrote %s (%d test)" % (path, len(tests)))
    # a file of the folder the generator does not write is a file nobody keeps up
    for name in sorted(os.listdir(OUT)):
        if name.endswith(".json") and name not in made:
            print("%s is not written by the generator (remove it, or add it here)" % os.path.join(OUT, name))
            bad = 1
    return bad


if __name__ == "__main__":
    sys.exit(main())
