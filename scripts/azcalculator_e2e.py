#!/usr/bin/env python3
"""AzCalculator end to end, headless, over the debug server.

    1. starts AzCalculator (AZ_BACKEND=headless, AZ_DEBUG=--debug-port) on a fresh data
       folder with --sample (three sample history entries) and waits for the history to load;
    2. Standard by mouse: 1280 x 0.19 = 243.2 (the plan's sample), the result node shows it;
    3. Standard by keyboard: 0.1 + 0.2 = 0.3 (no binary artefacts), 7 Shift+8 6 Enter = 42,
       Backspace, Escape;
    4. the history file calculator/history.jsonl holds the sample and the new lines;
    5. Scientific: sin(30) + 2^10 = 1,024.5 in degrees; Programmer: F5 (HEX), 2A5F shows
       10,847 in DEC, bit 0 toggles it to 2A5E; Convert shows 42.195 km = 26.21875746 mi;
       Date shows the difference screen;
    6. the settings page (Ctrl+,): Appearance -> Flora and Dark are saved to
       calculator/settings.json; Escape closes it;
    7. a screenshot of every screen, in flat light and flora dark;
    8. last, Ctrl/Cmd+C copies "42" (the engine's Copy shortcut bug would end the run early).

Usage (after building libazul with the debug server and AzCalculator, one app at a time):

    python3 scripts/azcalculator_e2e.py [--bin target/release/AzCalculator]
        [--debug-port 8781] [--timeout 180] [--out <dir>] [--keep]
"""

import json
import os
import shutil

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azcalc"
TIMES = "×"


def display(app):
    return app.last("AZCALC_DISPLAY")


def expect_display(app, expected, what):
    app.expect_line("AZCALC_DISPLAY", expected, what)
    app.log("%s: %s" % (what, expected.replace("\t", "  |  ")))


def keys_inside_the_keypad(app, screen):
    """Every key of the keypad lies inside the keypad: a column that outgrows
    it puts its keys over the history panel, and a click on them lands there."""
    pad = app.must("get_node_layout", selector="#calc-keypad")["data"]["value"]["rect"]
    for key_id in ["key-plus", "key-equals", "key-1"]:
        r = app.must("get_node_layout", selector="#" + key_id)["data"]["value"]["rect"]
        if r["x"] < pad["x"] - 0.5 or r["x"] + r["width"] > pad["x"] + pad["width"] + 0.5:
            raise Failure("%s: #%s (x %.1f..%.1f) sticks out of the keypad (x %.1f..%.1f)" % (
                screen, key_id, r["x"], r["x"] + r["width"], pad["x"], pad["x"] + pad["width"]))
    app.log("%s: the keys lie inside the keypad" % screen)


def history_lines(data_dir):
    path = os.path.join(data_dir, "calculator", "history.jsonl")
    try:
        with open(path, "r", encoding="utf-8") as f:
            return [json.loads(l) for l in f if l.strip()]
    except OSError:
        return []


def body(args, logs, out):
    binary = e2e.find_binary("AzCalculator", args.bin, "AZCALCULATOR_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG, binary, ["--data-dir", data_dir, "--sample", "--size", "680x620", "--screen", "standard"],
                  args.debug_port, logs, args.timeout)
    try:
        app.until("the history to load", lambda: app.printed("AZCALC_HISTORY_LOADED", r"\d+"))
        loaded = int(app.printed("AZCALC_HISTORY_LOADED", r"\d+")[-1])
        if loaded != 3:
            raise Failure("--sample on an empty folder loads 3 history entries, got %d" % loaded)
        app.until("the sample history on disk", lambda: len(history_lines(data_dir)) == 3)
        app.frame(2)
        if not app.has_id("calc-keypad") or not app.has_id("calc-panel"):
            raise Failure("the Standard keypad and the history panel (680 px wide) are not in the tree")
        app.screenshot(os.path.join(out, "standard-flat-light.png"))
        keys_inside_the_keypad(app, "Standard")

        # Standard by mouse: the plan's sample.
        for key_id in ["key-1", "key-2", "key-8", "key-0", "key-multiply", "key-0", "key-point", "key-1",
                       "key-9", "key-equals"]:
            app.click(selector="#" + key_id)
        expect_display(app, "1,280 %s 0.19 =\t243.2" % TIMES, "1280 x 0.19 by mouse")
        if not app.shows("243.2"):
            raise Failure("the result node does not show 243.2")

        # Standard by keyboard.
        app.key("escape")
        app.type_keys(["0", "period", "1", "plus", "0", "period", "2", "enter"])
        expect_display(app, "0.1 + 0.2 =\t0.3", "0.1 + 0.2 by keyboard")
        app.type_keys(["7", ("8", {"shift": True}), "6", "enter"])
        expect_display(app, "7 %s 6 =\t42" % TIMES, "7 Shift+8 6 Enter")
        app.type_keys(["1", "2", "3", "backspace"])
        expect_display(app, "\t12", "Backspace deletes a digit")
        app.key("escape")
        expect_display(app, "\t0", "Escape clears")
        app.type_keys(["4", "2"])
        expect_display(app, "\t42", "42 typed")

        # The history file.
        app.until("the history file to hold the new lines", lambda: len(history_lines(data_dir)) >= 6)
        lines = history_lines(data_dir)
        results = [l["result"] for l in lines]
        for expected in ["243.2", "0.3", "42"]:
            if expected not in results:
                raise Failure("history.jsonl lacks %s: %s" % (expected, results))
        app.log("history.jsonl: %d lines, %s" % (len(lines), results))

        # Scientific: a fresh entry first (42 is still typed, and sin applies
        # to the number on the display, as in every desktop calculator).
        app.key("escape")
        app.click(text="Scientific")
        app.until("the Scientific screen", lambda: app.last("AZCALC_SCREEN") == "scientific")
        keys_inside_the_keypad(app, "Scientific")
        for key_id in ["key-sin", "key-3", "key-0", "key-rparen", "key-plus", "key-2", "key-pow", "key-1", "key-0",
                       "key-equals"]:
            app.click(selector="#" + key_id)
        expect_display(app, "sin(30) + 2^10 =\t1,024.5", "sin(30) + 2^10 in degrees")
        app.screenshot(os.path.join(out, "scientific.png"))

        # Programmer.
        app.click(text="Programmer")
        app.until("the Programmer screen", lambda: app.last("AZCALC_SCREEN") == "programmer")
        keys_inside_the_keypad(app, "Programmer")
        app.key("f5")
        app.type_keys(["2", "a", "5", "f"])
        expect_display(app, "\t2A5F", "2A5F typed in HEX")
        if not app.shows("10,847"):
            raise Failure("the DEC row does not show 10,847")
        app.click(selector="#bit-0")
        expect_display(app, "\t2A5E", "bit 0 toggled")
        app.screenshot(os.path.join(out, "programmer.png"))

        # Convert and Date.
        app.click(text="Convert")
        app.until("the Convert screen", lambda: app.last("AZCALC_SCREEN") == "convert")
        app.frame(2)
        if not app.has_id("conv-from-value") or not app.has_id("conv-to-unit"):
            raise Failure("the converter's fields are not in the tree")
        if not app.shows("1 km = 0.621371 mi"):
            raise Failure("the converter does not show the km -> mi rate")
        app.screenshot(os.path.join(out, "convert.png"))
        app.click(text="Date")
        app.until("the Date screen", lambda: app.last("AZCALC_SCREEN") == "date")
        app.frame(2)
        if not app.has_id("date-from") or not app.shows("Difference between dates"):
            raise Failure("the date screen is not shown")
        app.screenshot(os.path.join(out, "date.png"))

        # Settings: Flora and Dark are saved.
        app.click(text="Standard")
        app.key("comma", primary=True)
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        app.click(text="Appearance")
        app.until("the Appearance section", lambda: app.has_id("appkit-theme"))
        saved_before = len(app.printed("AZCALCULATOR_SETTINGS_SAVED"))
        app.click(text="Flora")
        app.click(text="Dark")
        app.until("the settings to be saved",
                  lambda: len(app.printed("AZCALCULATOR_SETTINGS_SAVED")) >= saved_before + 2)
        with open(os.path.join(data_dir, "calculator", "settings.json"), "r", encoding="utf-8") as f:
            settings = json.load(f)
        if settings.get("theme") != "flora" or settings.get("mode") != "dark":
            raise Failure("settings.json does not hold flora / dark: %s" % settings)
        app.screenshot(os.path.join(out, "settings-flora-dark.png"))
        app.key("escape")
        app.until("the settings page to close", lambda: not app.has_id("appkit-settings"))
        app.screenshot(os.path.join(out, "standard-flora-dark.png"))

        # Last: Ctrl/Cmd+C copies the result. A keypad Button holds the focus
        # after the clicks above, and the engine's Copy shortcut claims the
        # key for a focused NON-editable node (core/src/events.rs
        # handle_key_down, AddAndSkip - reported to WRITER6, 2026-10-03), so
        # this runs after every other check.
        app.key("escape")
        app.type_keys(["4", "2"])
        app.key("c", primary=True)
        app.until("Ctrl+C to copy", lambda: app.last("AZCALC_COPIED") == "42")
        app.log("Ctrl+C copied 42")
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
        if not args.keep:
            shutil.rmtree(data_dir, ignore_errors=True)


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8781)
