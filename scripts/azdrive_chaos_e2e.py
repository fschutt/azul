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
                    its address: syncs; slow DNS needs /etc/resolver/azchaos.test (skipped without)
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
        self.enforce_sessions = False
        original = self.state.bundle

        def bundle(drive, token):
            made = original(drive, token)
            family = token[3:].split(".")[0] if token.startswith("dt_") else None
            expires = time.mktime(time.strptime(made["credentials"]["expires_at"],
                                                "%Y-%m-%dT%H:%M:%SZ")) - time.timezone
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
                 folder_root=None):
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
            keyring["azul-storage/s3/" + drive_id] = azlin_session(bundle)
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
        if wait:
            self.app.until("AzDrive up", lambda: self.app.printed("AZDRIVE_PLACE", r"this-pc"))
        return self

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
        return self.lines("AZDRIVE_SYNC_DONE", drive_id) + self.lines("AZDRIVE_SYNC_FAILED",
                                                                       drive_id)

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
            if not moved and "FAILED" not in lines[-1] and self.status(drive_id) == UP_TO_DATE:
                calm += 1
                if calm >= quiet:
                    return
            else:
                calm = 0
        raise Failure("%s: never settled (status %s, last passes %s)" % (
            what, self.status(drive_id), self.passes(drive_id)[-3:]))


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
                    # Straight to the S3: the proxy's faults are the laptop's.
                    "AZLIN_S3_URL": stack.stack.s3_url,
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
            laptop.wait_passes("a pass over the special files", 3, drive)
            status = laptop.status(drive)
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
