#!/usr/bin/env python3
"""AzMeet end to end: two people join a call from a link, see each other, and chat - directly,
then again with every packet going through a local iroh relay; then three devices share an
end-to-end encrypted chat room, and the meeting server's database is read to prove it holds
nothing readable.

Every AzMeet keeps its device key (CRYPTO.md section 3) in an identity file of this run
(`--identity-file`: a headless run's keyring lives in memory, so a restart would be another
device), every link carries its room's invite secret after `#`, and every chat message goes
through the meeting server sealed with a room key (CRYPTO.md).

Phase `direct` (the call on this machine, no relay):
    1. starts the meet Worker's dev server (src/handler.js over node:sqlite, a database file in
       this run's log folder, generous rate limits) on --worker-port - or uses the one at
       --worker-url (the lead's `wrangler dev`, with --sqld-url for its database);
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
       meeting already ("Ada is in this meeting": the meeting server's list of signed iroh
       announcements, each named by its member's sealed record, nothing announced); the
       microphone switch (`#__azmeet_mic`) flips "Mute" -> "Unmute" -> "Mute" and the camera
       switch (`#__azmeet_cam`) "Stop video" -> "Start video" -> "Stop video"; the gear
       (`#__azmeet_settings`, top right) opens azul-appkit's settings page with AzMeet's
       categories (Audio & Video, Meetings, Recording) and Escape closes it; then "Join now"
       (`#__azmeet_join_now`) enters the meeting (`AZMEET_ROOM` on his stdout). `--skip waiting`
       for a build before the waiting room (Ben then starts without `--waiting-room` and goes
       straight in);
    3. asserts each window has the other's camera tile (`#__azmeet_tile_<name>_camera`, laid out
       inside the window) and decodes the other's video (the statistics panel's "Video from
       <name> (camera ...): <codec>, decoded N" line, N > 0; H.264 with --require-h264); with a
       database, the room's iroh announcements there carry no name;
    4. Ada opens the chat, types a message and presses Enter: Ben's process prints
       `AZMEET_CHAT Ada: <message>`, his chat tab counts it unread, and once he opens the chat
       his panel shows it; then he answers with the Send button and Ada sees the answer;
    5. each side kept the meeting in its own data tree (`--data-dir`, one per app): a
       `meet/<room>/chat.jsonl` with both messages, one JSON object per line, and a
       `meet/<room>/meeting.json` that lists the other person (a build without the files says
       so and the check is skipped);
    6. rejoin: Ada leaves and joins the same link again on the same data tree and identity
       file - the chat of her first visit comes back (`AZMEET_CHAT_RESTORED 2`, both messages
       in her panel), a new message goes to Ben, and her chat.jsonl then holds all three
       (`--skip rejoin` for a build before the chat came back);
    7. takes a screenshot of each window (--out), stops both apps.

Phase `relay` (the same call through a relay, nothing direct):
    1. starts iroh's own relay server in dev mode on 127.0.0.1 (scripts/iroh_relay_dev.py: plain
       HTTP, no TLS, no external network; `--relay-bin` / AZMEET_RELAY_BIN, else
       ~/.cache/azul/iroh-relay/bin/iroh-relay, else PATH) - or uses the one at --relay-url
       (the lead's `iroh-relay --dev`; its byte counts only with --relay-metrics-url). Without
       either the phase is SKIPPED, says how to build it, and the run still passes;
    2. starts Ada and Ben as above with `--relay http://127.0.0.1:<port> --relay-only`: their
       endpoints bind no UDP socket (IrohConfig::with_relay_only), so the call cannot use a
       direct path. Both must print `AZMEET_TRANSPORT relay-only <relay url>`;
    3. Ben's waiting room, the tiles, the video both ways and the chat both ways, as above;
    4. proves the bytes went through the relay: each side prints `AZMEET_PATH <other> relayed`
       and never `direct`, its statistics say "<other>: relayed", and the relay's own metrics
       grew by at least --min-relayed-kib both in (what the clients sent it) and out (what it
       passed on), with both clients connected (accepts);
    5. screenshots (azmeet-relay-ada.png, azmeet-relay-ben.png), stops the apps and the relay.

Phase `crypto` (three devices in one encrypted chat room; CRYPTO.md):
    1. Ada (`--autocreate --chat-room`) makes a chat room: `AZMEET_IDENTITY <device> file`
       (her identity file is mode 0600), `AZMEET_SAFETY <code>`, `AZMEET_LINK
       azlin://meet/<room>#<invite secret>`, and the room view (`#__azmeet_room_view`) shows her
       safety code (`#__azmeet_my_code`);
    2. Ben opens the link (`--open <link>`) and becomes a member: each prints `AZMEET_MEMBER
       <room> joined <name> <code>` for the other, and every safety code a device shows of
       another is the one that device printed for itself - and the one this script computes
       from the public keys in the database;
    3. Ada writes in the room view (Enter): a room key is made (`AZMEET_KEY <room> epoch=1 ...
       members=2 by=me` on her side; Ben takes it) and Ben prints `AZMEET_CHAT Ada Lovelace:
       ...`; Ben answers with Send, under the same key (he makes none);
    4. Cleo opens only the room's code (`--open <code>`): no invite secret, so she knocks; Ada
       sees `AZMEET_MEMBER <room> knocking Cleo Martin <code>` (her code as Cleo printed it)
       and clicks "Admit" (`#__azmeet_admit_<device>`): Cleo prints `AZMEET_ADMITTED`, Ada
       makes key 2 for the three (`members=3`), Cleo's view names Ada and Ben (their names
       sealed with the link's key open for her now) and says the two earlier messages were
       sealed before she joined - she never prints them;
    5. Ada writes again: Ben and Cleo read it. Ben marks Ada verified (`#__azmeet_verify_...`:
       `AZMEET_VERIFIED`, the badge, and `meet/rooms.json` keeps it);
    6. Ben clicks "Leave room" (`AZMEET_LEFT_ROOM`): Ada and Cleo see him leave, Ada's next
       message is under key 3 for the two of them (`members=2`), Cleo reads it, Ben does not;
    7. the database (the dev server's file, --db-file, or the sqld at --sqld-url): the
       messages are under keys 1, 1, 2, 3; key 1 is sealed to Ada and Ben, key 2 to the three,
       key 3 to Ada and Cleo only; Ben's record says left, Cleo's name is sealed now; and no
       text value of any table - nor what it decodes to as base64 or hex - holds a message, a
       name, the invite secret or a device seed. Each device's rooms.json holds its invite
       secret sealed only;
    8. Cleo restarts on the same identity file and data tree: the same device, the room in
       "Your rooms" (`#__azmeet_room_<room>`), and its history read back from the ciphertext
       (`AZMEET_HISTORY`): the two messages sealed to her, not the two before her;
    9. a meeting with times: Ada goes Back and fills "Schedule" (a start in two hours, local
       time; the form's 60 minutes): her waiting room prints `AZMEET_TIMES <start> <end>` (UTC) and shows
       them (`#__azmeet_meeting_times`, "starts in ..."); Cleo pastes its link into "Join with
       a link or a code" and gets the same times from the meeting server; the database has
       them;
   10. screenshots (azmeet-crypto-*.png), stops the apps.

Usage (from the azul repository, after building libazul with the debug server and AzMeet):

    python3 scripts/azmeet_e2e.py [--bin target/release/AzMeet]
        [--worker-dir ../azul-apps/cf-workers/meet] [--capped <run_capped.sh>]
        [--port-a 8781] [--port-b 8782] [--worker-port 8790] [--timeout 150]
        [--app-seconds 140] [--require-h264] [--skip rejoin,waiting,relay] [--out <dir>] [--keep-logs]
        [--phases direct,relay,crypto] [--relay-bin <iroh-relay>] [--relay-port 0] [--relay-metrics-port 0]
        [--relay-port-a 8793] [--relay-port-b 8794] [--relay-timeout 150] [--min-relayed-kib 256]
        [--crypto-ports 8795,8796,8797] [--crypto-timeout 300]
        [--worker-url <url> [--sqld-url <url> [--sqld-token-file <jwt>] | --db-file <sqlite>]]
        [--relay-url <url> [--relay-metrics-url <url>]]

With azul-apps' local stack (`local/up.sh`: the meet Worker under `wrangler dev` on 8790 over
the apps' sqld on 8082 with JWT auth, `iroh-relay --dev` on 3340 with its metrics on 3341):

    python3 scripts/azmeet_e2e.py --worker-url http://127.0.0.1:8790 \\
        --sqld-url http://127.0.0.1:8082 \\
        --sqld-token-file ../azul-apps/local/state/keys/apps-db/rw.jwt \\
        --relay-url http://127.0.0.1:3340 --relay-metrics-url http://127.0.0.1:3341/metrics

(the debug ports above stay clear of the stack's: 8080-8082, 8783, 8790, 3340-3341, 8799, 9000,
9090).

`--phases crypto` runs the crypto phase alone. The relay binary is built once, outside the
repository: `cd /tmp && cargo install iroh-relay@1.2.0 --locked --features server --root
~/.cache/azul/iroh-relay`. `AZMEET_BIN`, `AZMEET_WORKER_DIR`, `AZMEET_RELAY_BIN` and
`AZ_RUN_CAPPED` name the binary, the Worker, the relay and the capped runner too. Without a
capped runner the apps run uncapped and the script says so. The debug-server client is the
Azlin apps' shared one (`scripts/azlin_e2e.py`). A Worker under `wrangler dev` keeps its rate
limits (wrangler.toml [vars]): for repeated runs raise ROOMS_PER_WINDOW and
CODE_LOOKUPS_PER_WINDOW there, or the run's room creations meet a 429.
"""

import argparse
import base64
import datetime
import hashlib
import json
import os
import re
import shutil
import signal
import sqlite3
import stat
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

# The crypto phase's people: names with a space, so no base64 or hex value of the database can
# hold one by chance (a scan for "Ada" alone finds one in enough ciphertext).
CRYPTO_NAMES = ("Ada Lovelace", "Ben Okafor", "Cleo Martin")
SEALED = (
    "Only the members can read this, Ben",
    "Agreed: the server keeps nothing it can read",
    "Welcome Cleo, this one is under a key for three",
    "Ben has left, so a new key for the two of us",
)
# What the room view says of where the device stands (lib.rs `room_page`).
MEMBER_STATUS = "End-to-end encrypted: only the members read it."
KNOCK_STATUS = "Waiting for a member to let you in. Compare your safety code with theirs."
MEMBER_LINE = re.compile(
    r"^(?P<room>\S+) (?P<what>joined|left|knocking) (?P<name>.+) "
    r"(?P<code>\d{5} \d{5} \d{5} \d{5})$"
)
KEY_LINE = re.compile(
    r"^(?P<room>\S+) epoch=(?P<epoch>\d+) key=(?P<key>\S+) members=(?P<members>\d+) by=(?P<by>.+)$"
)
# The meeting server's rate limits for this run's own dev server (src/handler.js ENV_KEYS): every
# app of every phase comes from 127.0.0.1.
DEV_LIMITS = {
    "ROOMS_PER_WINDOW": "500",
    "CODE_LOOKUPS_PER_WINDOW": "500",
    "MEMBER_CHANGES_PER_WINDOW": "2000",
    "KEYS_PER_WINDOW": "2000",
    "MESSAGES_PER_WINDOW": "5000",
}


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


def start_worker(worker_dir, port, logs, db_path=None):
    """The meet Worker's dev server on `port`, its database the file `db_path` (read by the
    crypto phase; in memory without one: azmeet_cpu.py), with this run's rate limits
    (`DEV_LIMITS`)."""
    node = shutil.which("node") or "node"
    env = dict(os.environ)
    env.update(DEV_LIMITS)
    store = ["--db", db_path] if db_path else ["--memory"]
    return Process("worker", [node, os.path.join(worker_dir, "dev-server.mjs")] + store
                   + ["--port", str(port)], env, logs)


# ==== The meeting server's database: what an attacker who reads it sees ====


class SqliteFileDb:
    """The dev server's database file (node:sqlite, WAL), read with Python's sqlite3."""

    def __init__(self, path):
        self.path = path
        self.where = path

    def query(self, sql, params=()):
        con = sqlite3.connect(self.path, timeout=5)
        try:
            cur = con.execute(sql, params)
            cols = [c[0] for c in cur.description or []]
            return [dict(zip(cols, row)) for row in cur.fetchall()]
        finally:
            con.close()


class HranaDb:
    """A sqld (libSQL server) over HTTP - the Hrana v2 pipeline the Worker's own adapter speaks
    (src/db/libsql-http.js): what `wrangler dev` reads and writes."""

    def __init__(self, url, token=None):
        self.url = url.rstrip("/").replace("libsql://", "https://")
        self.token = token
        self.where = self.url

    @staticmethod
    def _arg(value):
        if value is None:
            return {"type": "null"}
        if isinstance(value, bool):
            return {"type": "integer", "value": "1" if value else "0"}
        if isinstance(value, int):
            return {"type": "integer", "value": str(value)}
        if isinstance(value, float):
            return {"type": "float", "value": value}
        return {"type": "text", "value": str(value)}

    @staticmethod
    def _value(v):
        kind = (v or {}).get("type")
        if kind == "integer":
            return int(v.get("value"))
        if kind == "float":
            return float(v.get("value"))
        if kind == "text":
            return v.get("value")
        if kind == "blob":
            return base64.b64decode(v.get("base64") or "")
        return None

    def query(self, sql, params=()):
        body = {"baton": None, "requests": [
            {"type": "execute", "stmt": {"sql": sql, "args": [self._arg(p) for p in params]}},
            {"type": "close"},
        ]}
        headers = {"content-type": "application/json"}
        if self.token:
            headers["authorization"] = "Bearer %s" % self.token
        request = urllib.request.Request(self.url + "/v2/pipeline", data=json.dumps(body).encode("utf-8"),
                                         headers=headers, method="POST")
        with urllib.request.urlopen(request, timeout=10) as response:
            answer = json.loads(response.read().decode("utf-8") or "{}")
        result = (answer.get("results") or [{}])[0]
        if result.get("type") != "ok":
            raise Failure("sqld at %s: %s (in %s)" % (self.url, json.dumps(result)[:200], sql[:80]))
        rows = result["response"]["result"]
        cols = [c.get("name") for c in rows.get("cols") or []]
        return [dict(zip(cols, [self._value(v) for v in row])) for row in rows.get("rows") or []]


BASE64_VALUE = re.compile(r"^[A-Za-z0-9+/_-]{16,}={0,2}$")
HEX_VALUE = re.compile(r"^(?:[0-9a-fA-F]{2}){8,}$")


def readable_forms(value):
    """The bytes a text value of the database could carry a plaintext in: the value itself, and
    what it decodes to as hex or base64 (standard or URL-safe)."""
    forms = [value.encode("utf-8")]
    if HEX_VALUE.match(value):
        try:
            forms.append(bytes.fromhex(value))
        except ValueError:
            pass
    if BASE64_VALUE.match(value):
        padded = value + "=" * (-len(value) % 4)
        for decode in (base64.b64decode, base64.urlsafe_b64decode):
            try:
                forms.append(decode(padded))
            except (ValueError, TypeError):
                pass
    return forms


def scan_database(db, needles):
    """Every value of every table of `db`, searched for each of `needles` ({label: bytes}) in
    every readable form. Returns (values scanned, [(table, column, label)] found)."""
    tables = [r["name"] for r in db.query(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")]
    scanned, found = 0, []
    for table in tables:
        for row in db.query('SELECT * FROM "%s"' % table.replace('"', '""')):
            for column, value in row.items():
                if isinstance(value, bytes):
                    forms = [value]
                elif isinstance(value, str):
                    forms = readable_forms(value)
                else:
                    continue
                scanned += 1
                for label, raw in needles.items():
                    if raw and any(raw in form for form in forms):
                        found.append((table, column, label))
    return scanned, found


def assert_nothing_readable(db, needles, what):
    """Fails when any of `needles` is in the database in any readable form."""
    scanned, found = scan_database(db, needles)
    if found:
        raise Failure("the meeting server's database holds what it must not (%s): %s"
                      % (what, "; ".join("%s.%s has %s" % f for f in found[:12])))
    log("database %s: %d values scanned, none holds %s" % (db.where, scanned, what))


def safety_code(device_hex, dh_hex):
    """A device's safety code from its public keys (crypto.rs `safety_code`, CRYPTO.md section
    3): SHA-256 of "azmeet/v1/safety\\n" and both keys, four groups of 5 digits."""
    digest = hashlib.sha256(b"azmeet/v1/safety\n" + bytes.fromhex(device_hex) + bytes.fromhex(dh_hex)).digest()
    return " ".join("%05d" % (int.from_bytes(digest[5 * i:5 * i + 5], "big") % 100000) for i in range(4))


# ==== The apps ====


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


def start_app(name, binary, port, flags, logs, args, capped, timeout=None, seconds=None):
    """One AzMeet with `flags` (`app_flags`), under `capped` when given (for `seconds`, else
    --app-seconds)."""
    return App(name, binary, flags, port, logs, timeout or args.timeout, extra_env=app_env(port),
               capped=capped, cap_mb=args.cap_mb, cap_seconds=seconds or args.app_seconds)


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


def identity_file(logs, tag):
    """Where the participant `tag` keeps its device key in this run (`--identity-file`)."""
    return os.path.join(logs, "%s.identity.json" % tag)


def start_pair(args, binary, worker, logs, capped, ports, prefix, relay, extra, data, waiting,
               timeout, procs, deadline):
    """Ada (`--autocreate`) and, once her link is out, Ben (`--join <link>`, `--waiting-room`
    when `waiting`), both with `relay` / `extra` switches and their own data tree and identity
    file; both up, sized. Returns (ada, ben, link)."""
    ada_extra = list(extra) + ["--autocreate", "--data-dir", data[0],
                               "--identity-file", identity_file(logs, prefix + "ada")]
    ada = start_app(prefix + "ada", binary, ports[0], app_flags(worker, "Ada", relay, ada_extra),
                    logs, args, capped, timeout)
    procs.append(ada)
    link = until("Ada's meeting link (AZMEET_LINK)", lambda: (ada.printed("AZMEET_LINK") or [None])[0],
                 deadline, procs)
    log("Ada created %s" % link)
    ben_extra = list(extra) + ["--join", link, "--data-dir", data[1],
                               "--identity-file", identity_file(logs, prefix + "ben")]
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


def check_announcements(db, ada, names):
    """The room's iroh announcements in the database are signed by a device and carry none of
    `names` (an announcement names nobody: the members' names are sealed, CRYPTO.md 10)."""
    if db is None:
        return
    room = (ada.printed("AZMEET_ROOM") or [None])[-1]
    if not room:
        return
    rows = db.query("SELECT name, device, sig FROM peer WHERE room_id = ?", (room,))
    named = [r["name"] for r in rows if r.get("name") in names]
    unsigned = [r for r in rows if not r.get("device") or not r.get("sig")]
    if named or unsigned:
        raise Failure("the room's announcements in the database: names %s, unsigned %d of %d"
                      % (named, len(unsigned), len(rows)))
    log("database: the room's %d announcement(s) are signed and name nobody (%s)"
        % (len(rows), ", ".join(sorted(set(str(r.get("name")) for r in rows))) or "none"))


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
    if "#" not in link:
        raise Failure("Ada's link %r carries no invite secret (#...)" % link)
    if "waiting" in skip:
        log("waiting room skipped (--skip waiting): Ben went straight in")
    else:
        waiting_room(ben, args.width, args.height, deadline, procs)

    see_each_other(ada, ben, args, deadline, procs)
    check_announcements(args.db, ada, ("Ada", "Ben"))

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
                        app_flags(worker, "Ada", "off", ["--join", link, "--data-dir", data[0],
                                                         "--identity-file", identity_file(logs, "ada")]),
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


class ExternalRelay:
    """A relay this script did not start (--relay-url: the lead's `iroh-relay --dev`), with its
    metrics when --relay-metrics-url names them."""

    def __init__(self, url, metrics_url=None):
        self.tag = "iroh-relay (external)"
        self.url = url.rstrip("/")
        self.metrics_url = metrics_url
        self.version = None

    def start(self, deadline):
        last = None
        while time.time() < deadline:
            try:
                health = http_json(self.url + "/healthz", timeout=2)
                if health.get("status") == "ok":
                    self.version = health.get("version")
                    return self
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            try:
                # The probe path every relay answers (local/relay.sh waits for it).
                with urllib.request.urlopen(self.url + "/ping", timeout=2) as response:
                    if response.status == 200:
                        return self
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(0.25)
        raise Failure("the relay at %s does not answer /healthz or /ping (last error: %s)" % (self.url, last))

    def metrics(self):
        if not self.metrics_url:
            return None
        with urllib.request.urlopen(self.metrics_url, timeout=3) as response:
            return iroh_relay_dev.parse_metrics(response.read().decode("utf-8", errors="replace"))

    def relayed(self, since=None):
        now = self.metrics()
        if now is None:
            return None
        since = since or {}
        return tuple(iroh_relay_dev.metric(now, stem) - iroh_relay_dev.metric(since, stem)
                     for stem in ("bytes_recv", "bytes_sent", "accepts"))

    def alive(self):
        return True

    def tail(self, lines=30):
        return "(not started by this script)"

    def stop(self):
        pass


def relay_phase(args, binary, worker, logs, out, capped, skip, procs):
    """The call again through a local iroh relay with `--relay-only`, and the proof that the
    relay carried it (module docs, phase `relay`). False when skipped (no relay binary)."""
    deadline = time.time() + args.relay_timeout
    if args.relay_url:
        relay = ExternalRelay(args.relay_url, args.relay_metrics_url)
        relay.start(min(deadline, time.time() + 30))
    else:
        relay_bin = iroh_relay_dev.find_binary(args.relay_bin)
        if not relay_bin:
            log("relay phase SKIPPED: no iroh-relay binary (--relay-bin, AZMEET_RELAY_BIN, %s, PATH) "
                "and no --relay-url. Build it once, outside the repository:\n    %s"
                % (iroh_relay_dev.DEFAULT_ROOT, iroh_relay_dev.BUILD_COMMAND))
            return False
        relay = iroh_relay_dev.DevRelay(relay_bin, logs, args.relay_port or None, args.relay_metrics_port or None)
        procs.append(relay)
        try:
            relay.start(min(deadline, time.time() + 30))
        except iroh_relay_dev.RelayError as e:
            raise Failure(str(e))
    log("relay phase: iroh-relay %s at %s (metrics %s)" % (relay.version, relay.url, relay.metrics_url))
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
    # The relay's own counters: both clients connected, and the call's bytes went in and out. Two
    # 600 kbit/s cameras pass the floor within seconds; a call that just began may not have yet, so
    # the counters are read until they do (or the phase's deadline passes).
    floor = args.min_relayed_kib * 1024
    counted = relay.relayed(before)
    while (counted is not None and time.time() < deadline
           and (counted[0] < floor or counted[1] < floor)):
        time.sleep(1)
        counted = relay.relayed(before)
    if counted is None:
        log("relay phase: no metrics for the relay at %s (--relay-metrics-url): its byte counts "
            "are not checked; the paths above say relayed" % relay.url)
    else:
        got, passed_on, accepts = counted
        log("relay phase: the relay took %.0f KiB from the clients and passed %.0f KiB on, %d connections"
            % (got / 1024, passed_on / 1024, accepts))
        if accepts < 2:
            raise Failure("the relay accepted %d client connections, not both clients" % accepts)
        if got < floor or passed_on < floor:
            raise Failure("the relay carried %.0f KiB in and %.0f KiB out: less than --min-relayed-kib %d each "
                          "(the video did not go through it)" % (got / 1024, passed_on / 1024, args.min_relayed_kib))

    for app in (ada, ben):
        app.screenshot(os.path.join(out, "azmeet-%s.png" % app.tag))
    stop(procs, ada, ben, relay)
    log("relay phase passed: every packet of the call went through %s" % relay.url)
    return True


# ==== Phase `crypto`: the encrypted chat room of three ====


def identity_of(app, deadline, procs):
    """(device, where its seed lives, safety code) as `app` printed them at start."""
    line = until("%s's AZMEET_IDENTITY" % app.tag, lambda: (app.printed("AZMEET_IDENTITY") or [None])[-1],
                 deadline, procs)
    device, _, source = line.partition(" ")
    code = until("%s's AZMEET_SAFETY" % app.tag, lambda: (app.printed("AZMEET_SAFETY") or [None])[-1],
                 deadline, procs)
    return device, source.strip(), code.strip()


def member_events(app, room):
    """`app`'s `AZMEET_MEMBER` lines for `room`: (joined|left|knocking, name, safety code)."""
    found = []
    for value in app.printed("AZMEET_MEMBER"):
        m = MEMBER_LINE.match(value)
        if m and m.group("room") == room:
            found.append((m.group("what"), m.group("name"), m.group("code")))
    return found


def wait_member(app, room, what, name, deadline, procs):
    """The safety code `app` shows of `name` once it printed `AZMEET_MEMBER <room> <what> <name>`."""
    def seen():
        for w, n, code in member_events(app, room):
            if w == what and n == name:
                return code
        return None
    code = until("%s seeing %s %s" % (app.tag, name, what), seen, deadline, procs)
    log("%s: %s %s (safety code %s)" % (app.tag, name, what, code))
    return code


def keys_of(app, room):
    """`app`'s `AZMEET_KEY` lines for `room`, as dicts (epoch, key, members, by)."""
    found = []
    for value in app.printed("AZMEET_KEY"):
        m = KEY_LINE.match(value)
        if m and m.group("room") == room:
            found.append(m.groupdict())
    return found


def wait_key(app, room, epoch, deadline, procs, members=None, mine=None, key=None):
    """`app`'s room key of `epoch` (with `members`, made here or not, that key id) once printed."""
    def seen():
        for k in keys_of(app, room):
            if int(k["epoch"]) != epoch or (key and k["key"] != key):
                continue
            if members is not None and int(k["members"]) != members:
                continue
            if mine is not None and (k["by"] == "me") != mine:
                continue
            return k
        return None
    found = until("%s's room key %d%s" % (app.tag, epoch, " (made there)" if mine else ""), seen, deadline, procs)
    log("%s: AZMEET_KEY epoch=%s key=%s members=%s by=%s"
        % (app.tag, found["epoch"], found["key"], found["members"], found["by"]))
    return found


def wait_chat(app, name, text, deadline, procs):
    until("%s printing AZMEET_CHAT %s: %s" % (app.tag, name, text),
          lambda: "%s: %s" % (name, text) in app.printed("AZMEET_CHAT"), deadline, procs)
    log("%s read %s: %r" % (app.tag, name, text))


def room_send(app, text, use_enter):
    """`app` writes `text` in the room view's chat: Enter, or the Send button."""
    app.click(selector="#__azmeet_chat_field")
    app.frame()
    app.must("text_input", text=text)
    app.frame()
    if use_enter:
        app.must("key_down", key="Return")
        app.must("key_up", key="Return")
    else:
        app.click(selector="#__azmeet_chat_send")
    app.frame()


def seed_of(path):
    """The device seed an identity file holds (base64 in `seed`): a needle for the database."""
    with open(path, encoding="utf-8") as f:
        stored = json.load(f)
    return base64.b64decode(stored["seed"] + "=" * (-len(stored["seed"]) % 4))


def rooms_index(data):
    """`meet/rooms.json` of a data tree, as JSON, or None."""
    try:
        with open(os.path.join(data, "meet", "rooms.json"), encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def envelopes_of(db, room, key):
    return {r["recipient"] for r in db.query(
        "SELECT recipient FROM key_envelope WHERE room_id = ? AND key_id = ?", (room, key))}


def utc_iso(moment):
    return moment.astimezone(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def crypto_phase(args, binary, worker, logs, out, capped, procs):
    """Three devices in one encrypted chat room, a knock, a leave and its new key, the history
    after a restart, a meeting with times - and what the meeting server's database holds of it
    (module docs, phase `crypto`)."""
    db = args.db
    deadline = time.time() + args.crypto_timeout
    ada_name, ben_name, cleo_name = CRYPTO_NAMES
    ports = [int(p) for p in args.crypto_ports.split(",")]
    if len(ports) < 3:
        raise Failure("--crypto-ports names %d ports, the crypto phase needs 3" % len(ports))
    who = {}

    def start(tag, name, port, extra, files=None):
        files = files or tag
        data = os.path.join(logs, "crypto-data-%s" % files)
        identity = os.path.join(logs, "crypto-%s.identity.json" % files)
        flags = app_flags(worker, name, "off", ["--identity-file", identity, "--data-dir", data] + list(extra))
        app = start_app("crypto-%s" % tag, binary, port, flags, logs, args, capped,
                        args.crypto_timeout, seconds=args.crypto_timeout)
        procs.append(app)
        until("%s's debug server" % app.tag, lambda: app.op("wait_frame") is not None, deadline, procs)
        app.must("resize", width=args.width, height=args.height)
        app.frame()
        device, source, code = identity_of(app, deadline, procs)
        if source != "file":
            raise Failure("%s's device key is not in its identity file: %r" % (app.tag, source))
        mode = stat.S_IMODE(os.stat(identity).st_mode)
        if mode != 0o600:
            raise Failure("%s's identity file is mode %o, not 600" % (app.tag, mode))
        who[files] = {"device": device, "code": code, "data": data, "identity": identity}
        log("%s: device %s..., safety code %s (identity file, mode 600)" % (app.tag, device[:16], code))
        return app

    def room_view(app):
        until("%s's room view" % app.tag, lambda: app.has("#__azmeet_room_view"), deadline, procs)

    # 1. Ada makes a chat room.
    ada = start("ada", ada_name, ports[0], ["--autocreate", "--chat-room"])
    link = until("Ada's room link (AZMEET_LINK)", lambda: (ada.printed("AZMEET_LINK") or [None])[0],
                 deadline, procs)
    if "#" not in link:
        raise Failure("the room's link %r carries no invite secret (#...)" % link)
    bare, secret = link.split("#", 1)
    room = bare.rstrip("/").rsplit("/", 1)[-1]
    code = until("Ada's room code (AZMEET_CODE)", lambda: (ada.printed("AZMEET_CODE") or [None])[0],
                 deadline, procs)
    until("Ada's AZMEET_OPEN", lambda: room in ada.printed("AZMEET_OPEN"), deadline, procs)
    room_view(ada)
    until("Ada's room view showing her safety code",
          lambda: who["ada"]["code"] in ada.texts() and MEMBER_STATUS in ada.texts(), deadline, procs)
    log("Ada made chat room %s (code %s); she is a member, her view shows her safety code" % (room, code))

    # 2. Ben opens the link.
    ben = start("ben", ben_name, ports[1], ["--open", link])
    room_view(ben)
    until("Ben a member", lambda: MEMBER_STATUS in ben.texts(), deadline, procs)
    ben_at_ada = wait_member(ada, room, "joined", ben_name, deadline, procs)
    ada_at_ben = wait_member(ben, room, "joined", ada_name, deadline, procs)
    if ben_at_ada != who["ben"]["code"] or ada_at_ben != who["ada"]["code"]:
        raise Failure("the safety codes do not match: Ada sees Ben's as %s (he printed %s), Ben sees "
                      "Ada's as %s (she printed %s)" % (ben_at_ada, who["ben"]["code"], ada_at_ben,
                                                       who["ada"]["code"]))

    # 3. Ada writes (Enter): key 1 for the two; Ben reads it. Ben answers with Send.
    room_send(ada, SEALED[0], True)
    key1 = wait_key(ada, room, 1, deadline, procs, members=2, mine=True)["key"]
    wait_key(ben, room, 1, deadline, procs, key=key1)
    wait_chat(ben, ada_name, SEALED[0], deadline, procs)
    until("Ben's room view showing Ada's message", lambda: SEALED[0] in ben.texts(), deadline, procs)
    room_send(ben, SEALED[1], False)
    wait_chat(ada, ben_name, SEALED[1], deadline, procs)
    if any(k["by"] == "me" for k in keys_of(ben, room)):
        raise Failure("Ben made a room key although key 1 holds the same members")
    if db is not None:
        def two_sealed():
            rows = db.query("SELECT key_id FROM message WHERE room_id = ? ORDER BY seq", (room,))
            return rows if len(rows) >= 2 else None
        rows = until("the two messages in the database", two_sealed, deadline, procs)
        if [r["key_id"] for r in rows] != [key1, key1]:
            raise Failure("the database's messages are under keys %s, not key 1 twice" % [r["key_id"] for r in rows])
        if envelopes_of(db, room, key1) != {who["ada"]["device"], who["ben"]["device"]}:
            raise Failure("key 1 is sealed to %s, not to Ada and Ben" % envelopes_of(db, room, key1))

    # 4. Cleo has only the code: she knocks; Ada compares and admits; key 2 for the three.
    cleo = start("cleo", cleo_name, ports[2], ["--open", code])
    room_view(cleo)
    until("Cleo knocking", lambda: KNOCK_STATUS in cleo.texts(), deadline, procs)
    knock = wait_member(ada, room, "knocking", cleo_name, deadline, procs)
    if knock != who["cleo"]["code"]:
        raise Failure("Ada sees Cleo's knock with safety code %s, Cleo printed %s" % (knock, who["cleo"]["code"]))
    admit = "#__azmeet_admit_" + who["cleo"]["device"][:16]
    until("Ada's Admit button for Cleo", lambda: ada.has(admit), deadline, procs)
    ada.click(selector=admit)
    until("Ada admitting Cleo", lambda: "%s %s" % (room, who["cleo"]["device"]) in ada.printed("AZMEET_ADMITTING"),
          deadline, procs)
    until("Cleo let in (AZMEET_ADMITTED)", lambda: room in cleo.printed("AZMEET_ADMITTED"), deadline, procs)
    key2 = wait_key(ada, room, 2, deadline, procs, members=3, mine=True)["key"]
    wait_key(cleo, room, 2, deadline, procs, key=key2)
    wait_key(ben, room, 2, deadline, procs, key=key2)
    until("Cleo's view naming Ada and Ben (their sealed names open for her now)",
          lambda: ada_name in cleo.texts() and ben_name in cleo.texts() and MEMBER_STATUS in cleo.texts(),
          deadline, procs)
    until("Cleo's view saying the two earlier messages were sealed before she joined",
          lambda: any(t.startswith("2 sealed before you joined") for t in cleo.texts()), deadline, procs)
    for text in SEALED[:2]:
        if text in cleo.texts() or any(text in line for line in cleo.printed("AZMEET_CHAT")):
            raise Failure("Cleo read %r, sealed before she joined" % text)
    log("Cleo was let in: key 2 for the three; the two earlier messages stay sealed for her")

    # 5. Ada writes again; Ben and Cleo read it. Ben marks Ada verified.
    room_send(ada, SEALED[2], True)
    wait_chat(ben, ada_name, SEALED[2], deadline, procs)
    wait_chat(cleo, ada_name, SEALED[2], deadline, procs)
    verify = "#__azmeet_verify_" + who["ada"]["device"][:16]
    until("Ben's Mark verified button for Ada", lambda: ben.has(verify), deadline, procs)
    ben.click(selector=verify)
    until("Ben's AZMEET_VERIFIED", lambda: who["ada"]["device"] in ben.printed("AZMEET_VERIFIED"),
          deadline, procs)
    until("the verified badge in Ben's view", lambda: "verified" in ben.texts(), deadline, procs)
    until("Ben's rooms.json keeping Ada verified",
          lambda: who["ada"]["device"] in ((rooms_index(who["ben"]["data"]) or {}).get("verified") or {}),
          deadline, procs)
    log("Ben marked Ada verified (his rooms.json keeps it)")

    # 6. Ben leaves the room: the next message is under key 3 for the two who stay.
    ben.click(selector="#__azmeet_room_leave")
    until("Ben's AZMEET_LEFT_ROOM", lambda: room in ben.printed("AZMEET_LEFT_ROOM"), deadline, procs)
    wait_member(ada, room, "left", ben_name, deadline, procs)
    wait_member(cleo, room, "left", ben_name, deadline, procs)
    room_send(ada, SEALED[3], True)
    key3 = wait_key(ada, room, 3, deadline, procs, members=2, mine=True)["key"]
    wait_key(cleo, room, 3, deadline, procs, key=key3)
    wait_chat(cleo, ada_name, SEALED[3], deadline, procs)
    time.sleep(3)
    if any(SEALED[3] in line for line in ben.printed("AZMEET_CHAT")) or keys_of(ben, room)[-1]["key"] == key3:
        raise Failure("Ben read the message under key 3 after he left")
    log("Ben left: key 3 for Ada and Cleo; Cleo reads the next message, Ben does not")
    ada.screenshot(os.path.join(out, "azmeet-crypto-ada-room.png"))

    # 7. What the meeting server's database holds.
    seeds = {tag: seed_of(person["identity"]) for tag, person in who.items()}
    if db is None:
        log("crypto phase: no database to read (--worker-url without --sqld-url / --db-file): "
            "its checks are skipped")
    else:
        rows = db.query("SELECT key_id FROM message WHERE room_id = ? ORDER BY seq", (room,))
        if [r["key_id"] for r in rows] != [key1, key1, key2, key3]:
            raise Failure("the database's messages are under keys %s, not 1, 1, 2, 3 (%s)"
                          % ([r["key_id"] for r in rows], [key1, key2, key3]))
        devices = {tag: who[tag]["device"] for tag in ("ada", "ben", "cleo")}
        expected = {key1: {"ada", "ben"}, key2: {"ada", "ben", "cleo"}, key3: {"ada", "cleo"}}
        for key, tags in expected.items():
            sealed_to = envelopes_of(db, room, key)
            if sealed_to != {devices[t] for t in tags}:
                raise Failure("key %s is sealed to %s, not to %s" % (key, sorted(sealed_to), sorted(tags)))
        # The key's signed member list: the devices, comma-separated (src/store.js putKey).
        stored = db.query("SELECT members FROM room_key WHERE room_id = ? AND key_id = ?",
                          (room, key3))[0]["members"]
        listed = json.loads(stored) if stored.startswith("[") else stored.split(",")
        if sorted(listed) != sorted([devices["ada"], devices["cleo"]]):
            raise Failure("key 3's signed member list is %s, not Ada and Cleo" % listed)

        def records():
            found = {r["device"]: r for r in db.query(
                "SELECT device, dh, name, sealed_name, state FROM member WHERE room_id = ?", (room,))}
            cleo_row = found.get(devices["cleo"]) or {}
            # Cleo's knock named her in clear; once in, her record is sealed like the others'.
            return found if cleo_row.get("name") in (None, "") and cleo_row.get("sealed_name") else None
        found = until("Cleo's record sealed in the database", records, deadline, procs)
        if (found.get(devices["ben"]) or {}).get("state") != "left":
            raise Failure("Ben's record in the database says %r, not left" % (found.get(devices["ben"]) or {}).get("state"))
        for tag in ("ada", "ben", "cleo"):
            row = found.get(devices[tag]) or {}
            if row.get("name"):
                raise Failure("%s's record keeps a name in clear: %r" % (tag, row.get("name")))
            computed = safety_code(row["device"], row["dh"])
            if computed != who[tag]["code"]:
                raise Failure("%s's safety code from the database's keys is %s, the app printed %s"
                              % (tag, computed, who[tag]["code"]))
        log("database: messages under keys 1, 1, 2, 3; key 3 sealed to Ada and Cleo only; Ben's "
            "record left; every name sealed; every safety code matches the public keys")
        needles = {"message %d" % (i + 1): text.encode("utf-8") for i, text in enumerate(SEALED)}
        needles.update({"the name %r" % n: n.encode("utf-8") for n in CRYPTO_NAMES})
        needles["the invite secret"] = secret.encode("utf-8")
        needles.update({"%s's device seed" % tag: seed for tag, seed in seeds.items()})
        assert_nothing_readable(db, needles, "a message, a name, the invite secret or a device seed")
    # Each device's own rooms.json keeps the invite secret sealed with its key, never in clear.
    for tag in ("ada", "cleo"):
        with open(os.path.join(who[tag]["data"], "meet", "rooms.json"), encoding="utf-8") as f:
            index_text = f.read()
        if secret in index_text or room not in index_text:
            raise Failure("%s's rooms.json %s" % (tag, "holds the invite secret in clear" if secret in index_text
                                                   else "does not list the room"))
    log("each device's rooms.json lists the room with its invite secret sealed")

    # 8. Cleo restarts on the same identity file and data tree: the history from the ciphertext.
    stop(procs, cleo, ben)
    cleo_device = who["cleo"]["device"]
    cleo = start("cleo-again", cleo_name, ports[2], [], files="cleo")
    if who["cleo"]["device"] != cleo_device:
        raise Failure("Cleo came back as another device (%s, was %s)" % (who["cleo"]["device"], cleo_device))
    button = "#__azmeet_room_" + room
    until("the room in Cleo's Your rooms", lambda: cleo.has(button), deadline, procs)
    cleo.click(selector=button)
    room_view(cleo)
    history = until("Cleo's AZMEET_HISTORY",
                    lambda: [v for v in cleo.printed("AZMEET_HISTORY") if v.startswith(room + " ")], deadline, procs)
    until("Cleo's view with the two messages sealed to her",
          lambda: SEALED[2] in cleo.texts() and SEALED[3] in cleo.texts(), deadline, procs)
    if SEALED[0] in cleo.texts() or SEALED[1] in cleo.texts():
        raise Failure("Cleo's history shows a message sealed before she joined")
    log("Cleo is back as the same device: %s; her history (%s) holds the two messages sealed to her"
        % (cleo_device[:16], history[-1]))
    cleo.screenshot(os.path.join(out, "azmeet-crypto-cleo-room.png"))

    # 9. A meeting with times: Ada's "Schedule"; Cleo joins its link from the start screen.
    ada.click(selector="#__azmeet_room_back")
    until("Ada's start screen", lambda: ada.has("#__azmeet_schedule_start"), deadline, procs)
    # Two hours from now, this computer's time (the offset of that hour, summer time or not); the
    # minutes field starts at 60 (typing into it would add to that).
    local = (datetime.datetime.now() + datetime.timedelta(hours=2)).replace(second=0, microsecond=0)
    starts = local.astimezone()
    ends = starts + datetime.timedelta(minutes=60)
    ada.text_input("#__azmeet_schedule_start", local.strftime("%Y-%m-%d %H:%M"))
    ada.click(selector="#__azmeet_schedule")
    meeting = until("Ada's scheduled meeting (AZMEET_WAITING)", lambda: (ada.printed("AZMEET_WAITING") or [None])[-1],
                    deadline, procs)
    times = "%s %s" % (utc_iso(starts), utc_iso(ends))
    until("Ada's AZMEET_TIMES %s" % times, lambda: times in ada.printed("AZMEET_TIMES"), deadline, procs)
    until("the times in Ada's waiting room",
          lambda: any("starts in" in t for t in ada.texts_within("#__azmeet_meeting_times")), deadline, procs)
    cleo.click(selector="#__azmeet_room_back")
    until("Cleo's start screen", lambda: cleo.has("#__azmeet_join_field"), deadline, procs)
    cleo.text_input("#__azmeet_join_field", meeting)
    cleo.click(selector="#__azmeet_join")
    until("Cleo in the scheduled meeting's waiting room", lambda: meeting in cleo.printed("AZMEET_WAITING"),
          deadline, procs)
    until("Cleo's AZMEET_TIMES %s" % times, lambda: times in cleo.printed("AZMEET_TIMES"), deadline, procs)
    until("the times in Cleo's waiting room",
          lambda: any("starts in" in t for t in cleo.texts_within("#__azmeet_meeting_times")), deadline, procs)
    if db is not None:
        meeting_room = meeting.split("#", 1)[0].rstrip("/").rsplit("/", 1)[-1]
        row = (db.query("SELECT kind, starts_at, ends_at FROM room WHERE id = ?", (meeting_room,)) or [{}])[0]
        start_s, end_s = int(starts.timestamp()), int(ends.timestamp())
        if row.get("kind") != "meeting" or row.get("starts_at") not in (start_s, start_s * 1000) \
                or row.get("ends_at") not in (end_s, end_s * 1000):
            raise Failure("the scheduled meeting in the database is %s, not a meeting at %s" % (row, times))
    log("a meeting with times: %s - Ada's waiting room and Cleo's (from the meeting server) show them" % times)

    for app in (ada, cleo):
        app.screenshot(os.path.join(out, "azmeet-%s.png" % app.tag))
    stop(procs, ada, cleo)
    log("crypto phase passed")


PHASES = ("direct", "relay", "crypto")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--worker-dir")
    parser.add_argument("--capped", default=os.environ.get("AZ_RUN_CAPPED", DEFAULT_CAPPED))
    parser.add_argument("--port-a", type=int, default=8781)
    parser.add_argument("--port-b", type=int, default=8782)
    parser.add_argument("--worker-port", type=int, default=8790)
    parser.add_argument("--worker-url", help="a meet Worker already running (wrangler dev): no dev server is started")
    parser.add_argument("--sqld-url", help="the sqld behind --worker-url, read over HTTP (Hrana v2)")
    parser.add_argument("--sqld-token-file", help="a file with its token (a JWT), when it wants one")
    parser.add_argument("--db-file", help="the SQLite file behind --worker-url (a dev-server.mjs --db)")
    parser.add_argument("--timeout", type=int, default=150, help="seconds for the direct phase")
    parser.add_argument("--app-seconds", type=int, default=140)
    parser.add_argument("--cap-mb", type=int, default=1000)
    parser.add_argument("--width", type=int, default=1100)
    parser.add_argument("--height", type=int, default=720)
    parser.add_argument("--require-h264", action="store_true")
    parser.add_argument("--skip", help="stages to leave out, comma-separated (rejoin, waiting, relay, crypto)")
    parser.add_argument("--phases", default=",".join(PHASES), help="the phases to run, comma-separated")
    parser.add_argument("--relay-bin", help="the iroh-relay binary (else AZMEET_RELAY_BIN, ...)")
    parser.add_argument("--relay-url", help="an iroh relay already running (iroh-relay --dev): none is started")
    parser.add_argument("--relay-metrics-url", help="its metrics page (the relayed bytes are checked with it)")
    parser.add_argument("--relay-port", type=int, default=0, help="the relay's HTTP port (0: a free one)")
    parser.add_argument("--relay-metrics-port", type=int, default=0, help="its metrics port (0: a free one)")
    parser.add_argument("--relay-port-a", type=int, default=8793, help="Ada's debug port in the relay phase")
    parser.add_argument("--relay-port-b", type=int, default=8794, help="Ben's debug port in the relay phase")
    parser.add_argument("--relay-timeout", type=int, default=150, help="seconds for the relay phase")
    parser.add_argument("--min-relayed-kib", type=int, default=256,
                        help="what the relay must carry each way for the phase to pass")
    parser.add_argument("--crypto-ports", default="8795,8796,8797",
                        help="Ada's, Ben's and Cleo's debug ports in the crypto phase")
    parser.add_argument("--crypto-timeout", type=int, default=300, help="seconds for the crypto phase")
    parser.add_argument("--out")
    parser.add_argument("--keep-logs", action="store_true")
    args = parser.parse_args()

    logs = tempfile.mkdtemp(prefix="azmeet-e2e-")
    out = args.out or logs
    os.makedirs(out, exist_ok=True)
    skip = set(s.strip() for s in (args.skip or "").split(",") if s.strip())
    phases = [p.strip() for p in args.phases.split(",") if p.strip() and p.strip() not in skip]
    unknown = [p for p in phases if p not in PHASES]
    if unknown:
        parser.error("unknown phase(s) %s: %s" % (", ".join(unknown), ", ".join(PHASES)))
    procs = []
    passed = False
    try:
        binary = find_binary(args.bin)
        capped = args.capped if args.capped and os.access(args.capped, os.X_OK) else None
        if capped:
            log("apps run under %s: %d MB and %d s each (two at once)" % (capped, args.cap_mb, args.app_seconds))
        else:
            log("WARNING: no capped runner (--capped / AZ_RUN_CAPPED): the apps run uncapped")

        if args.worker_url:
            worker = args.worker_url.rstrip("/")
            if args.sqld_url:
                token = None
                if args.sqld_token_file:
                    with open(args.sqld_token_file, encoding="utf-8") as f:
                        token = f.read().strip()
                args.db = HranaDb(args.sqld_url, token)
            elif args.db_file:
                args.db = SqliteFileDb(args.db_file)
            else:
                args.db = None
            until("the meet Worker at %s" % worker, lambda: http_json(worker + "/health").get("ok") is True,
                  time.time() + 30, procs)
            log("meet Worker at %s (database: %s)" % (worker, args.db.where if args.db else "not read"))
        else:
            worker_dir = find_worker(args.worker_dir)
            worker = "http://127.0.0.1:%d" % args.worker_port
            db_path = os.path.join(logs, "meet-dev.sqlite")
            dev = start_worker(worker_dir, args.worker_port, logs, db_path)
            procs.append(dev)
            until("the dev server", lambda: http_json(worker + "/health").get("ok") is True,
                  time.time() + 30, procs)
            args.db = SqliteFileDb(db_path)
            log("dev server up on %s (database %s)" % (worker, db_path))

        ran = []
        if "direct" in phases:
            direct_phase(args, binary, worker, logs, out, capped, skip, procs)
            ran.append("direct")
        if "relay" in phases:
            ran.append("relay" if relay_phase(args, binary, worker, logs, out, capped, skip, procs)
                       else "relay SKIPPED")
        if "crypto" in phases:
            crypto_phase(args, binary, worker, logs, out, capped, procs)
            ran.append("crypto")
        if args.db is not None:
            # Every message of every phase went through the meeting server: none in clear.
            texts = []
            if "direct" in ran:
                texts += [MESSAGE, ANSWER] + ([] if "rejoin" in skip else [AGAIN])
            if "relay" in ran:
                texts += [RELAYED_MESSAGE, RELAYED_ANSWER]
            if texts:
                assert_nothing_readable(args.db, {"%r" % t: t.encode("utf-8") for t in texts},
                                        "a message of the calls' chats")
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
