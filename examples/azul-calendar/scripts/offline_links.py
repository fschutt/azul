#!/usr/bin/env python3
"""AzCalendar makes a meeting link with no meeting server, and registers it once one answers.

  1. starts AzCalendar headless (AZ_BACKEND=headless, AZ_DEBUG=<port>) with an empty data folder,
     a sync interval of a second (AZCAL_SYNC_SECONDS=1), and a meeting server address where
     NOTHING listens yet (AZMEET_WORKER=http://127.0.0.1:<free port>);
  2. the window never says there is no meeting server;
  3. "New event": title "Offline sync", "Add AzMeet link", "Save event": the event file is
     written AT ONCE, with an azlin://meet/<room id> link marked `pending` (AZCAL_LINK on stdout);
  4. starts the meet dev server (azul-apps cf-workers/meet/dev-server.mjs, in memory) on that
     port: AzCalendar registers the room it made (AZCAL_SYNCED), the file loses `pending` and
     gains the server's code and the meeting's times in UTC, and the dev server knows the room
     under the id AzCalendar made.

The dev server has to take the app's own room id (`POST /rooms {"room": ...}`):
scripts/cal2/meet-000*.patch in the azul repository, applied to azul-apps.

Usage (from the azul repository, after building AzCalendar and a libazul with the debug server):
  python3 examples/azul-calendar/scripts/offline_links.py
      [--bin target/release/AzCalendar] [--worker-dir ../azul-apps/cf-workers/meet]
      [--port 8773] [--timeout 30] [--keep-logs]

Also read from the environment: AZCAL_BIN, AZMEET_WORKER_DIR.
"""

import argparse
import datetime
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.dont_write_bytecode = True
from week_interactions import (  # noqa: E402  (the same debug-server client, not a copy)
    Debug,
    Failure,
    event_files,
    find_binary,
    main_repo,
    read_event,
    tail,
)

REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ROOM_LINK = re.compile(r"^azlin://meet/([0-9a-z]{26})$")
TITLE = "Offline sync"


def log(line):
    print(f"[offline-links] {line}", flush=True)


def free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def find_worker(explicit):
    candidates = [
        explicit,
        os.environ.get("AZMEET_WORKER_DIR"),
        os.path.join(main_repo(), "..", "azul-apps", "cf-workers", "meet"),
        os.path.join(REPO, "..", "azul-apps", "cf-workers", "meet"),
    ]
    for c in candidates:
        if c and os.path.exists(os.path.join(c, "dev-server.mjs")):
            return os.path.abspath(c)
    raise Failure("the meet Worker was not found (pass --worker-dir)")


def printed(path, key):
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            m = re.search(rf"^{key} (\S+)$", f.read(), re.M)
            return m.group(1) if m else None
    except OSError:
        return None


def get_json(url):
    try:
        with urllib.request.urlopen(url, timeout=5) as r:
            return r.status, json.loads(r.read().decode() or "{}")
    except urllib.error.HTTPError as e:
        return e.code, {}


def utc_of(date, hhmm):
    """The event's wall-clock time read in this machine's zone, in UTC (what AzCalendar sends)."""
    local = datetime.datetime.fromisoformat(f"{date}T{hhmm}:00").astimezone()
    return local.astimezone(datetime.timezone.utc)


def run(opts, logs, children):
    binary = find_binary(opts.bin)
    worker_dir = find_worker(opts.worker_dir)
    port = free_port()
    server = f"http://127.0.0.1:{port}"
    data = os.path.join(logs, "data")
    os.makedirs(data)
    log(f"AzCalendar: {binary}; meet Worker: {worker_dir}; meeting server (not up yet): {server}")

    env = dict(os.environ)
    env.update(
        {
            "AZ_BACKEND": "headless",
            "AZ_DEBUG": str(opts.port),
            "AZCAL_DATA": data,
            "AZMEET_WORKER": server,
            "AZCAL_SYNC_SECONDS": "1",
        }
    )
    cal_out = os.path.join(logs, "azcalendar.out")
    app = subprocess.Popen(
        [binary],
        env=env,
        stdout=open(cal_out, "w"),
        stderr=open(os.path.join(logs, "azcalendar.err"), "w"),
        stdin=subprocess.DEVNULL,
    )
    children.append(app)
    dbg = Debug(opts.port, opts.timeout)
    dbg.until("the week view", lambda: dbg.shows("New event"))

    # 2. No "there is no meeting server".
    if dbg.shows("No meeting server"):
        raise Failure("the window says there is no meeting server")

    # 3. A new event with a link, while nothing answers at the meeting server's address.
    dbg.must({"op": "click", "text": "New event"})
    dbg.until("the new-event form", lambda: dbg.shows("Add AzMeet link"))
    if dbg.shows("No meeting server"):
        raise Failure("the form says there is no meeting server")
    dbg.must({"op": "focus_node", "selector": "#event-title"})
    dbg.must({"op": "text_input", "text": TITLE})
    dbg.must({"op": "click", "text": "Add AzMeet link"})
    dbg.until("the form to say a link will be made", lambda: dbg.shows("A new AzMeet link is made"))
    dbg.must({"op": "click", "text": "Save event"})
    files = dbg.until("the event file, written at once (offline)", lambda: event_files(data) or None)
    name = files[0]
    event = read_event(data, name)
    meeting = event.get("meeting") or {}
    link = meeting.get("link", "")
    m = ROOM_LINK.match(link)
    if not m:
        raise Failure(f"{name} has no azlin://meet/<room id> link: {json.dumps(event)}")
    room = m.group(1)
    if meeting.get("pending") is not True:
        raise Failure(f"a link made with no server answering is not marked pending: {json.dumps(event)}")
    if meeting.get("server") != server:
        raise Failure(f"the link is not for {server}: {json.dumps(event)}")
    if printed(cal_out, "AZCAL_LINK") != link:
        raise Failure(f"AzCalendar did not print AZCAL_LINK {link}")
    log(f"offline: {name} holds {link}, pending")

    # 4. The meeting server comes up: the link is registered under the id AzCalendar made.
    worker = subprocess.Popen(
        ["node", os.path.join(worker_dir, "dev-server.mjs"), "--memory", "--port", str(port)],
        stdout=open(os.path.join(logs, "worker.out"), "w"),
        stderr=open(os.path.join(logs, "worker.err"), "w"),
        stdin=subprocess.DEVNULL,
    )
    children.append(worker)
    dbg.until("the dev server", lambda: get_json(f"{server}/health")[1].get("ok") is True)
    log(f"the meeting server is up at {server}")
    synced = dbg.until(
        "AzCalendar to register the link (AZCAL_SYNCED on stdout)",
        lambda: printed(cal_out, "AZCAL_SYNCED"),
    )
    if synced != link:
        raise Failure(f"AZCAL_SYNCED {synced} is not the link {link}")
    event = read_event(data, name)
    meeting = event.get("meeting") or {}
    if "pending" in meeting:
        raise Failure(f"the registered link is still marked pending: {json.dumps(event)}")
    if meeting.get("link") != link or not meeting.get("code"):
        raise Failure(f"the registered meeting lost its link or has no code: {json.dumps(event)}")
    starts = datetime.datetime.fromisoformat(meeting["starts_at"].replace("Z", "+00:00"))
    ends = datetime.datetime.fromisoformat(meeting["ends_at"].replace("Z", "+00:00"))
    if (starts, ends) != (utc_of(event["date"], event["start"]), utc_of(event["date"], event["end"])):
        raise Failure(f"the meeting times are not the event's in UTC: {json.dumps(event)}")
    status, known = get_json(f"{server}/rooms/{room}?format=json")
    if status != 200 or known.get("room") != room:
        raise Failure(f"the dev server does not know room {room}: {status} {known}")
    log(f"registered: {name} holds {link} (code {meeting['code']}), the server knows the room")
    log("PASS: a meeting link made offline was registered with the meeting server once it answered")
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--bin")
    parser.add_argument("--worker-dir")
    parser.add_argument("--port", type=int, default=8773)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--keep-logs", action="store_true")
    opts = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azcalendar-offline-links-")
    children = []
    passed = False
    try:
        passed = run(opts, logs, children)
    except Failure as e:
        log(f"FAIL: {e}")
        for name in ("azcalendar.out", "azcalendar.err", "worker.err"):
            print(f"\n----- {name} (tail) -----\n{tail(os.path.join(logs, name))}")
    finally:
        for child in children:
            if child.poll() is None:
                child.send_signal(signal.SIGTERM)
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
        if passed and not opts.keep_logs:
            shutil.rmtree(logs, ignore_errors=True)
        else:
            log(f"logs kept in {logs}")
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
