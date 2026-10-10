#!/usr/bin/env python3
"""AzCalculator end to end, headless, over the debug server.

    1. starts AzCalculator at its default SMALL size (340x560) on a fresh data folder with
       --sample (three sample history entries) and waits for the history: the window is the small
       standard calculator (AZCALC_MODE micro), no programmer panel, no graph;
    2. by mouse: 1280 x 0.19 = 243.2 (the plan's sample);
    3. TYPED, the way every native shell delivers a keystroke - the key AND the text it typed, in
       one pass: `12*3=` gives 36 with the US `*` (Shift+8) and with the German one (Shift and the
       key right of Ü, the US `]` position: only the TEXT says `*`), the keypad's `*`, `2^10=`,
       `(1+2)*3=`, `sqrt(16)=` (the name typed letter by letter), `5!=`, a German decimal comma;
       Backspace, Escape, Enter; a key that arrives without its text types nothing while the
       calculator holds the keyboard (Windows sends the text in a pass after the key);
    4. the history file calculator/history.jsonl holds the sample and the new lines;
    5. the state survives the sizes: `12*3` typed (no =) is still on the display after widening
       to the programmer view (AZCALC_MODE programmer), growing to the graphing view (AZCALC_MODE
       graph) and shrinking back (AZCALC_MODE micro); = then gives 36;
    6. Programmer (720x560): the bases, bits and bitwise keys are there; F5 (HEX), `2a5f` typed
       shows 10,847 in DEC, bit 0 toggles it to 2A5E; `0xff` switches to HEX by its prefix;
       `6 xor 3=` is 5;
    7. Graphing (1100x820): `sin(x)*x^2` is plotted live (#__azcalc_curve-draft), Enter commits
       it as y1 (AZCALC_PLOT 1, #__azcalc_curve-0), the expression is typeset (#__azcalc_math);
       the wheel zooms and a drag moves the plane (AZCALC_GRAPH changes); `y=2x+1=` adds y2;
    8. Date and Convert by Alt+4 / Alt+5 (the View menu's keys), back with Alt+0 (automatic);
    9. the settings page (Ctrl+,): General -> Flora and Dark are saved to calculator/settings.json;
       About; OK keeps them (screenshots of the three sizes in flora dark); on the page again
       Flat, then Escape (Cancel) puts Flora back into the file;
   10. Ctrl/Cmd+C copies "42", a paste of `2*21` gives 42;
   11. the LOOKS: one short run per look (--theme flat|flora, --mode light|dark), a screenshot of
       each size with a sample on it: <size>-<theme>-<mode>.png.

Usage (after building libazul with the debug server and AzCalculator, one app at a time):

    python3 scripts/azcalculator_e2e.py [--bin target/release/AzCalculator]
        [--debug-port 8781] [--timeout 240] [--out <dir>] [--keep]
"""

import json
import os
import shutil

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azcalc"
TIMES = "×"
# The app's DOM names carry its prefix (examples/azul-calculator/src/ids.rs).
P = "__azcalc_"

# The sizes of the three calculators (the breakpoints: 640 wide, 900 x 700).
MICRO = (340.0, 560.0)
PROGRAMMER = (720.0, 560.0)
GRAPH = (1100.0, 820.0)

# Where a character is on a US keyboard: (key, Shift). Digits and letters are their own key.
US = {
    "*": ("8", True), "(": ("9", True), ")": ("0", True), "^": ("6", True), "!": ("1", True),
    "%": ("5", True), "+": ("equals", True), "=": ("equals", False), "-": ("minus", False),
    "/": ("slash", False), ".": ("period", False), ",": ("comma", False), " ": ("space", False),
}
# ... and on a German one (QWERTZ): `*` is Shift and the key right of Ü - the US `]` key - `(` is
# Shift+8, `=` Shift+0, `/` Shift+7, `+` the `]` key alone, `,` the decimal separator.
GERMAN = {
    "*": ("rbracket", True), "+": ("rbracket", False), "(": ("8", True), ")": ("9", True),
    "=": ("0", True), "/": ("7", True), "-": ("slash", False), ",": ("comma", False),
    ".": ("period", False), " ": ("space", False),
}


def stroke(app, key, text=None, shift=False, alt=False, frames=1):
    """One keystroke as a native shell delivers it: the key's position and, if it typed one, the
    TEXT the user's layout made of it, in one pass (the debug server's `key_down` with `text`)."""
    params = {"key": key, "modifiers": {"shift": shift, "ctrl": False, "alt": alt, "meta": False}}
    if text is not None:
        params["text"] = text
    app.must("key_down", **params)
    app.must("key_up", key=key, modifiers=e2e.RELEASED)
    app.frame(frames)


def type_text(app, text, layout=US):
    """Types `text` character by character on `layout`."""
    for ch in text:
        key, shift = layout.get(ch) or (ch.lower(), ch.isupper())
        stroke(app, key, text=ch, shift=shift)


def expect_display(app, expected, what):
    app.expect_line("AZCALC_DISPLAY", expected, what)
    app.log("%s: %s" % (what, expected.replace("\t", "  |  ")))


def expect_mode(app, mode, what=None):
    app.expect_line("AZCALC_MODE", mode, what or "the %s view" % mode)
    app.frame(2)
    app.log("view: %s" % mode)


def resize(app, size, mode):
    app.must("resize", width=size[0], height=size[1])
    app.frame(2)
    expect_mode(app, mode, "%dx%d is the %s view" % (size[0], size[1], mode))


def focus_calculator(app):
    """The calculator's surface takes the keyboard (it does by itself when it mounts; a script
    makes sure before it types)."""
    app.must("focus_node", selector="#" + P + "calc")
    app.frame(1)


def keys_inside(app, screen, pad, key_ids):
    """Every key named lies inside its keypad: a column that outgrows the pad puts its keys over
    the panel beside it, and a click on them lands there."""
    box = app.must("get_node_layout", selector="#" + P + pad)["data"]["value"]["rect"]
    for key_id in key_ids:
        r = app.must("get_node_layout", selector="#" + P + key_id)["data"]["value"]["rect"]
        if r["x"] < box["x"] - 0.5 or r["x"] + r["width"] > box["x"] + box["width"] + 0.5:
            raise Failure("%s: #%s (x %.1f..%.1f) sticks out of #%s (x %.1f..%.1f)" % (
                screen, key_id, r["x"], r["x"] + r["width"], pad, box["x"], box["x"] + box["width"]))
    app.log("%s: the keys lie inside #%s" % (screen, pad))


def history_lines(data_dir):
    path = os.path.join(data_dir, "calculator", "history.jsonl")
    try:
        with open(path, "r", encoding="utf-8") as f:
            return [json.loads(l) for l in f if l.strip()]
    except OSError:
        return []


def plot_center(app):
    b = app.box("#" + P + "plot")
    return b["x"] + b["width"] / 2.0, b["y"] + b["height"] / 2.0


def body(args, logs, out):
    binary = e2e.find_binary("AzCalculator", args.bin, "AZCALCULATOR_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG, binary, ["--data-dir", data_dir, "--sample", "--size", "%dx%d" % MICRO],
                  args.debug_port, logs, args.timeout)
    try:
        app.until("the history to load", lambda: app.printed("AZCALC_HISTORY_LOADED", r"\d+"))
        loaded = int(app.printed("AZCALC_HISTORY_LOADED", r"\d+")[-1])
        if loaded != 3:
            raise Failure("--sample on an empty folder loads 3 history entries, got %d" % loaded)
        app.until("the sample history on disk", lambda: len(history_lines(data_dir)) == 3)
        expect_mode(app, "micro", "the default size is the small calculator")
        if not app.has_id(P + "keypad") or not app.has_id(P + "memory-row"):
            raise Failure("the small calculator's keypad and memory row are not in the tree")
        if app.has_id(P + "programmer") or app.has_id(P + "plot"):
            raise Failure("the small calculator shows the programmer panel or the graph")
        app.screenshot(os.path.join(out, "micro-flat-light.png"))
        keys_inside(app, "micro", "keypad", ["key-plus", "key-equals", "key-1"])

        # By mouse: the plan's sample.
        for key_id in ["key-1", "key-2", "key-8", "key-0", "key-multiply", "key-0", "key-point", "key-1",
                       "key-9", "key-equals"]:
            app.click(selector="#" + P + key_id)
        expect_display(app, "1,280 %s 0.19 =\t243.2" % TIMES, "1280 x 0.19 by mouse")
        if not app.shows("243.2"):
            raise Failure("the result node does not show 243.2")

        # Typed: the CHARACTER decides, not the key's position.
        focus_calculator(app)
        app.key("escape")
        type_text(app, "12*3=")
        expect_display(app, "12 %s 3 =\t36" % TIMES, "12*3= typed (US: Shift+8)")
        type_text(app, "7*6=", GERMAN)
        expect_display(app, "7 %s 6 =\t42" % TIMES, "7*6= typed on a German layout (Shift + the ] key)")
        stroke(app, "numpad7", "7")
        stroke(app, "numpadmultiply", "*")
        stroke(app, "numpad6", "6")
        app.key("numpadenter")
        expect_display(app, "7 %s 6 =\t42" % TIMES, "7*6 on the keypad, keypad Enter")
        type_text(app, "2^10=")
        expect_display(app, "2^10 =\t1,024", "2^10= typed")
        type_text(app, "(1+2)*3=")
        expect_display(app, "(1 + 2) %s 3 =\t9" % TIMES, "(1+2)*3= typed")
        type_text(app, "sqrt(16)=")
        expect_display(app, "√(16) =\t4", "sqrt(16)= typed letter by letter")
        type_text(app, "5!=")
        expect_display(app, "5! =\t120", "5!= typed")
        type_text(app, "2,5*2=", GERMAN)
        expect_display(app, "2.5 %s 2 =\t5" % TIMES, "a German decimal comma")
        type_text(app, "123")
        app.key("backspace")
        expect_display(app, "\t12", "Backspace deletes a digit")
        app.key("escape")
        expect_display(app, "\t0", "Escape clears")
        # Enter - a key that types nothing - evaluates (no binary artefacts).
        type_text(app, "0.1+0.2")
        app.key("enter")
        expect_display(app, "0.1 + 0.2 =\t0.3", "0.1 + 0.2, Enter")
        # A focused calculator waits for a key's TEXT (Windows sends it after the key): a key
        # that arrives without it types nothing twice and nothing wrong.
        app.key("8", shift=True)
        expect_display(app, "0.1 + 0.2 =\t0.3", "a key without its text")

        # The history file.
        app.until("the history file to hold the new lines", lambda: len(history_lines(data_dir)) >= 10)
        lines = history_lines(data_dir)
        results = [l["result"] for l in lines]
        for expected in ["243.2", "36", "1,024", "4", "0.3", "42"]:
            if expected not in results:
                raise Failure("history.jsonl lacks %s: %s" % (expected, results))
        app.log("history.jsonl: %d lines" % len(lines))

        # The state survives the sizes.
        app.key("escape")
        type_text(app, "12*3")
        expect_display(app, "12 %s\t3" % TIMES, "12*3 typed, no =")
        resize(app, PROGRAMMER, "programmer")
        expect_display(app, "12 %s\t3" % TIMES, "the entry in the programmer view")
        for part in ["bases", "bits", "progpad", "word", "keypad"]:
            if not app.has_id(P + part):
                raise Failure("the programmer view has no #%s%s" % (P, part))
        resize(app, GRAPH, "graph")
        expect_display(app, "12 %s\t3" % TIMES, "the entry in the graphing view")
        for part in ["plot", "math", "functions", "panel", "angle"]:
            if not app.has_id(P + part):
                raise Failure("the graphing view has no #%s%s" % (P, part))
        resize(app, MICRO, "micro")
        expect_display(app, "12 %s\t3" % TIMES, "the entry back in the small view")
        app.key("enter")
        expect_display(app, "12 %s 3 =\t36" % TIMES, "= after the round trip")

        # Programmer.
        resize(app, PROGRAMMER, "programmer")
        keys_inside(app, "programmer", "keypad", ["key-plus", "key-equals", "key-1"])
        keys_inside(app, "programmer", "progpad", ["key-and", "key-shr", "key-f"])
        app.screenshot(os.path.join(out, "programmer-flat-light.png"))
        app.key("escape")
        app.key("f5")
        type_text(app, "2a5f")
        expect_display(app, "\t2A5F", "2A5F typed in HEX")
        if not app.shows("10,847"):
            raise Failure("the DEC row does not show 10,847")
        app.click(selector="#" + P + "bit-0")
        expect_display(app, "\t2A5E", "bit 0 toggled")
        focus_calculator(app)
        app.key("escape")
        app.key("f6")
        type_text(app, "0xff")
        expect_display(app, "\tFF", "0x switches to HEX")
        if not app.shows("255"):
            raise Failure("the DEC row does not show 255 for 0xff")
        app.key("escape")
        type_text(app, "6 xor 3=")
        expect_display(app, "6 XOR 3 =\t5", "the operator typed as a word")

        # Graphing.
        resize(app, GRAPH, "graph")
        app.until("the plot to measure itself", lambda: app.printed("AZCALC_GRAPH"))
        keys_inside(app, "graph", "keypad", ["key-plus", "key-equals", "key-sin"])
        focus_calculator(app)
        app.key("escape")
        type_text(app, "sin(x)*x^2")
        app.until("the live curve of the entry", lambda: app.has_id(P + "curve-draft"))
        app.key("enter")
        app.until("y1 plotted", lambda: app.last("AZCALC_PLOT") == "1 | y1 = sin(x) %s x^2" % TIMES)
        expect_display(app, "y = sin(x) %s x^2\tPlotted as y₁" % TIMES, "= plots a function of x")
        app.until("the curve of y1", lambda: app.has_id(P + "curve-0"))
        app.screenshot(os.path.join(out, "graph-flat-light.png"))
        before = app.last("AZCALC_GRAPH")
        cx, cy = plot_center(app)
        app.must("wheel", x=cx, y=cy, delta_x=0.0, delta_y=120.0)
        app.frame(2)
        app.until("the wheel to zoom the plot", lambda: app.last("AZCALC_GRAPH") != before)
        app.log("zoomed: %s -> %s" % (before, app.last("AZCALC_GRAPH")))
        before = app.last("AZCALC_GRAPH")
        app.drag(cx, cy, cx + 120.0, cy + 40.0)
        app.until("the drag to move the plane", lambda: app.last("AZCALC_GRAPH") != before)
        app.log("moved: %s -> %s" % (before, app.last("AZCALC_GRAPH")))
        focus_calculator(app)
        type_text(app, "y=2x+1=")
        app.until("y2 plotted", lambda: (app.last("AZCALC_PLOT") or "").startswith("2 |"))
        app.until("the curve of y2", lambda: app.has_id(P + "curve-1"))

        # Date and Convert: the View menu's keys.
        app.key("4", alt=True)
        expect_mode(app, "date")
        if not app.has_id(P + "date-from") or not app.shows("Difference between dates"):
            raise Failure("the date screen is not shown")
        app.screenshot(os.path.join(out, "date.png"))
        app.key("5", alt=True)
        expect_mode(app, "convert")
        if not app.has_id(P + "conv-from-value") or not app.has_id(P + "conv-to-unit"):
            raise Failure("the converter's fields are not in the tree")
        if not app.shows("1 km = 0.621371 mi"):
            raise Failure("the converter does not show the km -> mi rate")
        app.screenshot(os.path.join(out, "convert.png"))
        app.key("0", alt=True)
        expect_mode(app, "graph", "Alt+0: automatic, by the window's size")

        # Settings: Flora and Dark are saved.
        app.key("comma", primary=True)
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        app.click(text="General")
        app.until("the General options", lambda: app.has_id("appkit-theme"))
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
        # About: the standard AboutDialog, from the settings' About section.
        app.click(text="About")
        app.until("the About section", lambda: app.has_id("appkit-about-open"))
        app.click(selector="#appkit-about-open")
        app.expect_line("AZCALCULATOR_ABOUT", "open", "the About box opens")
        app.screenshot(os.path.join(out, "about-flora-dark.png"))
        app.key("escape")
        app.expect_line("AZCALCULATOR_ABOUT", "closed", "Escape closes the About box first")
        if not app.has_id("appkit-settings"):
            raise Failure("Escape on the About box closed the settings page too")
        app.click(selector="#appkit-settings-ok")
        app.expect_line("AZCALCULATOR_SETTINGS_CLOSED", "ok", "OK closes the settings")
        app.until("the settings page to close", lambda: not app.has_id("appkit-settings"))
        app.screenshot(os.path.join(out, "graph-flora-dark-e2e.png"))
        resize(app, PROGRAMMER, "programmer")
        app.screenshot(os.path.join(out, "programmer-flora-dark-e2e.png"))
        resize(app, MICRO, "micro")
        app.screenshot(os.path.join(out, "micro-flora-dark-e2e.png"))
        # Cancel: Flat chosen on the page, then Escape - Flora is back, in the file too.
        app.key("comma", primary=True)
        app.until("the settings page again", lambda: app.has_id("appkit-settings"))
        app.click(text="General")
        app.until("the General options again", lambda: app.has_id("appkit-theme"))
        saved_before = len(app.printed("AZCALCULATOR_SETTINGS_SAVED"))
        app.click(text="Flat")
        app.until("Flat saved", lambda: len(app.printed("AZCALCULATOR_SETTINGS_SAVED")) > saved_before)
        app.key("escape")
        app.expect_line("AZCALCULATOR_SETTINGS_CLOSED", "cancel", "Escape cancels the settings")
        app.until("Flora written back",
                  lambda: len(app.printed("AZCALCULATOR_SETTINGS_SAVED")) >= saved_before + 2)
        with open(os.path.join(data_dir, "calculator", "settings.json"), "r", encoding="utf-8") as f:
            settings = json.load(f)
        if settings.get("theme") != "flora" or settings.get("mode") != "dark":
            raise Failure("Cancel did not put flora / dark back: %s" % settings)
        if app.has_id("appkit-settings"):
            raise Failure("Escape (Cancel) left the settings page open")

        # Copy and paste.
        focus_calculator(app)
        app.key("escape")
        type_text(app, "42")
        app.key("c", primary=True)
        app.until("Ctrl+C to copy", lambda: app.last("AZCALC_COPIED") == "42")
        app.log("Ctrl+C copied 42")
        app.key("escape")
        app.must("paste", text="2*21")
        app.until("the paste", lambda: app.last("AZCALC_PASTED") == "2*21")
        app.key("enter")
        expect_display(app, "2 %s 21 =\t42" % TIMES, "2*21 pasted, Enter")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
        if not args.keep:
            shutil.rmtree(data_dir, ignore_errors=True)
    looks(args, logs, out, binary)
    app.log("PASS")
    return True


def looks(args, logs, out, binary):
    """One short run per look: a sample on each of the three calculators, a screenshot of each."""
    for theme, mode in [("flat", "light"), ("flat", "dark"), ("flora", "light"), ("flora", "dark")]:
        tag = "%s-%s-%s" % (TAG, theme, mode)
        data_dir = os.path.join(logs, "data-%s-%s" % (theme, mode))
        os.makedirs(data_dir, exist_ok=True)
        app = e2e.App(tag, binary, ["--data-dir", data_dir, "--sample", "--size", "%dx%d" % MICRO,
                                    "--theme", theme, "--mode", mode],
                      args.debug_port, logs, args.timeout)
        try:
            app.until("the history to load", lambda: app.printed("AZCALC_HISTORY_LOADED", r"\d+"))
            expect_mode(app, "micro")
            focus_calculator(app)
            type_text(app, "1280*0.19=")
            expect_display(app, "1,280 %s 0.19 =\t243.2" % TIMES, "%s %s: a sample" % (theme, mode))
            app.screenshot(os.path.join(out, "micro-%s-%s.png" % (theme, mode)))
            resize(app, PROGRAMMER, "programmer")
            app.key("escape")
            app.key("f5")
            type_text(app, "2a5f")
            expect_display(app, "\t2A5F", "%s %s: 2A5F" % (theme, mode))
            app.screenshot(os.path.join(out, "programmer-%s-%s.png" % (theme, mode)))
            resize(app, GRAPH, "graph")
            app.until("the plot to measure itself", lambda: app.printed("AZCALC_GRAPH"))
            focus_calculator(app)
            app.key("escape")
            type_text(app, "sin(x)*x^2=")
            type_text(app, "y=x^2/4-3=")
            app.until("two functions", lambda: (app.last("AZCALC_PLOT") or "").startswith("2 |"))
            type_text(app, "sqrt(2)/(1+x)")
            app.screenshot(os.path.join(out, "graph-%s-%s.png" % (theme, mode)))
        except Failure:
            print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
            raise
        finally:
            app.stop()
            if not args.keep:
                shutil.rmtree(data_dir, ignore_errors=True)
        app.log("screenshots of the three sizes")


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8781)
