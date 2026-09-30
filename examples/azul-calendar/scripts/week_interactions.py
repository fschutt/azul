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

Each stage runs even when one before it failed (a draft it left open is cancelled first), and
the run names every stage that failed; "existing" needs the "Standup" event "click" makes.

Usage (from the azul repository, after building AzCalendar and a libazul with the debug server):
  python3 examples/azul-calendar/scripts/week_interactions.py
      [--bin target/release/AzCalendar] [--port 8769] [--timeout 30] [--keep-logs]

--timeout is how long each wait may take, in seconds. Also read from the environment: AZCAL_BIN.
Logs and the data folder go to a temporary directory that is printed at the end (kept on
failure, or always with --keep-logs).
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

    def __init__(self, port, timeout):
        self.url = f"http://127.0.0.1:{port}/"
        self.timeout = timeout

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

    def frames(self, n=1):
        """Lets the app run `n` frames. The engine's timers (the wheel's scroll physics, the
        scroll glide) run in real time and only advance frame by frame: reading state in a
        loop does not move them, a `wait_frame` does."""
        for _ in range(n):
            self.must({"op": "wait_frame"})

    def until(self, what, check, every=0.1):
        """Polls `check` until it answers, a frame at a time, for at most the script's
        --timeout."""
        last = None
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            try:
                self.frames()
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

    def max_scroll(self):
        return max(0.0, 24.0 * self.hour_px - self.scroll["height"])

    def scroll_to_y(self, y):
        """Scrolls the week to `y` (held to its range), and waits until it is there."""
        y = min(max(y, 0.0), self.max_scroll())
        self.dbg.must({"op": "scroll_node_to", "selector": "#week-scroll", "x": 0, "y": y})

        def there():
            self.refresh()
            return abs(self.scroll_y - y) < 1.0 or None

        self.dbg.until(f"the week to scroll to {y:.0f} px", there)

    def scroll_to_minute(self, minute):
        """Puts `minute` at the top of the week (or scrolls as far as the day goes), and waits
        until the week is there."""
        self.scroll_to_y(minute / 60.0 * self.hour_px)

    def point(self, day, minute):
        """The window point over `day` (0 = Monday) at `minute`, as the week is scrolled now."""
        self.refresh()
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
    """The draft (#draft) is in the week, with its popover's title field (#draft-title)."""
    return dbg.exists("#draft") and dbg.exists("#draft-title")


def popover_gone(dbg):
    return not dbg.exists("#draft") and not dbg.exists("#draft-title")


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
    return files[0], read_event(data, files[0])


def expect_event(event, name, title, date, start, end):
    got = (event.get("title"), event.get("date"), event.get("start"), event.get("end"))
    want = (title, date.isoformat(), start, end)
    if got != want:
        raise Failure(f"{name} holds {got}, expected {want}: {json.dumps(event)}")
    if f"{event.get('id')}.json" != name:
        raise Failure(f"{name} is not named by its event id {event.get('id')}")


def stage_zoom(dbg, week, data, ctx):
    """A pinch over Wednesday keeps the time under the pointer."""
    week.scroll_to_minute(8 * 60)
    before_px = week.hour_px
    x, _ = week.point(2, 8 * 60 + 30)
    y = week.scroll["y"] + 100.0
    minute_before = (week.scroll_y + 100.0) * 60.0 / before_px
    # The pointer first: the `pinch` op runs its own event pass at once, and a pinch goes to
    # the node under the pointer (macOS's magnify centre IS the pointer).
    dbg.must({"op": "mouse_move", "x": x, "y": y})
    dbg.frames()
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
    return (
        f"{before_px:.1f} -> {week.hour_px:.1f} px an hour, minute {minute_before:.1f} stayed "
        f"under the pointer ({minute_after:.1f})"
    )


def settle(dbg, week):
    """Waits until the week stops moving (a wheel glides for a while; a `scroll_node_to` made
    during the glide would be overtaken by it)."""
    last = [None]

    def still():
        week.refresh()
        now = week.scroll_y
        done = last[0] is not None and abs(now - last[0]) < 0.5
        last[0] = now
        return done or None

    dbg.until("the week to stop scrolling", still)


def popover_labels(dbg):
    """What the popover says the draft is: texts like "Wednesday 30 September, 10:30 - 11:30"."""
    shape = re.compile(r"^[A-Z][a-z]+day \d{1,2} [A-Z][a-z]+, \d\d:\d\d - \d\d:\d\d$")
    return sorted({t for t in dbg.texts() if shape.match(t)})


def expect_label(dbg, when):
    if not dbg.shows(when):
        raise Failure(f"the popover does not say {when!r}; it says {popover_labels(dbg)}")


def stage_wheel(dbg, week, data, ctx):
    """A plain wheel scrolls the week and does not zoom."""
    # From the middle of the range, so the wheel has room either way. A negative delta_y
    # scrolls down (the engine's traditional direction), by up to 120 px.
    week.scroll_to_y(week.max_scroll() / 2.0)
    column = week.column(2)
    x = column["x"] + column["width"] / 2.0
    y = week.scroll["y"] + week.scroll["height"] / 2.0
    start_y, start_px = week.scroll_y, week.hour_px
    dbg.must({"op": "wheel", "x": x, "y": y, "delta_x": 0, "delta_y": -120})

    def scrolled():
        week.refresh()
        return abs(week.scroll_y - start_y) > 20 or None

    dbg.until("a plain wheel to scroll the week", scrolled)
    settle(dbg, week)
    if abs(week.hour_px - start_px) > 0.05:
        raise Failure(f"a plain wheel zoomed the week ({start_px} -> {week.hour_px} px an hour)")
    return f"scrolled {start_y:.0f} -> {week.scroll_y:.0f} px, still {week.hour_px:.1f} px an hour"


def stage_click(dbg, week, data, ctx):
    """Click Wednesday 10:36: a draft 10:30 - 11:30 with its popover; a title, Save."""
    wednesday = this_week(2)
    week.scroll_to_minute(9 * 60)
    before = event_files(data)
    x, y = week.point(2, 10 * 60 + 36)
    dbg.must({"op": "click", "x": x, "y": y})
    dbg.until("the draft and its popover", lambda: popover_open(dbg))
    when = f"{wednesday.strftime('%A')} {wednesday.day} {wednesday.strftime('%B')}, 10:30 - 11:30"
    expect_label(dbg, when)
    if not dbg.shows(DRAFT_TITLE):
        raise Failure(f"the draft does not say {DRAFT_TITLE!r}")
    how = type_title(dbg, "Standup")
    name, event = save_and_read(dbg, data, before, "the clicked event")
    expect_event(event, name, "Standup", wednesday, "10:30", "11:30")
    dbg.until("the draft and its popover to close after Save", lambda: popover_gone(dbg))
    dbg.until("the week to show Standup", lambda: dbg.shows("Standup"))
    ctx["standup"] = True
    return f"the popover said {when!r}; typed ({how}), saved {name}: Standup, 10:30 - 11:30"


def stage_drag(dbg, week, data, ctx):
    """Drag Thursday 13:05 -> 14:50: 13:00 - 15:00."""
    thursday = this_week(3)
    week.scroll_to_minute(12 * 60)
    before = event_files(data)
    x, y0 = week.point(3, 13 * 60 + 5)
    _, y1 = week.point(3, 13 * 60 + 40)
    _, y2 = week.point(3, 14 * 60 + 50)
    dbg.must({"op": "mouse_down", "x": x, "y": y0})
    dbg.must({"op": "mouse_move", "x": x, "y": y1})
    dbg.must({"op": "mouse_move", "x": x, "y": y2})
    dbg.must({"op": "mouse_up", "x": x, "y": y2})
    dbg.until("the dragged draft and its popover", lambda: popover_open(dbg))
    when = f"{thursday.strftime('%A')} {thursday.day} {thursday.strftime('%B')}, 13:00 - 15:00"
    expect_label(dbg, when)
    type_title(dbg, "Review")
    name, event = save_and_read(dbg, data, before, "the dragged event")
    expect_event(event, name, "Review", thursday, "13:00", "15:00")
    dbg.until("the draft and its popover to close after Save", lambda: popover_gone(dbg))
    return f"saved {name}: Review, {thursday}, 13:00 - 15:00"


def friday_draft(dbg, week):
    week.scroll_to_minute(15 * 60)
    x, y = week.point(4, 16 * 60)
    # A press right after the popover closed by a click outside is that click (the app ignores
    # it for a moment); wait that moment out.
    time.sleep(0.5)
    dbg.must({"op": "click", "x": x, "y": y})
    dbg.until("the Friday draft and its popover", lambda: popover_open(dbg))


def stage_outside(dbg, week, data, ctx):
    """A click outside closes the popover and drops the draft."""
    before = event_files(data)
    friday_draft(dbg, week)
    dbg.must({"op": "click", "text": "AzCalendar"})
    dbg.until("a click outside to close the popover and drop the draft", lambda: popover_gone(dbg))
    if event_files(data) != before:
        raise Failure("a click outside the popover wrote an event")
    return "a click on the toolbar dropped the draft, nothing written"


def stage_cancel(dbg, week, data, ctx):
    """Cancel closes the popover and drops the draft."""
    before = event_files(data)
    friday_draft(dbg, week)
    press(dbg, "#draft-cancel")
    dbg.until("Cancel to close the popover and drop the draft", lambda: popover_gone(dbg))
    if event_files(data) != before:
        raise Failure("Cancel wrote an event")
    return "Cancel dropped the draft, nothing written"


def stage_existing(dbg, week, data, ctx):
    """A click on an existing event opens no draft."""
    if not ctx.get("standup"):
        raise Failure("skipped: the click stage made no Standup event to click")
    before = event_files(data)
    week.scroll_to_minute(9 * 60)
    x, y = week.point(2, 10 * 60 + 50)
    time.sleep(0.5)
    dbg.must({"op": "click", "x": x, "y": y})
    time.sleep(1.0)
    if dbg.exists("#draft") or dbg.exists("#draft-title"):
        raise Failure("a click on the Standup event opened a draft")
    if event_files(data) != before:
        raise Failure("a click on an event wrote an event")
    return "a click on Standup opened no draft"


STAGES = [
    ("zoom", stage_zoom),
    ("wheel", stage_wheel),
    ("click", stage_click),
    ("drag", stage_drag),
    ("outside", stage_outside),
    ("cancel", stage_cancel),
    ("existing", stage_existing),
]


def clean_up(dbg):
    """After a failed stage: drop a draft it left open, so the next stage starts clean."""
    try:
        if dbg.exists("#draft-cancel"):
            press(dbg, "#draft-cancel")
            dbg.until("a left-over draft to close", lambda: popover_gone(dbg))
    except (Failure, OSError, urllib.error.URLError, ValueError):
        pass


def run(opts, logs):
    binary = find_binary(opts.bin)
    data = os.path.join(logs, "data")
    os.makedirs(data)
    log(f"AzCalendar: {binary}")
    log(f"logs and data: {logs}")
    env = dict(os.environ)
    env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(opts.port), "AZCAL_DATA": data})
    # This test makes no meeting links, so AzCalendar's default meeting server is never reached.
    env.pop("AZMEET_WORKER", None)
    out = open(os.path.join(logs, "azcalendar.out"), "w")
    err = open(os.path.join(logs, "azcalendar.err"), "w")
    app = subprocess.Popen([binary], env=env, stdout=out, stderr=err, stdin=subprocess.DEVNULL)
    failed = []
    try:
        dbg = Debug(opts.port, opts.timeout)
        dbg.until("the week view", lambda: dbg.shows("This week"))
        if event_files(data):
            raise Failure("the data folder is not empty at the start")
        week = Week(dbg)
        ctx = {}
        chosen = [
            (name, stage)
            for name, stage in STAGES
            if (not opts.only or name in opts.only.split(","))
            and name not in (opts.skip or "").split(",")
        ]
        for name, stage in chosen:
            try:
                log(f"{name}: {stage(dbg, week, data, ctx)}")
            except Failure as e:
                failed.append(name)
                log(f"{name}: FAIL: {e}")
                clean_up(dbg)
        if failed:
            raise Failure(f"{len(failed)} of {len(chosen)} stages failed: {', '.join(failed)}")
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
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--keep-logs", action="store_true")
    parser.add_argument("--only", help="comma-separated stages to run (default: all)")
    parser.add_argument("--skip", help="comma-separated stages to leave out")
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
