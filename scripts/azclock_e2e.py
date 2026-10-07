#!/usr/bin/env python3
"""AzClock end to end, headless, over the debug server.

    1. starts AzClock (AZ_BACKEND=headless, AZ_DEBUG=--debug-port) on a fresh data folder with
       --sample and waits for the files to load: 3 alarms, 2 timers, 4 cities (the plan's
       sample), each record a file of its own in clock/ (alarms/<uuid>.json, timers/<uuid>.json,
       world.json, stopwatch.json);
    2. the OS schedule: the alarms that are on are handed to the notification backend
       (AZCLOCK_SCHEDULED, Notification::with_deliver_at; the headless backend records them);
    3. Alarms: three rows (Gym Mon Wed Fri, Wake up Weekdays, Market Sat - off); the first
       switch turns Gym off and on again; "New alarm" opens the editor, "Weekdays" and Save make
       a fourth alarm file;
    4. World: the analog face and four cities; "Add city" searches "lima" and adds America/Lima
       to world.json;
    5. Timer: "Tea" is shown; the 1 min preset starts a third timer file;
    6. Stopwatch: the sample's 04:17.36 with five laps; Reset, Start, Lap, Stop;
    7. the settings page (Mod+,): the Clock section, Flora + Dark saved, OK closes it;
    8. a screenshot of every screen;
    9. with --ring: the 1 min timer rings (AZCLOCK_RING timer), the overlay shows "Time is up",
       Dismiss ends it.

Usage (after building libazul with the debug server and AzClock; one app at a time):

    python3 scripts/azclock_e2e.py [--bin target/release/AzClock]
        [--debug-port 8795] [--timeout 240] [--out <dir>] [--keep] [--ring]
"""

import glob
import json
import os
import shutil
import sys
import time

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azclock"
# The app's DOM names carry its prefix (examples/azul-clock/src/ids.rs).
P = "__azclock_"
RING = "--ring" in sys.argv
if RING:
    sys.argv.remove("--ring")


def files(data_dir, pattern):
    return sorted(glob.glob(os.path.join(data_dir, "clock", pattern)))


def read_json(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def body(args, logs, out):
    binary = e2e.find_binary("AzClock", args.bin, "AZCLOCK_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG, binary, ["--data-dir", data_dir, "--sample", "--size", "720x560", "--screen", "alarms"],
                  args.debug_port, logs, args.timeout)
    try:
        # 1. The sample, loaded and written as files.
        app.until("the files to load", lambda: app.last("AZCLOCK_LOADED"))
        if app.last("AZCLOCK_LOADED") != "3 2 4":
            raise Failure("--sample loads 3 alarms, 2 timers, 4 cities; got %s" % app.last("AZCLOCK_LOADED"))
        app.until("the sample files on disk", lambda: len(files(data_dir, "alarms/*.json")) == 3
                  and len(files(data_dir, "timers/*.json")) == 2
                  and os.path.exists(os.path.join(data_dir, "clock", "world.json"))
                  and os.path.exists(os.path.join(data_dir, "clock", "stopwatch.json")))
        gym = [read_json(p) for p in files(data_dir, "alarms/*.json") if read_json(p).get("label") == "Gym"]
        if not gym or gym[0].get("repeat") != "FREQ=WEEKLY;BYDAY=MO,WE,FR":
            raise Failure("Gym's file does not repeat Mon / Wed / Fri: %s" % gym)
        app.log("sample files: 3 alarms, 2 timers, world.json, stopwatch.json")

        # 2. The OS schedule.
        app.until("the alarms to be scheduled", lambda: app.printed("AZCLOCK_SCHEDULED", r"\d+ \d+"))
        posted = int(app.printed("AZCLOCK_SCHEDULED", r"\d+ \d+")[0].split()[0])
        if posted < 2:
            raise Failure("the two alarms that are on schedule at least two notifications, got %d" % posted)
        app.log("scheduled %d notification(s) with a delivery time" % posted)

        # 3. Alarms.
        app.frame(2)
        rows = app.nodes_with_class(P + "alarm-row")
        if len(rows) != 3:
            raise Failure("3 alarm rows expected, got %d" % len(rows))
        for text in ["Gym", "Mon Wed Fri", "Wake up", "Weekdays", "Market", "Sat"]:
            if not app.shows(text):
                raise Failure("the alarms screen does not show %r" % text)
        app.screenshot(os.path.join(out, "alarms-flat-light.png"))
        app.after("the first switch to turn Gym off", "AZCLOCK_ALARM_SWITCH", r"\S+ off",
                  lambda: app.click(selector="." + P + "alarm-switch"))
        app.after("the first switch to turn Gym on", "AZCLOCK_ALARM_SWITCH", r"\S+ on",
                  lambda: app.click(selector="." + P + "alarm-switch"))
        # The editor, the city search and the ringing overlay are Modals: windows of their
        # own, where their nodes are laid out and clicked (e2e.modal_window).
        app.click(selector="#" + P + "new")
        app.until("the alarm editor", lambda: app.has_id(P + "editor"))
        editor = e2e.modal_window(app)
        editor.until("the editor in its window", lambda: editor.laid_out("#" + P + "editor-save"))
        editor.screenshot(os.path.join(out, "editor.png"))
        editor.click(text="Weekdays")
        app.after("the new alarm to be saved", "AZCLOCK_ALARM_SAVED", r"\S+",
                  lambda: editor.click(selector="#" + P + "editor-save"))
        app.until("a fourth alarm file", lambda: len(files(data_dir, "alarms/*.json")) == 4)
        app.until("the editor to close", lambda: not app.has_id(P + "editor"))
        app.log("a new weekday alarm is a fourth file")

        # 4. World.
        app.click(text="World")
        app.until("the World screen", lambda: app.last("AZCLOCK_SCREEN") == "world")
        app.frame(2)
        if not app.has_id(P + "face") or len(app.nodes_with_class(P + "city-row")) != 4:
            raise Failure("the face and four city rows are expected")
        for text in ["Reykjavik", "New York", "Tokyo", "Sydney"]:
            if not app.shows(text):
                raise Failure("the world clock does not show %r" % text)
        app.screenshot(os.path.join(out, "world.png"))
        app.click(selector="#" + P + "new")
        app.until("the city search", lambda: app.has_id(P + "city-query"))
        search = e2e.modal_window(app)
        search.until("the search in its window", lambda: search.laid_out("#" + P + "city-query"))
        search.text_input("#" + P + "city-query", "lima")
        search.until("Lima among the results", lambda: search.shows("Lima - America/Lima"))
        app.after("Lima to be added", "AZCLOCK_CITY_ADDED", r"\S+",
                  lambda: search.click(text="Lima - America/Lima"))
        app.until("world.json to hold Lima",
                  lambda: any(c.get("zone") == "America/Lima"
                              for c in read_json(os.path.join(data_dir, "clock", "world.json")).get("cities", [])))
        app.log("Lima added to world.json")

        # 5. Timer.
        app.click(text="Timer")
        app.until("the Timer screen", lambda: app.last("AZCLOCK_SCREEN") == "timer")
        app.frame(2)
        if not app.shows("Tea") or not app.has_id(P + "timer-time"):
            raise Failure("the timer screen does not show Tea")
        app.screenshot(os.path.join(out, "timer.png"))
        # A click by text takes the first node CONTAINING it: "+1 min" (the selected timer's
        # button) comes before the "1 min" preset, "Stopwatch" before "Stop". Exact texts.
        app.click_exact("1 min")
        app.until("a third timer file", lambda: len(files(data_dir, "timers/*.json")) == 3)
        app.log("the 1 min preset started a timer")

        # 6. Stopwatch.
        app.click(text="Stopwatch")
        app.until("the Stopwatch screen", lambda: app.last("AZCLOCK_SCREEN") == "stopwatch")
        app.frame(2)
        if not app.shows("00:04:17.36") or len(app.nodes_with_class(P + "lap-row")) != 5:
            raise Failure("the sample stopwatch (04:17.36, five laps) is not shown")
        app.screenshot(os.path.join(out, "stopwatch.png"))
        app.click_exact("Reset")
        app.until("the stopwatch reset", lambda: app.shows("00:00:00.00"))
        app.click_exact("Start")
        time.sleep(1.2)
        app.click_exact("Lap")
        app.click_exact("Stop")
        app.frame(2)
        if len(app.nodes_with_class(P + "lap-row")) != 1:
            raise Failure("one lap after Start, Lap, Stop")
        app.log("the stopwatch ran, took a lap and stopped")

        # 7. Settings.
        app.key("comma", primary=True)
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        if not app.shows("Ring while closed"):
            raise Failure("the Clock settings are not on the page")
        app.click(text="General")
        app.until("the General options", lambda: app.has_id("appkit-theme"))
        saved_before = len(app.printed("AZCLOCK_SETTINGS_SAVED"))
        app.click(text="Flora")
        app.click(text="Dark")
        app.until("the settings to be saved", lambda: len(app.printed("AZCLOCK_SETTINGS_SAVED")) >= saved_before + 2)
        app.screenshot(os.path.join(out, "settings-flora-dark.png"))
        app.click(selector="#appkit-settings-ok")  # OK keeps them (Escape would cancel)
        app.until("the settings page to close", lambda: not app.has_id("appkit-settings"))
        for screen in ["Alarms", "World", "Timer"]:
            app.click(text=screen)
            app.frame(2)
            app.screenshot(os.path.join(out, "%s-flora-dark.png" % screen.lower()))

        # 9. A ring (a minute long: --ring).
        if RING:
            app.until("the 1 min timer to ring", lambda: app.printed("AZCLOCK_RING", r"timer \S+"))
            app.until("the ringing overlay", lambda: app.shows("Time is up"))
            overlay = e2e.modal_window(app)
            overlay.until("the overlay in its window", lambda: overlay.laid_out("#" + P + "ring-dismiss"))
            overlay.screenshot(os.path.join(out, "ringing.png"))
            app.after("Dismiss", "AZCLOCK_DISMISSED", r"\S+",
                      lambda: overlay.click(selector="#" + P + "ring-dismiss"))
            app.log("the timer rang and was dismissed")

        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
        if not args.keep:
            shutil.rmtree(data_dir, ignore_errors=True)


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8795)
