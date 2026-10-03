#!/usr/bin/env python3
"""AzMeet end to end: two people join a call from a link, see each other, and chat.

    1. starts the meet Worker's dev server (in memory) on --worker-port;
    2. starts Ada (AZMEET_AUTOCREATE=1: she creates a meeting and prints its link) and, once the
       link is out, Ben (AZMEET_JOIN=<link>), both headless (AZ_BACKEND=headless) with the test
       tone and the test pattern for their devices, each under the capped runner
       (run_capped.sh, 1000 MB and --app-seconds each: two apps run at once here, so each gets
       the smaller cap);
    3. asserts each window has the other's camera tile (`#__azmeet_tile_<name>_camera`, laid out
       inside the window) and decodes the other's video (the statistics panel's "Video from
       <name> (camera ...): <codec>, decoded N" line, N > 0; H.264 with --require-h264);
    4. Ada opens the chat, types a message and presses Enter: Ben's process prints
       `AZMEET_CHAT Ada: <message>`, his chat tab counts it unread, and once he opens the chat
       his panel shows it; then he answers with the Send button and Ada sees the answer;
    5. each side kept the meeting in its own data tree (AZLIN_DATA, one per app): a
       `meet/<room>/chat.jsonl` with both messages, one JSON object per line, and a
       `meet/<room>/meeting.json` that lists the other person (a build without the files says
       so and the check is skipped);
    6. rejoin: Ada leaves and joins the same link again on the same data tree - the chat of
       her first visit comes back (`AZMEET_CHAT_RESTORED 2`, both messages in her panel), a new
       message goes to Ben, and her chat.jsonl then holds all three (`--skip rejoin` for a build
       before the chat came back);
    7. takes a screenshot of each window (--out), stops everything.

Usage (from the azul repository, after building libazul with the debug server and AzMeet):

    python3 scripts/azmeet_e2e.py [--bin target/release/AzMeet]
        [--worker-dir ../azul-apps/cf-workers/meet] [--capped <run_capped.sh>]
        [--port-a 8781] [--port-b 8782] [--worker-port 8790] [--timeout 150]
        [--app-seconds 140] [--require-h264] [--skip rejoin] [--out <dir>] [--keep-logs]

`AZMEET_BIN`, `AZMEET_WORKER_DIR` and `AZ_RUN_CAPPED` name the binary, the Worker and the
capped runner too. Without a capped runner the apps run uncapped and the script says so. The
debug-server client is the Azlin apps' shared one (`scripts/azlin_e2e.py`).
"""

import argparse
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

import azlin_e2e as e2e
from azlin_e2e import Failure

# The capped runner (scripts/waves/tools/run_capped.sh) holds a machine-wide lock, so a second
# runner inside one waits forever: run this WHOLE script under one runner (it caps the tree:
# node, both apps and this script) and leave --capped empty. --capped / AZ_RUN_CAPPED still cap
# each app on their own where no outer runner is used.
DEFAULT_CAPPED = None
MESSAGE = "Hello Ben, can you see me?"
ANSWER = "Loud and clear, Ada"
AGAIN = "I am back, Ben"


def log(line):
    print("[azmeet-e2e] %s" % line, flush=True)


def first_existing(what, candidates, executable=False):
    for c in candidates:
        if not c:
            continue
        if os.path.exists(c) and (not executable or os.access(c, os.X_OK)):
            return os.path.abspath(c)
    raise Failure("%s not found; tried %s" % (what, [c for c in candidates if c]))


def find_worker(explicit):
    candidates = [explicit, os.environ.get("AZMEET_WORKER_DIR")]
    for root in e2e.repo_roots():
        candidates.append(os.path.join(root, "..", "azul-apps", "cf-workers", "meet"))
    return first_existing("the meet Worker (pass --worker-dir)", candidates)


def http_json(url, timeout=5):
    with urllib.request.urlopen(url, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8") or "{}")


class Process:
    """A process with its output in a log file, in its own process group (so stopping the
    capped runner stops the app it runs too)."""

    def __init__(self, name, command, env, logs):
        self.name = name
        self.log_path = os.path.join(logs, "%s.log" % name)
        self.process = subprocess.Popen(
            command, env=env, stdin=subprocess.DEVNULL,
            stdout=open(self.log_path, "wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )

    def alive(self):
        return self.process.poll() is None

    def output(self):
        try:
            with open(self.log_path, "r", encoding="utf-8", errors="replace") as f:
                return f.read()
        except OSError:
            return ""

    def stop(self):
        if self.alive():
            try:
                os.killpg(self.process.pid, signal.SIGTERM)
                self.process.wait(timeout=3)
            except (OSError, subprocess.TimeoutExpired):
                try:
                    os.killpg(self.process.pid, signal.SIGKILL)
                except OSError:
                    pass

    def tail(self, lines=40):
        return "".join(self.output().splitlines(True)[-lines:])


class App(e2e.App):
    """AzMeet under its debug server (the shared driver), started through the capped runner
    when one is given: two frames an op, the ids by AzMeet's naming, a node's id with its
    rect."""

    def frame(self, n=2):
        super().frame(n)

    def node_rect(self, node_id):
        """The node's id and rect, or (None, {}) while no node has that id (a tile appears once
        the other side is connected)."""
        answer = self.op("get_node_layout", selector="#%s" % node_id)
        if not isinstance(answer, dict) or answer.get("status") == "error":
            return None, {}
        data = answer.get("data") or {}
        value = data.get("value") if isinstance(data, dict) else None
        value = value if isinstance(value, dict) else {}
        return value.get("node_id"), value.get("rect") or {}

    def id(self, short):
        """An AzMeet id by its short name (`chat-field`, `tile-ben-camera`): `__azmeet_chat_field`
        since the wave-6 prefix ruling (src/ids.rs), `azmeet-chat-field` on an older build. The
        naming is found once, from the Settings button every screen has."""
        if getattr(self, "prefixed", None) is None:
            found = self.op("get_node_layout", selector="#__azmeet_settings")
            self.prefixed = isinstance(found, dict) and found.get("status") != "error"
        if self.prefixed:
            return "__azmeet_" + short.replace("-", "_")
        return "azmeet-" + short


def start_app(name, binary, port, env, logs, args, capped):
    """One AzMeet: `env` its environment (`app_env`), under `capped` when given."""
    return App(name, binary, [], port, logs, args.timeout, extra_env=env, capped=capped,
               cap_mb=args.cap_mb, cap_seconds=args.app_seconds)


def until(what, check, deadline, procs=(), interval=0.5):
    last = None
    while time.time() < deadline:
        for p in procs:
            if not p.alive():
                raise Failure("%s exited (%s) while waiting for %s" % (p.name, p.process.returncode, what))
        try:
            value = check()
            if value:
                return value
        except (OSError, ValueError, KeyError, urllib.error.URLError) as e:
            last = e
        time.sleep(interval)
    raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))


def inside(rect, width, height):
    x, y = float(rect.get("x", -1)), float(rect.get("y", -1))
    w, h = float(rect.get("width", 0)), float(rect.get("height", 0))
    return w > 0 and h > 0 and x >= -0.5 and y >= -0.5 and x + w <= width + 0.5 and y + h <= height + 0.5


VIDEO_LINE = re.compile(
    r"^Video from (?P<name>.+?) \(camera(?: (?P<height>\d+)p)?(?: via [^)]+)?\): "
    r"(?P<codec>H\.264|JPEG), decoded (?P<decoded>\d+),"
)


def decoded_from(app, name):
    """(codec, frames decoded) of the newest "Video from <name> (camera ...)" line, or None."""
    best = None
    for line in app.texts():
        m = VIDEO_LINE.match(line)
        if m and m.group("name") == name:
            decoded = int(m.group("decoded"))
            if best is None or decoded > best[1]:
                best = (m.group("codec"), decoded)
    return best


def app_env(worker, name, port, extra):
    env = dict(os.environ)
    env.update({
        "AZ_BACKEND": "headless",
        "AZ_DEBUG": str(port),
        "AZMEET_WORKER": worker,
        "AZMEET_NAME": name,
        "AZMEET_RELAY": "off",
        "AZMEET_TEST_TONE": "1",
        "AZMEET_TEST_PATTERN": "1",
        # The statistics panel shows the "Video from ..." lines this script reads.
        "AZMEET_PANEL": "statistics",
    })
    env.update(extra)
    return env


def meeting_files(data):
    """The (chat lines, meeting record) of the one meeting folder under `data`/meet."""
    meet = os.path.join(data, "meet")
    if not os.path.isdir(meet):
        return None
    for name in sorted(os.listdir(meet)):
        folder = os.path.join(meet, name)
        chat_path = os.path.join(folder, "chat.jsonl")
        record_path = os.path.join(folder, "meeting.json")
        if os.path.isfile(chat_path) and os.path.isfile(record_path):
            try:
                with open(chat_path, encoding="utf-8") as f:
                    lines = [json.loads(line) for line in f if line.strip()]
                with open(record_path, encoding="utf-8") as f:
                    record = json.load(f)
            except (OSError, ValueError):
                return None
            return lines, record
    return None


def check_files(app, data, other, deadline, procs):
    """`app` wrote the meeting's chat (both messages) and its record (`other` met) into `data`."""
    grace = min(deadline, time.time() + 3)
    while not app.printed("AZMEET_SAVED") and time.time() < grace:
        time.sleep(0.25)
    if not app.printed("AZMEET_SAVED"):
        log("%s: no AZMEET_SAVED - a build without the meeting files; check skipped" % app.name)
        return

    def written():
        found = meeting_files(data)
        if not found:
            return None
        lines, record = found
        texts = [line.get("text") for line in lines]
        if MESSAGE in texts and ANSWER in texts and other in record.get("people", []):
            return found
        return None

    lines, record = until("%s's chat.jsonl and meeting.json" % app.name, written, deadline, procs)
    log("%s kept the meeting %s: %d chat lines, people %s"
        % (app.name, record.get("meeting"), len(lines), record.get("people")))


def chat(sender, receiver, sender_name, text, use_enter, deadline, procs):
    """`sender` opens the chat and sends `text`; `receiver` prints it, counts it, shows it."""
    sender.must("click", text="Chat")
    sender.frame()
    sender.must("click", selector="#" + sender.id("chat-field"))
    sender.frame()
    sender.must("text_input", text=text)
    sender.frame()
    if use_enter:
        sender.must("key_down", key="Return")
        sender.must("key_up", key="Return")
    else:
        sender.must("click", selector="#" + sender.id("chat-send"))
    sender.frame()
    until("%s's message on %s's stdout" % (sender_name, receiver.name),
          lambda: "%s: %s" % (sender_name, text) in receiver.printed("AZMEET_CHAT"),
          deadline, procs)
    log("%s printed AZMEET_CHAT %s: %s" % (receiver.name, sender_name, text))
    if not any(t.startswith("Chat (") for t in receiver.texts()):
        # The receiver's chat may already be open (the answer goes to Ada, whose chat is open).
        if not any(text in t for t in receiver.texts()):
            raise Failure("%s's window neither counts the message unread nor shows it" % receiver.name)
    receiver.must("click", text="Chat")
    receiver.frame()
    until("the message in %s's chat panel" % receiver.name,
          lambda: any(t == text for t in receiver.texts()), deadline, procs)
    log("%s's chat panel shows %r" % (receiver.name, text))
    field_after = [t for t in sender.texts() if t == text]
    # The sender lists its own message once (the field is empty again).
    if len(field_after) != 1:
        raise Failure("%s's window shows the sent text %d times (the field kept it?)" % (sender.name, len(field_after)))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--worker-dir")
    parser.add_argument("--capped", default=os.environ.get("AZ_RUN_CAPPED", DEFAULT_CAPPED))
    parser.add_argument("--port-a", type=int, default=8781)
    parser.add_argument("--port-b", type=int, default=8782)
    parser.add_argument("--worker-port", type=int, default=8790)
    parser.add_argument("--timeout", type=int, default=150)
    parser.add_argument("--app-seconds", type=int, default=140)
    parser.add_argument("--cap-mb", type=int, default=1000)
    parser.add_argument("--width", type=int, default=1100)
    parser.add_argument("--height", type=int, default=720)
    parser.add_argument("--require-h264", action="store_true")
    parser.add_argument("--skip", help="stages to leave out, comma-separated (rejoin)")
    parser.add_argument("--out")
    parser.add_argument("--keep-logs", action="store_true")
    args = parser.parse_args()

    deadline = time.time() + args.timeout
    logs = tempfile.mkdtemp(prefix="azmeet-e2e-")
    out = args.out or logs
    os.makedirs(out, exist_ok=True)
    procs = []
    passed = False
    try:
        binary = e2e.find_binary("AzMeet", args.bin, "AZMEET_BIN")
        worker_dir = find_worker(args.worker_dir)
        capped = args.capped if args.capped and os.access(args.capped, os.X_OK) else None
        if capped:
            log("apps run under %s: %d MB and %d s each (two at once)" % (capped, args.cap_mb, args.app_seconds))
        else:
            log("WARNING: no capped runner (--capped / AZ_RUN_CAPPED): the apps run uncapped")
        node = shutil.which("node") or "node"

        worker = "http://127.0.0.1:%d" % args.worker_port
        dev = Process("worker", [node, os.path.join(worker_dir, "dev-server.mjs"), "--memory",
                                 "--port", str(args.worker_port)], dict(os.environ), logs)
        procs.append(dev)
        until("the dev server", lambda: http_json(worker + "/health").get("ok") is True, deadline, procs)
        log("dev server up on %s" % worker)

        data_ada = os.path.join(logs, "data-ada")
        data_ben = os.path.join(logs, "data-ben")
        ada = start_app("ada", binary, args.port_a,
                        app_env(worker, "Ada", args.port_a, {"AZMEET_AUTOCREATE": "1", "AZLIN_DATA": data_ada}),
                        logs, args, capped)
        procs.append(ada)
        link = until("Ada's meeting link (AZMEET_LINK)", lambda: (ada.printed("AZMEET_LINK") or [None])[0],
                     deadline, procs)
        log("Ada created %s" % link)

        ben = start_app("ben", binary, args.port_b,
                        app_env(worker, "Ben", args.port_b, {"AZMEET_JOIN": link, "AZLIN_DATA": data_ben}),
                        logs, args, capped)
        procs.append(ben)

        for app in (ada, ben):
            until("%s's debug server" % app.name, lambda app=app: app.op("wait_frame") is not None, deadline, procs)
            app.must("resize", width=args.width, height=args.height)
            app.frame()

        # Each sees the other's camera tile, inside the window.
        for app, other in ((ada, "ben"), (ben, "ada")):
            def tile_shown(app=app, other=other):
                node, rect = app.node_rect(app.id("tile-%s-camera" % other))
                return node is not None and inside(rect, args.width, args.height)
            until("%s's tile in %s's window" % (other, app.name), tile_shown, deadline, procs)
            log("%s shows %s's tile" % (app.name, other))

        # Each decodes the other's video.
        for app, other in ((ada, "Ben"), (ben, "Ada")):
            got = until("%s decoding %s's video" % (app.name, other),
                        lambda app=app, other=other: (lambda d: d if d and d[1] > 0 else None)(decoded_from(app, other)),
                        deadline, procs)
            log("%s decodes %s's camera: %s, %d frames" % (app.name, other, got[0], got[1]))
            if args.require_h264 and got[0] != "H.264":
                raise Failure("%s gets %s's camera as %s, not H.264" % (app.name, other, got[0]))

        # Chat: Ada sends with Enter, Ben answers with the Send button.
        chat(ada, ben, "Ada", MESSAGE, True, deadline, procs)
        chat(ben, ada, "Ben", ANSWER, False, deadline, procs)

        # The side panel's link line: the link gives way, "Copy link" stays on one line (LOOK
        # 2026-10-03: the button was squeezed into two lines beside the long link).
        node, rect = ada.node_rect(ada.id("copy-link"))
        if node is None or not 0 < float(rect.get("height", 0)) <= 34.0:
            raise Failure("Ada's Copy link button is %s (one line is at most 34 px high)" % (rect or "not there"))

        # The meeting's files in each side's data tree.
        check_files(ada, data_ada, "Ben", deadline, procs)
        check_files(ben, data_ben, "Ada", deadline, procs)

        if "rejoin" in (args.skip or "").split(","):
            log("rejoin skipped (--skip rejoin)")
        else:
            procs.remove(ada)
            ada.stop()
            ada = start_app("ada-again", binary, args.port_a,
                            app_env(worker, "Ada", args.port_a, {"AZMEET_JOIN": link, "AZLIN_DATA": data_ada}),
                            logs, args, capped)
            procs.append(ada)
            until("Ada's debug server again", lambda: ada.op("wait_frame") is not None, deadline, procs)
            ada.must("resize", width=args.width, height=args.height)
            ada.frame()
            restored = until("AZMEET_CHAT_RESTORED", lambda: ada.printed("AZMEET_CHAT_RESTORED"), deadline, procs)
            if restored[-1].strip() != "2":
                raise Failure("Ada's first visit had 2 messages, %s came back" % restored[-1])
            ada.must("click", text="Chat")
            ada.frame()
            until("the first visit's chat in Ada's panel",
                  lambda: MESSAGE in ada.texts() and ANSWER in ada.texts(), deadline, procs)
            log("Ada rejoined: the chat of her first visit is back")
            chat(ada, ben, "Ada", AGAIN, True, deadline, procs)

            def all_three():
                found = meeting_files(data_ada)
                texts = [line.get("text") for line in found[0]] if found else []
                return texts if texts == [MESSAGE, ANSWER, AGAIN] else None

            until("Ada's chat.jsonl with all three messages", all_three, deadline, procs)
            log("Ada's chat.jsonl holds the first visit's chat and the new message")

        for app in (ada, ben):
            app.screenshot(os.path.join(out, "azmeet-%s.png" % app.name))
        passed = True
        log("PASS")
    except Failure as e:
        log("FAIL: %s" % e)
        for p in procs:
            log("----- %s (tail) -----\n%s" % (p.name, p.tail()))
    finally:
        for p in reversed(procs):
            p.stop()
        if passed and not args.keep_logs and out != logs:
            shutil.rmtree(logs, ignore_errors=True)
        else:
            log("logs in %s" % logs)
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
