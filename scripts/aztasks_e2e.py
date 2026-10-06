#!/usr/bin/env python3
"""AzTasks end to end, headless over the debug server.

    1. starts AzTasks (AZ_BACKEND=headless, AZ_DEBUG=--debug-port) on a fresh data folder with
       `--sample` and a fast reminder tick, waits for `AZTASKS_LOADED`;
    2. the sample's due reminder ("Call the dentist") shows: `AZTASKS_REMINDER`, the
       `#reminder-banner` in the tree, a recorded notification (`assert_notification`, where
       the debug server has the op); Dismiss closes the banner;
    3. quick add with a date phrase: types "Water the ferns tomorrow every week #home" into
       `#quick-add`, Enter -> `AZTASKS_ADDED <id> <list> <tomorrow>`, the task's file
       `tasks/<list>/<id>.json` lands on disk (with its repeat and tag);
    4. it lands in Upcoming: Cmd+2 -> `#task-<id>` in `#section-day-<tomorrow>`; a second
       quick add "Pay the plumber today 11:59pm !high" lands in Today (Cmd+1);
    5. All (Cmd+5): a click on a task title selects it alone, the list and the details stay;
       completes the repeating task (its check box) -> `AZTASKS_COMPLETED`, `AZTASKS_SPAWNED
       <new> <tomorrow + 7>`, both files on disk, the old one `completed`, the new one
       repeating;
    6. Settings (FILE) opens the backstage; Data: Export (`AZTASKS_EXPORTED <n> <key>`, the
       file lands in `aztasks/exports/` of the data tree with n VTODOs), Import of a one-to-do
       .ics (`AZTASKS_IMPORTED 1 <path>`); Appearance: Flora is kept in
       `aztasks/settings.json`; Escape closes it; flora + dark screenshot;
    7. restarts AzTasks on the same folder without `--sample` and `--view scheduled`: the
       spawned task is listed (`#task-<new>`), the completed one is in Completed (Cmd+6), and
       the counts match the files.

Usage (from the azul repository, after building libazul with the debug server and AzTasks;
run it through scratchpad/run_capped.sh, one app at a time):

    python3 scripts/aztasks_e2e.py [--bin target/release/AzTasks] [--debug-port 8772]
        [--timeout 180] [--out <dir>] [--keep]

`AZTASKS_BIN` also names the binary. Screenshots and logs go to --out (default: a temporary
folder printed at the end). The debug-server client is the Azlin apps' shared one
(`scripts/azlin_e2e.py`).
"""

import argparse
import datetime
import json
import os
import shutil
import sys
import tempfile
import time
import urllib.error

import azlin_e2e as e2e
from azlin_e2e import Failure



def log(line):
    print("[aztasks-e2e] %s" % line, flush=True)


# AzTasks' DOM ids carry the app's prefix `__aztasks_` (examples/azul-tasks/src/ids.rs, the
# wave-6 prefix ruling): the `#name`s in the steps above are `#__aztasks_name` (`app.sel`);
# an older build's bare names are detected (`app.detect_naming`).
PREFIX = "__aztasks_"


def detect_naming(app):
    app.detect_naming(PREFIX, "quick-add")


def read_task(data_dir, list_id, task_id):
    path = os.path.join(data_dir, "tasks", list_id, "%s.json" % task_id)
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def wait_file(app, data_dir, list_id, task_id, check=lambda t: True, what="the task file"):
    def ready():
        try:
            t = read_task(data_dir, list_id, task_id)
        except (OSError, ValueError):
            return None
        return t if check(t) else None
    return app.until("%s of %s" % (what, task_id), ready)


def quick_add(app, text):
    """Types `text` into the quick-add line and presses Enter; returns (id, list, due)."""
    before = len(app.printed("AZTASKS_ADDED", r"\S+ \S+ \S+"))
    # `app.click` settles first: dismissing the reminder banner slides the list up (a layout
    # animation of ~170 nodes), and a click at the field's centre mid-slide landed 14 px below it.
    app.click(selector=app.sel("quick-add"))
    app.frame(1)
    app.must("text_input", text=text)
    app.frame(2)
    app.key("Return")
    lines = app.until("AZTASKS_ADDED for %r" % text,
                      lambda: (lambda l: l if len(l) > before else None)(app.printed("AZTASKS_ADDED", r"\S+ \S+ \S+")))
    task_id, list_id, due = lines[-1].split(" ")
    log("added %r -> %s in %s, due %s" % (text, task_id, list_id, due))
    return task_id, list_id, due


def switch_layout(app, second):
    """Clicks the list header's layout switch: its second half ("Month" / "Board") or its first
    ("List")."""
    app.settle()  # a click at a layout rect while a layout animation runs misses (see quick_add)
    box = app.box(app.sel("layout-switch"))
    x = box["x"] + box["width"] * (0.75 if second else 0.25)
    app.must("click", x=x, y=box["y"] + box["height"] / 2.0)
    app.frame(2)


def centre(box):
    return box["x"] + box["width"] / 2.0, box["y"] + box["height"] / 2.0


def drag_onto(app, source, target):
    """Drags the node `source` onto the middle of the node `target` (both selectors). A pointer
    drag goes where the coordinates are, as a user's hand does: both nodes are scrolled into
    view first (a card low in a long column sat below the window - the drag started off it)."""
    app.settle()
    for node in (source, target):
        app.must("scroll_into_view", selector=node)
        app.frame(2)
    (x0, y0), (x1, y1) = centre(app.box(source)), centre(app.box(target))
    app.drag(x0, y0, x1, y1)


def planned_box(app, date, title):
    """The box of the task `title` in the planned month's day `date`, or None. A day's FIRST
    planned task is not the one to drag: the sample has tasks of its own on tomorrow, and the
    drag moved one of them."""
    nodes = {n["index"]: n for n in app.hierarchy()}
    day_id = app.name("month-day-%s" % date.isoformat())
    task_class = app.name("planned-task")
    for n in nodes.values():
        if title not in (n.get("text") or ""):
            continue
        at = n
        while at is not None and task_class not in (at.get("classes") or []):
            at = nodes.get(at.get("parent"))
        if at is None or (nodes.get(at.get("parent")) or {}).get("id") != day_id:
            continue
        r = (app.value("get_node_layout", node_id=at["index"]) or {}).get("rect") or {}
        if r.get("width"):
            return {key: float(r.get(key, 0)) for key in ("x", "y", "width", "height")}
    return None


def planned_and_board(app, data_dir, ferns, ferns_list, plumber, tomorrow, out):
    """Scheduled as the planned month (a drag onto a day moves the due day), the list as its
    board (a drag onto Doing starts a task). An older build has no layout switch: skipped."""
    app.key("3", primary=True)
    app.until("AZTASKS_VIEW scheduled", lambda: app.printed("AZTASKS_VIEW", r"\S+")[-1:] == ["scheduled"])
    app.frame(2)
    if not app.has(app.sel("layout-switch")):
        log("BLOCKED planned month / board: no layout switch (a build before layouts.rs)")
        return
    switch_layout(app, True)
    app.until("AZTASKS_LAYOUT scheduled month",
              lambda: "scheduled month" in app.printed("AZTASKS_LAYOUT", r".+"))
    ferns_on = lambda date: planned_box(app, date, "Water the ferns")
    app.until("the ferns on tomorrow in the planned month", lambda: ferns_on(tomorrow))
    app.screenshot(os.path.join(out, "planned-month.png"))
    # Next month, and back to this one.
    first = (tomorrow - datetime.timedelta(days=1)).replace(day=1)  # the month shown: today's
    next_month = (first + datetime.timedelta(days=32)).strftime("%Y-%m")
    seen = len(app.printed("AZTASKS_MONTH", r"\S+"))
    app.click(selector=app.sel("month-next"))
    app.frame(2)
    app.until("AZTASKS_MONTH %s" % next_month, lambda: next_month in app.printed("AZTASKS_MONTH", r"\S+")[seen:])
    app.click(selector=app.sel("month-today"))
    app.frame(2)
    app.until("this month again", lambda: ferns_on(tomorrow))
    # A drag onto the day after: due then (the time and the repeat kept); and back.
    later = tomorrow + datetime.timedelta(days=1)
    for start, due in ((tomorrow, later), (later, tomorrow)):
        app.settle()
        source = app.until("the ferns on %s" % start, lambda: ferns_on(start))
        target = app.box(app.sel("month-day-%s" % due.isoformat()))
        (x0, y0), (x1, y1) = centre(source), centre(target)
        log("dragging the ferns from %s (%.0f, %.0f) onto %s (%.0f, %.0f)" % (start, x0, y0, due, x1, y1))
        app.drag(x0, y0, x1, y1)
        app.until("AZTASKS_DUE %s %s" % (ferns, due),
                  lambda: "%s %s" % (ferns, due.isoformat()) in app.printed("AZTASKS_DUE", r".+"))
        wait_file(app, data_dir, ferns_list, ferns, lambda t: t.get("due") == due.isoformat(), "the due day %s" % due)
    switch_layout(app, False)
    app.until("the list of days again", lambda: app.has(app.sel("task-list")))
    log("planned month: the ferns on tomorrow; a drag moved them a day and back")

    # The board of the ferns' list.
    with open(os.path.join(data_dir, "tasks", ferns_list, "list.json"), "r", encoding="utf-8") as f:
        name = json.load(f)["name"]
    app.click(text=name)
    app.frame(2)
    app.until("AZTASKS_VIEW list:%s" % ferns_list,
              lambda: app.printed("AZTASKS_VIEW", r"\S+")[-1:] == ["list:%s" % ferns_list])
    switch_layout(app, True)
    app.until("AZTASKS_LAYOUT ... board", lambda: "list:%s board" % ferns_list in app.printed("AZTASKS_LAYOUT", r".+"))
    card = app.sel("card-%s" % plumber)
    app.until("the plumber's card in To do", lambda: app.has("%s %s" % (app.sel("column-todo"), card)))
    drag_onto(app, card, app.sel("column-doing"))
    app.until("AZTASKS_COLUMN %s doing" % plumber,
              lambda: "%s doing" % plumber in app.printed("AZTASKS_COLUMN", r".+"))
    wait_file(app, data_dir, ferns_list, plumber, lambda t: "started" in t, "the start")
    app.until("the plumber's card in Doing", lambda: app.has("%s %s" % (app.sel("column-doing"), card)))
    app.screenshot(os.path.join(out, "board.png"))
    switch_layout(app, False)
    app.until("the list again", lambda: app.has(app.sel("task-list")))
    log("board: the plumber's card dragged from To do to Doing is started")


def run(args, logs, out, data_dir):
    binary = e2e.find_binary("AzTasks", args.bin, "AZTASKS_BIN")
    env = {"AZTASKS_TICK_MS": "500"}
    today = datetime.date.today()
    tomorrow = today + datetime.timedelta(days=1)

    # ---- first run: the sample, a reminder, quick add, Upcoming / Today, completing a repeat
    app = e2e.App("first", binary,
                  ["--data", data_dir, "--sample", "--view", "today", "--size", "1280x800"],
                  args.debug_port, logs, args.timeout, extra_env=env)
    try:
        loaded = app.until("AZTASKS_LOADED", lambda: app.printed("AZTASKS_LOADED", r"\d+ \d+ \d+"))
        lists, tasks, skipped = (int(x) for x in loaded[-1].split())
        log("loaded %d lists, %d tasks, %d skipped" % (lists, tasks, skipped))
        if lists < 6 or tasks < 20:
            raise Failure("the sample is missing: %s" % loaded[-1])
        detect_naming(app)
        app.screenshot(os.path.join(out, "today.png"))

        # The sample's due reminder.
        reminded = app.until("AZTASKS_REMINDER", lambda: app.printed("AZTASKS_REMINDER", r"\S+"))
        app.frame(2)
        app.until("the reminder banner", lambda: app.has(app.sel("reminder-banner")))
        if not app.shows("Call the dentist"):
            raise Failure("the banner does not name the reminding task")
        try:
            answer = app.op("assert_notification", title="Reminder")
            if isinstance(answer, dict) and answer.get("status") == "error":
                log("WARN assert_notification: %s" % json.dumps(answer)[:200])
            else:
                log("the reminder posted a notification")
        except (urllib.error.URLError, OSError, ValueError) as e:
            log("WARN assert_notification unavailable: %s" % e)
        app.screenshot(os.path.join(out, "reminder.png"))
        app.click(selector=app.sel("dismiss-reminder"))
        app.frame(2)
        app.until("the banner to close", lambda: not app.has(app.sel("reminder-banner")))
        log("reminder %s shown and dismissed" % reminded[-1])

        # Quick add with a date phrase, a repeat and a tag.
        ferns, ferns_list, due = quick_add(app, "Water the ferns tomorrow every week #home")
        if due != tomorrow.isoformat():
            raise Failure("'tomorrow' parsed as %s, not %s" % (due, tomorrow))
        task = wait_file(app, data_dir, ferns_list, ferns)
        if task.get("title") != "Water the ferns" or "repeat" not in task or task.get("tags") != ["home"]:
            raise Failure("the file of the new task is wrong: %s" % json.dumps(task))

        # It lands in Upcoming, under tomorrow.
        app.key("2", primary=True)
        app.until("AZTASKS_VIEW upcoming", lambda: "upcoming" in app.printed("AZTASKS_VIEW", r"\S+"))
        app.frame(2)
        app.until("the task in Upcoming", lambda: app.has(app.sel("task-%s") % ferns))
        if not app.has(app.sel("section-day-%s") % tomorrow.isoformat()):
            raise Failure("Upcoming has no section for tomorrow")
        app.screenshot(os.path.join(out, "upcoming.png"))

        # A task due today lands in Today.
        plumber, plumber_list, due = quick_add(app, "Pay the plumber today 11:59pm !high")
        if due != today.isoformat():
            raise Failure("'today' parsed as %s" % due)
        app.key("1", primary=True)
        app.until("AZTASKS_VIEW today", lambda: app.printed("AZTASKS_VIEW", r"\S+")[-1] == "today")
        app.frame(2)
        app.until("the task in Today", lambda: app.has(app.sel("task-%s") % plumber))
        wait_file(app, data_dir, plumber_list, plumber, lambda t: t.get("priority") == "high", "the high priority")

        # All: a click on a task's title selects that task alone, the list stays and the details
        # show it (PIM6 saw both panes go blank; not seen again on the wave-6 build).
        app.key("5", primary=True)
        app.until("AZTASKS_VIEW all", lambda: app.printed("AZTASKS_VIEW", r"\S+")[-1] == "all")
        app.frame(2)
        for title, task_id in (("Pay the plumber", plumber), ("Water the ferns", ferns)):
            # The row's title (the To-Do bar lists a task due today under the same text), scrolled
            # into the list's view first (a click op at a row below it lands outside the list).
            app.must("scroll_into_view", selector=app.sel("task-%s") % task_id, block="center", behavior="instant")
            app.frame(2)
            app.click(selector="%s .%stask-title" % (app.sel("task-%s") % task_id, app.prefix))
            app.frame(2)
            app.until("AZTASKS_SELECTED %s" % task_id,
                      lambda: app.printed("AZTASKS_SELECTED", r"\S+")[-1:] == [task_id])
            app.until("the list and the details", lambda: app.has(app.sel("task-%s") % task_id)
                      and app.has(app.sel("detail-title")))
            if app.shows("tasks selected"):
                raise Failure("a plain click on %r added to the selection" % title)
        app.screenshot(os.path.join(out, "all-click.png"))
        log("All: a click on a title selects it alone; the list and the details stay")

        planned_and_board(app, data_dir, ferns, ferns_list, plumber, tomorrow, out)

        # Completing the repeating task leaves next week's behind.
        app.key("2", primary=True)
        app.frame(2)
        app.click(selector=app.sel("check-%s") % ferns)
        app.frame(2)
        app.until("AZTASKS_COMPLETED", lambda: ferns in app.printed("AZTASKS_COMPLETED", r"\S+"))
        spawned = app.until("AZTASKS_SPAWNED", lambda: app.printed("AZTASKS_SPAWNED", r"\S+ \S+"))
        new_id, new_due = spawned[-1].split(" ")
        if new_due != (tomorrow + datetime.timedelta(days=7)).isoformat():
            raise Failure("the next occurrence is due %s, not a week after %s" % (new_due, tomorrow))
        wait_file(app, data_dir, ferns_list, ferns, lambda t: "completed" in t and "repeat" not in t,
                  "the completion")
        wait_file(app, data_dir, ferns_list, new_id, lambda t: "repeat" in t and t.get("due") == new_due,
                  "the next occurrence")
        log("completed %s; the next one is %s, due %s" % (ferns, new_id, new_due))

        # FILE opens the backstage (settings); Escape closes it.
        app.click(text="FILE")
        app.frame(2)
        app.until("the backstage", lambda: app.has(app.sel("backstage")))
        app.screenshot(os.path.join(out, "settings.png"))

        # Settings > Data: export the tasks into the data tree, import an iCalendar to-do.
        app.click(text="Data")
        app.frame(2)
        app.until("the import and export controls", lambda: app.has(app.sel("settings-export")))
        app.click(selector=app.sel("settings-export"))
        app.frame(2)
        exported = app.until("AZTASKS_EXPORTED", lambda: app.printed("AZTASKS_EXPORTED", r"\d+ \S+"))
        count, key = exported[-1].split(" ", 1)
        export_path = os.path.join(data_dir, *key.split("/"))

        def export_landed():
            try:
                with open(export_path, "r", encoding="utf-8") as f:
                    text = f.read()
            except OSError:
                return None
            return text if text.count("BEGIN:VTODO") == int(count) else None

        app.until("the export file %s" % key, export_landed)
        if not key.startswith("aztasks/exports/"):
            raise Failure("the export %s is not in aztasks/exports of the data tree" % key)
        ics = os.path.join(logs, "import.ics")
        with open(ics, "w", encoding="utf-8", newline="") as f:
            f.write("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//e2e//EN\r\nBEGIN:VTODO\r\n"
                    "UID:e2e-1@example.org\r\nSUMMARY:Imported from iCal\r\n"
                    "DUE;VALUE=DATE:%s\r\nPRIORITY:1\r\nEND:VTODO\r\nEND:VCALENDAR\r\n"
                    % tomorrow.strftime("%Y%m%d"))
        app.click(selector=app.sel("settings-import-path"))
        app.frame(1)
        app.must("text_input", text=ics)
        app.frame(2)
        app.click(selector=app.sel("settings-import"))
        app.frame(2)
        imported = app.until("AZTASKS_IMPORTED", lambda: app.printed("AZTASKS_IMPORTED", r"\d+ .+"))
        if not imported[-1].startswith("1 "):
            raise Failure("AZTASKS_IMPORTED %s: expected the one to-do" % imported[-1])
        log("exported %s to-do(s) to %s; imported 1 from %s" % (count, key, ics))

        # Settings > Appearance: Flora is kept for the next start (aztasks/settings.json).
        app.click(text="Appearance")
        app.frame(2)
        app.until("the theme control", lambda: app.has(app.sel("settings-theme")))
        app.click(text="Flora")
        app.frame(2)
        appearance_file = os.path.join(data_dir, "aztasks", "settings.json")

        def kept_flora():
            try:
                with open(appearance_file, "r", encoding="utf-8") as f:
                    return json.load(f).get("theme") == "flora"
            except (OSError, ValueError):
                return False

        app.until("flora in aztasks/settings.json", kept_flora)
        app.key("Escape")
        app.until("the backstage to close", lambda: not app.has(app.sel("backstage")))

        # Flora, dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.screenshot(os.path.join(out, "flora-dark.png"))
        app.must("set_theme", theme="flat")
        app.must("set_mode", mode="light")
        app.frame(2)
        # Let the last writes land before the restart.
        app.until("the files to settle", lambda: app.printed("AZTASKS_SAVED", r"\S+"))
        time.sleep(1.0)
    except Failure as e:
        log("FAIL (first run): %s" % e)
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # ---- second run: the files are read back
    app = e2e.App("second", binary, ["--data", data_dir, "--view", "scheduled"], args.debug_port, logs,
                  args.timeout, extra_env=env)
    try:
        loaded = app.until("AZTASKS_LOADED", lambda: app.printed("AZTASKS_LOADED", r"\d+ \d+ \d+"))
        lists2, tasks2, skipped2 = (int(x) for x in loaded[-1].split())
        files = sum(
            1 for root, _, names in os.walk(os.path.join(data_dir, "tasks"))
            for n in names if n.endswith(".json") and n not in ("list.json", "settings.json")
        )
        log("restart: %d lists, %d tasks (%d task files), %d skipped" % (lists2, tasks2, files, skipped2))
        if tasks2 != files or skipped2 != 0:
            raise Failure("the restart read %d tasks from %d files (%d skipped)" % (tasks2, files, skipped2))
        if tasks2 != tasks + 4:
            raise Failure("expected the %d sample tasks + 4 (two added, one spawned, one imported), read %d" % (tasks, tasks2))
        detect_naming(app)
        app.until("the next occurrence in Scheduled", lambda: app.has(app.sel("task-%s") % new_id))
        app.key("6", primary=True)
        app.frame(2)
        app.until("the completed task in Completed", lambda: app.has(app.sel("task-%s") % ferns))
        app.screenshot(os.path.join(out, "restart-completed.png"))
        log("PASS")
        return True
    except Failure as e:
        log("FAIL (restart): %s" % e)
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8772)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--out")
    parser.add_argument("--keep", action="store_true", help="keep the data folder")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="aztasks-e2e-")
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    passed = False
    try:
        passed = run(args, logs, out, data_dir)
    except Failure:
        passed = False
    finally:
        if passed and not args.keep:
            shutil.rmtree(data_dir, ignore_errors=True)
        log("logs and screenshots in %s" % logs)
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
