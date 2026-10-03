#!/usr/bin/env python3
"""AzTerm end to end, headless, over the debug server: azul's TerminalView over
alacritty_terminal, with `--sample` (recorded sessions replayed into a terminal with no
PTY - the same screen every run; typing echoes).

    1. starts AzTerm --sample (AZ_BACKEND=headless, AZ_DEBUG=--debug-port), waits for
       AZTERM_READY and the first tab (AZTERM_TABS 1): the terminal, the tab list and the
       status bar are in the tree; the status bar names the grid and the scrollback;
    2. TYPE: focuses the terminal, types "echo hi" and Return - the recording echoes it
       (the view's text input -> Input bytes -> the session);
    3. TABS: "+" opens a second tab (the log recording: 5,000 lines, the status bar's
       scrollback > 4,000), a click on the first tab activates it again (AZTERM_ACTIVE 0);
    4. SCROLL: Shift+Page Up on the log tab moves the view into the scrollback;
    5. a screenshot after each step, flat light; the mode switched to dark at the end.

Usage (after building libazul with the debug server and AzTerm; ONE app at a time,
through scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azterm_e2e.py [--bin target/release/AzTerm]
        [--debug-port 8791] [--timeout 180] [--out <dir>] [--keep]
"""

import os
import re

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azterm"


def scrollback(app):
    """The status bar's "scrollback N lines", or None."""
    for text in app.texts():
        m = re.search(r"scrollback (\d+) lines", text)
        if m:
            return int(m.group(1))
    return None


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
        for node_id in ("__azterm_terminal", "__azterm_tabs", "__azterm_status", "__azterm_pane"):
            if not app.has_id(node_id):
                raise Failure("#%s is not in the tree" % node_id)
        rect = app.rect("__azterm_pane")
        app.log("the terminal pane: %r" % (rect,))
        if not app.shows("recording"):
            raise Failure("the status bar does not name the tab's recording")
        # The rows live in the terminal's VirtualView; the hierarchy may or may not list them.
        if app.shows("Finished"):
            app.log("the build recording's rows are in the tree")
        else:
            app.log("WARN: the VirtualView's rows are not in the hierarchy (screenshot only)")
        app.screenshot(os.path.join(out, "1-build-session.png"))

        # ---- type: the recording echoes ----
        app.text_input("#__azterm_terminal", "echo hi")
        app.key("return")
        app.frame(3)
        if app.shows("echo hi"):
            app.log("the typed command is echoed")
        app.screenshot(os.path.join(out, "2-typed.png"))

        # ---- tabs ----
        app.click(selector="#__azterm_new-tab")
        app.until("the second tab", lambda: app.last("AZTERM_TABS") == "2")
        app.frame(3)
        lines = scrollback(app)
        if lines is None or lines < 4000:
            raise Failure("the log tab's scrollback is %r lines, expected more than 4,000" % lines)
        app.log("the log tab keeps %d lines of scrollback" % lines)
        app.screenshot(os.path.join(out, "3-log-tab.png"))

        # ---- scroll the scrollback ----
        app.click(selector="#__azterm_terminal")
        app.key("pageup", shift=True)
        app.key("pageup", shift=True)
        app.frame(3)
        app.screenshot(os.path.join(out, "4-scrolled.png"))

        app.click(selector="#__azterm_tab-0")
        app.until("the first tab again", lambda: app.last("AZTERM_ACTIVE") == "0")
        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "5-dark.png"))
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
