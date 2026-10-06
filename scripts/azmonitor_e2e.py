#!/usr/bin/env python3
"""AzMonitor end to end, headless, over the debug server, on the deterministic sample
machine (--sample: 45 processes, 8 cores, 16 GB; nothing on this computer is ended).

    1. starts AzMonitor (AZ_BACKEND=headless, AZ_DEBUG=--debug-port), waits for the
       first reading (AZMON_READY 45), the cards strip and the process table;
    2. THE 1 HZ PATH: three more readings arrive (AZMON_TICK) and layout() does NOT run
       again (AZMON_LAYOUT unchanged) - yet the live views changed: the CPU card's
       headline reads differently (the VirtualViews re-rendered in place);
    3. SORT: a click on the "Name" header sorts by name (AZMON_SORT "Name asc") and the
       next reading's first row is the alphabetically first process (accounts-daemon);
    4. FILTER: "rustc" typed into the filter shows the 12 rustc processes (AZMON_SHOWN 12)
       - again without a layout() per keystroke;
    5. END PROCESS: filter "pipewire", click its row (AZMON_SELECT 812 pipewire), Delete
       asks (AZMON_ASK), "Kill" ends it (AZMON_END 812 true, AZMON_NOTICE "Killed
       pipewire (812)", 44 processes at the next reading); sshd (root) is refused
       ("administrator rights");
    6. PERFORMANCE: the tab shows the performance page (charts, the per-core bars) and
       its readings again run no layout();
    7. SETTINGS: Mod+, -> "2 s" (AZMON_SPEED 2000), "Export the last minute" writes
       monitor/history/<date>.csv into the data folder (AZMON_EXPORTED);
    8. a screenshot after each step (flat, light); the mode switched to dark at the end.

Usage (after building libazul with the debug server and AzMonitor; ONE app at a time,
through scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azmonitor_e2e.py [--bin target/release/AzMonitor]
        [--debug-port 8791] [--timeout 180] [--out <dir>] [--keep]
"""

import glob
import os

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azmonitor"
PROCESSES = 45
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


def cpu_headline(app):
    """The CPU card's headline as the window shows it ("CPU 23.4 %"): a VirtualView, so a DOM
    of its own."""
    for text in app.texts(every_dom=True):
        if text.startswith("CPU ") and "%" in text:
            return text
    return None


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
        for stem in ("cards", "table-view", "tools", "filter"):
            if not app.has_id("__azmonitor_" + stem):
                raise Failure("#__azmonitor_%s is not in the tree" % stem)
        # The table is a VirtualView: its rows are a DOM of their own.
        app.until("the table's rows", lambda: app.shows("cargo", every_dom=True))
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "1-processes.png"))

        # ---- 2. readings re-render the live views, never the page ----
        wait_ticks(app, 1, "a reading after the page was built")
        app.frame(2)
        before_layouts = layouts(app)
        before_cpu = cpu_headline(app)
        wait_ticks(app, 3, "three readings")
        app.frame(2)
        after_cpu = cpu_headline(app)
        if layouts(app) != before_layouts:
            raise Failure("readings ran layout(): %d -> %d" % (before_layouts, layouts(app)))
        if not before_cpu or not after_cpu or before_cpu == after_cpu:
            raise Failure("the CPU card did not follow the readings: %r -> %r" % (before_cpu, after_cpu))
        app.log("3 readings, layout() ran 0 times; CPU card %r -> %r" % (before_cpu, after_cpu))

        # ---- 3. sort by name ----
        app.click(text="Name", every_dom=True)
        app.until("the name sort", lambda: app.last("AZMON_SORT") == "Name asc")
        app.until("accounts-daemon on top",
                  lambda: (app.last("AZMON_TOP") or "").endswith(" accounts-daemon"))
        app.log("sorted by name; top: %s" % app.last("AZMON_TOP"))
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "2-sorted-name.png"))

        # ---- 4. filter ----
        before_layouts = layouts(app)
        app.text_input(P + "filter", "rustc")
        app.expect_line("AZMON_SHOWN", "12", "the rustc filter")
        if layouts(app) != before_layouts:
            raise Failure("typing into the filter ran layout(): %d -> %d" % (before_layouts, layouts(app)))
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "3-filtered.png"))

        # ---- 5. end a process; a root process is refused ----
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
        app.screenshot(os.path.join(out, "4-question.png"))
        app.click(text="Kill")
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
        app.click(text="Kill")
        app.until("the refusal", lambda: "administrator" in (app.last("AZMON_NOTICE") or ""))
        app.log("sshd refused: %s" % app.last("AZMON_NOTICE"))
        clear_filter(app)
        app.expect_line("AZMON_SHOWN", str(PROCESSES - 1), "every process again")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "5-ended.png"))

        # ---- 6. the performance page ----
        app.click(text="Performance")
        app.expect_line("AZMON_SCREEN", "performance", "the performance tab")
        app.frame(3)
        for stem in ("performance",):
            if not app.has_id("__azmonitor_" + stem):
                raise Failure("#__azmonitor_%s is not in the tree" % stem)
        wait_ticks(app, 1, "a reading on the performance page")
        app.frame(2)
        if not app.has_id("__azmonitor_cores"):
            raise Failure("the per-core bars (#__azmonitor_cores) are not shown")
        before_layouts = layouts(app)
        wait_ticks(app, 2, "two readings on the performance page")
        app.frame(2)
        if layouts(app) != before_layouts:
            raise Failure("readings on the performance page ran layout()")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "6-performance.png"))

        # ---- 7. settings: the update speed, the export ----
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
        app.screenshot(os.path.join(out, "7-settings.png"))
        app.key("escape")

        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "8-dark.png"))
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
