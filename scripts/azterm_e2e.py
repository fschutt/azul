#!/usr/bin/env python3
"""AzTerm end to end, headless, over the debug server: azul's TerminalView over
alacritty_terminal, with `--sample` (recorded sessions replayed into a terminal with no
PTY - the same screen every run; typing echoes).

    1. starts AzTerm --sample (AZ_BACKEND=headless, AZ_DEBUG=--debug-port), waits for
       AZTERM_READY and the first tab (AZTERM_TABS 1): the terminal, the strip of tabs (the
       first tab, its close button, the "+") are in the tree, the tab is titled by its
       session;
    2. TYPE: focuses the terminal, types "echo hi" and Return - the recording echoes it
       (the view's text input -> Input bytes -> the session);
    3. TABS: "+" opens a second tab (the log recording: 5,000 lines, AZTERM_HISTORY says
       more than 4,000 lines of scrollback);
    4. SCROLL: the wheel turned up over the terminal scrolls into the scrollback
       (AZTERM_SCROLL > 0), turned down back to the output (AZTERM_SCROLL 0); Shift+Page Up
       scrolls up, Shift+End back down;
    5. TABS: Mod+1 activates the first tab (AZTERM_ACTIVE 0), its close button closes it
       (AZTERM_TABS 1), Mod+T opens another (AZTERM_TABS 2);
    6. a screenshot after each step, flat light; the mode switched to dark at the end.

Usage (after building libazul with the debug server and AzTerm; ONE app at a time,
through scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azterm_e2e.py [--bin target/release/AzTerm]
        [--debug-port 8791] [--timeout 180] [--out <dir>] [--keep]
"""

import os
import sys

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azterm"


def history(app, index):
    """The scrollback (lines) of tab `index` when it opened (AZTERM_HISTORY), or None."""
    for value in app.printed("AZTERM_HISTORY"):
        parts = value.split()
        if len(parts) == 2 and parts[0] == str(index):
            return int(parts[1])
    return None


def scroll(app):
    """The view's display offset after the last scroll (AZTERM_SCROLL), or None."""
    value = app.last("AZTERM_SCROLL")
    return int(value) if value not in (None, "") else None


def window_chord(app, key, shift=False):
    """The window's chord: Cmd+key on macOS, Ctrl+Shift+key elsewhere."""
    if sys.platform == "darwin":
        app.key(key, meta=True, shift=shift)
    else:
        app.key(key, ctrl=True, shift=True)


def body(args, logs, out):
    binary = e2e.find_binary("AzTerm", args.bin, "AZTERM_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG, binary, ["--sample", "--data-dir", data_dir, "--size", "1100x700",
                                "--theme", "flat", "--mode", "light"],
                  args.debug_port, logs, args.timeout)
    try:
        app.until("the window", lambda: app.printed("AZTERM_READY"))
        app.until("the first tab", lambda: app.last("AZTERM_TABS") == "1")
        app.frame(3)
        for node_id in ("__azterm_terminal", "__azterm_tabs", "__azterm_tab-0",
                        "__azterm_tab-close-0", "__azterm_new-tab", "__azterm_pane"):
            if not app.has_id(node_id):
                raise Failure("#%s is not in the tree" % node_id)
        if app.has_id("__azterm_status"):
            raise Failure("the old status bar is still there")
        rect = app.rect("__azterm_pane")
        app.log("the terminal pane: %r" % (rect,))
        tabs = app.rect("__azterm_tabs")
        if rect and tabs and float(rect.get("y", 0)) < float(tabs.get("y", 0)):
            raise Failure("the terminal is above the strip of tabs: %r / %r" % (rect, tabs))
        if not app.shows("azul-apps"):
            raise Failure("the first tab is not titled by its session")
        # The rows live in the terminal's VirtualView, a DOM of its own.
        if app.shows("Finished", every_dom=True):
            app.log("the build recording's rows are in the tree")
        else:
            app.log("WARN: the VirtualView's rows are not in the hierarchy (screenshot only)")
        app.screenshot(os.path.join(out, "1-build-session.png"))

        # ---- type: the recording echoes ----
        app.text_input("#__azterm_terminal", "echo hi")
        app.key("return")
        app.frame(3)
        if app.shows("echo hi", every_dom=True):
            app.log("the typed command is echoed")
        app.screenshot(os.path.join(out, "2-typed.png"))

        # ---- a second tab: the long log ----
        app.click(selector="#__azterm_new-tab")
        app.until("the second tab", lambda: app.last("AZTERM_TABS") == "2")
        app.frame(3)
        lines = history(app, 1)
        if lines is None or lines < 4000:
            raise Failure("the log tab's scrollback is %r lines, expected more than 4,000" % lines)
        app.log("the log tab keeps %d lines of scrollback" % lines)
        app.screenshot(os.path.join(out, "3-log-tab.png"))

        # ---- the wheel: up into the scrollback, down back to the output ----
        pane = app.rect("__azterm_pane")
        x = float(pane.get("x", 0)) + float(pane.get("width", 400)) / 2.0
        y = float(pane.get("y", 0)) + float(pane.get("height", 300)) / 2.0
        # Raw wheel deltas are UP-positive (X11's button 4); 20 px is one click.
        app.must("wheel", x=x, y=y, delta_x=0.0, delta_y=60.0)
        app.frame(3)
        app.until("the wheel turned up scrolled into the scrollback",
                  lambda: (scroll(app) or 0) > 0)
        app.log("the wheel scrolled up to offset %d" % scroll(app))
        app.screenshot(os.path.join(out, "4-wheel-up.png"))
        app.must("wheel", x=x, y=y, delta_x=0.0, delta_y=-600.0)
        app.frame(3)
        app.until("the wheel turned down came back to the output", lambda: scroll(app) == 0)

        # ---- the keys: Shift+Page Up into the scrollback, Shift+End back ----
        app.click(selector="#__azterm_terminal")
        app.key("pageup", shift=True)
        app.key("pageup", shift=True)
        app.frame(3)
        app.until("Shift+Page Up scrolled into the scrollback", lambda: (scroll(app) or 0) > 0)
        app.screenshot(os.path.join(out, "5-scrolled.png"))
        app.key("end", shift=True)
        app.until("Shift+End came back to the output", lambda: scroll(app) == 0)

        # ---- the tabs: Mod+1, a close button, Mod+T ----
        window_chord(app, "1")
        app.until("the first tab again", lambda: app.last("AZTERM_ACTIVE") == "0")
        app.click(selector="#__azterm_tab-close-0")
        app.until("the first tab closed", lambda: app.last("AZTERM_TABS") == "1")
        window_chord(app, "t")
        app.until("a new tab", lambda: app.last("AZTERM_TABS") == "2")
        app.frame(3)
        app.screenshot(os.path.join(out, "6-tabs.png"))

        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "7-dark.png"))
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
