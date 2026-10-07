#!/usr/bin/env python3
"""AzMonitor end to end, headless, over the debug server, on the deterministic sample
machine (--sample: 45 processes, 8 cores, 16 GB; nothing on this computer is ended).

    1. starts AzMonitor (AZ_BACKEND=headless, AZ_DEBUG=--debug-port), waits for the
       first reading (AZMON_READY 45), the tab row and the Processes tab (the table, the
       filter and "End Process" under it);
    2. THE 1 HZ PATH: more readings arrive (AZMON_TICK) and layout() does NOT run again
       (AZMON_LAYOUT unchanged) - yet the status bar's "CPU Usage: x%" follows them;
    3. SORT: a click on the "Image Name" header sorts by name (AZMON_SORT "Image Name asc")
       and the next reading's first row is the alphabetically first process
       (accounts-daemon);
    4. SCROLL WHILE UPDATING (the user's report, 2026-10-07): a wheel DOWN over the table
       (the engine's raw delta_y < 0) scrolls it down by whole rows (AZMON_SCROLL > 0, the
       first process leaves the screen), and two readings later it is still there; a wheel
       up brings it back to the top;
    5. FILTER: "rustc" typed into the filter shows the 12 rustc processes (AZMON_SHOWN 12)
       - again without a layout() per keystroke;
    6. END PROCESS: filter "pipewire", click its row (AZMON_SELECT 812 pipewire), Delete
       asks (AZMON_ASK), "Kill" ends it (AZMON_END 812 true, AZMON_NOTICE "Killed
       pipewire (812)", 44 processes at the next reading); sshd (root) is refused
       ("administrator rights");
    7. PERFORMANCE: the tab shows the CPU / memory meters and history graphs (one per core:
       eight scrolling strips and the memory's) and its readings run no layout();
       NETWORKING: the network graph and its figures; USERS: one row per user (root, user);
    8. SETTINGS: Mod+, -> "2 s" (AZMON_SPEED 2000), "Export the last minute" writes
       monitor/history/<date>.csv into the data folder (AZMON_EXPORTED);
    9. a screenshot after each step (flat, light); the mode switched to dark at the end.

Usage (after building libazul with the debug server and AzMonitor; ONE app at a time,
through scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azmonitor_e2e.py [--bin target/release/AzMonitor]
        [--debug-port 8791] [--timeout 180] [--out <dir>] [--keep]
"""

import glob
import os
import time

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azmonitor"
PROCESSES = 45
CORES = 8  # the sample machine's (one CPU graph each)
P = "#__azmonitor_"


def ticks(app):
    return app.count("AZMON_TICK")


def wait_ticks(app, n, what):
    """Waits for `n` more readings; returns the last AZMON_TICK line."""
    start = ticks(app)
    app.until(what, lambda: ticks(app) >= start + n)
    return app.last("AZMON_TICK")


def layouts(app):
    last = app.last("AZMON_LAYOUT")
    return int(last) if last else 0


def cpu_label(app):
    """The status bar's "CPU Usage: x%" as the window shows it (a status label is a
    VirtualView: a DOM of its own)."""
    for text in app.texts(every_dom=True):
        if text.startswith("CPU Usage: ") and text.endswith("%"):
            return text
    return None


def within(app, seconds, check):
    """`check()` within `seconds` (a bounded wait: the run's deadline is minutes away)."""
    end = time.time() + seconds
    while time.time() < end:
        value = check()
        if value:
            return value
        time.sleep(0.1)
    return None


def table_center(app):
    box = app.box(P + "table-view")
    return box["x"] + box["width"] / 2.0, box["y"] + box["height"] / 2.0


def clear_filter(app):
    app.click(selector=P + "filter")
    app.key("a", primary=True)
    app.key("backspace")


def body(args, logs, out):
    binary = e2e.find_binary("AzMonitor", args.bin, "AZMONITOR_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG, binary, ["--sample", "--data-dir", data_dir, "--size", "1280x800",
                                "--theme", "flat", "--mode", "light"],
                  args.debug_port, logs, args.timeout)
    try:
        # ---- 1. the first reading builds the page ----
        app.until("the first reading", lambda: app.printed("AZMON_READY", r"\d+"))
        ready = int(app.printed("AZMON_READY", r"\d+")[-1])
        if ready != PROCESSES:
            raise Failure("the sample machine runs %d processes, not %d" % (ready, PROCESSES))
        app.frame(3)
        for stem in ("tools", "table-view", "process-actions", "filter", "end-process"):
            if not app.has_id("__azmonitor_" + stem):
                raise Failure("#__azmonitor_%s is not in the tree" % stem)
        for tab in ("Processes", "Performance", "Networking", "Users"):
            if not app.shows(tab):
                raise Failure("the tab row has no %r" % tab)
        # The table is a VirtualView: its rows are a DOM of their own.
        app.until("the table's rows", lambda: app.shows("cargo", every_dom=True))
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "1-processes.png"))

        # ---- 2. readings re-render the live views, never the page ----
        wait_ticks(app, 1, "a reading after the page was built")
        app.frame(2)
        before_layouts = layouts(app)
        seen = set()
        for _ in range(6):
            label = cpu_label(app)
            if label:
                seen.add(label)
            if len(seen) >= 2:
                break
            wait_ticks(app, 1, "a reading")
            app.frame(2)
        if layouts(app) != before_layouts:
            raise Failure("readings ran layout(): %d -> %d" % (before_layouts, layouts(app)))
        if len(seen) < 2:
            raise Failure("the status bar's CPU usage did not follow the readings: %r" % sorted(seen))
        app.log("readings ran layout() 0 times; the status bar said %s" % ", ".join(sorted(seen)))

        # ---- 3. sort by name ----
        app.click(text="Image Name", every_dom=True)
        app.until("the name sort", lambda: app.last("AZMON_SORT") == "Image Name asc")
        app.until("accounts-daemon on top",
                  lambda: (app.last("AZMON_TOP") or "").endswith(" accounts-daemon"))
        app.log("sorted by name; top: %s" % app.last("AZMON_TOP"))
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "2-sorted-name.png"))

        # ---- 4. the table scrolls, also while the readings go on ----
        app.until("accounts-daemon in the table", lambda: app.shows("accounts-daemon", every_dom=True))
        x, y = table_center(app)
        before = app.count("AZMON_SCROLL")
        app.must("wheel", x=x, y=y, delta_x=0, delta_y=-120)
        if not within(app, 5.0, lambda: app.count("AZMON_SCROLL") > before):
            raise Failure("a wheel down over the table did not scroll it (no AZMON_SCROLL): "
                          "the table took the wheel the other way, against its first row")
        top = int(app.last("AZMON_SCROLL"))
        if top <= 0:
            raise Failure("a wheel down scrolled the table to row %d" % top)
        app.frame(3)
        if app.shows("accounts-daemon", every_dom=True):
            raise Failure("the first process is still shown after scrolling %d rows down" % top)
        wait_ticks(app, 2, "two readings after the scroll")
        app.frame(2)
        view_top = int((app.last("AZMON_VIEW") or "0 false").split()[0])
        if view_top != top or app.shows("accounts-daemon", every_dom=True):
            raise Failure("a reading scrolled the table back: row %d -> %d" % (top, view_top))
        app.log("scrolled to row %d; two readings later it is still there" % top)
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "3-scrolled.png"))
        app.must("wheel", x=x, y=y, delta_x=0, delta_y=600)
        if not within(app, 5.0, lambda: app.last("AZMON_SCROLL") == "0"):
            raise Failure("a wheel up did not bring the table back to its first row (%s)"
                          % app.last("AZMON_SCROLL"))
        app.frame(2)

        # ---- 5. filter ----
        before_layouts = layouts(app)
        app.text_input(P + "filter", "rustc")
        app.expect_line("AZMON_SHOWN", "12", "the rustc filter")
        if layouts(app) != before_layouts:
            raise Failure("typing into the filter ran layout(): %d -> %d" % (before_layouts, layouts(app)))
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "4-filtered.png"))

        # ---- 6. end a process; a root process is refused ----
        clear_filter(app)
        app.text_input(P + "filter", "pipewire")
        app.expect_line("AZMON_SHOWN", "1", "the pipewire filter")
        app.frame(2)
        app.click(text="pipewire", every_dom=True)
        app.expect_line("AZMON_SELECT", "812 pipewire", "pipewire selected")
        app.key("delete")
        app.expect_line("AZMON_ASK", "812 pipewire", "the question")
        if not app.has_id("__azmonitor_confirm-end"):
            raise Failure("the end-process question is not shown")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "5-question.png"))
        # The question is a Modal: a window of its own.
        app.click_exact("Kill", window=app.until("the question's window", app.popup))
        app.expect_line("AZMON_END", "812 true", "the kill")
        app.until("the kill's notice", lambda: app.last("AZMON_NOTICE") == "Killed pipewire (812)")
        app.until("44 processes", lambda: (app.last("AZMON_TICK") or "").split()[1:2] == [str(PROCESSES - 1)])
        app.log("pipewire killed: %s" % app.last("AZMON_TICK"))

        clear_filter(app)
        app.text_input(P + "filter", "sshd")
        app.expect_line("AZMON_SHOWN", "1", "the sshd filter")
        app.frame(2)
        app.click(text="sshd", every_dom=True)
        app.expect_line("AZMON_SELECT", "702 sshd", "sshd selected")
        app.click(selector=P + "end-process")
        app.expect_line("AZMON_ASK", "702 sshd", "the question for sshd")
        # The question is a Modal: a window of its own.
        app.click_exact("Kill", window=app.until("the question's window", app.popup))
        app.until("the refusal", lambda: "administrator" in (app.last("AZMON_NOTICE") or ""))
        app.log("sshd refused: %s" % app.last("AZMON_NOTICE"))
        clear_filter(app)
        app.expect_line("AZMON_SHOWN", str(PROCESSES - 1), "every process again")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "6-ended.png"))

        # ---- 7. the Performance, Networking and Users tabs ----
        app.click(text="Performance")
        app.expect_line("AZMON_SCREEN", "performance", "the Performance tab")
        app.frame(3)
        if not app.has_id("__azmonitor_performance"):
            raise Failure("#__azmonitor_performance is not in the tree")
        wait_ticks(app, 1, "a reading on the Performance tab")
        app.frame(2)
        for stem in ("cpu-usage", "cpu-history", "memory-usage", "memory-history", "stats"):
            if not app.has_id("__azmonitor_" + stem, every_dom=True):  # in the page's VirtualView
                raise Failure("the Performance tab has no #__azmonitor_%s" % stem)
        # One graph per core, then the memory's: each a black box with a strip that scrolls.
        if not app.has(".__azmonitor_graph-strip", every_dom=True):
            raise Failure("the Performance tab has no scrolling graph strips")
        if not app.has(".__azmonitor_meter", every_dom=True):
            raise Failure("the Performance tab has no usage meters")
        before_layouts = layouts(app)
        wait_ticks(app, 2, "two readings on the Performance tab")
        app.frame(2)
        if layouts(app) != before_layouts:
            raise Failure("readings on the Performance tab ran layout()")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "7-performance.png"))

        app.click(text="Networking")
        app.expect_line("AZMON_SCREEN", "networking", "the Networking tab")
        wait_ticks(app, 1, "a reading on the Networking tab")
        app.frame(2)
        for stem in ("network-history", "network-stats"):
            if not app.has_id("__azmonitor_" + stem, every_dom=True):
                raise Failure("the Networking tab has no #__azmonitor_%s" % stem)
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "8-networking.png"))

        app.click(text="Users")
        app.expect_line("AZMON_SCREEN", "users", "the Users tab")
        wait_ticks(app, 1, "a reading on the Users tab")
        app.frame(2)
        if not app.has_id("__azmonitor_users-table", every_dom=True):
            raise Failure("the Users tab has no table")
        if not app.shows("root", every_dom=True):
            raise Failure("the Users tab does not list root")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "9-users.png"))

        # ---- 8. settings: the update speed, the export ----
        app.key("comma", primary=True)
        app.until("the settings", lambda: app.has_id("appkit-settings"))
        app.click(text="2 s")
        app.expect_line("AZMON_SPEED", "2000", "the slower speed")
        app.click(selector=P + "export")
        app.until("the export", lambda: app.last("AZMON_EXPORTED"))
        key = app.last("AZMON_EXPORTED")
        if not key.startswith("monitor/history/") or not key.endswith(".csv"):
            raise Failure("the export went to %r" % key)
        files = glob.glob(os.path.join(data_dir, "monitor", "history", "*.csv"))
        if not files:
            raise Failure("no CSV under %s/monitor/history" % data_dir)
        with open(files[0], "r", encoding="utf-8") as f:
            header = f.readline().strip()
        if not header.startswith("seconds_ago,cpu_percent,memory_percent"):
            raise Failure("the CSV starts %r" % header)
        app.log("exported %s" % key)
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "10-settings.png"))
        app.click(selector="#appkit-settings-ok")  # OK keeps the 2 s (Escape would cancel)
        app.until("the settings closed", lambda: not app.has_id("appkit-settings"))

        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "11-dark.png"))
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
