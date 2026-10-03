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
    5. completes the repeating task (its check box) -> `AZTASKS_COMPLETED`, `AZTASKS_SPAWNED
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
folder printed at the end).
"""

import argparse
import base64
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, ".."))
CMD = "meta" if sys.platform == "darwin" else "ctrl"


def log(line):
    print("[aztasks-e2e] %s" % line, flush=True)


class Failure(Exception):
    pass


def find_binary(explicit):
    candidates = [explicit, os.environ.get("AZTASKS_BIN")]
    roots = [REPO]
    try:
        common = subprocess.run(
            ["git", "-C", REPO, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            check=True, capture_output=True, text=True,
        ).stdout.strip()
        if common:
            roots.append(os.path.dirname(common))
    except (OSError, subprocess.CalledProcessError):
        pass
    for root in roots:
        for sub in ("release", "debug"):
            candidates.append(os.path.join(root, "target", sub, "AzTasks"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no AzTasks binary; pass --bin or set AZTASKS_BIN (tried %s)" % candidates)


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for v in value:
            yield from strings(v)
    elif isinstance(value, dict):
        for v in value.values():
            yield from strings(v)


def tail(path, lines=40):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return "".join(f.readlines()[-lines:])
    except OSError:
        return "(no output)"


class App:
    """AzTasks under its debug server."""

    def __init__(self, binary, port, args, env, logs, name, deadline):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "%s.stdout" % name)
        self.err_path = os.path.join(logs, "%s.stderr" % name)
        self.process = subprocess.Popen(
            [binary] + args, env=env, stdin=subprocess.DEVNULL,
            stdout=open(self.out_path, "wb"), stderr=open(self.err_path, "wb"),
        )

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()

    def op(self, op, **params):
        body = {"op": op}
        body.update(params)
        request = urllib.request.Request(
            "http://127.0.0.1:%d/" % self.port,
            data=json.dumps(body).encode("utf-8"), method="POST",
        )
        with urllib.request.urlopen(request, timeout=15) as response:
            return json.loads(response.read().decode("utf-8") or "{}")

    def must(self, op, **params):
        answer = self.op(op, **params)
        if isinstance(answer, dict) and answer.get("status") == "error":
            raise Failure("%s %s failed: %s" % (op, json.dumps(params), json.dumps(answer)[:300]))
        return answer

    def value(self, op, **params):
        answer = self.must(op, **params)
        data = answer.get("data") if isinstance(answer, dict) else None
        if isinstance(data, dict) and "value" in data:
            return data["value"]
        return data if data is not None else answer

    def frame(self, n=1):
        for _ in range(n):
            self.must("wait_frame")

    def texts(self):
        return list(strings(self.op("get_node_hierarchy")))

    def shows(self, text):
        return any(text in t for t in self.texts())

    def has(self, selector):
        """Whether `selector` names a laid-out node."""
        try:
            value = self.value("get_node_layout", selector=selector)
        except (Failure, urllib.error.URLError, OSError, ValueError):
            return False
        return isinstance(value, dict) and value.get("node_id") is not None

    def rect(self, selector):
        value = self.value("get_node_layout", selector=selector)
        return value.get("rect") or {}

    def printed(self, key, pattern=r".+"):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^%s (%s)$" % (re.escape(key), pattern), text, re.M)

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("AzTasks exited (%s) while waiting for %s" % (self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, urllib.error.URLError, Failure) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def key(self, key, shift=False, ctrl=False, alt=False, meta=False):
        mods = {"shift": shift, "ctrl": ctrl, "alt": alt, "meta": meta}
        self.must("key_down", key=key, modifiers=mods)
        self.must("key_up", key=key, modifiers=mods)
        self.frame(2)

    def cmd(self, key):
        self.key(key, ctrl=(CMD == "ctrl"), meta=(CMD == "meta"))

    def screenshot(self, path):
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            log("take_screenshot returned no PNG (%s)" % json.dumps(value)[:120])
            return
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


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
    app.must("click", selector="#quick-add")
    app.frame(1)
    app.must("text_input", text=text)
    app.frame(2)
    app.key("Return")
    lines = app.until("AZTASKS_ADDED for %r" % text,
                      lambda: (lambda l: l if len(l) > before else None)(app.printed("AZTASKS_ADDED", r"\S+ \S+ \S+")))
    task_id, list_id, due = lines[-1].split(" ")
    log("added %r -> %s in %s, due %s" % (text, task_id, list_id, due))
    return task_id, list_id, due


def run(args, logs, out, data_dir):
    binary = find_binary(args.bin)
    deadline = time.time() + args.timeout
    env = dict(os.environ)
    env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(args.debug_port), "AZTASKS_TICK_MS": "500"})
    today = datetime.date.today()
    tomorrow = today + datetime.timedelta(days=1)

    # ---- first run: the sample, a reminder, quick add, Upcoming / Today, completing a repeat
    app = App(binary, args.debug_port,
              ["--data", data_dir, "--sample", "--view", "today", "--size", "1280x800"],
              env, logs, "first", deadline)
    try:
        loaded = app.until("AZTASKS_LOADED", lambda: app.printed("AZTASKS_LOADED", r"\d+ \d+ \d+"))
        lists, tasks, skipped = (int(x) for x in loaded[-1].split())
        log("loaded %d lists, %d tasks, %d skipped" % (lists, tasks, skipped))
        if lists < 6 or tasks < 20:
            raise Failure("the sample is missing: %s" % loaded[-1])
        app.until("the window", lambda: app.has("#quick-add"))
        app.screenshot(os.path.join(out, "today.png"))

        # The sample's due reminder.
        reminded = app.until("AZTASKS_REMINDER", lambda: app.printed("AZTASKS_REMINDER", r"\S+"))
        app.frame(2)
        app.until("the reminder banner", lambda: app.has("#reminder-banner"))
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
        app.must("click", selector="#dismiss-reminder")
        app.frame(2)
        app.until("the banner to close", lambda: not app.has("#reminder-banner"))
        log("reminder %s shown and dismissed" % reminded[-1])

        # Quick add with a date phrase, a repeat and a tag.
        ferns, ferns_list, due = quick_add(app, "Water the ferns tomorrow every week #home")
        if due != tomorrow.isoformat():
            raise Failure("'tomorrow' parsed as %s, not %s" % (due, tomorrow))
        task = wait_file(app, data_dir, ferns_list, ferns)
        if task.get("title") != "Water the ferns" or "repeat" not in task or task.get("tags") != ["home"]:
            raise Failure("the file of the new task is wrong: %s" % json.dumps(task))

        # It lands in Upcoming, under tomorrow.
        app.cmd("2")
        app.until("AZTASKS_VIEW upcoming", lambda: "upcoming" in app.printed("AZTASKS_VIEW", r"\S+"))
        app.frame(2)
        app.until("the task in Upcoming", lambda: app.has("#task-%s" % ferns))
        if not app.has("#section-day-%s" % tomorrow.isoformat()):
            raise Failure("Upcoming has no section for tomorrow")
        app.screenshot(os.path.join(out, "upcoming.png"))

        # A task due today lands in Today.
        plumber, plumber_list, due = quick_add(app, "Pay the plumber today 11:59pm !high")
        if due != today.isoformat():
            raise Failure("'today' parsed as %s" % due)
        app.cmd("1")
        app.until("AZTASKS_VIEW today", lambda: app.printed("AZTASKS_VIEW", r"\S+")[-1] == "today")
        app.frame(2)
        app.until("the task in Today", lambda: app.has("#task-%s" % plumber))
        wait_file(app, data_dir, plumber_list, plumber, lambda t: t.get("priority") == "high", "the high priority")

        # Completing the repeating task leaves next week's behind.
        app.cmd("2")
        app.frame(2)
        app.must("click", selector="#check-%s" % ferns)
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
        app.must("click", text="FILE")
        app.frame(2)
        app.until("the backstage", lambda: app.has("#backstage"))
        app.screenshot(os.path.join(out, "settings.png"))

        # Settings > Data: export the tasks into the data tree, import an iCalendar to-do.
        app.must("click", text="Data")
        app.frame(2)
        app.until("the import and export controls", lambda: app.has("#settings-export"))
        app.must("click", selector="#settings-export")
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
        app.must("click", selector="#settings-import-path")
        app.frame(1)
        app.must("text_input", text=ics)
        app.frame(2)
        app.must("click", selector="#settings-import")
        app.frame(2)
        imported = app.until("AZTASKS_IMPORTED", lambda: app.printed("AZTASKS_IMPORTED", r"\d+ .+"))
        if not imported[-1].startswith("1 "):
            raise Failure("AZTASKS_IMPORTED %s: expected the one to-do" % imported[-1])
        log("exported %s to-do(s) to %s; imported 1 from %s" % (count, key, ics))

        # Settings > Appearance: Flora is kept for the next start (aztasks/settings.json).
        app.must("click", text="Appearance")
        app.frame(2)
        app.until("the theme control", lambda: app.has("#settings-theme"))
        app.must("click", text="Flora")
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
        app.until("the backstage to close", lambda: not app.has("#backstage"))

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
        print("---- stdout ----\n%s---- stderr ----\n%s" % (tail(app.out_path), tail(app.err_path)))
        raise
    finally:
        app.stop()

    # ---- second run: the files are read back
    app = App(binary, args.debug_port, ["--data", data_dir, "--view", "scheduled"], env, logs, "second",
              time.time() + args.timeout)
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
        app.until("the window", lambda: app.has("#quick-add"))
        app.until("the next occurrence in Scheduled", lambda: app.has("#task-%s" % new_id))
        app.cmd("6")
        app.frame(2)
        app.until("the completed task in Completed", lambda: app.has("#task-%s" % ferns))
        app.screenshot(os.path.join(out, "restart-completed.png"))
        log("PASS")
        return True
    except Failure as e:
        log("FAIL (restart): %s" % e)
        print("---- stdout ----\n%s---- stderr ----\n%s" % (tail(app.out_path), tail(app.err_path)))
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
