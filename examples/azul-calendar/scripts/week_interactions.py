#!/usr/bin/env python3
"""AzCalendar's week, headless: zoom it, scroll it, and click or drag empty time to make events.

Through AzCalendar's debug server (AZ_BACKEND=headless, AZ_DEBUG=<port>), with an empty data
folder (AZCAL_DATA):

  1. zoom: puts 08:00 at the top of the week (#week-scroll), pinches (scale 1.5, a trackpad's
     magnify step) with the pointer over Wednesday, and checks the hour is 1.5 times taller and
     the time under the pointer is still under it;
  2. scroll: a plain wheel over the week scrolls it and does not zoom;
  3. click: clicks Wednesday 10:36; the week shows the draft "(No title)" and its popover says
     "<Wednesday> ..., 10:30 - 11:30"; types "Standup" into the popover's title (#draft-title) and
     presses Save (#draft-save); checks ONE events/<uuid>.json: "Standup", this week's Wednesday,
     10:30 - 11:30; the draft and the popover are gone and the week shows "Standup";
  4. drag: presses Thursday 13:05, drags to 14:50 and lets go: the draft is 13:00 - 15:00; types
     "Review", Save: the file says Thursday 13:00 - 15:00;
  5. outside: clicks Friday 16:00, then the app's name in the toolbar: the draft and the popover
     are gone and nothing was written;
  6. cancel: clicks Friday 16:00 again, then Cancel (#draft-cancel): the same;
  7. an existing event: clicks inside "Standup": no draft opens.

The popover is a <transient-window>: a window of its own. The headless backend creates that window
but never runs it (HeadlessWindow::run pumps child windows only for Close), and a key the parent
receives while a focus-taking popup is open is forwarded to the popup and spent
(forward_keys_to_popup). So Enter and Escape cannot reach the popover here, and neither can a
pointer. What the script drives instead are the popover's own nodes, which are part of the
app's DOM (the popup shows a subtree of it): `focus_node` + `text_input` on #draft-title, and
the accessibility default action (the click a screen reader sends) on Save / Cancel. Enter and
Escape in the popover run the same code as Save and as a dismissal; they need a real window.

Usage (from the azul repository, after building AzCalendar and a libazul with the debug server):
  python3 examples/azul-calendar/scripts/week_interactions.py
      [--bin target/release/AzCalendar] [--port 8769] [--timeout 60] [--keep-logs]

Also read from the environment: AZCAL_BIN. Logs and the data folder go to a temporary directory
that is printed at the end (kept on failure, or always with --keep-logs).
"""

import argparse
import datetime
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
EVENT_FILE = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.json$")
DRAFT_TITLE = "(No title)"


class Failure(Exception):
    pass


def log(line):
    print(f"[week-interactions] {line}", flush=True)


def main_repo():
    """The main checkout when this runs from a git worktree (for its target/)."""
    try:
        common = subprocess.run(
            ["git", "-C", REPO, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        return os.path.dirname(common)
    except (OSError, subprocess.CalledProcessError):
        return REPO


def find_binary(explicit):
    exe = "AzCalendar.exe" if os.name == "nt" else "AzCalendar"
    candidates = [explicit, os.environ.get("AZCAL_BIN")]
    for root in (REPO, main_repo()):
        for parts in (("release",), ("debug",), ("consumer", "release"), ("consumer", "debug")):
            candidates.append(os.path.join(root, "target", *parts, exe))
    for c in candidates:
        if c and os.path.exists(c):
            return os.path.abspath(c)
    tried = "\n  ".join(c for c in candidates if c)
    raise Failure(f"the AzCalendar binary was not found (pass --bin); tried:\n  {tried}")


class Debug:
    """AzCalendar's debug server: one op per POST, answered once the app processed it."""

    def __init__(self, port, deadline):
        self.url = f"http://127.0.0.1:{port}/"
        self.deadline = deadline

    def op(self, body):
        data = json.dumps(body).encode()
        req = urllib.request.Request(self.url, data=data, method="POST")
        with urllib.request.urlopen(req, timeout=15) as res:
            return json.loads(res.read().decode() or "{}")

    def must(self, body):
        answer = self.op(body)
        if answer.get("status") == "error":
            raise Failure(f"{json.dumps(body)} failed: {answer.get('message')}")
        return answer

    def value(self, body):
        return (self.must(body).get("data") or {}).get("value") or {}

    def until(self, what, check, every=0.25):
        last = None
        while time.time() < self.deadline:
            try:
                got = check()
                if got:
                    return got
            except (Failure, OSError, urllib.error.URLError, ValueError, KeyError) as e:
                last = e
            time.sleep(every)
        raise Failure(f"timed out waiting for {what}" + (f" (last error: {last})" if last else ""))

    # ---- reading the window ----

    def texts(self):
        out = []

        def walk(v):
            if isinstance(v, str):
                out.append(v)
            elif isinstance(v, list):
                for x in v:
                    walk(x)
            elif isinstance(v, dict):
                for x in v.values():
                    walk(x)

        walk(self.must({"op": "get_node_hierarchy"}))
        return out

    def shows(self, text):
        return any(text in t for t in self.texts())

    def rect(self, selector):
        """The node's laid-out rect (window coordinates, before any scrolling)."""
        v = self.value({"op": "get_node_layout", "selector": selector})
        r = v.get("rect")
        if not r:
            raise Failure(f"{selector} has no laid-out rect: {v}")
        return v["node_id"], r

    def exists(self, selector):
        answer = self.op({"op": "get_node_layout", "selector": selector})
        return answer.get("status") != "error"

    def scroll_y(self, node_id):
        v = self.value({"op": "get_scroll_states"})
        for s in v.get("scroll_states", []):
            if s.get("node_id") == node_id:
                return float(s["scroll_y"])
        raise Failure(f"node {node_id} (#week-scroll) is not a scroll box: {v}")


class Week:
    """Where things are in the week view, read from the app's own layout."""

    def __init__(self, dbg):
        self.dbg = dbg
        self.refresh()

    def refresh(self):
        self.scroll_node, self.scroll = self.dbg.rect("#week-scroll")
        _, grid = self.dbg.rect("#week-grid")
        self.hour_px = grid["height"] / 24.0
        self.scroll_y = self.dbg.scroll_y(self.scroll_node)

    def column(self, day):
        _, r = self.dbg.rect(f"#day-{day}")
        return r

    def scroll_to_minute(self, minute):
        """Puts `minute` at the top of the week, and waits until the week is there."""
        y = minute / 60.0 * self.hour_px
        self.dbg.must({"op": "scroll_node_to", "selector": "#week-scroll", "x": 0, "y": y})

        def there():
            self.refresh()
            return abs(self.scroll_y - y) < 1.0 or None

        self.dbg.until(f"the week to scroll to minute {minute}", there)

    def point(self, day, minute):
        """The window point over `day` (0 = Monday) at `minute`, as the week is scrolled now."""
        col = self.column(day)
        x = col["x"] + col["width"] / 2.0
        y = col["y"] + minute / 60.0 * self.hour_px - self.scroll_y
        top, bottom = self.scroll["y"], self.scroll["y"] + self.scroll["height"]
        if not top + 2 <= y <= bottom - 2:
            raise Failure(f"minute {minute} of day {day} is not in view (y {y}, view {top}..{bottom})")
        return x, y


def this_week(weekday):
    today = datetime.date.today()
    return today - datetime.timedelta(days=today.weekday()) + datetime.timedelta(days=weekday)


def event_files(data):
    folder = os.path.join(data, "events")
    try:
        return sorted(n for n in os.listdir(folder) if EVENT_FILE.match(n))
    except FileNotFoundError:
        return []


def read_event(data, name):
    with open(os.path.join(data, "events", name), encoding="utf-8") as f:
        return json.load(f)


def popover_open(dbg):
    return dbg.exists("#draft-title") and dbg.shows(DRAFT_TITLE)


def popover_gone(dbg):
    return not dbg.exists("#draft-title") and not dbg.shows(DRAFT_TITLE)


def type_title(dbg, title):
    """Types `title` into the popover's title field (see the module docs for why this way)."""
    focused = dbg.op({"op": "focus_node", "selector": "#draft-title"})
    if focused.get("status") != "error":
        typed = dbg.op({"op": "text_input", "text": title})
        if typed.get("status") != "error":
            return "focus_node + text_input"
    dbg.must(
        {"op": "accessibility_action", "action": "set_value", "value": title, "selector": "#draft-title"}
    )
    return "accessibility set_value"


def press(dbg, selector):
    """A screen reader's click on a popover button."""
    dbg.must({"op": "accessibility_action", "action": "default", "selector": selector})


def save_and_read(dbg, data, before, what):
    press(dbg, "#draft-save")
    files = dbg.until(
        f"{what} to be saved (a new events/<uuid>.json)",
        lambda: [f for f in event_files(data) if f not in before] or None,
    )
    if len(files) != 1:
        raise Failure(f"expected one new event file, found {files}")
    dbg.until("the draft and its popover to close after Save", lambda: popover_gone(dbg))
    return files[0], read_event(data, files[0])


def expect_event(event, name, title, date, start, end):
    got = (event.get("title"), event.get("date"), event.get("start"), event.get("end"))
    want = (title, date.isoformat(), start, end)
    if got != want:
        raise Failure(f"{name} holds {got}, expected {want}: {json.dumps(event)}")
    if f"{event.get('id')}.json" != name:
        raise Failure(f"{name} is not named by its event id {event.get('id')}")


def run(opts, logs):
    binary = find_binary(opts.bin)
    data = os.path.join(logs, "data")
    os.makedirs(data)
    log(f"AzCalendar: {binary}")
    log(f"logs and data: {logs}")
    env = dict(os.environ)
    env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(opts.port), "AZCAL_DATA": data})
    # No meeting server: this test makes no links, and must not reach one.
    env.pop("AZMEET_WORKER", None)
    out = open(os.path.join(logs, "azcalendar.out"), "w")
    err = open(os.path.join(logs, "azcalendar.err"), "w")
    app = subprocess.Popen([binary], env=env, stdout=out, stderr=err, stdin=subprocess.DEVNULL)
    try:
        dbg = Debug(opts.port, time.time() + opts.timeout)
        dbg.until("the week view", lambda: dbg.shows("This week"))
        if event_files(data):
            raise Failure("the data folder is not empty at the start")

        # 1. Zoom: a pinch over Wednesday keeps the time under the pointer.
        week = Week(dbg)
        week.scroll_to_minute(8 * 60)
        before_px = week.hour_px
        x, _ = week.point(2, 8 * 60 + 30)
        y = week.scroll["y"] + 100.0
        minute_before = (week.scroll_y + 100.0) * 60.0 / before_px
        dbg.must(
            {
                "op": "pinch",
                "scale": 1.5,
                "center_x": x,
                "center_y": y,
                "initial_distance": 100.0,
                "current_distance": 150.0,
                "duration_ms": 0,
            }
        )
        # The injected pinch is delivered by the next pass, with the pointer over the week.
        dbg.must({"op": "mouse_move", "x": x, "y": y})

        def zoomed():
            week.refresh()
            return abs(week.hour_px - before_px * 1.5) < 0.05 or None

        dbg.until(f"the hour to grow from {before_px} px to {before_px * 1.5} px", zoomed)
        minute_after = (week.scroll_y + 100.0) * 60.0 / week.hour_px
        if abs(minute_after - minute_before) > 1.0:
            raise Failure(
                f"after the pinch the pointer is over minute {minute_after:.1f}, "
                f"not {minute_before:.1f}"
            )
        log(f"pinch: {before_px:.1f} -> {week.hour_px:.1f} px an hour, minute {minute_before:.1f} "
            f"stayed under the pointer ({minute_after:.1f})")

        # 2. A plain wheel scrolls the week and does not zoom.
        start_y, start_px = week.scroll_y, week.hour_px
        dbg.must({"op": "wheel", "x": x, "y": y, "delta_x": 0, "delta_y": -120})

        def scrolled():
            week.refresh()
            return abs(week.scroll_y - start_y) > 20 or None

        dbg.until("a plain wheel to scroll the week", scrolled)
        if abs(week.hour_px - start_px) > 0.05:
            raise Failure(f"a plain wheel zoomed the week ({start_px} -> {week.hour_px} px an hour)")
        log(f"wheel: scrolled {start_y:.0f} -> {week.scroll_y:.0f} px, still {week.hour_px:.1f} px an hour")

        # 3. Click Wednesday 10:36: a draft 10:30 - 11:30 with its popover; title, Save.
        wednesday = this_week(2)
        week.scroll_to_minute(9 * 60)
        before = event_files(data)
        cx, cy = week.point(2, 10 * 60 + 36)
        dbg.must({"op": "click", "x": cx, "y": cy})
        dbg.until("the draft and its popover", lambda: popover_open(dbg))
        when = f"{wednesday.strftime('%A')} {wednesday.day} {wednesday.strftime('%B')}, 10:30 - 11:30"
        if not dbg.shows(when):
            raise Failure(f"the popover does not say {when!r}")
        log(f"click: the draft shows, and its popover says {when!r}")
        how = type_title(dbg, "Standup")
        name, event = save_and_read(dbg, data, before, "the clicked event")
        expect_event(event, name, "Standup", wednesday, "10:30", "11:30")
        dbg.until("the week to show Standup", lambda: dbg.shows("Standup"))
        log(f"click: typed ({how}), saved {name}: Standup, {wednesday}, 10:30 - 11:30")

        # 4. Drag Thursday 13:05 -> 14:50: 13:00 - 15:00.
        thursday = this_week(3)
        week.scroll_to_minute(12 * 60)
        before = event_files(data)
        dx, y0 = week.point(3, 13 * 60 + 5)
        _, y1 = week.point(3, 13 * 60 + 40)
        _, y2 = week.point(3, 14 * 60 + 50)
        dbg.must({"op": "mouse_down", "x": dx, "y": y0})
        dbg.must({"op": "mouse_move", "x": dx, "y": y1})
        dbg.must({"op": "mouse_move", "x": dx, "y": y2})
        dbg.must({"op": "mouse_up", "x": dx, "y": y2})
        dbg.until("the dragged draft and its popover", lambda: popover_open(dbg))
        when = f"{thursday.strftime('%A')} {thursday.day} {thursday.strftime('%B')}, 13:00 - 15:00"
        if not dbg.shows(when):
            raise Failure(f"the popover does not say {when!r}")
        type_title(dbg, "Review")
        name, event = save_and_read(dbg, data, before, "the dragged event")
        expect_event(event, name, "Review", thursday, "13:00", "15:00")
        log(f"drag: saved {name}: Review, {thursday}, 13:00 - 15:00")

        # 5. A click outside closes the popover and drops the draft.
        week.scroll_to_minute(15 * 60)
        before = event_files(data)
        fx, fy = week.point(4, 16 * 60)
        time.sleep(0.5)
        dbg.must({"op": "click", "x": fx, "y": fy})
        dbg.until("the Friday draft and its popover", lambda: popover_open(dbg))
        dbg.must({"op": "click", "text": "AzCalendar"})
        dbg.until("a click outside to close the popover and drop the draft", lambda: popover_gone(dbg))
        if event_files(data) != before:
            raise Failure("a click outside the popover wrote an event")
        log("outside: a click outside dropped the draft, nothing written")

        # 6. Cancel does the same.
        time.sleep(0.5)
        dbg.must({"op": "click", "x": fx, "y": fy})
        dbg.until("the Friday draft again", lambda: popover_open(dbg))
        press(dbg, "#draft-cancel")
        dbg.until("Cancel to close the popover and drop the draft", lambda: popover_gone(dbg))
        if event_files(data) != before:
            raise Failure("Cancel wrote an event")
        log("cancel: Cancel dropped the draft, nothing written")

        # 7. A click on an existing event opens no draft.
        week.scroll_to_minute(9 * 60)
        ex, ey = week.point(2, 10 * 60 + 50)
        time.sleep(0.5)
        dbg.must({"op": "click", "x": ex, "y": ey})
        time.sleep(1.0)
        if dbg.exists("#draft-title") or dbg.shows(DRAFT_TITLE):
            raise Failure("a click on the Standup event opened a draft")
        if event_files(data) != before:
            raise Failure("a click on an event wrote an event")
        log("existing: a click on Standup opened no draft")

        log("PASS: the week zooms around the pointer, scrolls, and makes events from a click or a drag")
        return True
    finally:
        if app.poll() is None:
            app.send_signal(signal.SIGTERM)
            try:
                app.wait(timeout=3)
            except subprocess.TimeoutExpired:
                app.kill()
        out.close()
        err.close()


def tail(path, lines=25):
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return "".join(f.readlines()[-lines:])
    except OSError:
        return "(no output)"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--bin")
    parser.add_argument("--port", type=int, default=8769)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--keep-logs", action="store_true")
    opts = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azcalendar-week-interactions-")
    passed = False
    try:
        passed = run(opts, logs)
    except Failure as e:
        log(f"FAIL: {e}")
        for name in ("azcalendar.out", "azcalendar.err"):
            print(f"\n----- {name} (tail) -----\n{tail(os.path.join(logs, name))}")
    finally:
        if passed and not opts.keep_logs:
            shutil.rmtree(logs, ignore_errors=True)
        else:
            log(f"logs kept in {logs}")
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
