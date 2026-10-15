#!/usr/bin/env python3
"""AzDrive's sync under hostile conditions, end to end and headless: the client side of the chaos
tests. Each scenario starts the mock Azlin stack (scripts/azlin_mock_stack.py: the token server
and the S3), the fault proxy in front of the S3 (scripts/azchaos.py: every bundle names the proxy
as the drive's node), AzDrive (the laptop: the app under test, its folder paired through its view
settings) and, where a second device is needed, the `azcloud` CLI (the desktop: the same sync
engine, its own token family). Then it does to them what real life does and checks two things:

- the EXPECTED OUTCOME of the scenario (below), and
- NO SILENT FAILURE: when AzDrive's status line says "Up to date", every file of the folder is on
  the drive with its content and every file of the drive is here (or cloud only, and AzDrive says
  so); when it does not, it says a plain state (paused, the drive is full, this device was
  removed, ...) - and nothing is lost: every version a device wrote is still a file somewhere
  (its name, or a conflict copy), and the drive holds no duplicate objects.

Scenarios (`--only NAME,...`, `--list`):

  flap-upload       the network goes down and up every few seconds while files and a 120 MiB
                    file go up (a lost request, a lost answer of a write that landed): every file
                    arrives once, the big one resumes per part (no part sent more than twice), no
                    unfinished upload stays, the status says it waits while it is down
  sleep             the laptop sleeps mid-upload (SIGSTOP while a part is held) longer than its
                    credentials live; it wakes: the session is refreshed, the upload ends, nothing
                    is sent twice
  clock-skew        the node's clock is 20 minutes ahead (RequestTimeTooSkewed): the status says the
                    clock is off - not "Up to date", not a raw error; the clock fixed, it syncs. A
                    file dated in the future (another clock) does not make every pass wait
  offline-conflict  both devices edit the same file while the laptop is offline: AzDrive asks
                    (Someone changed this file), Keep both: both versions on both devices
  delete-vs-edit    a delete on one device and an edit on the other, both ways: the edit wins,
                    nothing is lost
  rename-storm      100 renames and folder moves while a pass runs: the drive ends with the new
                    names, no blob is uploaded again (a rename moves no bytes), nothing is lost
  names             NFC and NFD spellings of one name, and names that differ only in case, from a
                    case-sensitive device, on this case-insensitive disk; a case-only rename here:
                    no twin silently shows the other's bytes, nothing loops "changed during the
                    run", nothing is deleted
  long-names        a 255-byte name, a 1,000-character path, 60 folders deep, names with
                    spaces / emoji / reserved characters: they sync or are named as not syncable
  changed-during    a 120 MiB file rewritten while its upload runs: never a torn upload (the blob
                    named by the index is the bytes of one version), the next pass sends the new one
  special-files     a symbolic link, a FIFO, a file without read permission and a folder without
                    one beside ordinary files: the ordinary ones sync, the others are named and the
                    status does not say "Up to date" about them
  disk-full         the folder lies on a 64 MiB disk image; the drive holds a 100 MiB file and small
                    ones: the small ones come down, the big one fails plainly, no partial file,
                    nothing is deleted on the drive
  quota             the drive's quota is reached mid-sync: "Your drive is full"-like status, the
                    files that fit are up, nothing lost here; quota raised: the rest goes up
  revoked           this device is removed at the token server mid-sync: the status says so, no
                    file here is deleted, nothing more goes up
  rotation          (encrypted) another device rotates the drive key ("I was hacked") during a
                    pass: AzDrive does not upload under the old key, says what it needs
  ransomware        300 files rewritten with random bytes while a big upload runs under latency:
                    the guard pauses the uploads, the drive keeps the old versions
  many-small        (encrypted, packing) 100,000 small files: one pass, a handful of packs, memory
                    and time stated
  fifty-gb          a 50 GB file (sparse with random blocks; the proxy's sink keeps only hashes):
                    streamed, memory bounded, resumed after a kill, the hash matches
  kill-every-step   AzDrive is killed (SIGKILL) at every step of a pass - the index read, before
                    and after each blob, before and after the index write, during the downloads -
                    and started again: the drive and the folder converge, nothing is lost or
                    uploaded twice
  encrypted-bulk    (encrypted, PACK22) unzip 10,000 files, mv the folder (no data/ write), rm -rf
                    it (one commit, no DeleteObject per file), rewrite 300 files (the guard)
  dns               the drive's node name never resolves (`.invalid`) and the node list carries
                    its address: syncs
  slow-dns          the node's name answers after 8 s: passes do not wait for it every time
                    (needs /etc/resolver/azchaos.test - `nameserver 127.0.0.1`, `port 53535` -
                    and is skipped, saying so, without it)
  iroh-lane         the node list names an iroh node at a UDP port that swallows everything: the
                    requests fall back to HTTPS at once, the lane rests, nothing is sent twice
  server-errors     503 SlowDown with Retry-After, 500 bursts, 409 with x-azlin-map (a stale map),
                    maintenance (Retry-After 30): retried, nothing duplicated, a plain status

Usage (from the azul repository, after building libazul with the debug server, AzDrive with its
default features and the azcloud CLI):

    cargo build --release -p azul-dll --features build-dll
    AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzDrive
    (cd examples/azcloud-api && cargo build --release --features encryption)
    python3 scripts/azdrive_chaos_e2e.py [--only NAME,...] [--bin AzDrive] [--azcloud PATH]
        [--debug-port 8851] [--timeout 300] [--keep] [--big-gb 50] [--many 100000]

Every scenario runs in a folder of its own under one temporary folder (deleted unless --keep);
one AzDrive at a time uses the debug port, a second one the next port. The summary lists each
scenario as PASS, FAIL (what was expected, what happened) or SKIP (why). Exit code 0 when no
scenario failed.
"""

import argparse
import calendar
import hashlib
import json
import os
import re
import shutil
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
import traceback
import unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "examples", "azul-drive", "scripts"))

import azchaos  # noqa: E402
import azlin_e2e as e2e  # noqa: E402
import azlin_mock_stack  # noqa: E402
from azdrive_e2e import Drive, I, azlin_session, other_device_writes  # noqa: E402

Failure = e2e.Failure
MIB = 1024 * 1024
GIB = 1024 * MIB
# What the sync never takes (azcloud-kit sync/rules.rs BASE_RULES), for comparing trees.
NEVER_SYNCED = re.compile(r"(^|/)(\.DS_Store|\._.*|.*\.tmp|.*\.part|.*~|\.azlin(/.*)?)$")
# The status keys that are no failure of their own.
UP_TO_DATE = "azdrive-sync-status-up-to-date"


class Skip(Exception):
    """A scenario that cannot run here, and why."""


def log(line):
    print("[azchaos-e2e] %s" % line, flush=True)


def expect(condition, message):
    if not condition:
        raise Failure(message)


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(MIB), b""):
            digest.update(block)
    return digest.hexdigest()


def write(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)


def read(path):
    try:
        with open(path, "rb") as f:
            return f.read()
    except OSError:
        return None


def tree(root):
    """{key: sha256} of the regular files under `root` the sync takes (symbolic links, special
    files and what it never syncs left out)."""
    out = {}
    for folder, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d != ".azlin"]
        for name in files:
            path = os.path.join(folder, name)
            key = os.path.relpath(path, root).replace(os.sep, "/")
            if NEVER_SYNCED.search(key):
                continue
            try:
                mode = os.lstat(path).st_mode
            except OSError:
                continue
            if not stat.S_ISREG(mode):
                continue
            try:
                out[key] = sha256_file(path)
            except OSError:
                out[key] = None
    return out


def random_file(path, size, seed=None):
    """A file of `size` random bytes (in pieces); its SHA-256."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    digest = hashlib.sha256()
    with open(path, "wb") as f:
        left = size
        while left:
            n = min(left, 4 * MIB)
            block = os.urandom(n)
            digest.update(block)
            f.write(block)
            left -= n
    return digest.hexdigest()


def wait_port_free(port, timeout=20):
    """Waits until nothing listens on `port` (the last AzDrive's debug server is gone)."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            socket.create_connection(("127.0.0.1", port), timeout=0.2).close()
        except OSError:
            return
        time.sleep(0.2)
    raise Failure("port %d is still taken (another AzDrive?)" % port)


def free_port():
    sock = socket.socket()
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    sock.close()
    return port


# ---------------------------------------------------------------------------------------------
# The stack: the mock token server, its S3, the fault proxy in front of the S3
# ---------------------------------------------------------------------------------------------


class Stack:
    """The mock stack of one scenario with the proxy as every drive's node. It knows every
    session the token server handed out (its family and expiry), so the proxy can refuse an
    expired or revoked one as a node would."""

    def __init__(self, base, ttl=azlin_mock_stack.DEFAULT_TTL):
        self.base = base
        self.s3_root = os.path.join(base, "s3")
        self.stack = azlin_mock_stack.start(self.s3_root, ttl=ttl)
        self.proxy = azchaos.ChaosProxy(self.stack.s3_url,
                                        log_path=os.path.join(base, "proxy.jsonl")).start()
        self.state = self.stack.token.state
        self.state.s3_url = self.proxy.url
        self.state.set_nodes([azlin_mock_stack.node_entry("n1", self.proxy.url)])
        # session token -> (family id, expires at): every bundle the token server answered.
        self.sessions = {}
        # The members whose bundles name the proxy (the devices under test); the others are
        # sent to the S3 directly.
        self.proxied = {"laptop", "laptop2"}
        self.enforce_sessions = False
        original = self.state.bundle

        def bundle(drive, token):
            made = original(drive, token)
            family = token[3:].split(".")[0] if token.startswith("dt_") else None
            member = self.state.families.get(family, {}).get("member") if family else None
            if member not in self.proxied:
                # Another device talks to the S3 itself: the proxy's faults are the laptop's.
                direct = self.stack.s3_url
                made["drive"]["location"]["endpoint"] = direct
                made["nodes"] = [azlin_mock_stack.node_entry("n1", direct)]
            expires = calendar.timegm(time.strptime(made["credentials"]["expires_at"],
                                                    "%Y-%m-%dT%H:%M:%SZ"))
            self.sessions[made["credentials"]["session_token"]] = (family, expires)
            return made

        self.state.bundle = bundle
        self.proxy.authorize = self._authorize

    def _authorize(self, request):
        if not self.enforce_sessions:
            return None
        token = request.header("x-amz-security-token")
        known = self.sessions.get(token or "")
        if known is None:
            return (403, "InvalidAccessKeyId", "credentials_revoked")
        family, expires = known
        revoked = self.state.families.get(family, {}).get("revoked") if family else None
        if revoked:
            return (403, "InvalidAccessKeyId", "credentials_revoked")
        if time.time() + self.proxy.clock_offset > expires:
            return (400, "ExpiredToken", None)
        return None

    @property
    def token_url(self):
        return self.stack.token_url

    def signup(self, name, tier=None):
        body = {"name": name}
        if tier:
            body["tier"] = tier
        return self.state.signup(body)

    def family(self, drive_id, member):
        """A bundle of a new token family of `drive_id` (a device of its own)."""
        with self.state.lock:
            token = self.state.new_family(drive_id, member)
            return self.state.bundle(self.state.drives[drive_id], token)

    def bucket(self, drive_id):
        return self.state.drives[drive_id]["bucket"]

    def bucket_dir(self, drive_id):
        return os.path.join(self.s3_root, self.bucket(drive_id))

    def index(self, drive_id):
        """The plain sync index of the drive (prefix ""), or {}."""
        data = read(os.path.join(self.bucket_dir(drive_id), ".azlin", "index.json"))
        return json.loads(data) if data else {}

    def blob_path(self, drive_id, digest):
        return os.path.join(self.bucket_dir(drive_id), ".azlin", "blobs", digest[:2], digest)

    def drive_tree(self, drive_id):
        """{key: sha256 of its blob} of the plain index's files (None: the blob is missing)."""
        out = {}
        for key, entry in self.index(drive_id).get("files", {}).items():
            path = self.blob_path(drive_id, entry["hash"])
            out[key] = sha256_file(path) if os.path.isfile(path) else None
        return out

    def unfinished_uploads(self):
        """The multipart uploads the S3 still holds parts of (never completed nor aborted)."""
        folder = os.path.join(self.s3_root, ".s3-server-uploads")
        if not os.path.isdir(folder):
            return []
        return [d for d in os.listdir(folder) if not d.startswith(".")]

    def stop(self):
        self.proxy.stop()
        self.stack.stop()


# ---------------------------------------------------------------------------------------------
# The devices
# ---------------------------------------------------------------------------------------------


class Laptop:
    """AzDrive with Azlin drives paired with folders under its home (its view settings), every
    pass every `poll` seconds, its keyring a file of the run."""

    def __init__(self, ctx, stack, tag, drives, port=None, poll=1, env=None, member="laptop",
                 folder_root=None, session_expires_in=None):
        self.ctx = ctx
        self.stack = stack
        self.tag = tag
        self.base = os.path.join(stack.base, tag)
        self.home = os.path.join(self.base, "home")
        os.makedirs(self.home, exist_ok=True)
        self.port = port or ctx.args.debug_port
        self.poll = poll
        self.drives = []
        self.folders = {}
        entries, keyring, synced = [], {}, []
        for drive_id, name in drives:
            bundle = stack.family(drive_id, member)
            entries.append(bundle["drive"])
            session = azlin_session(bundle)
            if session_expires_in is not None:
                # A session about to run out: the first request refreshes it, and the refresh
                # brings the node list (its addresses, its iroh ids) - as an app that has run
                # before has it in its nodes file.
                kept = json.loads(session)
                kept["expires_at"] = int(time.time()) + session_expires_in
                session = json.dumps(kept)
            keyring["azul-storage/s3/" + drive_id] = session
            root = folder_root or os.path.join(self.home, "AzDrive")
            folder = os.path.join(root, name)
            self.folders[drive_id] = folder
            synced.append({"drive_id": drive_id, "prefix": "", "folder": folder})
            self.drives.append(drive_id)
        self.drives_file = os.path.join(self.base, "config", "drives.json")
        os.makedirs(os.path.dirname(self.drives_file), exist_ok=True)
        with open(self.drives_file, "w", encoding="utf-8") as f:
            json.dump({"format": "azul-storage.drives", "version": 1, "drives": entries}, f)
        self.keyring = os.path.join(self.base, "keyring.json")
        with open(self.keyring, "w", encoding="utf-8") as f:
            json.dump(keyring, f)
        self.view = os.path.join(self.base, "data", "drive", "view.json")
        os.makedirs(os.path.dirname(self.view), exist_ok=True)
        with open(self.view, "w", encoding="utf-8") as f:
            json.dump({"synced": synced}, f)
        self.env = {"AZ_KEYRING_FILE": self.keyring, "AZDRIVE_SYNC_POLL": str(poll),
                    "AZCLOUD_DEVICE": member, "TMPDIR": os.path.join(self.base, "tmp")}
        os.makedirs(self.env["TMPDIR"], exist_ok=True)
        self.env.update(env or {})
        self.app = None
        self.runs = 0
        self.outputs = []
        # (seconds since 1970, line) of every line AzDrive printed, as the watcher saw it.
        self._timeline = []
        self._watching = None

    def switches(self):
        return ["--screen", "this-pc", "--theme", "flat", "--mode", "light", "--language", "en",
                "--home", self.home, "--downloads", os.path.join(self.base, "downloads"),
                "--data-dir", os.path.join(self.base, "data"), "--drives", self.drives_file,
                "--dialogs", "inline", "--cache-dir", os.path.join(self.base, "cache"),
                "--token-url", self.stack.token_url]

    def start(self, wait=True):
        wait_port_free(self.port)
        self.runs += 1
        tag = "%s-%d" % (self.tag, self.runs)
        self.app = Drive(tag, self.ctx.binary, self.switches(), self.port, self.base,
                         self.ctx.args.timeout, extra_env=self.env)
        self.outputs.append(self.app.out_path)
        self._watch(self.app.out_path)
        if wait:
            self.app.until("AzDrive up", lambda: self.app.printed("AZDRIVE_PLACE", r"this-pc"))
        return self

    def _watch(self, path):
        """Timestamps every line the app prints (a thread reading its stdout file)."""
        if self._watching:
            self._watching.set()
        stop = threading.Event()
        self._watching = stop

        def run():
            position = 0
            pending = b""
            while not stop.is_set():
                try:
                    with open(path, "rb") as f:
                        f.seek(position)
                        data = f.read()
                        position += len(data)
                except OSError:
                    data = b""
                if data:
                    now = time.time()
                    pending += data
                    *lines, pending = pending.split(b"\n")
                    for line in lines:
                        self._timeline.append((now, line.decode("utf-8", "replace")))
                stop.wait(0.05)

        threading.Thread(target=run, name="laptop-watch", daemon=True).start()

    def timeline(self):
        return list(self._timeline)

    def kill(self):
        """SIGKILL: no shutdown, no save - a crash, or a power cut."""
        if self.app and self.app.alive():
            self.app.process.kill()
            self.app.process.wait(10)

    def suspend(self):
        os.kill(self.app.process.pid, signal.SIGSTOP)

    def resume(self):
        os.kill(self.app.process.pid, signal.SIGCONT)

    def stop(self):
        if self.app:
            self.app.stop()
        if self._watching:
            self._watching.set()

    def pid(self):
        return self.app.process.pid

    def rss_mb(self):
        """The process's resident memory (MB)."""
        try:
            out = subprocess.run(["ps", "-o", "rss=", "-p", str(self.pid())],
                                 capture_output=True, text=True, timeout=5).stdout.strip()
            return int(out) / 1024.0
        except (ValueError, OSError, subprocess.TimeoutExpired):
            return 0.0

    def folder(self, drive_id=None):
        return self.folders[drive_id or self.drives[0]]

    # ---- what it printed ----

    def lines(self, key, drive_id=None, pattern=r".*"):
        drive_id = drive_id or self.drives[0]
        if not self.app:
            return []
        return self.app.printed(key, re.escape(drive_id) + r"(?:\s|$)" + pattern)

    def status(self, drive_id=None):
        said = self.lines("AZDRIVE_SYNC_STATUS", drive_id)
        if not said:
            return None
        parts = said[-1].split(" ", 1)
        return parts[1] if len(parts) > 1 else ""

    def passes(self, drive_id=None):
        """Every pass's end, DONE or FAILED, in the order AzDrive printed them."""
        drive_id = drive_id or self.drives[0]
        if not self.app:
            return []
        try:
            with open(self.app.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^AZDRIVE_SYNC_(?:DONE|FAILED) (%s(?:\s.*)?)$" % re.escape(drive_id),
                          text, re.M)

    def failures(self, drive_id=None):
        return self.lines("AZDRIVE_SYNC_FAILED", drive_id)

    def file_states(self, drive_id=None):
        """{key: state word} from the AZDRIVE_SYNC_FILE lines (the last one of each key)."""
        states = {}
        for line in self.lines("AZDRIVE_SYNC_FILE", drive_id):
            parts = line.split(" ", 2)
            if len(parts) == 3:
                states[parts[2]] = parts[1]
        return states

    def wait_status(self, what, pattern, drive_id=None, timeout=None):
        """Waits for a status line matching `pattern` (a regular expression of the key form)."""
        deadline = time.time() + (timeout or self.ctx.args.timeout)
        while time.time() < deadline:
            current = self.status(drive_id)
            if current is not None and re.search(pattern, current):
                return current
            if not self.app.alive():
                raise Failure("AzDrive died while waiting for %s:\n%s" % (what, self.app.tail()))
            time.sleep(0.2)
        raise Failure("timed out waiting for %s (status: %s)" % (what, self.status(drive_id)))

    def wait_passes(self, what, count, drive_id=None, timeout=None):
        """Waits until `count` more passes ended than now; returns their lines."""
        before = len(self.passes(drive_id))
        deadline = time.time() + (timeout or self.ctx.args.timeout)
        while time.time() < deadline:
            done = self.passes(drive_id)
            if len(done) >= before + count:
                return done[before:]
            if not self.app.alive():
                raise Failure("AzDrive died while waiting for %s:\n%s" % (what, self.app.tail()))
            time.sleep(0.2)
        raise Failure("timed out waiting for %s (status: %s)" % (what, self.status(drive_id)))

    def wait_settled(self, what, drive_id=None, timeout=None, quiet=2):
        """Waits for `quiet` passes in a row that moved nothing and ended "Up to date"."""
        deadline = time.time() + (timeout or self.ctx.args.timeout)
        calm = 0
        while time.time() < deadline:
            lines = self.wait_passes(what, 1, drive_id, max(1, deadline - time.time()))
            moved = re.search(r"up=([1-9]|\d\d)|down=([1-9]|\d\d)|deleted=([1-9]|\d\d)", lines[-1])
            # The status line follows the pass's line: give it a moment.
            said_by = time.time() + 2
            while self.status(drive_id) != UP_TO_DATE and time.time() < said_by:
                time.sleep(0.05)
            if not moved and "FAILED" not in lines[-1] and self.status(drive_id) == UP_TO_DATE:
                calm += 1
                if calm >= quiet:
                    return
            else:
                calm = 0
        raise Failure("%s: never settled (status %s, last passes %s)" % (
            what, self.status(drive_id), self.passes(drive_id)[-3:]))


def status_after_passes(laptop, drive_id, count=2, timeout=None):
    """The status line AzDrive printed right after the last of `count` more passes ended (not
    the "Syncing" of the next one, which a short poll starts at once)."""
    laptop.wait_passes("%d passes" % count, count, drive_id, timeout)
    deadline = time.time() + 5
    while time.time() < deadline:
        said = None
        ended = False
        for _, line in laptop.timeline():
            if line.startswith(("AZDRIVE_SYNC_DONE %s" % drive_id,
                                "AZDRIVE_SYNC_FAILED %s" % drive_id)):
                ended, said = True, None
            elif ended and said is None and line.startswith("AZDRIVE_SYNC_STATUS %s " % drive_id):
                said = line.split(" ", 2)[2]
        if said is not None:
            return said
        time.sleep(0.05)
    return laptop.status(drive_id)


class Desktop:
    """The `azcloud` CLI as another device (its own HOME, state folder and token family)."""

    def __init__(self, ctx, stack, name="desktop"):
        self.ctx = ctx
        self.stack = stack
        self.name = name
        self.root = os.path.join(stack.base, name)
        self.home = os.path.join(self.root, "home")
        os.makedirs(self.home, exist_ok=True)
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(("AZLIN_", "AZCLOUD_", "AZMEET_", "XDG_"))}
        env.update({"HOME": self.home, "AZCLOUD_HOME": os.path.join(self.root, "state"),
                    "AZLIN_DATA": os.path.join(self.home, "AzlinData"),
                    "AZLIN_HOME": os.path.join(self.home, ".azlin"),
                    "AZLIN_CONFIG": os.path.join(self.home, ".azlin", "config.json"),
                    "AZLIN_TOKEN_URL": stack.token_url, "AZCLOUD_DEVICE": name,
                    "TMPDIR": os.path.join(self.root, "tmp")})
        os.makedirs(env["TMPDIR"], exist_ok=True)
        self.env = env

    def az(self, *args, check=True, timeout=900):
        if not self.ctx.azcloud:
            raise Skip("no azcloud binary (--azcloud, AZCLOUD_BIN or examples/azcloud-api/"
                       "target/release/azcloud)")
        command = [self.ctx.azcloud, "--json"] + [str(a) for a in args]
        done = subprocess.run(command, env=self.env, capture_output=True, text=True,
                              timeout=timeout)
        try:
            out = json.loads(done.stdout) if done.stdout.strip() else {}
        except ValueError:
            raise Failure("%s: azcloud %s printed no JSON:\n%s\n%s" % (
                self.name, " ".join(map(str, args)), done.stdout[-1500:], done.stderr[-1500:]))
        out["_exit"] = done.returncode
        if check and (done.returncode != 0 or out.get("ok") is False):
            raise Failure("%s: azcloud %s failed: %s\n%s" % (
                self.name, " ".join(map(str, args)), out.get("error"), done.stderr[-1500:]))
        return out

    def signup(self, name, plaintext=True):
        """A drive made by this device (the owner): its id."""
        args = ["signup", "--name", name]
        if plaintext:
            args.append("--plaintext")
        else:
            args += ["--recovery-out", os.path.join(self.root, "recovery.txt")]
        return self.az(*args)["drive"]

    def sync(self, folder, check=True):
        """One sync of `folder` with the drive's root; its report."""
        os.makedirs(folder, exist_ok=True)
        out = self.az("sync", folder, "--prefix", "", check=check)
        reports = out.get("reports") or [{}]
        return reports[0]


# ---------------------------------------------------------------------------------------------
# The checks every scenario ends with
# ---------------------------------------------------------------------------------------------


def no_silent_failure(laptop, drive_id=None, allowed_missing=()):
    """When AzDrive says "Up to date", the folder and the drive hold the same files; files the
    drive cannot take (`allowed_missing`: keys) must be named by a state, not hidden."""
    drive_id = drive_id or laptop.drives[0]
    status = laptop.status(drive_id)
    here = tree(laptop.folder(drive_id))
    there = laptop.stack.drive_tree(drive_id)
    states = laptop.file_states(drive_id)
    problems = []
    for key, digest in sorted(here.items()):
        if key in allowed_missing:
            continue
        if key not in there:
            problems.append("%s is here but not on the drive" % key)
        elif there[key] != digest:
            problems.append("%s differs here and on the drive" % key)
    for key, digest in sorted(there.items()):
        if digest is None:
            problems.append("%s: the index names a blob the bucket does not hold" % key)
        elif key not in here and states.get(key) not in ("cloud-only", "error", "conflict"):
            problems.append("%s is on the drive, not here, and AzDrive says %s" % (
                key, states.get(key)))
    if status == UP_TO_DATE and problems:
        raise Failure("silent failure: the status says Up to date while %s" % "; ".join(
            problems[:8]))
    return problems


def nothing_lost(versions, places):
    """Every content in `versions` ({what: sha256}) is a file in one of `places` (trees)."""
    held = set()
    for place in places:
        held.update(v for v in place.values() if v)
    lost = [what for what, digest in versions.items() if digest not in held]
    expect(not lost, "lost: %s" % ", ".join(sorted(lost)[:10]))


# ---------------------------------------------------------------------------------------------
# The scenarios
# ---------------------------------------------------------------------------------------------

SCENARIOS = []


def scenario(name):
    def register(fn):
        SCENARIOS.append((name, fn))
        return fn
    return register


class Ctx:
    """One run: its switches, binaries and folder."""

    def __init__(self, args, logs):
        self.args = args
        self.logs = logs
        self.binary = e2e.find_binary("AzDrive", args.bin, "AZDRIVE_BIN")
        self.azcloud = find_azcloud(args.azcloud)

    def folder(self, name):
        path = os.path.join(self.logs, name)
        os.makedirs(path, exist_ok=True)
        return path


def find_azcloud(explicit):
    candidates = [explicit, os.environ.get("AZCLOUD_BIN")]
    for root in e2e.repo_roots():
        candidates.append(os.path.join(root, "examples", "azcloud-api", "target", "release",
                                       "azcloud"))
    for candidate in candidates:
        if candidate and os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
    return None


def plain_world(ctx, name, ttl=azlin_mock_stack.DEFAULT_TTL, laptop_env=None, poll=1,
                desktop=False):
    """A stack, a plain drive, AzDrive paired with it: (stack, drive id, laptop, desktop) - the
    laptop started and its first pass done. With `desktop` the azcloud CLI made the drive (the
    owner: a second device), else the token server did."""
    stack = Stack(ctx.folder(name), ttl=ttl)
    other = None
    if desktop:
        other = Desktop(ctx, stack)
        drive = other.signup("Chaos drive")
    else:
        drive = stack.signup("Chaos drive")["drive"]["id"]
    laptop = Laptop(ctx, stack, "laptop", [(drive, "Chaos drive")], env=laptop_env, poll=poll)
    os.makedirs(laptop.folder(drive), exist_ok=True)
    try:
        laptop.start()
        laptop.wait_status("the first pass", UP_TO_DATE, drive)
    except BaseException:
        laptop.stop()
        stack.stop()
        raise
    return stack, drive, laptop, other


@scenario("special-files")
def special_files(ctx):
    """Ordinary files sync; a symbolic link, a FIFO, an unreadable file and an unreadable folder
    are named - the status does not say "Up to date" about a folder whose files never sync."""
    stack, drive, laptop, _ = plain_world(ctx, "special-files")
    try:
        folder = laptop.folder(drive)
        write(os.path.join(folder, "plain.txt"), b"an ordinary file\n")
        write(os.path.join(folder, "sub", "deep.txt"), b"another\n")
        os.symlink(os.path.join(folder, "plain.txt"), os.path.join(folder, "link.txt"))
        os.mkfifo(os.path.join(folder, "pipe"))
        write(os.path.join(folder, "secret.txt"), b"no read permission\n")
        os.chmod(os.path.join(folder, "secret.txt"), 0)
        write(os.path.join(folder, "locked", "inside.txt"), b"in a folder nobody may list\n")
        os.chmod(os.path.join(folder, "locked"), 0)
        try:
            status = status_after_passes(laptop, drive, 3)
            there = stack.drive_tree(drive)
            expect("plain.txt" in there and "sub/deep.txt" in there,
                   "the ordinary files did not sync: %s" % sorted(there))
            expect("link.txt" not in there and "pipe" not in there,
                   "a link or a FIFO went up as a file: %s" % sorted(there))
            expect(status != UP_TO_DATE,
                   "silent failure: Up to date while secret.txt and locked/inside.txt never "
                   "sync (the status should name the files it cannot read)")
            return "ordinary files up; status %s" % status
        finally:
            os.chmod(os.path.join(folder, "secret.txt"), 0o644)
            os.chmod(os.path.join(folder, "locked"), 0o755)
    finally:
        laptop.stop()
        stack.stop()


@scenario("names")
def names(ctx):
    """NFC / NFD twins and case twins made by a case-sensitive device come to this
    case-insensitive disk; then a case-only rename here."""
    stack, drive, laptop, desktop = plain_world(ctx, "names", desktop=True)
    try:
        laptop.stop()
        image = CaseSensitiveImage(ctx.folder("names/image"))
        try:
            other = os.path.join(image.mount, "desktop-folder")
            os.makedirs(other)
            # Case twins from a case-sensitive disk, through the desktop's real sync.
            write(os.path.join(other, "Notes.txt"), b"capital N\n")
            write(os.path.join(other, "notes.txt"), b"small n\n")
            report = desktop.sync(other)
            expect(report.get("files_up") == 2, "the desktop sent %s" % report)
            # NFC and NFD twins: APFS cannot hold both (it is normalization-insensitive), a
            # Linux device can - its commits, written into the bucket as its sync would.
            nfc = unicodedata.normalize("NFC", "Caf\u00e9.txt")
            nfd = unicodedata.normalize("NFD", "Caf\u00e9.txt")
            other_device_writes(stack.s3_root, nfc, b"written as NFC\n", device="linux",
                                bucket=stack.bucket(drive))
            other_device_writes(stack.s3_root, nfd, b"written as NFD\n", device="linux",
                                bucket=stack.bucket(drive))
            versions = {
                "Notes.txt": hashlib.sha256(b"capital N\n").hexdigest(),
                "notes.txt": hashlib.sha256(b"small n\n").hexdigest(),
                "NFC": hashlib.sha256(b"written as NFC\n").hexdigest(),
                "NFD": hashlib.sha256(b"written as NFD\n").hexdigest(),
            }
            laptop.start()
            laptop.wait_passes("passes over the twins", 4, drive)
            here = tree(laptop.folder(drive))
            states = laptop.file_states(drive)
            # A twin this disk cannot hold beside the other is not "on this device" with the
            # other's bytes.
            for twin in ((nfc, nfd), ("Notes.txt", "notes.txt")):
                held = [k for k in twin if k in here]
                for key in twin:
                    if key not in here and states.get(key) in ("on-device", "pinned"):
                        raise Failure("%r shows as on this device but the file here is %r "
                                      "(its twin's bytes)" % (key, held))
            nothing_lost(versions, [here, stack.drive_tree(drive)])
            # A case-only rename here: the drive gets the new name, nothing is deleted.
            os.rename(os.path.join(laptop.folder(drive), "Notes.txt") if "Notes.txt" in here
                      else os.path.join(laptop.folder(drive), "notes.txt"),
                      os.path.join(laptop.folder(drive), "NOTES.txt"))
            laptop.wait_passes("the rename's pass", 3, drive)
            there = stack.drive_tree(drive)
            nothing_lost(versions, [tree(laptop.folder(drive)), there])
            no_silent_failure(laptop, drive)
            return "twins: here %s; states %s; status %s" % (
                sorted(tree(laptop.folder(drive))), states, laptop.status(drive))
        finally:
            image.detach()
    finally:
        laptop.stop()
        stack.stop()


def statuses_seen(laptop, drive_id=None):
    """Every distinct status line the laptop printed, in order."""
    seen = []
    for line in laptop.lines("AZDRIVE_SYNC_STATUS", drive_id):
        said = line.split(" ", 1)[1] if " " in line else ""
        if said not in seen:
            seen.append(said)
    return seen


def raw_errors(statuses):
    """Status lines that show a technical error to the user (an HTTP code, an S3 code, a Rust
    error's words) instead of a sentence."""
    technical = re.compile(r"HTTP \d{3}|x-azlin-error|Error\b|\(os error|InternalError|SlowDown|"
                           r"ExpiredToken|InvalidAccessKeyId|RequestTimeTooSkewed|"
                           r"Connection refused|Connection reset|no answer from|https?://|"
                           r"timed out|errno", re.I)
    return [s for s in statuses if s.startswith("azdrive-sync-status-failed") and
            technical.search(s)]


def blob_writes(stack, drive_id):
    """How often each blob key of the drive was written and landed (forwarded, 2xx)."""
    return {k: n for k, n in stack.proxy.writes(key=r"^\.azlin/blobs/").items()}


@scenario("server-errors")
def server_errors(ctx):
    """A node's transient errors: retried, nothing duplicated, the status plain."""
    stack, drive, laptop, _ = plain_world(ctx, "server-errors")
    proxy = stack.proxy
    try:
        folder = laptop.folder(drive)
        steps = [
            ("503 unavailable (Retry-After 0), twice", dict(
                op="PutObject", key=r"^\.azlin/blobs/", action="azlin",
                azlin_error="unavailable", times=2)),
            ("500 InternalError, five times", dict(
                op="PutObject", key=r"^\.azlin/blobs/", action="status", status=500,
                code="InternalError", times=5)),
            ("409 stale map (x-azlin-map), once", dict(
                op="PutObject", key=r"^\.azlin/", action="stale_map", epoch=7, times=1)),
            ("maintenance (Retry-After 30), once", dict(
                op="PutObject", key=r"^\.azlin/index", action="azlin",
                azlin_error="maintenance", times=1)),
            ("the index read answers 500 three times", dict(
                op=("GetObject", "HeadObject"), key=r"^\.azlin/index", action="status",
                status=500, code="InternalError", times=3)),
            # Last: the node named in the hint answers from then on (the failover keeps it
            # first for ten minutes), so the proxy sees no more requests.
            ("a 503 that names another node (x-azlin-alt-endpoints)", dict(
                op="PutObject", key=r"^\.azlin/blobs/", action="azlin",
                azlin_error="unavailable", alt_endpoints=stack.stack.s3_url, times=1)),
        ]
        notes = []
        for i, (what, rule) in enumerate(steps):
            fault = proxy.add(**rule)
            data = ("file %d under %s\n" % (i, what)).encode("utf-8") * 50
            write(os.path.join(folder, "e%d.txt" % i), data)
            started = time.time()
            laptop.wait_settled(what, drive, timeout=120)
            proxy.clear()
            notes.append("%s: applied %d, settled in %.0f s" % (what, fault.applied,
                                                                time.time() - started))
            no_silent_failure(laptop, drive)
        twice = {k: n for k, n in blob_writes(stack, drive).items() if n > 1}
        expect(not twice, "blobs written more than once: %s" % twice)
        expect(not stack.unfinished_uploads(), "unfinished uploads stay: %s" %
               stack.unfinished_uploads())
        raw = raw_errors(statuses_seen(laptop, drive))
        expect(not raw, "the status showed technical errors to the user: %s" % raw[:3])
        return "; ".join(notes)
    finally:
        laptop.stop()
        stack.stop()


@scenario("flap-upload")
def flap_upload(ctx):
    """The network flaps while 30 small files and a 120 MiB file go up; lost requests and lost
    answers on top. Every file arrives once, the big one resumes per part."""
    stack, drive, laptop, _ = plain_world(ctx, "flap-upload")
    proxy = stack.proxy
    try:
        folder = laptop.folder(drive)
        versions = {}
        big = os.path.join(folder, "video.bin")
        versions["video.bin"] = random_file(big, 120 * MIB)
        for i in range(30):
            data = os.urandom(1000 + i * 1500)
            write(os.path.join(folder, "small", "s%02d.bin" % i), data)
            versions["small/s%02d.bin" % i] = hashlib.sha256(data).hexdigest()
        proxy.add(op=("PutObject", "UploadPart"), action="reset_before", probability=0.08,
                  seed=11)
        proxy.add(op=("PutObject", "UploadPart", "CompleteMultipartUpload"),
                  action="reset_after", probability=0.08, seed=12)
        proxy.flap(up=2.5, down=1.5)
        time.sleep(45)
        proxy.stop_flap()
        proxy.clear()
        laptop.wait_settled("after the flapping", drive, timeout=300)
        here, there = tree(folder), stack.drive_tree(drive)
        nothing_lost(versions, [here])
        nothing_lost(versions, [there])
        no_silent_failure(laptop, drive)
        big_key = [k for k in stack.proxy.writes(key=r"^\.azlin/blobs/") if k.endswith(
            stack.index(drive)["files"]["video.bin"]["hash"])]
        parts = proxy.requests(op="UploadPart", key=r"^\.azlin/blobs/")
        forwarded = [p for p in parts if p["forwarded"]]
        expected_parts = (120 * MIB + 16 * MIB - 1) // (16 * MIB)
        expect(len(forwarded) <= 2 * expected_parts,
               "the 120 MiB file's parts were sent %d times for %d parts: an upload is not "
               "resumed per part after a network drop" % (len(forwarded), expected_parts))
        expect(not stack.unfinished_uploads(), "unfinished uploads stay: %s" %
               stack.unfinished_uploads())
        raw = raw_errors(statuses_seen(laptop, drive))
        expect(not raw, "the status showed technical errors to the user: %s" % raw[:3])
        return "settled; %d part uploads for %d parts; %s; statuses %s" % (
            len(forwarded), expected_parts, big_key, statuses_seen(laptop, drive))
    finally:
        proxy.stop_flap()
        laptop.stop()
        stack.stop()


# The points of a pass where kill-every-step kills AzDrive: (name, the rule that holds the
# request there, whether the request was forwarded when it is held).
KILL_POINTS = [
    ("index read", dict(op=("GetObject", "HeadObject"), key=r"^\.azlin/index\.json$",
                        action="stall"), False),
    ("before the first blob", dict(op="PutObject", key=r"^\.azlin/blobs/", action="stall",
                                   nth=1), False),
    ("after the third blob landed", dict(op="PutObject", key=r"^\.azlin/blobs/",
                                         action="stall_after", nth=3), True),
    ("in the big file's second part", dict(op="UploadPart", action="stall", nth=2), False),
    ("after the big file's Complete landed", dict(op="CompleteMultipartUpload",
                                                  action="stall_after"), True),
    ("before the index write", dict(op="PutObject", key=r"^\.azlin/index\.json$",
                                    action="stall"), False),
    ("after the index write landed", dict(op="PutObject", key=r"^\.azlin/index\.json$",
                                          action="stall_after"), True),
    ("in the downloads", dict(op="GetObject", key=r"^\.azlin/blobs/", action="stall", nth=2),
     False),
]


@scenario("kill-every-step")
def kill_every_step(ctx):
    """AzDrive killed at every step of a pass that uploads and downloads, then started again:
    the drive and both folders converge, nothing is lost, no big blob goes up twice."""
    stack, drive, laptop, desktop = plain_world(ctx, "kill-every-step", desktop=True)
    proxy = stack.proxy
    desk = os.path.join(desktop.root, "folder")
    notes = []
    try:
        folder = laptop.folder(drive)
        versions = {}
        for n, (point, rule, _) in enumerate(KILL_POINTS):
            # This round's changes: new files here (one big), new files there.
            for i in range(4):
                data = ("round %d file %d\n" % (n, i)).encode("utf-8") * (200 + i)
                write(os.path.join(folder, "r%d" % n, "up%d.txt" % i), data)
                versions["r%d/up%d.txt" % (n, i)] = hashlib.sha256(data).hexdigest()
            versions["r%d/big.bin" % n] = random_file(os.path.join(folder, "r%d" % n,
                                                                   "big.bin"), 24 * MIB)
            for i in range(3):
                data = ("from the desktop, round %d, %d\n" % (n, i)).encode("utf-8") * 30
                write(os.path.join(desk, "d%d" % n, "down%d.txt" % i), data)
                versions["d%d/down%d.txt" % (n, i)] = hashlib.sha256(data).hexdigest()
            fault = proxy.add(**rule)
            desktop.sync(desk)
            deadline = time.time() + 120
            while time.time() < deadline and proxy.held() == 0:
                time.sleep(0.05)
            if proxy.held() == 0:
                notes.append("%s: never reached (applied %d)" % (point, fault.applied))
                proxy.clear()
                laptop.wait_settled("after %s" % point, drive)
                continue
            laptop.kill()
            proxy.clear()
            proxy.release()
            laptop.start()
            laptop.wait_settled("after a kill %s" % point, drive, timeout=180)
            desktop.sync(desk)
            here, there, theirs = tree(folder), stack.drive_tree(drive), tree(desk)
            nothing_lost(versions, [here])
            nothing_lost(versions, [there])
            nothing_lost(versions, [theirs])
            expect(here == there, "%s: the folder and the drive differ: %s" % (
                point, sorted(set(here.items()) ^ set(there.items()))[:6]))
            no_silent_failure(laptop, drive)
            notes.append("%s: converged" % point)
        big = {k: n for k, n in blob_writes(stack, drive).items() if n > 1 and
               os.path.getsize(os.path.join(stack.bucket_dir(drive), *k.split("/"))) > 8 * MIB}
        expect(not big, "big blobs went up twice: %s" % big)
        expect(not stack.unfinished_uploads(), "unfinished uploads stay: %s" %
               stack.unfinished_uploads())
        small_twice = sum(1 for n in blob_writes(stack, drive).values() if n > 1)
        return "; ".join(notes) + "; small blobs sent again after a kill: %d" % small_twice
    finally:
        laptop.stop()
        stack.stop()


@scenario("offline-conflict")
def offline_conflict(ctx):
    """Both devices edit one file while the laptop is offline; AzDrive asks; Keep both."""
    stack, drive, laptop, desktop = plain_world(ctx, "offline-conflict", desktop=True)
    desk = os.path.join(desktop.root, "folder")
    try:
        folder = laptop.folder(drive)
        write(os.path.join(folder, "plan.txt"), b"version 1\n")
        laptop.wait_settled("plan.txt up", drive)
        desktop.sync(desk)
        expect(read(os.path.join(desk, "plan.txt")) == b"version 1\n", "the desktop lacks v1")
        stack.proxy.network("down")
        write(os.path.join(folder, "plan.txt"), b"version 2 from the laptop, offline\n")
        write(os.path.join(desk, "plan.txt"), b"version 2 from the desktop\n")
        desktop.sync(desk)
        offline_status = status_after_passes(laptop, drive, 2)
        expect(offline_status != UP_TO_DATE, "offline, the status says Up to date")
        stack.proxy.network("up")
        laptop.app.until("the conflict question", lambda: laptop.app.printed(
            "AZDRIVE_SYNC_CONFLICT", re.escape(drive) + r" plan\.txt"))
        expect(read(os.path.join(folder, "plan.txt")) ==
               b"version 2 from the laptop, offline\n",
               "the laptop's version changed before the question was answered")
        laptop.app.until("the question shows", lambda: laptop.app.has("#" + I("sync-conflict")))
        laptop.app.must("click", selector="#" + I("sync-keep-both"))
        laptop.app.frame()
        laptop.wait_settled("after Keep both", drive)
        desktop.sync(desk)
        versions = {"laptop": hashlib.sha256(b"version 2 from the laptop, offline\n").hexdigest(),
                    "desktop": hashlib.sha256(b"version 2 from the desktop\n").hexdigest()}
        here, theirs = tree(folder), tree(desk)
        nothing_lost(versions, [here])
        nothing_lost(versions, [theirs])
        expect(here == theirs, "the devices differ: %s" % sorted(set(here) ^ set(theirs)))
        no_silent_failure(laptop, drive)
        return "offline status %s; both versions on both devices: %s" % (offline_status,
                                                                         sorted(here))
    finally:
        laptop.stop()
        stack.stop()


@scenario("delete-vs-edit")
def delete_vs_edit(ctx):
    """A delete on one device against an edit on the other, both ways: the edit wins."""
    stack, drive, laptop, desktop = plain_world(ctx, "delete-vs-edit", desktop=True)
    desk = os.path.join(desktop.root, "folder")
    try:
        folder = laptop.folder(drive)
        write(os.path.join(folder, "a.txt"), b"a, first\n")
        write(os.path.join(folder, "b.txt"), b"b, first\n")
        laptop.wait_settled("a and b up", drive)
        desktop.sync(desk)
        stack.proxy.network("down")
        os.remove(os.path.join(folder, "a.txt"))
        write(os.path.join(folder, "b.txt"), b"b, edited on the laptop\n")
        write(os.path.join(desk, "a.txt"), b"a, edited on the desktop\n")
        os.remove(os.path.join(desk, "b.txt"))
        desktop.sync(desk)
        laptop.wait_passes("passes while offline", 2, drive)
        stack.proxy.network("up")
        laptop.wait_settled("after the network came back", drive)
        desktop.sync(desk)
        here, theirs, there = tree(folder), tree(desk), stack.drive_tree(drive)
        want = {"a.txt": hashlib.sha256(b"a, edited on the desktop\n").hexdigest(),
                "b.txt": hashlib.sha256(b"b, edited on the laptop\n").hexdigest()}
        for place, name in ((here, "the laptop"), (theirs, "the desktop"), (there, "the drive")):
            for key, digest in want.items():
                expect(place.get(key) == digest, "%s: %s is %s, the edit should have won" % (
                    name, key, "gone" if key not in place else "another version"))
        no_silent_failure(laptop, drive)
        return "the edits won on both sides"
    finally:
        laptop.stop()
        stack.stop()


@scenario("rename-storm")
def rename_storm(ctx):
    """100 renames and folder moves while a slow pass runs: the new names arrive, no blob goes
    up again, nothing is lost."""
    stack, drive, laptop, desktop = plain_world(ctx, "rename-storm", desktop=True)
    desk = os.path.join(desktop.root, "folder")
    try:
        folder = laptop.folder(drive)
        versions = {}
        for d in range(10):
            for i in range(20):
                data = ("folder %d file %d\n" % (d, i)).encode("utf-8") * (20 + i)
                write(os.path.join(folder, "f%d" % d, "n%02d.txt" % i), data)
                versions["f%d/n%02d" % (d, i)] = hashlib.sha256(data).hexdigest()
        laptop.wait_settled("200 files up", drive, timeout=180)
        uploaded = sum(blob_writes(stack, drive).values())
        stack.proxy.add(action="delay", ms=150)
        # The storm: files renamed in their folders, folders renamed and moved into others.
        for d in range(0, 10, 2):
            for i in range(0, 20, 2):
                os.rename(os.path.join(folder, "f%d" % d, "n%02d.txt" % i),
                          os.path.join(folder, "f%d" % d, "renamed-%02d.txt" % i))
        for d in range(1, 10, 2):
            os.rename(os.path.join(folder, "f%d" % d), os.path.join(folder, "moved-%d" % d))
        os.makedirs(os.path.join(folder, "nest"))
        for d in range(1, 10, 4):
            os.rename(os.path.join(folder, "moved-%d" % d), os.path.join(folder, "nest",
                                                                         "m%d" % d))
        time.sleep(3)
        stack.proxy.clear()
        laptop.wait_settled("after the storm", drive, timeout=240)
        desktop.sync(desk)
        here, there, theirs = tree(folder), stack.drive_tree(drive), tree(desk)
        nothing_lost(versions, [here])
        expect(here == there, "the folder and the drive differ: %s" % sorted(
            set(here.items()) ^ set(there.items()))[:6])
        expect(here == theirs, "the desktop differs: %s" % sorted(set(here) ^ set(theirs))[:6])
        again = sum(blob_writes(stack, drive).values()) - uploaded
        expect(again == 0, "the renames uploaded %d blobs again (a rename moves no bytes)" % again)
        no_silent_failure(laptop, drive)
        return "200 files, 100 moved; no blob uploaded again"
    finally:
        laptop.stop()
        stack.stop()


@scenario("changed-during")
def changed_during(ctx):
    """A 120 MiB file rewritten while its upload runs: never a torn upload."""
    stack, drive, laptop, _ = plain_world(ctx, "changed-during")
    try:
        folder = laptop.folder(drive)
        path = os.path.join(folder, "movie.bin")
        first = random_file(path, 120 * MIB)
        stack.proxy.add(op="UploadPart", action="stall", nth=3)
        stack.proxy.add(op="PutObject", key=r"^\.azlin/blobs/", action="stall", nth=1)
        deadline = time.time() + 120
        while stack.proxy.held() == 0 and time.time() < deadline:
            time.sleep(0.05)
        expect(stack.proxy.held() > 0, "the upload never started")
        second = random_file(path, 120 * MIB)
        stack.proxy.clear()
        stack.proxy.release()
        laptop.wait_settled("after the rewrite", drive, timeout=300)
        there = stack.drive_tree(drive)
        expect(there.get("movie.bin") in (first, second),
               "TORN: the drive's movie.bin is neither version (%s)" % there.get("movie.bin"))
        expect(there.get("movie.bin") == second, "the drive kept the old version")
        for key in stack.proxy.writes(key=r"^\.azlin/blobs/"):
            blob = os.path.join(stack.bucket_dir(drive), *key.split("/"))
            if os.path.getsize(blob) == 120 * MIB:
                expect(sha256_file(blob) in (first, second),
                       "TORN: the blob %s holds parts of both versions" % key)
        expect(not stack.unfinished_uploads(), "unfinished uploads stay")
        no_silent_failure(laptop, drive)
        return "no torn upload; the second version is on the drive"
    finally:
        laptop.stop()
        stack.stop()


@scenario("long-names")
def long_names(ctx):
    """Long and odd names: they sync, or they are named as not syncable."""
    stack, drive, laptop, _ = plain_world(ctx, "long-names")
    try:
        folder = laptop.folder(drive)
        made = {}
        names = [
            "a" * 250 + ".txt",                                   # 254 bytes
            ("é" * 120) + ".txt",                            # 244 bytes, 124 characters
            "with space and #hash %percent ?question +plus.txt",
            "emoji \U0001F680\U0001F30D.txt",
            "trailing dot.",
            "  leading spaces.txt",
            "CON",
            "x" * 20 + "/" + "deep/" * 60 + "bottom.txt",          # 60 folders deep
            "/".join(["p%03d-" % i + "q" * 40 for i in range(22)]) + "/long-path.txt",
        ]
        for name in names:
            data = ("content of %s\n" % name[:40]).encode("utf-8")
            path = os.path.join(folder, *name.split("/"))
            try:
                write(path, data)
                made[name.strip("/")] = hashlib.sha256(data).hexdigest()
            except OSError as e:
                log("cannot make %r here: %s" % (name[:60], e))
        status = status_after_passes(laptop, drive, 4)
        there = stack.drive_tree(drive)
        missing = [n for n in made if n not in there]
        if missing:
            expect(status != UP_TO_DATE,
                   "silent failure: Up to date while %d names never sync: %s" % (
                       len(missing), [m[:50] for m in missing]))
        return "%d of %d names on the drive; status %s; missing %s" % (
            len(made) - len(missing), len(made), status, [m[:40] for m in missing])
    finally:
        laptop.stop()
        stack.stop()


@scenario("clock-skew")
def clock_skew(ctx):
    """The node's clock 20 minutes ahead; a file dated a day in the future."""
    stack, drive, laptop, _ = plain_world(ctx, "clock-skew")
    try:
        folder = laptop.folder(drive)
        stack.proxy.clock_offset = 20 * 60
        write(os.path.join(folder, "skewed.txt"), b"signed with a clock 20 minutes off\n")
        skewed = status_after_passes(laptop, drive, 2)
        expect(skewed != UP_TO_DATE, "the status says Up to date while every request is refused")
        stack.proxy.clock_offset = 0
        laptop.wait_settled("the clocks agree again", drive)
        future = os.path.join(folder, "from-the-future.txt")
        write(future, b"made on a computer whose clock is a day ahead\n")
        ahead = time.time() + 86400
        os.utime(future, (ahead, ahead))
        laptop.wait_passes("passes over the future file", 3, drive, timeout=150)
        durations = pass_durations(laptop, drive)
        slow = [d for d in durations[-2:] if d > 10]
        no_silent_failure(laptop, drive)
        raw = raw_errors([skewed])
        expect(not raw, "the skew showed as a technical error: %s" % skewed)
        expect(not slow, "a file dated in the future makes every pass wait: %s s" % slow)
        return "skew status: %s; last pass times %s" % (skewed, durations[-3:])
    finally:
        laptop.stop()
        stack.stop()


def pass_durations(laptop, drive_id):
    """Seconds each pass of `drive_id` took (AZDRIVE_SYNC_STARTED to its DONE / FAILED), by the
    times the laptop's watcher saw the lines."""
    out, started = [], None
    for at, line in laptop.timeline():
        if line.startswith("AZDRIVE_SYNC_STARTED %s" % drive_id):
            started = at
        elif started is not None and (line.startswith("AZDRIVE_SYNC_DONE %s" % drive_id) or
                                      line.startswith("AZDRIVE_SYNC_FAILED %s" % drive_id)):
            out.append(round(at - started, 1))
            started = None
    return out


@scenario("quota")
def quota(ctx):
    """The drive's quota is reached mid-sync; then raised."""
    stack, drive, laptop, _ = plain_world(ctx, "quota")
    try:
        folder = laptop.folder(drive)
        bucket = stack.bucket(drive)
        used = lambda: stack.stack.s3.store.stored_bytes(bucket)  # noqa: E731
        stack.proxy.set_quota(bucket, used() + 3 * MIB + 500 * 1024, used)
        versions = {}
        for i in range(8):
            data = os.urandom(MIB)
            write(os.path.join(folder, "q%d.bin" % i), data)
            versions["q%d" % i] = hashlib.sha256(data).hexdigest()
        full = status_after_passes(laptop, drive, 3)
        expect(full != UP_TO_DATE, "the drive is full and the status says Up to date")
        nothing_lost(versions, [tree(folder)])
        raw = raw_errors([full])
        stack.proxy.set_quota(bucket, None)
        laptop.wait_settled("the quota raised", drive)
        nothing_lost(versions, [stack.drive_tree(drive)])
        no_silent_failure(laptop, drive)
        expect(not raw, "the full drive showed as a technical error: %s" % full)
        return "full status: %s" % full
    finally:
        laptop.stop()
        stack.stop()


@scenario("revoked")
def revoked(ctx):
    """This device is removed at the token server mid-sync."""
    stack, drive, laptop, _ = plain_world(ctx, "revoked")
    try:
        stack.enforce_sessions = True
        folder = laptop.folder(drive)
        for i in range(5):
            write(os.path.join(folder, "before%d.txt" % i), b"before %d\n" % i)
        laptop.wait_settled("files before", drive)
        local_before = tree(folder)
        random_file(os.path.join(folder, "big.bin"), 48 * MIB)
        stack.proxy.add(op="UploadPart", action="stall", nth=2)
        deadline = time.time() + 120
        while stack.proxy.held() == 0 and time.time() < deadline:
            time.sleep(0.05)
        with stack.state.lock:
            stack.state.revoke_all(stack.state.drives[drive], lambda f: f["member"] == "laptop")
        stack.proxy.clear()
        stack.proxy.release()
        revoked_at = time.time()
        laptop.app.until("a pass refused", lambda: laptop.failures(drive))
        status = status_after_passes(laptop, drive, 0)
        # A removed device stops: it does not knock every second with refused credentials.
        passes_before = len(laptop.passes(drive))
        time.sleep(10)
        knocked = len(laptop.passes(drive)) - passes_before
        here = tree(folder)
        for key in local_before:
            expect(key in here, "the removal deleted %s here" % key)
        later = [e for e in stack.proxy.requests(op=azchaos.WRITES)
                 if e["t"] > revoked_at + 1 and 200 <= (e.get("status") or 0) < 300]
        expect(not later, "writes landed after the removal: %s" % later[:3])
        expect(status != UP_TO_DATE, "removed, the status says Up to date")
        raw = raw_errors(statuses_seen(laptop, drive))
        expect(not raw, "the removal showed as a technical error: %s" % raw[:2])
        expect(knocked <= 2, "a removed device ran %d passes in 10 s against refused "
                             "credentials" % knocked)
        return "status %s" % status
    finally:
        laptop.stop()
        stack.stop()


@scenario("sleep")
def sleep_mid_upload(ctx):
    """The laptop sleeps mid-upload longer than its credentials live, then wakes."""
    stack, drive, laptop, _ = plain_world(ctx, "sleep", ttl=25)
    try:
        stack.enforce_sessions = True
        folder = laptop.folder(drive)
        digest = random_file(os.path.join(folder, "dream.bin"), 64 * MIB)
        stack.proxy.add(op="UploadPart", action="stall", nth=2)
        deadline = time.time() + 120
        while stack.proxy.held() == 0 and time.time() < deadline:
            time.sleep(0.05)
        expect(stack.proxy.held() > 0, "the upload never started")
        laptop.suspend()
        stack.proxy.clear()
        stack.proxy.release()
        time.sleep(40)
        laptop.resume()
        laptop.wait_settled("after waking", drive, timeout=240)
        there = stack.drive_tree(drive)
        expect(there.get("dream.bin") == digest, "dream.bin is not on the drive after waking")
        parts = [p for p in stack.proxy.requests(op="UploadPart") if p["forwarded"]]
        expect(len(parts) <= 2 * 4, "the 4 parts were sent %d times" % len(parts))
        expect(not stack.unfinished_uploads(), "unfinished uploads stay")
        no_silent_failure(laptop, drive)
        return "woke, refreshed, %d part uploads for 4 parts" % len(parts)
    finally:
        laptop.stop()
        stack.stop()


@scenario("disk-full")
def disk_full(ctx):
    """The folder's disk fills up while files come down."""
    stack = Stack(ctx.folder("disk-full"))
    image = None
    laptop = None
    try:
        desktop = Desktop(ctx, stack)
        drive = desktop.signup("Chaos drive")
        image = CaseSensitiveImage(ctx.folder("disk-full/image"), size_mb=24,
                                   case_sensitive=False)
        # Leave about 6 MiB free.
        free = shutil.disk_usage(image.mount).free
        with open(os.path.join(image.mount, "filler.bin"), "wb") as f:
            f.truncate(0)
            left = max(0, free - 6 * MIB)
            while left > 0:
                n = min(left, 4 * MIB)
                f.write(b"\0" * n)
                left -= n
        desk = os.path.join(desktop.root, "folder")
        versions = {}
        for i in range(5):
            data = os.urandom(64 * 1024)
            write(os.path.join(desk, "small%d.bin" % i), data)
            versions["small%d" % i] = hashlib.sha256(data).hexdigest()
        versions["big"] = random_file(os.path.join(desk, "big.bin"), 12 * MIB)
        desktop.sync(desk)
        laptop = Laptop(ctx, stack, "laptop", [(drive, "Chaos drive")],
                        folder_root=os.path.join(image.mount, "AzDrive"))
        laptop.start()
        status = status_after_passes(laptop, drive, 3)
        folder = laptop.folder(drive)
        here = tree(folder)
        for i in range(5):
            expect(here.get("small%d.bin" % i) == versions["small%d" % i],
                   "small%d.bin did not come down" % i)
        partial = [n for n in os.listdir(folder) if n not in here]
        expect("big.bin" not in here or here["big.bin"] == versions["big"],
               "a partial big.bin is in the folder")
        expect(status != UP_TO_DATE, "the disk is full, big.bin is not here, and the status "
                                     "says Up to date")
        nothing_lost(versions, [stack.drive_tree(drive)])
        desktop.sync(desk)
        nothing_lost(versions, [tree(desk)])
        raw = raw_errors([status])
        expect(not raw, "the full disk showed as a technical error: %s" % status)
        return "status %s; leftovers %s" % (status, partial)
    finally:
        if laptop:
            laptop.stop()
        if image:
            image.detach()
        stack.stop()


@scenario("dns")
def dns(ctx):
    """The node's name never resolves; the node list carries its address."""
    stack = Stack(ctx.folder("dns"))
    laptop = None
    try:
        host = "n1.azchaos.invalid"
        url = "http://%s:%d" % (host, stack.proxy.port)
        stack.state.s3_url = url
        stack.state.set_nodes([azlin_mock_stack.node_entry("n1", url, addresses=["127.0.0.1"])])
        drive = stack.signup("Chaos drive")["drive"]["id"]
        laptop = Laptop(ctx, stack, "laptop", [(drive, "Chaos drive")], session_expires_in=120)
        laptop.start()
        laptop.wait_status("the first pass", UP_TO_DATE, drive, timeout=120)
        write(os.path.join(laptop.folder(drive), "no-dns.txt"), b"over the node's address\n")
        laptop.wait_settled("a file up without DNS", drive)
        expect("no-dns.txt" in stack.drive_tree(drive), "the file did not reach the drive")
        no_silent_failure(laptop, drive)
        return "synced at the node's address (the slow-dns scenario covers a slow name)"
    finally:
        if laptop:
            laptop.stop()
        stack.stop()


@scenario("slow-dns")
def slow_dns(ctx):
    """The node's name answers only after 8 s (a DNS server under load): the requests do not
    wait for it every time - the node's address is used, or the answer kept."""
    resolver = "/etc/resolver/azchaos.test"
    try:
        with open(resolver, "r", encoding="utf-8") as f:
            text = f.read()
    except OSError:
        raise Skip("needs %s (macOS, root): `nameserver 127.0.0.1` and `port 53535`, the port "
                   "this scenario's slow DNS server listens on" % resolver)
    found = re.search(r"^\s*port\s+(\d+)", text, re.M)
    if not found:
        raise Skip("%s names no port" % resolver)
    host = "n1.azchaos.test"
    stack = Stack(ctx.folder("slow-dns"))
    dns_server = azchaos.SlowDns({host: "127.0.0.1"}, delay=8.0, port=int(found.group(1))).start()
    laptop = None
    try:
        url = "http://%s:%d" % (host, stack.proxy.port)
        stack.state.s3_url = url
        stack.state.set_nodes([azlin_mock_stack.node_entry("n1", url, addresses=["127.0.0.1"])])
        drive = stack.signup("Chaos drive")["drive"]["id"]
        laptop = Laptop(ctx, stack, "laptop", [(drive, "Chaos drive")], session_expires_in=120)
        laptop.start()
        laptop.wait_status("the first pass", UP_TO_DATE, drive, timeout=180)
        write(os.path.join(laptop.folder(drive), "slow.txt"), b"over a slow name\n")
        laptop.wait_settled("a file up", drive, timeout=180)
        durations = pass_durations(laptop, drive)
        slow = [d for d in durations[-3:] if d > 6]
        expect(not slow, "passes wait for the slow name every time: %s s" % slow)
        no_silent_failure(laptop, drive)
        return "%d lookups; pass times %s" % (len(dns_server.queries), durations[-3:])
    finally:
        if laptop:
            laptop.stop()
        dns_server.stop()
        stack.stop()


@scenario("iroh-lane")
def iroh_lane(ctx):
    """The node list names an iroh node at a UDP port that swallows everything."""
    import azlin_ed25519  # noqa: PLC0415
    stack = Stack(ctx.folder("iroh-lane"))
    sink = azchaos.UdpSink().start()
    laptop = None
    try:
        secret = os.urandom(32)
        node_id = azlin_ed25519.public_key(secret).hex()
        stack.state.set_nodes([azlin_mock_stack.node_entry(
            "n1", stack.proxy.url, iroh_id=node_id,
            iroh_addrs=["%s:%d" % sink.address])])
        drive = stack.signup("Chaos drive")["drive"]["id"]
        laptop = Laptop(ctx, stack, "laptop", [(drive, "Chaos drive")],
                        env={"AZCLOUD_TRANSPORT": "auto"}, session_expires_in=120)
        started = time.time()
        laptop.start()
        laptop.wait_status("the first pass", UP_TO_DATE, drive, timeout=300)
        first = time.time() - started
        write(os.path.join(laptop.folder(drive), "lane.txt"), b"over https after the lane\n")
        started = time.time()
        laptop.wait_settled("a file up", drive, timeout=300)
        second = time.time() - started
        expect(sink.datagrams > 0, "the lane was never dialed (no datagram reached the sink)")
        expect(first < 30, "the first pass waited %.0f s for a lane that never answers" % first)
        expect(second < 15, "a later pass waited %.0f s: the lane did not rest" % second)
        no_silent_failure(laptop, drive)
        return "first pass %.0f s, later %.0f s, %d datagrams swallowed" % (
            first, second, sink.datagrams)
    finally:
        if laptop:
            laptop.stop()
        sink.stop()
        stack.stop()


def encrypted_world(ctx, name, poll=1, laptop_env=None):
    """A stack, an ENCRYPTED drive made by the azcloud CLI (the desktop: it keeps the drive key;
    its recovery code in a file), AzDrive paired with it and unlocked with that code: (stack,
    drive id, laptop, desktop, code) - the first pass done."""
    stack = Stack(ctx.folder(name))
    desktop = Desktop(ctx, stack)
    laptop = None
    try:
        drive = desktop.signup("Encrypted drive", plaintext=False)
        code = read(os.path.join(desktop.root, "recovery.txt")).decode("utf-8").strip()
        laptop = Laptop(ctx, stack, "laptop", [(drive, "Encrypted drive")], env=laptop_env,
                        poll=poll)
        os.makedirs(laptop.folder(drive), exist_ok=True)
        laptop.start()
        unlock(laptop, drive, code)
        laptop.wait_status("the first pass after the unlock", UP_TO_DATE, drive)
    except BaseException:
        if laptop:
            laptop.stop()
        stack.stop()
        raise
    return stack, drive, laptop, desktop, code


def unlock(laptop, drive_id, code):
    """AzDrive's "Unlock with the recovery code..." of the drive's menu, the code typed."""
    from azdrive_add_e2e import drive_menu, side_drive  # noqa: PLC0415
    app = laptop.app
    app.until("the debug server", lambda: app.op("get_dom_tree"))
    app.must("resize", width=1280.0, height=800.0)
    app.frame(3)
    app.until("the drive's row", lambda: app.has(side_drive(drive_id)))
    drive_menu(app, drive_id, "Unlock with the recovery code…")
    app.frame(3)
    box = "#" + I("unlock-code")
    app.until("the code's box", lambda: app.has(box) or len(app.window_ids()) > 1)
    window = app if app.has(box) else e2e.modal_window(app)
    window.until("the code's box", lambda: window.has(box))
    window.text_input(box, code)
    window.click_exact("Unlock")
    app.until("the drive unlocked", lambda: any(
        "is unlocked on this computer" in t for t in app.texts(True)))


def bucket_keys(stack, drive_id, prefix=""):
    return [k for k in stack.stack.s3.store.keys(stack.bucket(drive_id)) if k.startswith(prefix)]


def data_writes(stack, since=0.0):
    """The proxy's writes under data/ (an encrypted drive's objects) after `since`."""
    return [e for e in stack.proxy.requests(op=("PutObject", "CopyObject",
                                                "CompleteMultipartUpload"))
            if e["key"].startswith("data/") and e["t"] >= since and e["forwarded"]]


@scenario("encrypted-bulk")
def encrypted_bulk(ctx):
    """PACK22's bulk steps on an encrypted drive: unzip 10,000 files, move the folder, delete it,
    rewrite 300 synced files with random bytes."""
    import zipfile  # noqa: PLC0415
    stack, drive, laptop, desktop, _ = encrypted_world(ctx, "encrypted-bulk")
    try:
        folder = laptop.folder(drive)
        many = 10000
        archive = os.path.join(stack.base, "big.zip")
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_STORED) as z:
            for i in range(many):
                z.writestr("big/d%02d/f%05d.txt" % (i % 50, i), ("file %d\n" % i) * (1 + i % 7))
        log_before = len(bucket_keys(stack, drive, ".azlin/meta/log/"))
        data_before = len(bucket_keys(stack, drive, "data/"))
        started = time.time()
        with zipfile.ZipFile(archive) as z:
            z.extractall(folder)
        laptop.wait_settled("10,000 files up", drive, timeout=max(ctx.args.timeout, 900))
        took = time.time() - started
        on_device = [k for k, v in laptop.file_states(drive).items()
                     if k.startswith("big/") and v == "on-device"]
        log_after = len(bucket_keys(stack, drive, ".azlin/meta/log/"))
        data_after = len(bucket_keys(stack, drive, "data/"))
        notes = ["unzip: %d files on-device in %.0f s, log +%d, data/ +%d objects" % (
            len(on_device), took, log_after - log_before, data_after - data_before)]
        log(notes[-1])
        expect(len(on_device) == many, "%d of %d files said on-device" % (len(on_device), many))
        expect(log_after - log_before <= 5, "the drive index grew by %d log entries for one "
                                            "unzip (<= 5 expected)" % (log_after - log_before))
        expect(data_after - data_before <= 50, "%d new objects in data/ for 10,000 small files "
                                               "(packs expected)" % (data_after - data_before))
        # mv: the index only.
        moved_at = time.time()
        log_before = log_after
        os.rename(os.path.join(folder, "big"), os.path.join(folder, "big2"))
        laptop.wait_settled("the move", drive, timeout=600)
        log_after = len(bucket_keys(stack, drive, ".azlin/meta/log/"))
        writes = data_writes(stack, moved_at)
        listed = desktop.az("ls", "big2/").get("objects", [])
        notes.append("mv: log +%d, %d data/ writes, %d files under big2/ at the desktop" % (
            log_after - log_before, len(writes), len(listed)))
        log(notes[-1])
        expect(not writes, "the move wrote %d objects under data/ (a rename moves no bytes)"
               % len(writes))
        expect(log_after - log_before <= 2, "the move took %d log entries" % (
            log_after - log_before))
        # rm -rf: one commit, after the mass-delete question.
        deleted_at = time.time()
        log_before = log_after
        shutil.rmtree(os.path.join(folder, "big2"))
        laptop.app.until("the mass-delete question", lambda: laptop.app.has(
            "#" + I("sync-mass")))
        laptop.app.must("click", selector="#" + I("sync-mass-delete"))
        laptop.app.frame()
        laptop.wait_settled("the delete", drive, timeout=600)
        log_after = len(bucket_keys(stack, drive, ".azlin/meta/log/"))
        single_deletes = [e for e in stack.proxy.requests(op="DeleteObject")
                          if e["t"] >= deleted_at and e["key"].startswith("data/")]
        notes.append("rm -rf: log +%d, %d single DeleteObjects" % (
            log_after - log_before, len(single_deletes)))
        log(notes[-1])
        expect(log_after - log_before <= 2, "the delete took %d log entries" % (
            log_after - log_before))
        expect(len(single_deletes) <= 50, "%d DeleteObject requests, one per file" % len(
            single_deletes))
        # 300 files rewritten with random bytes: the guard holds the uploads.
        for i in range(300):
            write(os.path.join(folder, "text", "t%03d.txt" % i), ("a note %d\n" % i).encode() * 20)
        laptop.wait_settled("300 text files up", drive, timeout=600)
        rewritten_at = time.time()
        for i in range(300):
            write(os.path.join(folder, "text", "t%03d.txt" % i), os.urandom(2000))
        laptop.app.until("the guard's question or status", lambda: laptop.app.has(
            "#" + I("sync-burst")) or "paused" in (laptop.status(drive) or "") or
            "encrypted" in (laptop.status(drive) or "") or "burst" in (laptop.status(drive) or ""))
        time.sleep(3)
        status = laptop.status(drive)
        sample = desktop.az("down", "text/t007.txt", os.path.join(desktop.root, "t007.txt"))
        kept = read(os.path.join(desktop.root, "t007.txt"))
        notes.append("rewrite: status %s; the drive's t007.txt is %s" % (
            status, "the old note" if kept == ("a note 7\n").encode() * 20 else "REPLACED"))
        expect(kept == ("a note 7\n").encode() * 20,
               "the drive took the random bytes of a ransomware-like rewrite")
        return "; ".join(notes)
    finally:
        laptop.stop()
        stack.stop()


@scenario("rotation")
def rotation(ctx):
    """Another device rotates the drive key ("I was hacked") while AzDrive uploads."""
    stack, drive, laptop, desktop, _ = encrypted_world(ctx, "rotation")
    try:
        stack.enforce_sessions = True
        folder = laptop.folder(drive)
        for i in range(5):
            write(os.path.join(folder, "before%d.txt" % i), b"before the rotation %d\n" % i)
        laptop.wait_settled("files before", drive)
        here_before = tree(folder)
        random_file(os.path.join(folder, "during.bin"), 40 * MIB)
        stack.proxy.add(op=("UploadPart", "PutObject"), key=r"^data/", action="stall", nth=1)
        deadline = time.time() + 120
        while stack.proxy.held() == 0 and time.time() < deadline:
            time.sleep(0.05)
        rotated = desktop.az("rotate", "--yes", "--out", os.path.join(desktop.root, "new.txt"))
        rotated_at = time.time()
        stack.proxy.clear()
        stack.proxy.release()
        status = status_after_passes(laptop, drive, 2, timeout=180)
        here = tree(folder)
        for key, digest in here_before.items():
            expect(here.get(key) == digest, "the rotation changed %s here" % key)
        landed = [e for e in data_writes(stack, rotated_at + 1)
                  if 200 <= (e.get("status") or 0) < 300]
        expect(not landed, "AzDrive wrote %d objects after the rotation (under the old key, "
                           "with revoked credentials?)" % len(landed))
        expect(status != UP_TO_DATE, "after the rotation the status says Up to date")
        raw = raw_errors(statuses_seen(laptop, drive))
        expect(not raw, "the rotation showed as a technical error: %s" % raw[:2])
        return "rotated (%s); status %s" % (rotated.get("ok"), status)
    finally:
        laptop.stop()
        stack.stop()


@scenario("ransomware")
def ransomware(ctx):
    """300 files rewritten with random bytes while a big upload runs under latency."""
    stack, drive, laptop, _ = plain_world(ctx, "ransomware")
    try:
        folder = laptop.folder(drive)
        versions = {}
        for i in range(300):
            data = ("a letter, number %d\n" % i).encode() * 30
            write(os.path.join(folder, "letters", "l%03d.txt" % i), data)
            versions["l%03d" % i] = hashlib.sha256(data).hexdigest()
        laptop.wait_settled("300 letters up", drive, timeout=300)
        before = {k: v for k, v in stack.drive_tree(drive).items() if k.startswith("letters/")}
        random_file(os.path.join(folder, "video.bin"), 64 * MIB)
        stack.proxy.add(action="delay", ms=40)
        time.sleep(1.5)
        for i in range(300):
            write(os.path.join(folder, "letters", "l%03d.txt" % i), os.urandom(3000))
        laptop.app.until("the guard", lambda: laptop.app.printed(
            "AZDRIVE_SYNC_BURST", re.escape(drive) + r" .*"))
        stack.proxy.clear()
        status = status_after_passes(laptop, drive, 2, timeout=300)
        after = {k: v for k, v in stack.drive_tree(drive).items() if k.startswith("letters/")}
        expect(after == before, "the drive took %d of the random rewrites" % sum(
            1 for k in before if after.get(k) != before[k]))
        expect("burst" in status or "encrypted" in status,
               "the status does not say the uploads are paused: %s" % status)
        return "guard: %s; the drive kept all 300 letters" % status
    finally:
        laptop.stop()
        stack.stop()


@scenario("many-small")
def many_small(ctx):
    """100,000 small files on an encrypted drive (packing)."""
    stack, drive, laptop, _, _ = encrypted_world(ctx, "many-small")
    try:
        folder = laptop.folder(drive)
        count = ctx.args.many
        for i in range(count):
            write(os.path.join(folder, "m%03d" % (i % 500), "s%06d.txt" % i),
                  ("small file %d\n" % i).encode())
        started = time.time()
        peak = [0.0]

        def watch():
            while laptop.app and laptop.app.alive() and not getattr(watch, "done", False):
                peak[0] = max(peak[0], laptop.rss_mb())
                time.sleep(1)

        threading.Thread(target=watch, daemon=True).start()
        laptop.wait_settled("%d files up" % count, drive, timeout=max(ctx.args.timeout, 3600))
        watch.done = True
        took = time.time() - started
        data = len(bucket_keys(stack, drive, "data/"))
        logs = len(bucket_keys(stack, drive, ".azlin/meta/log/"))
        expect(peak[0] < 2048, "AzDrive used %.0f MB for %d small files" % (peak[0], count))
        expect(data <= count // 100, "%d objects in data/ for %d small files" % (data, count))
        return "%d files in %.0f s, peak %.0f MB, %d data/ objects, %d log entries" % (
            count, took, peak[0], data, logs)
    finally:
        laptop.stop()
        stack.stop()


def sparse_file(path, size, seed=7):
    """A file of `size` bytes that takes little disk: zeros with a random MiB every GiB (APFS
    keeps the holes); its SHA-256, computed as the bytes would read."""
    import random as _random  # noqa: PLC0415
    rng = _random.Random(seed)
    digest = hashlib.sha256()
    with open(path, "wb") as f:
        f.truncate(size)
        position = 0
        while position < size:
            block = bytes(rng.getrandbits(8) for _ in range(4096)) * 256
            block = block[:min(MIB, size - position)]
            f.seek(position)
            f.write(block)
            position += GIB
    with open(path, "rb") as f:
        for piece in iter(lambda: f.read(8 * MIB), b""):
            digest.update(piece)
    return digest.hexdigest()


@scenario("fifty-gb")
def fifty_gb(ctx):
    """A 50 GB file: streamed in parts, memory bounded, resumed after a kill, its hash whole."""
    size = int(ctx.args.big_gb * GIB)
    stack, drive, laptop, _ = plain_world(ctx, "fifty-gb")
    try:
        sink = stack.proxy.sink(r"^\.azlin/blobs/")
        folder = laptop.folder(drive)
        path = os.path.join(folder, "huge.bin")
        laptop.stop()
        digest = sparse_file(path, size)
        laptop.start()
        peak = [0.0]
        parts_target = 40

        def held_parts():
            return len([e for e in stack.proxy.requests(op="UploadPart") if e["status"] == 200])

        deadline = time.time() + max(ctx.args.timeout, 1800)
        while held_parts() < parts_target and time.time() < deadline:
            peak[0] = max(peak[0], laptop.rss_mb())
            time.sleep(1)
        laptop.kill()
        before_kill = held_parts()
        laptop.start()
        while time.time() < deadline:
            peak[0] = max(peak[0], laptop.rss_mb())
            if any(k.startswith(".azlin/blobs/") for k in sink.objects):
                break
            time.sleep(2)
        laptop.wait_settled("the 50 GB file", drive, timeout=1800)
        objects = {k: v for k, v in sink.objects.items()}
        expect(objects, "the big file never completed")
        whole = list(objects.values())[0]
        parts = held_parts()
        expected = (size + 16 * MIB - 1) // (16 * MIB)
        expect(whole["size"] == size, "the object holds %d bytes of %d" % (whole["size"], size))
        expect(whole["sha256"] in (digest, None), "the object's bytes are not the file's")
        expect(peak[0] < 1536, "AzDrive used %.0f MB while sending a %d GB file" % (
            peak[0], size // GIB))
        expect(parts <= expected + 8,
               "%d parts sent for %d (%d before the kill): the upload started again after the "
               "kill instead of resuming" % (parts, expected, before_kill))
        return "%d GB: %d parts (%d before the kill), peak %.0f MB" % (
            size // GIB, parts, before_kill, peak[0])
    finally:
        laptop.stop()
        stack.stop()


class CaseSensitiveImage:
    """A small case-sensitive APFS disk image, attached without root (hdiutil)."""

    def __init__(self, folder, size_mb=64, case_sensitive=True):
        if sys.platform != "darwin":
            raise Skip("disk images are made with hdiutil (macOS)")
        self.image = os.path.join(folder, "disk.dmg")
        self.mount = os.path.join(folder, "mnt")
        os.makedirs(self.mount, exist_ok=True)
        fs = "Case-sensitive APFS" if case_sensitive else "APFS"
        made = subprocess.run(["hdiutil", "create", "-size", "%dm" % size_mb, "-fs", fs,
                               "-volname", "azchaos", "-ov", self.image],
                              capture_output=True, text=True)
        if made.returncode != 0:
            raise Skip("hdiutil create failed: %s" % made.stderr.strip())
        attached = subprocess.run(["hdiutil", "attach", "-nobrowse", "-mountpoint", self.mount,
                                   self.image], capture_output=True, text=True)
        if attached.returncode != 0:
            raise Skip("hdiutil attach failed: %s" % attached.stderr.strip())

    def detach(self):
        subprocess.run(["hdiutil", "detach", "-force", self.mount], capture_output=True)
        try:
            os.remove(self.image)
        except OSError:
            pass


# ---------------------------------------------------------------------------------------------
# The run
# ---------------------------------------------------------------------------------------------


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", help="comma-separated scenario names")
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--bin", help="AzDrive (default: target/release/AzDrive)")
    parser.add_argument("--azcloud", help="the azcloud CLI")
    parser.add_argument("--debug-port", type=int, default=8851)
    parser.add_argument("--timeout", type=int, default=300, help="seconds per wait")
    parser.add_argument("--keep", action="store_true", help="keep the run's folder")
    parser.add_argument("--big-gb", type=float, default=50.0, help="fifty-gb: the file's size")
    parser.add_argument("--many", type=int, default=100000, help="many-small: how many files")
    args = parser.parse_args(argv)
    names = [n for n, _ in SCENARIOS]
    if args.list:
        print("\n".join(names))
        return 0
    chosen = names if not args.only else [n.strip() for n in args.only.split(",") if n.strip()]
    unknown = [n for n in chosen if n not in names]
    if unknown:
        parser.error("unknown scenarios: %s (--list)" % ", ".join(unknown))
    logs = tempfile.mkdtemp(prefix="azchaos-e2e-")
    ctx = Ctx(args, logs)
    log("AzDrive %s, azcloud %s, logs in %s" % (ctx.binary, ctx.azcloud, logs))
    results = []
    for name, fn in SCENARIOS:
        if name not in chosen:
            continue
        log("---- %s" % name)
        started = time.time()
        try:
            said = fn(ctx)
            results.append((name, "PASS", said or ""))
        except Skip as e:
            results.append((name, "SKIP", str(e)))
        except Failure as e:
            results.append((name, "FAIL", str(e)))
        except Exception as e:  # noqa: BLE001 - a scenario's own bug is a failure too
            results.append((name, "FAIL", "%s\n%s" % (e, traceback.format_exc())))
        log("%s %s (%.0f s): %s" % (name, results[-1][1], time.time() - started,
                                    results[-1][2][:2000]))
    print()
    for name, verdict, said in results:
        print("%-18s %s  %s" % (name, verdict, said.splitlines()[0][:300] if said else ""))
    failed = any(v == "FAIL" for _, v, _ in results)
    if not args.keep and not failed:
        shutil.rmtree(logs, ignore_errors=True)
    else:
        print("logs: %s" % logs)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
