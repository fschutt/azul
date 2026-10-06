#!/usr/bin/env python3
"""AzCalendar on the Office scaffold, headless, through its debug server.

Starts AzCalendar (AZ_BACKEND=headless, AZ_DEBUG=<port>) with an empty data folder (AZCAL_DATA) on
the Week view, and walks its main flows. Every op is addressed to its window by `window_id`: the
main window is `azcalendar`, the event editor `azcalendar-editor`. The app's DOM ids carry its
prefix `__azcal_` (examples/azul-calendar/src/ids.rs): `#view-week` below is
`#__azcal_view-week` (`wi.sel("view-week")`); an older build's bare names are detected at the
start. The shell's own ids (`#shell-backstage`, `#shell-ribbon`) have no app prefix.

Stages (each runs even when one before it failed; `--only` / `--skip` pick them):

  views     Ctrl/Cmd+Alt+1..6 switch Day, Work Week, Week, Month, Schedule View and List: each
            view's root (#view-<name>) is there, the hours views have their day columns (#day-N:
            1 / 5 / 7), the month its grid (#month-<first day of the month>), and stdout says
            `AZCAL_VIEW <name> <first> <last>`. The ribbon's "Month" and "Schedule View" do the
            same; FILE opens the backstage (#shell-backstage) and Escape closes it again.
  import    writes an .ics file (a weekly event with an EXDATE, an all-day event over two days,
            an event at a time in Europe/Berlin through the file's VTIMEZONE), FILE > Open &
            Export: types its path into #import-path, presses Import (#import-run): stdout says
            `AZCAL_IMPORTED 3 <file>`, the event files hold the rule, the exception and the UID,
            and the week shows the weekly one (#event-<id>-<yyyymmdd>).
  editor    Ctrl/Cmd+N opens the event editor window (`AZCAL_EDITOR open`); in that window: a
            title (#editor-title), "Weekly" on the repeat row, Save & Close (#editor-save): a new
            event file with `"repeat": "FREQ=WEEKLY;BYDAY=<its weekday>"`, `AZCAL_EDITOR closed`.
            BLOCKED (not failed) when the debug server does not reach a window made at runtime -
            the headless backend does not run child windows yet (MAIL2's engine change).
  close     Ctrl/Cmd+N, a title typed, the editor window's `close` op: the close is held and
            `AZCAL_EDITOR asking`, the question shows; "Don't Save" closes it and writes no
            event file. An unedited editor closes at once, without asking.
  repeat    the weekly events (the imported one, and the editor's when it was made) are on the
            next week too (Forward, #view-next), not on the week their exception names, and again
            the week after.
  occurrence  Enter on the editor's weekly event a week on: the editor opens on that day with
            "This occurrence" (#editor-scope); Save writes a one-off event on that day and the
            series skips it (`except`).
  contrast  under flat and flora, light and dark: for the Week, Month and List views and the
            backstage, every piece of text the display list paints (button labels among them) is
            read against the rectangles painted under it; under 2:1 is a finding (the threshold of
            the widget lint `widgets::theme_contrast`). Screenshots of each go to --out.

Usage (from the azul repository, with AzCalendar built against a libazul with the debug server):
  python3 scripts/azcalendar_e2e.py [--bin target/release/AzCalendar] [--port 8781]
      [--timeout 30] [--only views,import] [--skip contrast] [--out DIR] [--keep-logs]

Run it through the capped runner (one app at a time, killed at 1.5 GB):
  <scratchpad>/run_capped.sh --cap-mb 1500 --seconds 300 --log /tmp/azcal-e2e.log -- \
      python3 scripts/azcalendar_e2e.py --bin target/release/AzCalendar
"""

import argparse
import base64
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

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, ".."))
sys.path.insert(0, os.path.join(REPO, "examples", "azul-calendar", "scripts"))
sys.dont_write_bytecode = True
import week_interactions as wi  # noqa: E402  (the same debug-server client and week helpers)

MAIN = "azcalendar"
EDITOR = "azcalendar-editor"
STANDUP_UID = "e2e-standup@example.org"
EDITOR_TITLE = "Weekly review"


class Failure(wi.Failure):
    pass


class Blocked(Exception):
    """The stage cannot run here (and says why): not a failure of AzCalendar."""


def log(line):
    print(f"[azcalendar-e2e] {line}", flush=True)


class Window(wi.Debug):
    """The debug server, every op addressed to one window."""

    def __init__(self, port, timeout, window):
        super().__init__(port, timeout)
        self.window = window

    def op(self, body):
        return super().op(dict(body, window_id=self.window))

    def key(self, key, ctrl=False, alt=False, shift=False, meta=False, primary=False):
        # `primary`: the platform's shortcut modifier, as the apps read it
        # (KeyModifiers::primary_down) - Cmd on macOS, Ctrl elsewhere.
        if primary:
            if sys.platform == "darwin":
                meta = True
            else:
                ctrl = True
        mods = {"shift": shift, "ctrl": ctrl, "alt": alt, "meta": meta}
        self.must({"op": "key_down", "key": key, "modifiers": mods})
        # A tap: the key and its modifiers come up together (an op's modifiers are the whole
        # modifier state; with the chord's they stayed held for every later click).
        released = {"shift": False, "ctrl": False, "alt": False, "meta": False}
        self.must({"op": "key_up", "key": key, "modifiers": released})
        self.frames(2)

    def click(self, selector=None, text=None):
        body = {"op": "click"}
        if selector:
            body["selector"] = selector
        if text:
            body["text"] = text
        self.must(body)
        self.frames(2)

    def type_into(self, selector, text):
        self.must({"op": "focus_node", "selector": selector})
        self.frames(1)
        self.must({"op": "text_input", "text": text})
        self.frames(2)

    def wait_for(self, selector):
        self.until(f"{selector} in {self.window}", lambda: self.exists(selector))

    def wait_gone(self, selector):
        self.until(f"{selector} to go from {self.window}", lambda: not self.exists(selector))

    def screenshot(self, path):
        value = self.value({"op": "take_screenshot"})
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure(f"take_screenshot returned no PNG: {json.dumps(value)[:200]}")
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))


class App:
    """AzCalendar, its stdout and its data folder."""

    def __init__(self, binary, port, timeout, logs):
        self.data = os.path.join(logs, "data")
        os.makedirs(self.data)
        self.out = os.path.join(logs, "azcalendar.out")
        self.err = os.path.join(logs, "azcalendar.err")
        env = dict(os.environ)
        env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(port), "AZCAL_DATA": self.data})
        # No meeting links are made here: the default meeting server is never asked.
        env.pop("AZMEET_WORKER", None)
        self.process = subprocess.Popen(
            [binary, "--screen", "week"],
            env=env,
            stdout=open(self.out, "w"),
            stderr=open(self.err, "w"),
            stdin=subprocess.DEVNULL,
        )
        self.main = Window(port, timeout, MAIN)
        self.port = port
        self.timeout = timeout

    def printed(self, key):
        """Every `<key> <rest>` line on stdout, the rest of each."""
        try:
            with open(self.out, encoding="utf-8", errors="replace") as f:
                return re.findall(rf"^{re.escape(key)} (.*)$", f.read(), re.M)
        except OSError:
            return []

    def stop(self):
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(5)
            except subprocess.TimeoutExpired:
                self.process.kill()


def monday():
    today = datetime.date.today()
    return today - datetime.timedelta(days=today.weekday())


def ymd(day):
    return day.strftime("%Y%m%d")


def event_with(app, key, value):
    """The (file name, JSON) of the event whose `key` is `value`."""
    for name in wi.event_files(app.data):
        event = wi.read_event(app.data, name)
        if event.get(key) == value:
            return name, event
    return None


# ==== views ====

VIEWS = [
    ("1", "day", 1),
    ("2", "work-week", 5),
    ("3", "week", 7),
    ("4", "month", None),
    ("5", "schedule", None),
    ("6", "agenda", None),
]


def stage_views(app, ctx):
    w = app.main
    for key, name, columns in VIEWS:
        seen = len(app.printed("AZCAL_VIEW"))
        w.key(key, primary=True, alt=True)
        w.wait_for(wi.sel(f"view-{name}"))
        line = w.until(
            f"AZCAL_VIEW {name}",
            lambda: next((l for l in app.printed("AZCAL_VIEW")[seen:] if l.startswith(name + " ")), None),
        )
        first = datetime.date.fromisoformat(line.split()[1])
        if columns is not None:
            if not w.exists(wi.sel(f"day-{columns - 1}")) or w.exists(wi.sel(f"day-{columns}")):
                raise Failure(f"the {name} view does not have {columns} day column(s)")
        if name == "month" and not w.exists(wi.sel(f"month-{ymd(first)}")):
            raise Failure(f"the month view has no cell #month-{ymd(first)} for its first day")
        if name == "schedule":
            w.wait_for(wi.sel("schedule"))
    # The ribbon does the same.
    w.key("3", primary=True, alt=True)
    w.click(text="Month")
    w.wait_for(wi.sel("view-month"))
    w.click(text="Schedule View")
    w.wait_for(wi.sel("view-schedule"))
    w.key("3", primary=True, alt=True)
    w.wait_for(wi.sel("view-week"))
    # The view is kept for the next start: the file thread writes it into settings.txt.
    settings_file = os.path.join(app.data, "settings.txt")

    def kept_view():
        try:
            with open(settings_file, encoding="utf-8") as f:
                lines = f.read().splitlines()
        except FileNotFoundError:
            return None
        views = [l for l in lines if l.startswith("view=")]
        return views[-1] if views else None

    w.until("view=week in settings.txt", lambda: kept_view() == "view=week")
    # FILE opens the backstage over the window; Escape closes it.
    w.click(text="FILE")
    w.wait_for("#shell-backstage")
    w.key("escape")
    w.wait_gone("#shell-backstage")
    w.wait_for("#shell-ribbon")
    return "six views by key and by ribbon, FILE and back"


# ==== import ====

def ics_text():
    """A calendar file: a weekly event on this week's Wednesday with an exception two weeks on, an
    all-day event over Friday and Saturday, and a call at a Europe/Berlin time."""
    wed = monday() + datetime.timedelta(days=2)
    fri = monday() + datetime.timedelta(days=4)
    sun = monday() + datetime.timedelta(days=6)
    thu = monday() + datetime.timedelta(days=3)
    skip = wed + datetime.timedelta(days=14)
    lines = [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//azul//azcalendar-e2e//EN",
        "X-WR-CALNAME:E2E",
        "BEGIN:VTIMEZONE",
        "TZID:Europe/Berlin",
        "BEGIN:DAYLIGHT",
        "TZOFFSETFROM:+0100",
        "TZOFFSETTO:+0200",
        "DTSTART:19700329T020000",
        "RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU",
        "END:DAYLIGHT",
        "BEGIN:STANDARD",
        "TZOFFSETFROM:+0200",
        "TZOFFSETTO:+0100",
        "DTSTART:19701025T030000",
        "RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU",
        "END:STANDARD",
        "END:VTIMEZONE",
        "BEGIN:VEVENT",
        f"UID:{STANDUP_UID}",
        "SUMMARY:Imported standup",
        f"DTSTART:{ymd(wed)}T093000",
        f"DTEND:{ymd(wed)}T100000",
        "RRULE:FREQ=WEEKLY;BYDAY=" + ["MO", "TU", "WE", "TH", "FR", "SA", "SU"][wed.weekday()],
        f"EXDATE:{ymd(skip)}T093000",
        "LOCATION:Room 4\\, second floor",
        "END:VEVENT",
        "BEGIN:VEVENT",
        "UID:e2e-offsite@example.org",
        "SUMMARY:Offsite",
        f"DTSTART;VALUE=DATE:{ymd(fri)}",
        f"DTEND;VALUE=DATE:{ymd(sun)}",
        "END:VEVENT",
        "BEGIN:VEVENT",
        "UID:e2e-berlin@example.org",
        "SUMMARY:Call with Berlin",
        f"DTSTART;TZID=Europe/Berlin:{ymd(thu)}T150000",
        f"DTEND;TZID=Europe/Berlin:{ymd(thu)}T153000",
        "END:VEVENT",
        "END:VCALENDAR",
    ]
    return "\r\n".join(lines) + "\r\n", wed, skip


def stage_import(app, ctx):
    w = app.main
    text, wed, skip = ics_text()
    path = os.path.join(ctx["logs"], "e2e.ics")
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(text)
    w.click(text="FILE")
    w.wait_for("#shell-backstage")
    w.click(text="Open & Export")
    w.wait_for(wi.sel("import-path"))
    w.type_into(wi.sel("import-path"), path)
    w.click(selector=wi.sel("import-run"))
    line = w.until("AZCAL_IMPORTED", lambda: next(iter(app.printed("AZCAL_IMPORTED")), None))
    if not line.startswith("3 "):
        raise Failure(f"AZCAL_IMPORTED {line}: expected 3 events")
    found = w.until("the imported standup's file", lambda: event_with(app, "uid", STANDUP_UID))
    name, event = found
    if event.get("version") != 3:
        raise Failure(f"{name}: an imported event is version 3 (it has a uid): {event}")
    if not str(event.get("repeat", "")).startswith("FREQ=WEEKLY"):
        raise Failure(f"{name}: no weekly rule: {event}")
    if event.get("except") != [skip.isoformat()]:
        raise Failure(f"{name}: the exception is not {skip}: {event}")
    if event.get("location") != "Room 4, second floor":
        raise Failure(f"{name}: the location was not unescaped: {event}")
    offsite = event_with(app, "uid", "e2e-offsite@example.org")
    if not offsite or not offsite[1].get("all_day") or offsite[1].get("last_day") is None:
        raise Failure(f"the all-day event over two days was not imported as one: {offsite}")
    if not event_with(app, "uid", "e2e-berlin@example.org"):
        raise Failure("the Europe/Berlin call was not imported")
    ctx["standup"] = (event["id"], wed, skip)
    # The import closes the backstage and shows the first imported day.
    w.wait_gone("#shell-backstage")
    w.key("3", primary=True, alt=True)
    w.wait_for(wi.sel(f"event-{event['id']}-{ymd(wed)}"))
    return f"3 events from {os.path.basename(path)}, the weekly one shown on {wed}"


# ==== editor ====

def reach_editor(app):
    """The editor window's debug client, once the debug server answers for that window."""
    ed = Window(app.port, app.timeout, EDITOR)
    last = None
    for _ in range(10):
        answer = ed.op({"op": "get_node_layout", "selector": wi.sel("editor-title")})
        if answer.get("status") != "error":
            return ed
        last = answer.get("message")
        app.main.frames(2)
        time.sleep(0.3)
    raise Blocked(
        "the debug server does not reach the editor window "
        f"(window_id {EDITOR}): {last} - the headless backend does not run windows made at "
        "runtime yet (MAIL2's engine change)"
    )


def stage_editor(app, ctx):
    w = app.main
    opened = len(app.printed("AZCAL_EDITOR"))
    before = set(wi.event_files(app.data))
    w.key("n", primary=True)
    w.until(
        "AZCAL_EDITOR open",
        lambda: "open" in app.printed("AZCAL_EDITOR")[opened:],
    )
    ed = reach_editor(app)
    ed.type_into(wi.sel("editor-title"), EDITOR_TITLE)
    # The repeat row is the DateRepeatPicker (#editor-repeat): "Weekly" on its frequency row
    # brings its "Ends" row and the weekday toggles.
    ed.wait_for(wi.sel("editor-repeat"))
    ed.click(text="Weekly")
    ed.until("the date repeat picker's weekly rows", lambda: ed.shows("Ends"))
    # Save & Close closes the editor window: no frames are asked of it after
    # the click (`click` waits two frames on the window it clicked, and a
    # closed window answers "No window has the id"). The main window waits.
    ed.must({"op": "click", "selector": wi.sel("editor-save")})
    w.until(
        "AZCAL_EDITOR closed",
        lambda: "closed" in app.printed("AZCAL_EDITOR")[opened:],
    )
    # Save & Close's own close_window goes through CloseRequested too: a saved form closes
    # without asking.
    if "asking" in app.printed("AZCAL_EDITOR")[opened:]:
        raise Failure("Save & Close asked 'save changes?' after saving")
    new = w.until(
        "the editor's event file",
        lambda: [n for n in wi.event_files(app.data) if n not in before] or None,
    )
    event = wi.read_event(app.data, new[0])
    if event.get("title") != EDITOR_TITLE:
        raise Failure(f"{new[0]}: not the editor's event: {event}")
    date = datetime.date.fromisoformat(event["date"])
    weekday = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"][date.weekday()]
    if event.get("repeat") != f"FREQ=WEEKLY;BYDAY={weekday}":
        raise Failure(f"{new[0]}: the repeat is not weekly on its weekday: {event}")
    ctx["editor_event"] = (event["id"], date)
    return f"{EDITOR_TITLE!r} saved from the editor window, weekly on {weekday}"


# ==== close ====

def reach_modal(app, known):
    """The debug client of the window a Modal opened (`azul-transient`, `azul-transient-2`,
    ...): the one window not in `known`."""

    def other():
        windows = (app.main.value({"op": "list_windows"}) or {}).get("windows") or []
        ids = [w.get("window_id") for w in windows if w.get("window_id") not in known]
        return ids[0] if ids else None

    return Window(app.port, app.timeout, app.main.until("the question's window", other))


def stage_close(app, ctx):
    """An edited appointment is not lost to the window's close: the close is held and the window
    asks "save changes?"; Don't Save closes it and writes nothing. An unedited one closes at
    once."""
    w = app.main
    before = set(wi.event_files(app.data))
    # Edited: held, asked, discarded.
    opened = len(app.printed("AZCAL_EDITOR"))
    w.key("n", primary=True)
    w.until("AZCAL_EDITOR open", lambda: "open" in app.printed("AZCAL_EDITOR")[opened:])
    ed = reach_editor(app)
    ed.type_into(wi.sel("editor-title"), "Not to be kept")
    ed.must({"op": "close"})
    ed.frames(3)
    w.until("AZCAL_EDITOR asking", lambda: "asking" in app.printed("AZCAL_EDITOR")[opened:])
    if "closed" in app.printed("AZCAL_EDITOR")[opened:]:
        raise Failure("the edited appointment's window closed without asking")
    ed.until("the question", lambda: ed.shows("Don't Save"))
    # The question is a Modal: a window of its own over the editor (the CloseGuard's
    # MessageBox in a <transient-window>), where the user's click lands - its nodes in the
    # editor's DOM are not laid out there. The answer closes it and the editor: no frames
    # are asked of it after the click.
    question = reach_modal(app, (MAIN, EDITOR))
    question.until("Don't Save in the question's window", lambda: question.shows("Don't Save"))
    question.must({"op": "click", "text": "Don't Save"})
    w.until("AZCAL_EDITOR closed", lambda: "closed" in app.printed("AZCAL_EDITOR")[opened:])
    if set(wi.event_files(app.data)) != before:
        raise Failure("Don't Save wrote an event file")
    # Unedited: closes at once.
    opened = len(app.printed("AZCAL_EDITOR"))
    w.key("n", primary=True)
    w.until("AZCAL_EDITOR open", lambda: "open" in app.printed("AZCAL_EDITOR")[opened:])
    ed = reach_editor(app)
    ed.must({"op": "close"})
    w.until("AZCAL_EDITOR closed", lambda: "closed" in app.printed("AZCAL_EDITOR")[opened:])
    if "asking" in app.printed("AZCAL_EDITOR")[opened:]:
        raise Failure("an unedited appointment asked before it closed")
    return "an edited appointment asks before it closes (Don't Save writes nothing); an unedited one closes"


# ==== repeat ====

def show_week_of(w, day):
    """Puts the Week view on the week of `day` (Today, then Forward / Back)."""
    w.key("3", primary=True, alt=True)
    w.click(selector=wi.sel("view-today"))
    weeks = (day - monday()).days // 7
    for _ in range(abs(weeks)):
        w.click(selector=wi.sel("view-next") if weeks > 0 else wi.sel("view-prev"))


def stage_repeat(app, ctx):
    w = app.main
    checked = []
    if "standup" in ctx:
        sid, wed, skip = ctx["standup"]
        show_week_of(w, wed)
        w.wait_for(wi.sel(f"event-{sid}-{ymd(wed)}"))
        w.click(selector=wi.sel("view-next"))
        w.wait_for(wi.sel(f"event-{sid}-{ymd(wed + datetime.timedelta(days=7))}"))
        w.click(selector=wi.sel("view-next"))
        w.frames(3)
        if w.exists(wi.sel(f"event-{sid}-{ymd(skip)}")):
            raise Failure(f"the imported standup shows on {skip}, the day its EXDATE skips")
        w.click(selector=wi.sel("view-next"))
        w.wait_for(wi.sel(f"event-{sid}-{ymd(wed + datetime.timedelta(days=21))}"))
        checked.append("the imported weekly event (and its exception)")
    if "editor_event" in ctx:
        eid, date = ctx["editor_event"]
        show_week_of(w, date)
        w.wait_for(wi.sel(f"event-{eid}-{ymd(date)}"))
        w.click(selector=wi.sel("view-next"))
        w.wait_for(wi.sel(f"event-{eid}-{ymd(date + datetime.timedelta(days=7))}"))
        checked.append("the editor's weekly event")
    if not checked:
        raise Failure("no weekly event to check: the import and the editor stages made none")
    return "on the next week: " + ", ".join(checked)


# ==== occurrence ====

def stage_occurrence(app, ctx):
    """One occurrence of the editor's weekly event, edited alone: Enter on its block a week on
    opens the editor on that day with "This occurrence" chosen (#editor-scope); a new title
    and Save write a new event file on that day without a repeat, and the series' file skips
    the day (`except`)."""
    if "editor_event" not in ctx:
        raise Blocked("the editor stage made no weekly event to take an occurrence of")
    w = app.main
    eid, date = ctx["editor_event"]
    day = date + datetime.timedelta(days=7)
    show_week_of(w, day)
    block = wi.sel(f"event-{eid}-{ymd(day)}")
    w.wait_for(block)
    opened = len(app.printed("AZCAL_EDITOR"))
    before = set(wi.event_files(app.data))
    w.must({"op": "focus_node", "selector": block})
    w.frames(1)
    w.key("return")
    w.until("AZCAL_EDITOR open", lambda: "open" in app.printed("AZCAL_EDITOR")[opened:])
    ed = reach_editor(app)
    ed.wait_for(wi.sel("editor-scope"))
    ed.type_into(wi.sel("editor-title"), " (moved)")
    # Save & Close closes the editor window: no frames are asked of it (see stage_editor).
    ed.must({"op": "click", "selector": wi.sel("editor-save")})
    w.until("AZCAL_EDITOR closed", lambda: "closed" in app.printed("AZCAL_EDITOR")[opened:])
    new = w.until(
        "the occurrence's own event file",
        lambda: [n for n in wi.event_files(app.data) if n not in before] or None,
    )
    one = wi.read_event(app.data, new[0])
    if one.get("date") != day.isoformat() or one.get("repeat"):
        raise Failure(f"{new[0]}: not a one-off event on {day}: {one}")
    series = wi.read_event(app.data, f"{eid}.json")
    if day.isoformat() not in (series.get("except") or []):
        raise Failure(f"the series does not skip {day}: {series}")
    return f"the occurrence of {day} became {new[0]}; the series skips that day"


# ==== contrast ====

def color(text):
    """`#rrggbbaa` / `#rrggbb` as (r, g, b, a) with a in 0..1."""
    h = text.lstrip("#")
    r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    a = int(h[6:8], 16) / 255.0 if len(h) >= 8 else 1.0
    return r, g, b, a


def over(top, base):
    r, g, b, a = top
    return tuple(c * a + d * (1.0 - a) for c, d in zip((r, g, b), base))


def luminance(rgb):
    def lin(v):
        v /= 255.0
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4

    r, g, b = rgb
    return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)


def ratio(a, b):
    la, lb = luminance(a), luminance(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


FILLS = ("rect", "linear_gradient", "radial_gradient", "conic_gradient")


def contrast_findings(items, base):
    """Text items whose ink reads under 2:1 against the rectangles painted under their centre
    (at the same scroll depth, in paint order) over the window's ground `base`."""
    rects = []
    found = []
    for it in items:
        kind = it.get("type")
        if not it.get("color") or it.get("width") is None or it.get("height") is None:
            continue
        # A gradient (flora's title bar, its active ribbon tab) is a fill too: the debug server
        # lists it with its bounds and the mean of its stops.
        if kind in FILLS:
            rects.append(it)
            continue
        if kind not in ("text", "text_layout"):
            continue
        ink = color(it["color"])
        if ink[3] == 0 or it["width"] <= 0 or it["height"] <= 0:
            continue
        cx = it["x"] + it["width"] / 2.0
        cy = it["y"] + it["height"] / 2.0
        bg = base
        for r in rects:
            if r.get("scroll_depth") != it.get("scroll_depth"):
                continue
            if r["x"] <= cx <= r["x"] + r["width"] and r["y"] <= cy <= r["y"] + r["height"]:
                bg = over(color(r["color"]), bg)
        seen = over(ink, bg)
        q = ratio(seen, bg)
        if q < 2.0:
            found.append(
                f"item {it.get('index')} at ({it['x']:.0f}, {it['y']:.0f}) {it['width']:.0f}x"
                f"{it['height']:.0f}: ink {it['color']} on {tuple(round(c) for c in bg)} = {q:.2f}:1"
            )
    return found


def stage_contrast(app, ctx):
    w = app.main
    out = ctx["out"]
    os.makedirs(out, exist_ok=True)
    findings = []
    shots = 0
    for theme in ("flat", "flora"):
        for mode in ("light", "dark"):
            w.must({"op": "set_theme", "theme": theme})
            w.must({"op": "set_mode", "mode": mode})
            w.frames(4)
            base = (255.0, 255.0, 255.0) if mode == "light" else (30.0, 30.0, 30.0)
            screens = [("week", "3"), ("month", "4"), ("agenda", "6")]
            for name, key in screens:
                w.key(key, primary=True, alt=True)
                w.wait_for(wi.sel(f"view-{name}"))
                w.frames(3)
                items = (w.value({"op": "get_display_list"}) or {}).get("items", [])
                for f in contrast_findings(items, base):
                    findings.append(f"{theme}/{mode}/{name}: {f}")
                w.screenshot(os.path.join(out, f"{theme}-{mode}-{name}.png"))
                shots += 1
            w.click(text="FILE")
            w.wait_for("#shell-backstage")
            w.click(text="Open & Export")
            w.wait_for(wi.sel("import-path"))
            w.frames(3)
            items = (w.value({"op": "get_display_list"}) or {}).get("items", [])
            for f in contrast_findings(items, base):
                findings.append(f"{theme}/{mode}/backstage: {f}")
            w.screenshot(os.path.join(out, f"{theme}-{mode}-backstage.png"))
            shots += 1
            w.key("escape")
            w.wait_gone("#shell-backstage")
    w.must({"op": "set_theme", "theme": "flat"})
    w.must({"op": "set_mode", "mode": "light"})
    if findings:
        raise Failure(
            f"{len(findings)} text(s) under 2:1 (screenshots in {out}):\n  " + "\n  ".join(findings[:40])
        )
    return f"every text reads at 2:1 or better in flat / flora x light / dark; {shots} screenshots in {out}"


STAGES = [
    ("views", stage_views),
    ("import", stage_import),
    ("editor", stage_editor),
    ("close", stage_close),
    ("repeat", stage_repeat),
    ("occurrence", stage_occurrence),
    ("contrast", stage_contrast),
]


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--bin")
    p.add_argument("--port", type=int, default=8781)
    p.add_argument("--timeout", type=float, default=30.0)
    p.add_argument("--only")
    p.add_argument("--skip")
    p.add_argument("--out")
    p.add_argument("--keep-logs", action="store_true")
    opts = p.parse_args()
    logs = tempfile.mkdtemp(prefix="azcalendar-e2e-")
    binary = wi.find_binary(opts.bin)
    log(f"AzCalendar: {binary}; logs and data: {logs}")
    app = App(binary, opts.port, opts.timeout, logs)
    ctx = {"logs": logs, "out": opts.out or os.path.join(logs, "screenshots")}
    results = []
    try:
        wi.detect_naming(app.main)
        app.main.until("the week view", lambda: app.main.exists(wi.sel("week-scroll")))
        for name, stage in STAGES:
            if opts.only and name not in opts.only.split(","):
                continue
            if name in (opts.skip or "").split(","):
                continue
            try:
                what = stage(app, ctx)
                results.append((name, "PASS", what))
            except Blocked as e:
                results.append((name, "BLOCKED", str(e)))
            except (Failure, wi.Failure, OSError, urllib.error.URLError, ValueError, KeyError) as e:
                results.append((name, "FAIL", str(e)))
            log(f"{results[-1][1]} {name}: {results[-1][2]}")
            if app.process.poll() is not None:
                results.append(("app", "FAIL", f"AzCalendar exited ({app.process.returncode})"))
                break
    finally:
        app.stop()
    failed = [r for r in results if r[1] == "FAIL"]
    for name, status, what in results:
        print(f"{status:8} {name}: {what}")
    if failed:
        print(f"stderr tail:\n{wi.tail(app.err)}")
    if failed or opts.keep_logs:
        log(f"logs kept in {logs}")
    else:
        shutil.rmtree(logs, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
