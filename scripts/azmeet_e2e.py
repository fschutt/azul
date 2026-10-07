#!/usr/bin/env python3
"""AzMeet end to end: two people join a call from a link, see each other, and chat - directly,
then again with every packet going through a local iroh relay.

Phase `direct` (the call on this machine, no relay):
    1. starts the meet Worker's dev server (in memory) on --worker-port;
    2. starts Ada (`--autocreate`: she creates a meeting, enters it and prints its link) and,
       once the link is out, Ben (`--join <link> --waiting-room`), both headless
       (AZ_BACKEND=headless) with the test tone and the test pattern for their devices
       (`--test-tone --test-pattern`), each under the capped runner when one is given
       (run_capped.sh, 1000 MB and --app-seconds each: two apps run at once here, so each gets
       the smaller cap). Every AzMeet setting is a switch (`--worker`, `--relay off`, `--panel
       statistics`, ...; AzMeet's --help); the AZMEET_* variables of this shell are blanked
       for the apps, so none changes a run behind its back;
    2b. Ben's waiting room: `AZMEET_WAITING <link>` on his stdout, the preview
       (`#__azmeet_preview`) laid out, the meeting's code and "Join now", and who is in the
       meeting already ("Ada is in this meeting", read from the meeting server while he waits,
       nothing announced); the microphone switch
       (`#__azmeet_mic`) flips "Mute" -> "Unmute" -> "Mute" and the camera switch (`#__azmeet_cam`)
       "Stop video" -> "Start video" -> "Stop video"; the gear (`#__azmeet_settings`, top right)
       opens azul-appkit's settings page with AzMeet's categories (Audio & Video, Meetings,
       Recording) and Escape closes it; then "Join now" (`#__azmeet_join_now`) enters the meeting
       (`AZMEET_ROOM` on his stdout). `--skip waiting` for a build before the waiting room (Ben
       then starts without `--waiting-room` and goes straight in);
    3. asserts each window has the other's camera tile (`#__azmeet_tile_<name>_camera`, laid out
       inside the window) and decodes the other's video (the statistics panel's "Video from
       <name> (camera ...): <codec>, decoded N" line, N > 0; H.264 with --require-h264);
    4. Ada opens the chat, types a message and presses Enter: Ben's process prints
       `AZMEET_CHAT Ada: <message>`, his chat tab counts it unread, and once he opens the chat
       his panel shows it; then he answers with the Send button and Ada sees the answer;
    5. each side kept the meeting in its own data tree (`--data-dir`, one per app): a
       `meet/<room>/chat.jsonl` with both messages, one JSON object per line, and a
       `meet/<room>/meeting.json` that lists the other person (a build without the files says
       so and the check is skipped);
    6. rejoin: Ada leaves and joins the same link again on the same data tree - the chat of
       her first visit comes back (`AZMEET_CHAT_RESTORED 2`, both messages in her panel), a new
       message goes to Ben, and her chat.jsonl then holds all three (`--skip rejoin` for a build
       before the chat came back);
    7. takes a screenshot of each window (--out), stops both apps.

Phase `relay` (the same call through a relay, nothing direct):
    1. starts iroh's own relay server in dev mode on 127.0.0.1 (scripts/iroh_relay_dev.py: plain
       HTTP, no TLS, no external network; `--relay-bin` / AZMEET_RELAY_BIN, else
       ~/.cache/azul/iroh-relay/bin/iroh-relay, else PATH). Without the binary the phase is
       SKIPPED, says how to build it, and the run still passes on the direct phase;
    2. starts Ada and Ben as above with `--relay http://127.0.0.1:<port> --relay-only`: their
       endpoints bind no UDP socket (IrohConfig::with_relay_only), so the call cannot use a
       direct path. Both must print `AZMEET_TRANSPORT relay-only <relay url>`;
    3. Ben's waiting room, the tiles, the video both ways and the chat both ways, as above;
    4. proves the bytes went through the relay: each side prints `AZMEET_PATH <other> relayed`
       and never `direct`, its statistics say "<other>: relayed", and the relay's own metrics
       grew by at least --min-relayed-kib both in (what the clients sent it) and out (what it
       passed on), with both clients connected (accepts);
    5. screenshots (azmeet-relay-ada.png, azmeet-relay-ben.png), stops the apps and the relay.

Usage (from the azul repository, after building libazul with the debug server and AzMeet):

    python3 scripts/azmeet_e2e.py [--bin target/release/AzMeet]
        [--worker-dir ../azul-apps/cf-workers/meet] [--capped <run_capped.sh>]
        [--port-a 8781] [--port-b 8782] [--worker-port 8790] [--timeout 150]
        [--app-seconds 140] [--require-h264] [--skip rejoin,waiting,relay] [--out <dir>] [--keep-logs]
        [--phases direct,relay] [--relay-bin <iroh-relay>] [--relay-port 0] [--relay-metrics-port 0]
        [--relay-port-a 8783] [--relay-port-b 8784] [--relay-timeout 150] [--min-relayed-kib 256]

`--phases relay` runs the relay phase alone. The relay binary is built once, outside the
repository: `cd /tmp && cargo install iroh-relay@1.2.0 --locked --features server --root
~/.cache/azul/iroh-relay`. `AZMEET_BIN`, `AZMEET_WORKER_DIR`, `AZMEET_RELAY_BIN` and
`AZ_RUN_CAPPED` name the binary, the Worker, the relay and the capped runner too. Without a
capped runner the apps run uncapped and the script says so. The debug-server client is the
Azlin apps' shared one (`scripts/azlin_e2e.py`).
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

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as e2e  # noqa: E402
import iroh_relay_dev  # noqa: E402
from azlin_e2e import Failure  # noqa: E402

# The capped runner (scripts/waves/tools/run_capped.sh) holds a machine-wide lock, so a second
# runner inside one waits forever: run this WHOLE script under one runner (it caps the tree:
# node, both apps and this script) and leave --capped empty. --capped / AZ_RUN_CAPPED still cap
# each app on their own where no outer runner is used.
DEFAULT_CAPPED = None
MESSAGE = "Hello Ben, can you see me?"
ANSWER = "Loud and clear, Ada"
AGAIN = "I am back, Ben"
RELAYED_MESSAGE = "Through the relay, Ben?"
RELAYED_ANSWER = "Every byte of it, Ada"


def log(line):
    print("[azmeet-e2e] %s" % line, flush=True)


def first_existing(what, candidates, executable=False):
    for c in candidates:
        if not c:
            continue
        if os.path.exists(c) and (not executable or os.access(c, os.X_OK)):
            return os.path.abspath(c)
    raise Failure("%s not found; tried %s" % (what, [c for c in candidates if c]))


def find_binary(explicit=None):
    """The AzMeet binary: `explicit`, AZMEET_BIN, else the checkout's target/."""
    return e2e.find_binary("AzMeet", explicit, "AZMEET_BIN")


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
        self.tag = name
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


def start_worker(worker_dir, port, logs):
    """The meet Worker's dev server, in memory, on `port`."""
    node = shutil.which("node") or "node"
    return Process("worker", [node, os.path.join(worker_dir, "dev-server.mjs"), "--memory",
                              "--port", str(port)], dict(os.environ), logs)


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

    def rect(self, node_id):
        """`node_rect` (azmeet_cpu.py's name for it)."""
        return self.node_rect(node_id)

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


def app_env(port):
    """The environment of an AzMeet run on top of this one: headless, the debug server on
    `port`, and every AZMEET_* variable of this shell blanked (AzMeet reads a blank one as
    unset): each is a switch now (`app_flags`), and one left over would change the run."""
    env = {name: "" for name in os.environ if name.startswith("AZMEET_")}
    env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(port)})
    return env


def app_flags(worker, name, relay="off", extra=()):
    """AzMeet's switches for a scripted participant: the meeting server, the name, the relays
    (`off`, or a relay URL - then `--relay-only` is in `extra`), the test tone and the test
    pattern for the devices, and the statistics panel open (the "Video from ..." lines this
    script reads)."""
    flags = ["--worker", worker, "--name", name, "--relay", relay, "--test-tone", "--test-pattern",
             "--panel", "statistics"]
    flags.extend(extra)
    return flags


def start_app(name, binary, port, flags, logs, args, capped, timeout=None):
    """One AzMeet with `flags` (`app_flags`), under `capped` when given."""
    return App(name, binary, flags, port, logs, timeout or args.timeout, extra_env=app_env(port),
               capped=capped, cap_mb=args.cap_mb, cap_seconds=args.app_seconds)


def until(what, check, deadline, procs=(), interval=0.5):
    last = None
    while time.time() < deadline:
        for p in procs:
            if not p.alive():
                raise Failure("%s exited (%s) while waiting for %s" % (p.tag, p.process.returncode, what))
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
        log("%s: no AZMEET_SAVED - a build without the meeting files; check skipped" % app.tag)
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

    lines, record = until("%s's chat.jsonl and meeting.json" % app.tag, written, deadline, procs)
    log("%s kept the meeting %s: %d chat lines, people %s"
        % (app.tag, record.get("meeting"), len(lines), record.get("people")))


SETTINGS_CATEGORIES = ("Audio & Video", "Meetings", "Recording")


def waiting_room(app, width, height, deadline, procs, here="Ada is in this meeting"):
    """`app` (started with `--join <link> --waiting-room`) stops in the meeting's waiting room:
    its preview is laid out, it says who is in the meeting already (`here`: the meeting server's
    list, read while waiting), the switches say what a click does and flip with each click, the
    gear opens the settings page with AzMeet's categories (Escape closes it), and "Join now"
    enters the meeting."""
    until("%s's waiting room (AZMEET_WAITING)" % app.tag, lambda: app.printed("AZMEET_WAITING"),
          deadline, procs)

    def shown(short):
        node, rect = app.node_rect(app.id(short))
        return node is not None and inside(rect, width, height)

    until("%s's Join now button" % app.tag, lambda: shown("join-now"), deadline, procs)
    node, rect = app.node_rect(app.id("preview"))
    if node is None or float(rect.get("width", 0)) < 200:
        raise Failure("%s's preview is %s (at least 200 px wide)" % (app.tag, rect or "not there"))
    if app.node_rect(app.id("meeting-code"))[0] is None:
        raise Failure("%s's waiting room shows no meeting code" % app.tag)
    if here:
        until("%s's waiting room saying %r" % (app.tag, here), lambda: here in app.texts(), deadline, procs)
        log("%s's waiting room says %r" % (app.tag, here))
    if app.printed("AZMEET_ROOM"):
        raise Failure("%s entered the meeting before Join now" % app.tag)
    # The test tone and the test pattern start on: each switch says what a click does, and says
    # the opposite after one.
    for short, on, off in (("mic", "Mute", "Unmute"), ("cam", "Stop video", "Start video")):
        if on not in app.texts():
            raise Failure("%s's %s switch does not say %r: %s" % (app.tag, short, on, app.texts()))
        app.click(selector="#" + app.id(short))
        until("%s's %s switch saying %r" % (app.tag, short, off),
              lambda off=off: off in app.texts(), deadline, procs)
        app.click(selector="#" + app.id(short))
        until("%s's %s switch saying %r again" % (app.tag, short, on),
              lambda on=on: on in app.texts(), deadline, procs)
        log("%s's waiting room: %s %r -> %r -> %r" % (app.tag, short, on, off, on))
    # The gear at the top right: azul-appkit's settings page, AzMeet's categories first.
    app.click(selector="#" + app.id("settings"))
    until("%s's settings page with %s" % (app.tag, ", ".join(SETTINGS_CATEGORIES)),
          lambda: all(any(c in t for t in app.texts(every_dom=True)) for c in SETTINGS_CATEGORIES),
          deadline, procs)
    app.key("Escape")
    until("%s's waiting room after Escape" % app.tag, lambda: shown("join-now"), deadline, procs)
    log("%s's gear opened the settings (%s); Escape closed them"
        % (app.tag, ", ".join(SETTINGS_CATEGORIES)))
    app.click(selector="#" + app.id("join-now"))
    until("%s in the meeting (AZMEET_ROOM)" % app.tag, lambda: app.printed("AZMEET_ROOM"),
          deadline, procs)
    log("%s joined from the waiting room" % app.tag)


def chat(sender, receiver, sender_name, text, use_enter, deadline, procs):
    """`sender` opens the chat and sends `text`; `receiver` prints it, counts it, shows it.

    Every click goes through the shared `click`, which waits for the animations first: a
    rebuild (a tile coming in, the rejoin) slides the side panel's nodes, and a click at their
    layout rects missed the Chat tab (2026-10-06)."""
    sender.click(text="Chat")
    sender.frame()
    sender.click(selector="#" + sender.id("chat-field"))
    sender.frame()
    sender.must("text_input", text=text)
    sender.frame()
    if use_enter:
        sender.must("key_down", key="Return")
        sender.must("key_up", key="Return")
    else:
        sender.click(selector="#" + sender.id("chat-send"))
    sender.frame()
    until("%s's message on %s's stdout" % (sender_name, receiver.tag),
          lambda: "%s: %s" % (sender_name, text) in receiver.printed("AZMEET_CHAT"),
          deadline, procs)
    log("%s printed AZMEET_CHAT %s: %s" % (receiver.tag, sender_name, text))
    if not any(t.startswith("Chat (") for t in receiver.texts()):
        # The receiver's chat may already be open (the answer goes to Ada, whose chat is open).
        if not any(text in t for t in receiver.texts()):
            raise Failure("%s's window neither counts the message unread nor shows it" % receiver.tag)
    receiver.click(text="Chat")
    receiver.frame()
    until("the message in %s's chat panel" % receiver.tag,
          lambda: any(t == text for t in receiver.texts()), deadline, procs)
    log("%s's chat panel shows %r" % (receiver.tag, text))
    field_after = [t for t in sender.texts() if t == text]
    # The sender lists its own message once (the field is empty again).
    if len(field_after) != 1:
        raise Failure("%s's window shows the sent text %d times (the field kept it?)" % (sender.tag, len(field_after)))


def start_pair(args, binary, worker, logs, capped, ports, prefix, relay, extra, data, waiting,
               timeout, procs, deadline):
    """Ada (`--autocreate`) and, once her link is out, Ben (`--join <link>`, `--waiting-room`
    when `waiting`), both with `relay` / `extra` switches and their own data tree; both up,
    sized. Returns (ada, ben, link)."""
    ada = start_app(prefix + "ada", binary, ports[0],
                    app_flags(worker, "Ada", relay, list(extra) + ["--autocreate", "--data-dir", data[0]]),
                    logs, args, capped, timeout)
    procs.append(ada)
    link = until("Ada's meeting link (AZMEET_LINK)", lambda: (ada.printed("AZMEET_LINK") or [None])[0],
                 deadline, procs)
    log("Ada created %s" % link)
    ben_extra = list(extra) + ["--join", link, "--data-dir", data[1]]
    if waiting:
        # Ben stops in the waiting room; `waiting_room` joins from there.
        ben_extra.append("--waiting-room")
    ben = start_app(prefix + "ben", binary, ports[1], app_flags(worker, "Ben", relay, ben_extra),
                    logs, args, capped, timeout)
    procs.append(ben)
    for app in (ada, ben):
        until("%s's debug server" % app.tag, lambda app=app: app.op("wait_frame") is not None, deadline, procs)
        app.must("resize", width=args.width, height=args.height)
        app.frame()
    return ada, ben, link


def see_each_other(ada, ben, args, deadline, procs):
    """Each window has the other's camera tile, inside the window, and decodes the other's
    video (H.264 with --require-h264)."""
    for app, other in ((ada, "ben"), (ben, "ada")):
        def tile_shown(app=app, other=other):
            node, rect = app.node_rect(app.id("tile-%s-camera" % other))
            return node is not None and inside(rect, args.width, args.height)
        until("%s's tile in %s's window" % (other, app.tag), tile_shown, deadline, procs)
        log("%s shows %s's tile" % (app.tag, other))
    for app, other in ((ada, "Ben"), (ben, "Ada")):
        got = until("%s decoding %s's video" % (app.tag, other),
                    lambda app=app, other=other: (lambda d: d if d and d[1] > 0 else None)(decoded_from(app, other)),
                    deadline, procs)
        log("%s decodes %s's camera: %s, %d frames" % (app.tag, other, got[0], got[1]))
        if args.require_h264 and got[0] != "H.264":
            raise Failure("%s gets %s's camera as %s, not H.264" % (app.tag, other, got[0]))


def stop(procs, *apps):
    """Stops `apps` and takes them off `procs`."""
    for app in apps:
        if app in procs:
            procs.remove(app)
        app.stop()


def direct_phase(args, binary, worker, logs, out, capped, skip, procs):
    """The call on this machine without a relay: waiting room, tiles, video, chat, the files,
    the rejoin (module docs, phase `direct`)."""
    deadline = time.time() + args.timeout
    data = (os.path.join(logs, "data-ada"), os.path.join(logs, "data-ben"))
    ada, ben, link = start_pair(args, binary, worker, logs, capped, (args.port_a, args.port_b), "",
                                "off", (), data, "waiting" not in skip, args.timeout, procs, deadline)
    if "waiting" in skip:
        log("waiting room skipped (--skip waiting): Ben went straight in")
    else:
        waiting_room(ben, args.width, args.height, deadline, procs)

    see_each_other(ada, ben, args, deadline, procs)

    # Chat: Ada sends with Enter, Ben answers with the Send button.
    chat(ada, ben, "Ada", MESSAGE, True, deadline, procs)
    chat(ben, ada, "Ben", ANSWER, False, deadline, procs)

    # The side panel's link line: the link gives way, "Copy link" stays on one line (LOOK
    # 2026-10-03: the button was squeezed into two lines beside the long link).
    node, rect = ada.node_rect(ada.id("copy-link"))
    if node is None or not 0 < float(rect.get("height", 0)) <= 34.0:
        raise Failure("Ada's Copy link button is %s (one line is at most 34 px high)" % (rect or "not there"))

    # The meeting's files in each side's data tree.
    check_files(ada, data[0], "Ben", deadline, procs)
    check_files(ben, data[1], "Ada", deadline, procs)

    if "rejoin" in skip:
        log("rejoin skipped (--skip rejoin)")
    else:
        stop(procs, ada)
        ada = start_app("ada-again", binary, args.port_a,
                        app_flags(worker, "Ada", "off", ["--join", link, "--data-dir", data[0]]),
                        logs, args, capped)
        procs.append(ada)
        until("Ada's debug server again", lambda: ada.op("wait_frame") is not None, deadline, procs)
        ada.must("resize", width=args.width, height=args.height)
        ada.frame()
        restored = until("AZMEET_CHAT_RESTORED", lambda: ada.printed("AZMEET_CHAT_RESTORED"), deadline, procs)
        if restored[-1].strip() != "2":
            raise Failure("Ada's first visit had 2 messages, %s came back" % restored[-1])
        ada.click(text="Chat")
        ada.frame()
        until("the first visit's chat in Ada's panel",
              lambda: MESSAGE in ada.texts() and ANSWER in ada.texts(), deadline, procs)
        log("Ada rejoined: the chat of her first visit is back")
        chat(ada, ben, "Ada", AGAIN, True, deadline, procs)

        def all_three():
            found = meeting_files(data[0])
            texts = [line.get("text") for line in found[0]] if found else []
            return texts if texts == [MESSAGE, ANSWER, AGAIN] else None

        until("Ada's chat.jsonl with all three messages", all_three, deadline, procs)
        log("Ada's chat.jsonl holds the first visit's chat and the new message")

    for app in (ada, ben):
        app.screenshot(os.path.join(out, "azmeet-%s.png" % app.tag))
    stop(procs, ada, ben)
    log("direct phase passed")


def relay_phase(args, binary, worker, logs, out, capped, skip, procs):
    """The call again through a local iroh relay with `--relay-only`, and the proof that the
    relay carried it (module docs, phase `relay`). False when skipped (no relay binary)."""
    relay_bin = iroh_relay_dev.find_binary(args.relay_bin)
    if not relay_bin:
        log("relay phase SKIPPED: no iroh-relay binary (--relay-bin, AZMEET_RELAY_BIN, %s, PATH). "
            "Build it once, outside the repository:\n    %s"
            % (iroh_relay_dev.DEFAULT_ROOT, iroh_relay_dev.BUILD_COMMAND))
        return False
    deadline = time.time() + args.relay_timeout
    relay = iroh_relay_dev.DevRelay(relay_bin, logs, args.relay_port or None, args.relay_metrics_port or None)
    procs.append(relay)
    try:
        relay.start(min(deadline, time.time() + 30))
    except iroh_relay_dev.RelayError as e:
        raise Failure(str(e))
    log("relay phase: iroh-relay %s at %s (metrics %s, binary %s)"
        % (relay.version, relay.url, relay.metrics_url, relay_bin))
    before = relay.metrics()

    data = (os.path.join(logs, "relay-data-ada"), os.path.join(logs, "relay-data-ben"))
    ada, ben, _link = start_pair(args, binary, worker, logs, capped, (args.relay_port_a, args.relay_port_b),
                                 "relay-", relay.url, ["--relay-only"], data, "waiting" not in skip,
                                 args.relay_timeout, procs, deadline)
    # Both endpoints are relay-only, at this relay: no UDP socket, so no direct path can form.
    expected = "relay-only %s" % relay.url
    for app in (ada, ben):
        transport = until("%s's AZMEET_TRANSPORT" % app.tag, lambda app=app: app.printed("AZMEET_TRANSPORT"),
                          deadline, procs)
        if transport[-1].strip() != expected:
            raise Failure("%s's transport is %r, not %r (a build without --relay-only?)"
                          % (app.tag, transport[-1], expected))
        log("%s: AZMEET_TRANSPORT %s" % (app.tag, transport[-1]))

    if "waiting" in skip:
        log("waiting room skipped (--skip waiting): Ben went straight in")
    else:
        waiting_room(ben, args.width, args.height, deadline, procs)
    see_each_other(ada, ben, args, deadline, procs)

    # Each side's path to the other is relayed, never direct: the stdout marker and the
    # statistics (still open: the chat below switches the panel).
    for app, other in ((ada, "Ben"), (ben, "Ada")):
        until("%s's path to %s (AZMEET_PATH)" % (app.tag, other),
              lambda app=app, other=other: [p for p in app.printed("AZMEET_PATH") if p.startswith(other + " ")],
              deadline, procs)
        until("%s's statistics saying %s: relayed" % (app.tag, other),
              lambda app=app, other=other: any(t.startswith("%s: relayed" % other) for t in app.texts()),
              deadline, procs)
        if "Transport: %s" % expected not in app.texts():
            raise Failure("%s's statistics do not say %r" % (app.tag, "Transport: %s" % expected))
        log("%s reaches %s through the relay (%s)" % (app.tag, other, "; ".join(app.printed("AZMEET_PATH"))))

    chat(ada, ben, "Ada", RELAYED_MESSAGE, True, deadline, procs)
    chat(ben, ada, "Ben", RELAYED_ANSWER, False, deadline, procs)

    for app in (ada, ben):
        direct = [p for p in app.printed("AZMEET_PATH") if p.endswith(" direct")]
        if direct:
            raise Failure("%s found a direct path although relay-only: %s" % (app.tag, direct))
    # The relay's own counters: both clients connected, and the call's bytes went in and out.
    got, passed_on, accepts = relay.relayed(before)
    log("relay phase: the relay took %.0f KiB from the clients and passed %.0f KiB on, %d connections"
        % (got / 1024, passed_on / 1024, accepts))
    if accepts < 2:
        raise Failure("the relay accepted %d client connections, not both clients" % accepts)
    floor = args.min_relayed_kib * 1024
    if got < floor or passed_on < floor:
        raise Failure("the relay carried %.0f KiB in and %.0f KiB out: less than --min-relayed-kib %d each "
                      "(the video did not go through it)" % (got / 1024, passed_on / 1024, args.min_relayed_kib))

    for app in (ada, ben):
        app.screenshot(os.path.join(out, "azmeet-%s.png" % app.tag))
    stop(procs, ada, ben, relay)
    log("relay phase passed: every packet of the call went through %s" % relay.url)
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--worker-dir")
    parser.add_argument("--capped", default=os.environ.get("AZ_RUN_CAPPED", DEFAULT_CAPPED))
    parser.add_argument("--port-a", type=int, default=8781)
    parser.add_argument("--port-b", type=int, default=8782)
    parser.add_argument("--worker-port", type=int, default=8790)
    parser.add_argument("--timeout", type=int, default=150, help="seconds for the direct phase")
    parser.add_argument("--app-seconds", type=int, default=140)
    parser.add_argument("--cap-mb", type=int, default=1000)
    parser.add_argument("--width", type=int, default=1100)
    parser.add_argument("--height", type=int, default=720)
    parser.add_argument("--require-h264", action="store_true")
    parser.add_argument("--skip", help="stages to leave out, comma-separated (rejoin, waiting, relay)")
    parser.add_argument("--phases", default="direct,relay", help="the phases to run, comma-separated")
    parser.add_argument("--relay-bin", help="the iroh-relay binary (else AZMEET_RELAY_BIN, ...)")
    parser.add_argument("--relay-port", type=int, default=0, help="the relay's HTTP port (0: a free one)")
    parser.add_argument("--relay-metrics-port", type=int, default=0, help="its metrics port (0: a free one)")
    parser.add_argument("--relay-port-a", type=int, default=8783, help="Ada's debug port in the relay phase")
    parser.add_argument("--relay-port-b", type=int, default=8784, help="Ben's debug port in the relay phase")
    parser.add_argument("--relay-timeout", type=int, default=150, help="seconds for the relay phase")
    parser.add_argument("--min-relayed-kib", type=int, default=256,
                        help="what the relay must carry each way for the phase to pass")
    parser.add_argument("--out")
    parser.add_argument("--keep-logs", action="store_true")
    args = parser.parse_args()

    logs = tempfile.mkdtemp(prefix="azmeet-e2e-")
    out = args.out or logs
    os.makedirs(out, exist_ok=True)
    skip = set(s.strip() for s in (args.skip or "").split(",") if s.strip())
    phases = [p.strip() for p in args.phases.split(",") if p.strip() and p.strip() not in skip]
    unknown = [p for p in phases if p not in ("direct", "relay")]
    if unknown:
        parser.error("unknown phase(s) %s: direct, relay" % ", ".join(unknown))
    procs = []
    passed = False
    try:
        binary = find_binary(args.bin)
        worker_dir = find_worker(args.worker_dir)
        capped = args.capped if args.capped and os.access(args.capped, os.X_OK) else None
        if capped:
            log("apps run under %s: %d MB and %d s each (two at once)" % (capped, args.cap_mb, args.app_seconds))
        else:
            log("WARNING: no capped runner (--capped / AZ_RUN_CAPPED): the apps run uncapped")

        worker = "http://127.0.0.1:%d" % args.worker_port
        dev = start_worker(worker_dir, args.worker_port, logs)
        procs.append(dev)
        until("the dev server", lambda: http_json(worker + "/health").get("ok") is True,
              time.time() + 30, procs)
        log("dev server up on %s" % worker)

        ran = []
        if "direct" in phases:
            direct_phase(args, binary, worker, logs, out, capped, skip, procs)
            ran.append("direct")
        if "relay" in phases:
            ran.append("relay" if relay_phase(args, binary, worker, logs, out, capped, skip, procs)
                       else "relay SKIPPED")
        passed = True
        log("PASS (%s)" % ", ".join(ran or ["nothing"]))
    except Failure as e:
        log("FAIL: %s" % e)
        for p in procs:
            log("----- %s (tail) -----\n%s" % (p.tag, p.tail()))
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
