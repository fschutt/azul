#!/usr/bin/env python3
"""AzTerm end to end, headless, over the debug server: azul's TerminalView over
alacritty_terminal, with `--sample` (recorded sessions replayed into a terminal with no
PTY - the same screen every run; typing echoes; the sample shell runs `seq N` and
`yes | head -n N`, a few lines a tick).

    1. starts AzTerm --sample (AZ_BACKEND=headless, AZ_DEBUG=--debug-port), waits for
       AZTERM_READY and the first tab (AZTERM_TABS 1): the terminal, the strip of tabs (its
       scroller, the first tab, its close button, the "+") are in the tree, the tab is titled
       by its session; NO TITLE ROW: the strip is at the window's top, a grab strip above its
       tabs, on macOS the first tab clear of the traffic lights;
    2. TYPE: focuses the terminal, types "echo hi" and Return - the recording echoes it
       (the view's text input -> Input bytes -> the session);
    3. TABS: "+" opens a second tab (the log recording: 5,000 lines, AZTERM_HISTORY says
       more than 4,000 lines of scrollback);
    4. SCROLL: the wheel turned up over the terminal scrolls into the scrollback
       (AZTERM_SCROLL > 0), turned down back to the output (AZTERM_SCROLL 0); Shift+Page Up
       scrolls up, Shift+End back down;
    5. FOLLOW: `seq 30000` streams into the log tab; the wheel turned up leaves the output
       (AZTERM_FOLLOW 0) - the rows in view stay the same while the lines stream in below, the
       round follow button counts them (more and more); a click on it follows the output
       again (AZTERM_FOLLOW 1, AZTERM_SCROLL 0, the button gone);
    6. TABS: Mod+1 activates the first tab (AZTERM_ACTIVE 0), its close button closes it
       (AZTERM_TABS 1), Mod+T opens another (AZTERM_TABS 2);
    7. MANY TABS: Mod+T up to 12 tabs - every tab as wide, the strip scrolls sideways (its
       scroller can scroll, the new active last tab is scrolled into it, the "+" in reach), the
       wheel over the strip scrolls it; screenshots in flat and flora, light and dark;
    8. THE LAST TAB CLOSES THE WINDOW: Mod+W on every tab; the last one prints AZTERM_CLOSE
       and the app ends (exit status 0) - never a window without a tab.

Usage (after building libazul with the debug server and AzTerm; ONE app at a time,
through scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azterm_e2e.py [--bin target/release/AzTerm]
        [--debug-port 8791] [--timeout 180] [--out <dir>] [--keep]
"""

import os
import re
import sys
import time
import urllib.error

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azterm"

# The terminal widget's follow button (layout/src/widgets/terminal_view.rs FOLLOW_CLASS_NAME).
FOLLOW = ".__azul-terminal-view-follow"
# AzTerm's tabs: as wide as TAB_PX while they fit, TAB_MIN_PX at the narrowest (src/lib.rs).
TAB_PX = 180.0
TAB_MIN_PX = 110.0
# Tabs that cannot fit a 1100 px window at TAB_MIN_PX, on any platform.
MANY_TABS = 12


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


def tabs(app):
    """The number of tabs after the last change (AZTERM_TABS), or None."""
    value = app.last("AZTERM_TABS")
    return int(value) if value not in (None, "") else None


def window_chord(app, key, shift=False):
    """The window's chord: Cmd+key on macOS, Ctrl+Shift+key elsewhere."""
    if sys.platform == "darwin":
        app.key(key, meta=True, shift=shift)
    else:
        app.key(key, ctrl=True, shift=True)


def tab_width(app, index):
    """Tab `index`'s laid-out width (its layout rect: a tab scrolled half out of the strip is
    still that wide, its on-screen box is cut)."""
    return float(app.rect("__azterm_tab-%d" % index).get("width", 0))


def numbers_in_view(app):
    """The rows the terminal shows that are a number alone (`seq`'s), in order - the rows live
    in the terminal's VirtualView, a DOM of its own."""
    return [t.strip() for t in app.texts(every_dom=True) if t.strip().isdigit()]


def follow_label(app):
    """The follow button's label ("1,234 new lines" and its arrow, or the arrow alone), or None
    while the view follows the output."""
    for t in app.texts(every_dom=True):
        if t.strip().endswith("↓"):
            return t.strip()
    return None


def follow_count(label):
    """The new lines a follow button's label counts (0 for the arrow alone)."""
    m = re.match(r"([\d,]+) new lines?", label or "")
    return int(m.group(1).replace(",", "")) if m else 0


def follow_box(app):
    """Where the follow button is on screen (window coordinates), or None: it lives in the
    terminal's VirtualView, so every DOM of the window is asked."""
    for dom in app.dom_ids():
        try:
            answer = app.op("get_node_layout", selector=FOLLOW, dom_id=dom)
        except (OSError, ValueError, urllib.error.URLError):
            continue
        if not isinstance(answer, dict) or answer.get("status") == "error":
            continue
        data = answer.get("data") or {}
        value = data.get("value") if isinstance(data, dict) else None
        if not isinstance(value, dict) or value.get("node_id") is None:
            continue
        r = value.get("screen_rect")
        if not r:
            # A build whose debug server has no screen rects: the VirtualView fills the pane.
            local, pane = value.get("rect") or {}, app.box("#__azterm_pane")
            r = {"x": pane["x"] + float(local.get("x", 0)), "y": pane["y"] + float(local.get("y", 0)),
                 "width": local.get("width", 0), "height": local.get("height", 0)}
        return {key: float(r.get(key, 0)) for key in ("x", "y", "width", "height")}
    return None


def scroll_state(app, selector):
    """The scroll state (scroll_x, max_scroll_x, ...) of the scroll box `selector`, or None while
    it is no scroll box (its content fits)."""
    node = app.value("get_node_layout", selector=selector).get("node_id")
    states = app.value("get_scroll_states").get("scroll_states") or []
    return next((s for s in states if s.get("node_id") == node), None)


def strip_scroll_x(app):
    """How far the strip of tabs is scrolled sideways, px (0 while it is no scroll box)."""
    state = scroll_state(app, "#__azterm_tabs-scroller") or {}
    return float(state.get("scroll_x", 0))


def close_last_tab(app):
    """Mod+W on the last tab: the window closes and the app ends - the debug server with it, so
    the key's answer (or the frame after it) may never come."""
    mods = {"shift": False, "ctrl": False, "alt": False, "meta": False}
    if sys.platform == "darwin":
        mods["meta"] = True
    else:
        mods.update(ctrl=True, shift=True)
    released = {"shift": False, "ctrl": False, "alt": False, "meta": False}
    for op, params in (("key_down", {"key": "w", "modifiers": mods}),
                       ("key_up", {"key": "w", "modifiers": released})):
        try:
            app.op(op, **params)
        except (OSError, ValueError, urllib.error.URLError):
            break


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
        for node_id in ("__azterm_terminal", "__azterm_tabs", "__azterm_tabs-scroller",
                        "__azterm_tab-0", "__azterm_tab-close-0", "__azterm_new-tab",
                        "__azterm_pane"):
            if not app.has_id(node_id):
                raise Failure("#%s is not in the tree" % node_id)
        if app.has_id("__azterm_status"):
            raise Failure("the old status bar is still there")
        rect = app.rect("__azterm_pane")
        app.log("the terminal pane: %r" % (rect,))
        strip_rect = app.rect("__azterm_tabs")
        if rect and strip_rect and float(rect.get("y", 0)) < float(strip_rect.get("y", 0)):
            raise Failure("the terminal is above the strip of tabs: %r / %r" % (rect, strip_rect))
        if not app.shows("azul-apps"):
            raise Failure("the first tab is not titled by its session")

        # ---- no title row: the strip of tabs is the title bar ----
        strip = app.box("#__azterm_tabs")
        first = app.box("#__azterm_tab-0")
        app.log("the strip %r, the first tab %r" % (strip, first))
        if strip["y"] > 1.0:
            raise Failure("the strip of tabs is not at the window's top (a title row above it?): %r"
                          % (strip,))
        if first["y"] < strip["y"] + 4.0:
            raise Failure("no grab strip above the tabs: %r in %r" % (first, strip))
        if sys.platform == "darwin" and first["x"] < 70.0:
            raise Failure("the first tab is under the traffic lights: %r" % (first,))
        if abs(tab_width(app, 0) - TAB_PX) > 1.0:
            raise Failure("one tab is %.1f px wide, expected %.0f" % (tab_width(app, 0), TAB_PX))
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
        widths = [tab_width(app, i) for i in range(2)]
        if abs(widths[0] - widths[1]) > 1.0 or abs(widths[0] - TAB_PX) > 1.0:
            raise Failure("two tabs are not %.0f px wide each: %r" % (TAB_PX, widths))
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
        app.until("the follow button off the output", lambda: follow_label(app) is not None)
        app.screenshot(os.path.join(out, "4-wheel-up.png"))
        app.must("wheel", x=x, y=y, delta_x=0.0, delta_y=-600.0)
        app.frame(3)
        app.until("the wheel turned down came back to the output", lambda: scroll(app) == 0)
        app.until("no follow button at the output", lambda: follow_label(app) is None)

        # ---- the keys: Shift+Page Up into the scrollback, Shift+End back ----
        app.click(selector="#__azterm_terminal")
        app.key("pageup", shift=True)
        app.key("pageup", shift=True)
        app.frame(3)
        app.until("Shift+Page Up scrolled into the scrollback", lambda: (scroll(app) or 0) > 0)
        app.screenshot(os.path.join(out, "5-scrolled.png"))
        app.key("end", shift=True)
        app.until("Shift+End came back to the output", lambda: scroll(app) == 0)

        # ---- a flood while scrolled up: the view stays, the follow button counts ----
        app.text_input("#__azterm_terminal", "seq 30000")
        app.key("return")
        app.until("the sample shell's seq started", lambda: app.printed("AZTERM_STREAM"))
        app.frame(4)
        app.must("wheel", x=x, y=y, delta_x=0.0, delta_y=100.0)
        app.frame(2)
        app.until("the view left the output", lambda: app.last("AZTERM_FOLLOW") == "0")
        app.until("the follow button", lambda: follow_label(app) is not None)
        before, count_before = numbers_in_view(app), follow_count(follow_label(app))
        if not before:
            raise Failure("no seq numbers in view after the wheel: %r" % app.texts(every_dom=True))
        app.log("scrolled up over %s .. %s (%d new lines so far)"
                % (before[0], before[-1], count_before))
        # Lines stream in below for a while (the view is drawn a few times a second).
        time.sleep(1.0)
        app.frame(3)
        after, count_after = numbers_in_view(app), follow_count(follow_label(app))
        app.log("after a second: %s .. %s (%d new lines)"
                % (after[0] if after else "-", after[-1] if after else "-", count_after))
        if after != before:
            raise Failure("the view moved while the output streamed in below it: %r -> %r"
                          % (before[:3], after[:3]))
        if count_after <= count_before:
            raise Failure("the follow button does not count the lines coming in: %d -> %d"
                          % (count_before, count_after))
        app.screenshot(os.path.join(out, "6-flood-scrolled-up.png"))
        button = follow_box(app)
        if button is None:
            raise Failure("the follow button has no box")
        app.must("click", x=button["x"] + button["width"] / 2.0,
                 y=button["y"] + button["height"] / 2.0)
        app.frame(3)
        app.until("the follow button follows the output again",
                  lambda: app.last("AZTERM_FOLLOW") == "1")
        if scroll(app) != 0:
            raise Failure("the follow button left the view at offset %r" % scroll(app))
        app.until("the follow button is gone", lambda: follow_label(app) is None)
        app.screenshot(os.path.join(out, "7-following.png"))

        # ---- the tabs: Mod+1, a close button, Mod+T ----
        window_chord(app, "1")
        app.until("the first tab again", lambda: app.last("AZTERM_ACTIVE") == "0")
        app.click(selector="#__azterm_tab-close-0")
        app.until("the first tab closed", lambda: app.last("AZTERM_TABS") == "1")
        window_chord(app, "t")
        app.until("a new tab", lambda: app.last("AZTERM_TABS") == "2")
        app.frame(3)
        app.screenshot(os.path.join(out, "8-tabs.png"))

        # ---- many tabs: as wide, the strip scrolls sideways ----
        while (tabs(app) or 0) < MANY_TABS:
            n = tabs(app) or 0
            window_chord(app, "t")
            app.until("tab %d" % (n + 1), lambda n=n: (tabs(app) or 0) == n + 1)
        app.frame(6)
        app.settle()
        widths = [tab_width(app, i) for i in range(MANY_TABS)]
        app.log("%d tabs, %.1f to %.1f px wide" % (MANY_TABS, min(widths), max(widths)))
        if max(widths) - min(widths) > 1.0:
            raise Failure("the tabs are not as wide: %r" % (widths,))
        if min(widths) < TAB_MIN_PX - 1.0:
            raise Failure("the tabs shrank under %.0f px: %r" % (TAB_MIN_PX, widths))
        state = app.until("the strip of tabs scrolls sideways",
                          lambda: scroll_state(app, "#__azterm_tabs-scroller"))
        if float(state.get("max_scroll_x", 0)) <= 1.0:
            raise Failure("the strip of tabs cannot scroll: %r" % (state,))
        scroller = app.box("#__azterm_tabs-scroller")
        app.until("the new (last, active) tab scrolled into the strip",
                  lambda: strip_scroll_x(app) > 1.0)
        app.settle()
        last = app.box("#__azterm_tab-%d" % (MANY_TABS - 1))
        # On screen and whole: its box after the scroll, as wide as its layout says.
        if (last["x"] + last["width"] > scroller["x"] + scroller["width"] + 1.0
                or last["x"] < scroller["x"] - 1.0
                or last["width"] < tab_width(app, MANY_TABS - 1) - 1.0):
            raise Failure("the active tab is not in view: %r in %r" % (last, scroller))
        plus = app.box("#__azterm_new-tab")
        window_width = 1100.0
        if plus["x"] + plus["width"] > window_width + 0.5 or plus["width"] < 1.0:
            raise Failure("the \"+\" is out of reach: %r" % (plus,))
        app.screenshot(os.path.join(out, "9-many-tabs-flat.png"))
        # The wheel over the strip (a vertical wheel, turned up: toward the first tab) scrolls
        # it sideways.
        before_x = strip_scroll_x(app)
        sx = scroller["x"] + scroller["width"] / 2.0
        sy = scroller["y"] + scroller["height"] / 2.0
        app.must("wheel", x=sx, y=sy, delta_x=0.0, delta_y=120.0)
        app.frame(6)
        app.until("the wheel over the strip scrolled it sideways",
                  lambda: abs(strip_scroll_x(app) - before_x) > 1.0)
        app.log("the wheel moved the strip from %.1f to %.1f px" % (before_x, strip_scroll_x(app)))
        app.must("set_theme", theme="flora")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "10-many-tabs-flora.png"))
        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "11-many-tabs-flora-dark.png"))
        app.must("set_theme", theme="flat")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "12-many-tabs-flat-dark.png"))

        # ---- the last tab closes the window ----
        while (tabs(app) or 0) > 1:
            n = tabs(app)
            window_chord(app, "w")
            app.until("tab closed (%d left)" % (n - 1), lambda n=n: tabs(app) == n - 1)
        close_last_tab(app)
        deadline = time.time() + 15
        while app.alive() and time.time() < deadline:
            time.sleep(0.1)
        if app.alive():
            raise Failure("closing the last tab left the window open (AZTERM_TABS %r)" % tabs(app))
        if not app.printed("AZTERM_CLOSE"):
            raise Failure("the app ended without closing its window (no AZTERM_CLOSE)")
        if app.process.returncode != 0:
            raise Failure("the app ended with status %r" % app.process.returncode)
        app.log("the last tab closed the window")
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
