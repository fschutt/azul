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
    4. THE TABLE KEEPS ITS PLACE (the user, 2026-10-07: "no real scroll position being saved
       if the content updates"): AzFiles (by name the third row) is selected, a wheel DOWN
       over the table (the engine's raw delta_y < 0) scrolls it out of view by whole rows
       (AZMON_SCROLL > 2); three readings later the same process is the first row
       (AZMON_VIEW) and the scroll bar's thumb has not moved; then AzFiles - above the view -
       is killed: the next build shows the SAME process first, one row up in the list (the
       position is a place in the processes, not a row number); a rustc in view is selected
       and the table sorted by "User Name": the rustc is still selected and in view (it was
       past the first screen in the new order); a wheel up brings the table to its top;
    5. FILTER: "rustc" typed into the filter shows the 12 rustc processes (AZMON_SHOWN 12)
       - again without a layout() per keystroke;
    6. END PROCESS: filter "pipewire", click its row (AZMON_SELECT 812 pipewire), Delete
       asks (AZMON_ASK), "Kill" ends it (AZMON_END 812 true, AZMON_NOTICE "Killed
       pipewire (812)", 43 processes at the next reading); sshd (root) is refused
       ("administrator rights");
    7. PERFORMANCE: the tab shows the CPU / memory meters and history graphs (one per core:
       eight scrolling strips and the memory's) and its readings run no layout(); between
       readings no graph box and no strip changes its rect (the scroll moves the strips'
       transform, never their boxes); NETWORKING: the network graph and its figures; USERS:
       one row per user (root, user);
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
# The DataTable's scroll bar thumb; the graphs' boxes and strips (classes).
THUMB = ".__azul-native-data-table-thumb"
GRAPH = "__azmonitor_graph"
STRIP = "__azmonitor_graph-strip"


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


def view_of(value):
    """An AZMON_VIEW value `<top> <pid> <selected> <name>` as (top, pid, selected, name):
    the table's first row shown and its process, the selected process' row of the screen
    ("-" none selected, "out" not in view)."""
    parts = value.split(" ", 3)
    if len(parts) < 4:
        return None
    return int(parts[0]), int(parts[1]), parts[2], parts[3]


def last_view(app):
    """The table's latest build (AZMON_VIEW), or None."""
    value = app.last("AZMON_VIEW")
    return view_of(value) if value else None


def built_from(app, top):
    """The table's latest build when it shows row `top` first, else None."""
    view = last_view(app)
    return view if view and view[0] == top else None


def views_after(app, starts):
    """The table's builds printed after the first stdout line `starts(line)` holds for, oldest
    first (a reading's or a sort's line: the builds before it showed the rows before it)."""
    try:
        with open(app.out_path, "r", encoding="utf-8", errors="replace") as f:
            lines = f.read().splitlines()
    except OSError:
        return []
    found, seen = [], False
    for line in lines:
        if not seen:
            seen = starts(line)
        elif line.startswith("AZMON_VIEW "):
            view = view_of(line[len("AZMON_VIEW "):])
            if view:
                found.append(view)
    return found


def thumb_rect(app):
    """The process table's scroll bar thumb: its rect in the table's own DOM, or None."""
    for dom in app.dom_ids():
        if app._has_in(THUMB, dom):
            r = app.value("get_node_layout", selector=THUMB, dom_id=dom).get("rect") or {}
            return tuple(round(float(r.get(k, 0)), 1) for k in ("x", "y", "width", "height"))
    return None


def graph_rects(app):
    """The rects of the graphs' boxes and strips (in the page's own DOM), in tree order, each
    with whether it is a strip."""
    found = []
    for dom in app.dom_ids():
        for d in e2e.dicts(app.op("get_all_nodes_layout", dom_id=dom)):
            classes = d.get("classes") or []
            if (GRAPH in classes or STRIP in classes) and isinstance(d.get("rect"), dict):
                found.append((STRIP in classes,) + tuple(
                    round(float(d["rect"].get(k, 0)), 1) for k in ("x", "y", "width", "height")))
    return found


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

        # ---- 4. the table keeps its place in the processes while the readings go on ----
        app.until("accounts-daemon in the table", lambda: app.shows("accounts-daemon", every_dom=True))
        x, y = table_center(app)
        # AzFiles, by name the third row: selected now, out of view above after the scroll.
        app.click(text="AzFiles", every_dom=True)
        app.expect_line("AZMON_SELECT", "3310 AzFiles", "AzFiles selected")
        top = 0
        for _ in range(4):
            before = app.count("AZMON_SCROLL")
            app.must("wheel", x=x, y=y, delta_x=0, delta_y=-120)
            if not within(app, 5.0, lambda: app.count("AZMON_SCROLL") > before):
                raise Failure("a wheel down over the table did not scroll it (no AZMON_SCROLL): "
                              "the table took the wheel the other way, against its first row")
            top = int(app.last("AZMON_SCROLL"))
            if top >= 3:
                break
        if top < 3:
            raise Failure("four wheel turns down scrolled the table to row %d: AzFiles (row 2) "
                          "is still in view" % top)
        first = app.until("the table built from row %d" % top, lambda: built_from(app, top))
        app.frame(3)
        if app.shows("accounts-daemon", every_dom=True):
            raise Failure("the first process is still shown after scrolling %d rows down" % top)
        thumb = thumb_rect(app)
        if thumb is None:
            raise Failure("the process table has no scroll bar thumb after scrolling")
        # Three readings: the same process stays on top, the thumb where it was.
        wait_ticks(app, 3, "three readings after the scroll")
        app.frame(2)
        now = last_view(app)
        if now is None or now[:2] != first[:2]:
            raise Failure("a reading moved the table: row %d (%s) -> %s"
                          % (first[0], first[3], now and "row %d (%s)" % (now[0], now[3])))
        if thumb_rect(app) != thumb:
            raise Failure("the scroll bar's thumb moved between readings: %s -> %s"
                          % (thumb, thumb_rect(app)))
        app.log("scrolled to row %d (%s); three readings later the same process is on top, the "
                "thumb where it was" % (top, first[3]))
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "3-scrolled.png"))
        # AzFiles - above the view - ends: the view stays on its processes, one row up.
        app.key("delete")
        app.expect_line("AZMON_ASK", "3310 AzFiles", "the question for AzFiles")
        # The question is a Modal: a window of its own.
        app.click_exact("Kill", window=app.until("the question's window", app.popup))
        app.expect_line("AZMON_END", "3310 true", "the kill of AzFiles")
        alive = PROCESSES - 1

        def after_kill():
            return views_after(app, lambda line: line.startswith("AZMON_TICK ")
                               and line.split()[2:3] == [str(alive)])

        kept = app.until("the table built after AzFiles ended", after_kill)[-1]
        if kept[:2] != (top - 1, first[1]):
            raise Failure("a process ending above the view moved it: %s on row %d before, row %d "
                          "shows %s (pid %d) after" % (first[3], top, kept[0], kept[3], kept[1]))
        app.log("AzFiles ended above the view: %s is still the first row (row %d of %d)"
                % (kept[3], kept[0], alive))
        # A new sort keeps the selected process selected and in view.
        picked = app.after("a rustc selected", "AZMON_SELECT", r"\d+ rustc",
                           lambda: app.click(text="rustc", every_dom=True))
        pid = picked.split()[0]
        app.until("the selected rustc in view",
                  lambda: (last_view(app) or (0, 0, "-"))[2].isdigit())
        app.click(text="User Name", every_dom=True)
        app.until("the user sort", lambda: app.last("AZMON_SORT") == "User Name asc")
        sorted_views = app.until("the table built after the sort",
                                 lambda: views_after(app, lambda l: l == "AZMON_SORT User Name asc"))
        row = sorted_views[-1][2]
        if not row.isdigit():
            raise Failure("sorting by User Name left the selected rustc (%s) %s"
                          % (pid, "out of view" if row == "out" else "unselected"))
        if not app.shows(pid, every_dom=True):
            raise Failure("the selected rustc's PID %s is not in the table after the sort" % pid)
        app.log("sorted by User Name: rustc %s still selected, on row %s of the screen" % (pid, row))
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "3b-sorted-user.png"))
        # A wheel turn moves at most 12 rows: turn it up until the first row shows.
        for _ in range(4):
            app.must("wheel", x=x, y=y, delta_x=0, delta_y=600)
            if within(app, 3.0, lambda: app.last("AZMON_SCROLL") == "0"):
                break
        else:
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
        alive -= 1
        app.until("%d processes" % alive,
                  lambda: (app.last("AZMON_TICK") or "").split()[1:2] == [str(alive)])
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
        app.expect_line("AZMON_SHOWN", str(alive), "every process again")
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
        # A reading moves no graph: every box and strip keeps its rect (the scroll moves the
        # strips' transform, never their boxes) - the user saw them jitter at each reading.
        rects = app.until("the graphs' rects", lambda: graph_rects(app))
        strips = sum(1 for r in rects if r[0])
        if strips != CORES + 1:
            raise Failure("the Performance tab has %d graph strips, expected %d (one per core, "
                          "the memory's)" % (strips, CORES + 1))
        before_layouts = layouts(app)
        for _ in range(2):
            wait_ticks(app, 1, "a reading on the Performance tab")
            app.frame(2)
            again = graph_rects(app)
            if again != rects:
                moved = [(a, b) for a, b in zip(rects, again) if a != b]
                raise Failure("a reading moved the graphs (%d of %d rects, first %s): %s"
                              % (len(moved) or abs(len(again) - len(rects)), len(rects),
                                 moved[:1], "the strips' boxes must not move"))
        if layouts(app) != before_layouts:
            raise Failure("readings on the Performance tab ran layout()")
        app.log("two readings on the Performance tab: %d graphs, no box or strip moved"
                % (len(rects) - strips))
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
