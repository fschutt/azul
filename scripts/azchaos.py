#!/usr/bin/env python3
"""A fault harness for AzDrive's sync: a proxy between the app and the local S3 (or the mock
Azlin stack) that injects what real networks and nodes do, plus a UDP sink for the iroh lane and a
slow DNS server. Python's standard library only.

The proxy forwards every request byte for byte (the Host header stays, so SigV4 still verifies
at the server behind it) and logs it by its S3 operation. Faults are rules a test adds; the first
rule that takes a request decides what happens to it:

    delay        wait `ms` (+ up to `jitter` ms) before forwarding
    refuse       reset the connection at once (nothing read, nothing forwarded)
    reset_before read the request, reset, forward nothing (the request is lost)
    reset_after  forward, read the answer, reset (the write landed, its answer is lost)
    stall        hold the request before forwarding until `release()` (or `seconds`)
    stall_after  forward, hold the answer until `release()` (or `seconds`)
    cut_body     forward, send the answer's head and `bytes` of its body, reset
    stall_body   forward, send the head and `bytes` of the body, hold the rest
    corrupt      forward, flip one byte in the middle of the answer's body
    throttle     forward, both bodies at `bps` bytes a second
    status       answer `status` with an S3 error body (`code`, `message`, `headers`)
    azlin        answer an Azlin node's error `azlin_error` with the node's status and S3 code
                 (azul-apps iso azlin-proto errors.rs) and its Retry-After
    stale_map    answer 409 with `x-azlin-map: <epoch> <hash>` (a client's map is older)

A rule matches by `op` (PutObject, UploadPart, ...), `method`, `key` and `bucket` (regular
expressions, searched), `query` (searched in the raw query), `client` (a substring of the
request's x-amz-security-token or access key id: one device of several); it takes the `nth`
match (one number or a list, from 1), every match after the first `after`, at most `times`
times, with `probability` (repeatable with `seed`).

The network as a whole: `network("down")` resets every connection, also the ones in flight;
`"blackhole"` takes requests and never answers (a dead Wi-Fi: the client times out); `flap(up,
down)` alternates. The node's side: `clock_offset` (seconds the node's clock is ahead: a request
signed more than 15 minutes off its clock is answered 403 RequestTimeTooSkewed with the node's
Date), `set_quota(bucket, bytes, used)` (the write that crosses it answers quota_exceeded),
`sink(pattern)` (uploads of matching keys are taken and hashed, never stored: a 50 GB upload on
a laptop's disk).

Another process steers a running proxy over HTTP on the same port: `POST /__azchaos/` with
{"add": {rule}}, {"clear": true}, {"release": true}, {"network": "down"}, {"flap": [up, down]},
{"clock_offset": secs}, {"quota": [bucket, bytes]}; `GET /__azchaos/log` (the requests),
`GET /__azchaos/health`.
"""

import argparse
import email.utils
import hashlib
import json
import os
import random
import re
import socket
import struct
import sys
import threading
import time
import urllib.parse
import uuid
from xml.sax.saxutils import escape

USAGE = """\
azchaos.py proxy --upstream URL [--host H] [--port P] [--rules FILE] [--log FILE]
    a fault proxy in front of URL (the local S3 or the mock stack's); FILE: a JSON list of rules
    (see the module's doc); steer it at http://H:P/__azchaos/
azchaos.py udp-sink [--port P]
    swallows every datagram (an iroh lane that never answers)
azchaos.py slow-dns --name NAME=IP [--delay S] [--port P]
    answers A records after S seconds (point /etc/resolver/<domain> at it: nameserver 127.0.0.1,
    port P)
"""

# An Azlin node's errors (x-azlin-error): (HTTP status, S3 code, Retry-After), as azlin-proto's
# errors.rs sends them.
AZLIN_ERRORS = {
    "unavailable": (503, "SlowDown", 0),
    "maintenance": (503, "ServiceUnavailable", 30),
    "quota_exceeded": (403, "QuotaExceeded", None),
    "read_only_unpaid": (403, "AccessDenied", None),
    "credentials_revoked": (403, "InvalidAccessKeyId", None),
    "object_too_large": (400, "EntityTooLarge", None),
    "name_conflict": (412, "PreconditionFailed", None),
    "link_blocked": (403, "AccessDenied", None),
    "integrity": (500, "InternalError", None),
    "delete_paused": (503, "SlowDown", 5),
    "wrong_block": (301, "PermanentRedirect", None),
    "internal": (500, "InternalError", None),
    "drive_banned": (403, "AccessDenied", None),
}

# How far a request's x-amz-date may be off the node's clock (S3's 15 minutes).
MAX_SKEW_SECONDS = 15 * 60
# The path prefix of the proxy's own control requests.
CONTROL = "/__azchaos"
# Answers bigger than this are not buffered for `corrupt` (they pass unchanged).
CORRUPT_LIMIT = 64 * 1024 * 1024
CHUNK = 1024 * 1024
# Parts a sink keeps in memory while an earlier part has not arrived yet.
SINK_PENDING_LIMIT = 512 * 1024 * 1024
WRITES = ("PutObject", "UploadPart", "CompleteMultipartUpload", "CopyObject")


class Reset(Exception):
    """The client's connection is to be reset (or was)."""


# ---------------------------------------------------------------------------------------------
# What a request is
# ---------------------------------------------------------------------------------------------


def s3_op(method, path, query, headers):
    """(operation, bucket, key) of a path-style S3 request; `path` still percent-encoded, `query`
    a dict, `headers` a dict with lowercase or any-case names."""
    lower = {k.lower(): v for k, v in (headers or {}).items()}
    raw = path.split("?", 1)[0]
    parts = raw.lstrip("/").split("/", 1)
    bucket = urllib.parse.unquote(parts[0]) if parts and parts[0] else ""
    key = urllib.parse.unquote(parts[1]) if len(parts) > 1 else ""
    method = method.upper()
    if not bucket:
        return ("ListBuckets" if method == "GET" else "Service", "", "")
    if not key:
        if method == "GET":
            if "uploads" in query:
                return ("ListMultipartUploads", bucket, "")
            if "location" in query:
                return ("GetBucketLocation", bucket, "")
            return ("ListObjectsV2" if query.get("list-type") == "2" else "ListObjects",
                    bucket, "")
        if method == "HEAD":
            return ("HeadBucket", bucket, "")
        if method == "PUT":
            return ("CreateBucket", bucket, "")
        if method == "POST" and "delete" in query:
            return ("DeleteObjects", bucket, "")
        if method == "DELETE":
            return ("DeleteBucket", bucket, "")
        return ("Bucket" + method.title(), bucket, "")
    if method == "GET":
        return ("ListParts" if "uploadId" in query else "GetObject", bucket, key)
    if method == "HEAD":
        return ("HeadObject", bucket, key)
    if method == "PUT":
        if "partNumber" in query and "uploadId" in query:
            return ("UploadPartCopy" if "x-amz-copy-source" in lower else "UploadPart",
                    bucket, key)
        if "x-amz-copy-source" in lower:
            return ("CopyObject", bucket, key)
        return ("PutObject", bucket, key)
    if method == "POST":
        if "uploads" in query:
            return ("CreateMultipartUpload", bucket, key)
        if "uploadId" in query:
            return ("CompleteMultipartUpload", bucket, key)
        return ("PostObject", bucket, key)
    if method == "DELETE":
        return ("AbortMultipartUpload" if "uploadId" in query else "DeleteObject", bucket, key)
    return ("Object" + method.title(), bucket, key)


class Request:
    """One request as the proxy read its head."""

    def __init__(self, head):
        lines = head.decode("latin-1").split("\r\n")
        self.method, self.target, self.version = (lines[0].split(" ", 2) + ["", ""])[:3]
        self.headers = []
        for line in lines[1:]:
            if ":" in line:
                name, value = line.split(":", 1)
                self.headers.append((name.strip(), value.strip()))
        self.head = head
        path, _, raw_query = self.target.partition("?")
        self.path = path
        self.raw_query = raw_query
        self.query = {k: v for k, v in urllib.parse.parse_qsl(raw_query, keep_blank_values=True)}
        self.op, self.bucket, self.key = s3_op(self.method, path, self.query, dict(self.headers))

    def header(self, name):
        name = name.lower()
        for k, v in self.headers:
            if k.lower() == name:
                return v
        return None

    @property
    def client(self):
        """What tells the device apart: its session token, else its access key id."""
        token = self.header("x-amz-security-token") or ""
        auth = self.header("authorization") or ""
        found = re.search(r"Credential=([^/,\s]+)", auth)
        return token + " " + (found.group(1) if found else "")

    @property
    def amz_date(self):
        text = self.header("x-amz-date")
        if not text:
            return None
        try:
            return time.mktime(time.strptime(text, "%Y%m%dT%H%M%SZ")) - time.timezone
        except ValueError:
            return None

    def body_length(self):
        """The request body's framing: (length or None, chunked)."""
        if (self.header("transfer-encoding") or "").lower() == "chunked":
            return None, True
        length = self.header("content-length")
        return (int(length) if length else 0), False

    def wants_close(self):
        return (self.header("connection") or "").lower() == "close" or self.version == "HTTP/1.0"


# ---------------------------------------------------------------------------------------------
# Faults
# ---------------------------------------------------------------------------------------------

ACTIONS = ("delay", "refuse", "reset_before", "reset_after", "stall", "stall_after", "cut_body",
           "stall_body", "corrupt", "throttle", "status", "azlin", "stale_map")


class Fault:
    """One rule: what it matches, which of the matches it takes, what it does."""

    def __init__(self, action, op=None, method=None, key=None, bucket=None, query=None,
                 client=None, nth=None, after=0, times=None, probability=None, seed=None,
                 **params):
        if action not in ACTIONS:
            raise ValueError("unknown action %r (one of %s)" % (action, ", ".join(ACTIONS)))
        self.action = action
        self.ops = None if op is None else ([op] if isinstance(op, str) else list(op))
        self.method = method.upper() if method else None
        self.key = re.compile(key) if key else None
        self.bucket = re.compile(bucket) if bucket else None
        self.query = re.compile(query) if query else None
        self.client = client
        self.nth = None if nth is None else ({nth} if isinstance(nth, int) else set(nth))
        self.after = int(after or 0)
        self.times = times
        self.probability = probability
        self.random = random.Random(seed)
        self.params = params
        self.seen = 0
        self.applied = 0
        self.lock = threading.Lock()

    def describe(self):
        out = {"action": self.action, "applied": self.applied, "seen": self.seen}
        if self.ops:
            out["op"] = self.ops
        if self.key:
            out["key"] = self.key.pattern
        out.update(self.params)
        return out

    def matches(self, request):
        if self.ops and request.op not in self.ops:
            return False
        if self.method and request.method.upper() != self.method:
            return False
        if self.key and not self.key.search(request.key):
            return False
        if self.bucket and not self.bucket.search(request.bucket):
            return False
        if self.query and not self.query.search(request.raw_query):
            return False
        if self.client and self.client not in request.client:
            return False
        return True

    def rolls(self):
        """One throw of this rule's dice: whether a probability rule takes the request."""
        return self.random.random() < (1.0 if self.probability is None else self.probability)

    def take(self, request):
        """Whether this rule acts on `request` (counted)."""
        if not self.matches(request):
            return False
        with self.lock:
            self.seen += 1
            if self.seen <= self.after:
                return False
            if self.nth is not None and self.seen not in self.nth:
                return False
            if self.times is not None and self.applied >= self.times:
                return False
            if self.probability is not None and not self.rolls():
                return False
            self.applied += 1
            return True

    def param(self, name, default=None):
        return self.params.get(name, default)


class Sink:
    """Uploads the proxy takes itself for keys matching `pattern`: each object's size, SHA-256
    and part count, never its bytes (`objects`, by key)."""

    def __init__(self, pattern):
        self.pattern = re.compile(pattern)
        self.objects = {}
        self.uploads = {}
        self.lock = threading.Lock()

    @staticmethod
    def take_part(upload, number, data, etag):
        """Keeps part `number`'s size and ETag and feeds the object's SHA-256 in part order: a
        part ahead of its turn waits in memory (a few, as uploads run some parts at once); a
        part sent again with other bytes after its turn makes the hash unknowable."""
        old = upload["parts"].get(number)
        upload["parts"][number] = {"size": len(data), "etag": etag}
        if number < upload["next"]:
            if old is None or old["etag"] != etag:
                upload["broken"] = True
            return
        upload["pending"][number] = data
        while upload["next"] in upload["pending"]:
            upload["hasher"].update(upload["pending"].pop(upload["next"]))
            upload["next"] += 1
        if sum(len(d) for d in upload["pending"].values()) > SINK_PENDING_LIMIT:
            upload["broken"] = True
            upload["pending"].clear()

    @staticmethod
    def whole_hash(upload, listed):
        """The SHA-256 of the completed object; None when the parts were not 1..n each hashed
        in its turn."""
        if upload["broken"] or listed != list(range(1, len(listed) + 1)):
            return None
        if upload["next"] != len(listed) + 1:
            return None
        return upload["hasher"].hexdigest()

    def takes(self, request):
        return bool(self.pattern.search(request.key)) and request.op in (
            "PutObject", "CreateMultipartUpload", "UploadPart", "ListParts",
            "CompleteMultipartUpload", "AbortMultipartUpload", "HeadObject", "GetObject")


# ---------------------------------------------------------------------------------------------
# Sockets
# ---------------------------------------------------------------------------------------------


def reset(sock):
    """Closes `sock` with a TCP reset (SO_LINGER 0): the peer sees "connection reset"."""
    try:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
    except OSError:
        pass
    try:
        sock.close()
    except OSError:
        pass


class Reader:
    """Buffered reads from a socket."""

    def __init__(self, sock):
        self.sock = sock
        self.buf = b""

    def fill(self):
        data = self.sock.recv(CHUNK)
        if not data:
            raise EOFError("closed")
        self.buf += data

    def head(self):
        """The bytes up to and including the blank line; None at a clean end."""
        while b"\r\n\r\n" not in self.buf:
            try:
                self.fill()
            except EOFError:
                if not self.buf:
                    return None
                raise
        head, _, rest = self.buf.partition(b"\r\n\r\n")
        self.buf = rest
        return head + b"\r\n\r\n"

    def exactly(self, n):
        """Up to `n` bytes in pieces (a generator)."""
        left = n
        while left > 0:
            if not self.buf:
                self.fill()
            piece, self.buf = self.buf[:left], self.buf[left:]
            left -= len(piece)
            yield piece

    def line(self):
        while b"\r\n" not in self.buf:
            self.fill()
        line, _, rest = self.buf.partition(b"\r\n")
        self.buf = rest
        return line + b"\r\n"

    def chunked(self):
        """A chunked body's raw bytes in pieces, the framing kept."""
        while True:
            size_line = self.line()
            yield size_line
            size = int(size_line.split(b";")[0].strip() or b"0", 16)
            if size == 0:
                while True:
                    trailer = self.line()
                    yield trailer
                    if trailer == b"\r\n":
                        return
            yield from self.exactly(size)
            yield self.line()

    def rest(self):
        """Everything until the peer closes."""
        if self.buf:
            piece, self.buf = self.buf, b""
            yield piece
        while True:
            data = self.sock.recv(CHUNK)
            if not data:
                return
            yield data


def unchunk(pieces):
    """The payload of a chunked body's raw pieces (for hashing)."""
    data = b"".join(pieces)
    out = b""
    while data:
        line, _, data = data.partition(b"\r\n")
        size = int(line.split(b";")[0].strip() or b"0", 16)
        if size == 0:
            break
        out += data[:size]
        data = data[size + 2:]
    return out


class Response:
    """An answer's head as read from the upstream."""

    def __init__(self, head, method):
        lines = head.decode("latin-1").split("\r\n")
        first = lines[0].split(" ", 2)
        self.version = first[0]
        self.status = int(first[1]) if len(first) > 1 and first[1].isdigit() else 0
        self.head = head
        self.headers = {}
        for line in lines[1:]:
            if ":" in line:
                name, value = line.split(":", 1)
                self.headers[name.strip().lower()] = value.strip()
        self.method = method

    def framing(self):
        """('none' | 'length' | 'chunked' | 'close', length)."""
        if self.method == "HEAD" or self.status in (204, 304) or 100 <= self.status < 200:
            return "none", 0
        if self.headers.get("transfer-encoding", "").lower() == "chunked":
            return "chunked", None
        if "content-length" in self.headers:
            return "length", int(self.headers["content-length"])
        return "close", None

    def closes(self):
        return (self.headers.get("connection", "").lower() == "close"
                or self.version == "HTTP/1.0" and
                self.headers.get("connection", "").lower() != "keep-alive")


def body_pieces(reader, framing, length):
    kind = framing
    if kind == "none":
        return iter(())
    if kind == "length":
        return reader.exactly(length)
    if kind == "chunked":
        return reader.chunked()
    return reader.rest()


def error_body(code, message, resource="", extra=None):
    items = "".join("<%s>%s</%s>" % (k, escape(str(v)), k) for k, v in (extra or {}).items())
    return ("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Error><Code>%s</Code><Message>%s"
            "</Message><Resource>%s</Resource>%s<RequestId>azchaos-%s</RequestId></Error>" % (
                escape(code), escape(message), escape(resource), items,
                uuid.uuid4().hex[:12])).encode("utf-8")


def answer_bytes(status, body=b"", headers=None, reason=None):
    reason = reason or {200: "OK", 301: "Moved Permanently", 304: "Not Modified",
                        400: "Bad Request", 403: "Forbidden", 404: "Not Found",
                        409: "Conflict", 412: "Precondition Failed", 500: "Internal Server Error",
                        503: "Service Unavailable"}.get(status, "Status")
    out = ["HTTP/1.1 %d %s" % (status, reason)]
    sent = {k.lower() for k in (headers or {})}
    for name, value in (headers or {}).items():
        out.append("%s: %s" % (name, value))
    if "content-length" not in sent:
        out.append("Content-Length: %d" % len(body))
    if "date" not in sent:
        out.append("Date: %s" % email.utils.formatdate(usegmt=True))
    return ("\r\n".join(out) + "\r\n\r\n").encode("latin-1") + body


# ---------------------------------------------------------------------------------------------
# The proxy
# ---------------------------------------------------------------------------------------------


class ChaosProxy:
    """The fault proxy in front of `upstream` (http://host:port)."""

    def __init__(self, upstream, host="127.0.0.1", port=0, log_path=None, verbose=False):
        parsed = urllib.parse.urlsplit(upstream)
        self.upstream = (parsed.hostname, parsed.port or 80)
        self.host = host
        self.port = port
        self.log_path = log_path
        self.verbose = verbose
        self.faults = []
        self.sinks = []
        self.clock_offset = 0
        self.quotas = {}
        self._state = "up"
        self._log = []
        self._lock = threading.Lock()
        self._released = threading.Event()
        self._held = 0
        self._clients = set()
        self._listener = None
        self._control_listener = None
        self.control_port = None
        self._stopping = False
        self._flap = None

    # -- life -------------------------------------------------------------------------------

    def start(self):
        self._open()
        self.port = self._listener.getsockname()[1]
        # The control port stays open whatever the network does: a test can always bring the
        # network back up.
        control_listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        control_listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        control_listener.bind((self.host, 0))
        control_listener.listen(16)
        self._control_listener = control_listener
        self.control_port = control_listener.getsockname()[1]
        threading.Thread(target=self._accept, args=(control_listener, True),
                         name="azchaos-control", daemon=True).start()
        return self

    def _open(self):
        """Listens on the proxy's port (the same one again after the network was down)."""
        listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind((self.host, self.port))
        listener.listen(128)
        self._listener = listener
        threading.Thread(target=self._accept, args=(listener, False), name="azchaos-accept",
                         daemon=True).start()

    def _close(self):
        """Stops listening: a connect is refused, as on a network that is down."""
        listener, self._listener = self._listener, None
        if listener is not None:
            try:
                listener.close()
            except OSError:
                pass

    @property
    def address(self):
        return (self.host, self.port)

    @property
    def url(self):
        return "http://%s:%d" % (self.host, self.port)

    @property
    def control_url(self):
        return "http://%s:%d" % (self.host, self.control_port)

    def stop(self):
        self._stopping = True
        self.stop_flap()
        self._released.set()
        self._close()
        try:
            self._control_listener.close()
        except OSError:
            pass
        with self._lock:
            clients = list(self._clients)
        for sock in clients:
            reset(sock)

    # -- the test's handles -----------------------------------------------------------------

    def add(self, **rule):
        """Adds a rule (keyword arguments as the module's doc lists); returns it."""
        fault = Fault(**rule)
        with self._lock:
            self.faults.append(fault)
        return fault

    def clear(self):
        """Drops every rule (requests already held stay held until `release`)."""
        with self._lock:
            self.faults = []

    def release(self):
        """Lets every held request and answer go on."""
        self._released.set()
        time.sleep(0.01)
        self._released = threading.Event()

    def held(self):
        with self._lock:
            return self._held

    def network(self, state, kill=True):
        """"up", "down" (connects refused; with `kill` the connections in flight reset too) or
        "blackhole" (requests taken, never answered)."""
        if state not in ("up", "down", "blackhole"):
            raise ValueError("network state %r" % state)
        with self._lock:
            before, self._state = self._state, state
            clients = list(self._clients) if (kill and state == "down") else []
        if state == "down" and before != "down":
            self._close()
        if state != "down" and before == "down" and not self._stopping:
            self._open()
        for sock in clients:
            reset(sock)
        if state == "up":
            self._released.set()
            time.sleep(0.01)
            self._released = threading.Event()

    def state(self):
        with self._lock:
            return self._state

    def flap(self, up, down, mode="down", count=None):
        """Alternates `up` seconds up and `down` seconds `mode`, from up; `count` flaps or until
        `stop_flap`."""
        self.stop_flap()
        stop = threading.Event()

        def run():
            n = 0
            while not stop.is_set() and (count is None or n < count):
                if stop.wait(up):
                    break
                self.network(mode)
                if stop.wait(down):
                    break
                self.network("up")
                n += 1
            self.network("up")

        thread = threading.Thread(target=run, name="azchaos-flap", daemon=True)
        self._flap = (stop, thread)
        thread.start()

    def stop_flap(self):
        if self._flap:
            stop, thread = self._flap
            stop.set()
            thread.join(5)
            self._flap = None
            self.network("up")

    def set_quota(self, bucket, quota, used=None):
        """The bucket's quota of stored bytes (`used()` says what it stores now); None lifts it."""
        with self._lock:
            if quota is None:
                self.quotas.pop(bucket, None)
            else:
                self.quotas[bucket] = (int(quota), used or (lambda: 0))

    def sink(self, pattern):
        sink = Sink(pattern)
        with self._lock:
            self.sinks.append(sink)
        return sink

    def requests(self, op=None, key=None):
        with self._lock:
            out = [dict(e) for e in self._log]
        if op:
            ops = {op} if isinstance(op, str) else set(op)
            out = [e for e in out if e["op"] in ops]
        if key:
            pattern = re.compile(key)
            out = [e for e in out if pattern.search(e["key"])]
        return out

    def clear_log(self):
        with self._lock:
            self._log = []

    def writes(self, key=None):
        """The writes that landed at the server (forwarded, 2xx), by key: how often each."""
        counts = {}
        for e in self.requests(op=("PutObject", "CompleteMultipartUpload", "CopyObject"), key=key):
            if e["forwarded"] and 200 <= (e.get("upstream_status") or 0) < 300:
                counts[e["key"]] = counts.get(e["key"], 0) + 1
        return counts

    # -- the loop ---------------------------------------------------------------------------

    def _accept(self, listener, control_only):
        while not self._stopping:
            try:
                sock, _ = listener.accept()
            except OSError:
                return
            if not control_only and self.state() == "down":
                reset(sock)
                continue
            with self._lock:
                if not control_only:
                    self._clients.add(sock)
            threading.Thread(target=self._serve, args=(sock, control_only),
                             name="azchaos-conn", daemon=True).start()

    def _record(self, entry):
        with self._lock:
            self._log.append(entry)
        if self.log_path:
            with open(self.log_path, "a", encoding="utf-8") as f:
                f.write(json.dumps(entry) + "\n")
        if self.verbose:
            print("[azchaos] %s" % json.dumps(entry), flush=True)

    def _wait(self, seconds=None):
        """Holds the calling request until `release`, the network comes up after a blackhole,
        `seconds` pass, or the proxy stops. Counted in `held()`."""
        released = self._released
        with self._lock:
            self._held += 1
        try:
            end = None if seconds is None else time.time() + seconds
            while not self._stopping:
                left = 0.05 if end is None else min(0.05, end - time.time())
                if left <= 0:
                    return
                if released.wait(left):
                    return
                if self.state() == "down":
                    raise Reset("the network went down")
        finally:
            with self._lock:
                self._held -= 1

    def _serve(self, sock, control_only=False):
        reader = Reader(sock)
        try:
            while not self._stopping:
                try:
                    head = reader.head()
                except (EOFError, OSError):
                    return
                if head is None:
                    return
                request = Request(head)
                if control_only or request.path.startswith(CONTROL):
                    self._control(sock, reader, request)
                    if request.wants_close():
                        return
                    continue
                keep = self._one(sock, reader, request)
                if not keep:
                    return
        except Reset:
            reset(sock)
        except (OSError, EOFError, ValueError):
            try:
                sock.close()
            except OSError:
                pass
        finally:
            with self._lock:
                self._clients.discard(sock)
            try:
                sock.close()
            except OSError:
                pass

    def _pick(self, request):
        with self._lock:
            faults = list(self.faults)
        for fault in faults:
            if fault.take(request):
                return fault
        return None

    def _read_body(self, reader, request):
        length, chunked = request.body_length()
        if chunked:
            return list(reader.chunked())
        return list(reader.exactly(length))

    def _one(self, sock, reader, request):
        """Handles one request; whether the connection stays."""
        started = time.time()
        entry = {"t": round(started, 3), "op": request.op, "method": request.method,
                 "bucket": request.bucket, "key": request.key, "query": request.raw_query,
                 "status": None, "upstream_status": None, "sent": 0, "received": 0,
                 "fault": None, "forwarded": False, "client": request.client.strip()[:48]}
        try:
            return self._handle(sock, reader, request, entry)
        finally:
            entry["ms"] = int((time.time() - started) * 1000)
            self._record(entry)

    def _send(self, sock, data, bps=None):
        if not bps:
            sock.sendall(data)
            return
        step = max(1, int(bps / 20))
        for i in range(0, len(data), step):
            sock.sendall(data[i:i + step])
            time.sleep(step / float(bps))

    def _answer(self, sock, request, entry, status, body=b"", headers=None):
        self._send(sock, answer_bytes(status, body, headers))
        entry["status"] = status
        entry["received"] = len(body)
        return not request.wants_close()

    def _handle(self, sock, reader, request, entry):
        state = self.state()
        if state == "down":
            entry["fault"] = "network-down"
            raise Reset("down")
        fault = self._pick(request)
        action = fault.action if fault else None
        entry["fault"] = action
        if state == "blackhole":
            entry["fault"] = "network-blackhole"
            self._read_body(reader, request)
            self._wait()
            raise Reset("blackhole lifted")
        if action == "refuse":
            raise Reset("refused")
        if action == "delay":
            jitter = fault.param("jitter", 0)
            time.sleep((fault.param("ms", 0) + (random.random() * jitter if jitter else 0)) / 1000.0)
        if action == "stall":
            self._wait(fault.param("seconds"))
        body = self._read_body(reader, request)
        entry["sent"] = sum(len(p) for p in body)
        if action == "reset_before":
            raise Reset("reset before forwarding")
        # The node's clock.
        if self.clock_offset:
            signed = request.amz_date
            now = time.time() + self.clock_offset
            if signed is not None and abs(now - signed) > MAX_SKEW_SECONDS:
                entry["fault"] = entry["fault"] or "clock-skew"
                return self._answer(sock, request, entry, 403, error_body(
                    "RequestTimeTooSkewed",
                    "The difference between the request time and the current time is too large.",
                    request.path, {"RequestTime": request.header("x-amz-date") or "",
                                   "ServerTime": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                                               time.gmtime(now)),
                                   "MaxAllowedSkewMilliseconds": MAX_SKEW_SECONDS * 1000}),
                    {"Content-Type": "application/xml",
                     "Date": email.utils.formatdate(now, usegmt=True)})
        if action == "status":
            headers = {"Content-Type": "application/xml"}
            headers.update(fault.param("headers", {}) or {})
            if fault.param("azlin_error"):
                headers["x-azlin-error"] = fault.param("azlin_error")
            if fault.param("retry_after") is not None:
                headers["Retry-After"] = str(fault.param("retry_after"))
            return self._answer(sock, request, entry, int(fault.param("status", 500)), error_body(
                fault.param("code", "InternalError"), fault.param("message", "azchaos"),
                request.path), headers)
        if action == "azlin":
            code = fault.param("azlin_error", "unavailable")
            status, s3_code, retry_after = AZLIN_ERRORS[code]
            headers = {"Content-Type": "application/xml", "x-azlin-error": code}
            if retry_after is not None:
                headers["Retry-After"] = str(fault.param("retry_after", retry_after))
            if fault.param("alt_endpoints"):
                headers["x-azlin-alt-endpoints"] = fault.param("alt_endpoints")
            return self._answer(sock, request, entry, status, error_body(
                s3_code, fault.param("message", "azchaos: " + code), request.path), headers)
        if action == "stale_map":
            epoch = int(fault.param("epoch", 2))
            digest = fault.param("hash") or hashlib.sha256(b"map %d" % epoch).hexdigest()
            return self._answer(sock, request, entry, 409, error_body(
                "Conflict", "the client's map is older than the node's", request.path),
                {"Content-Type": "application/xml", "x-azlin-map": "%d %s" % (epoch, digest)})
        # Quotas: the write that crosses the bucket's quota.
        with self._lock:
            quota = self.quotas.get(request.bucket)
        if quota and request.op in ("PutObject", "UploadPart", "CopyObject"):
            limit, used = quota
            if used() + entry["sent"] > limit:
                entry["fault"] = entry["fault"] or "quota"
                return self._answer(sock, request, entry, 403, error_body(
                    "QuotaExceeded", "The drive is full.", request.path),
                    {"Content-Type": "application/xml", "x-azlin-error": "quota_exceeded"})
        with self._lock:
            sinks = list(self.sinks)
        for sink in sinks:
            if sink.takes(request):
                entry["fault"] = entry["fault"] or "sink"
                return self._sink(sock, request, entry, sink, body)
        return self._forward(sock, request, entry, fault, body)

    def _forward(self, sock, request, entry, fault, body):
        action = fault.action if fault else None
        bps = fault.param("bps") if action == "throttle" else None
        upstream = socket.create_connection(self.upstream, timeout=120)
        try:
            self._send(upstream, request.head, bps)
            for piece in body:
                self._send(upstream, piece, bps)
            entry["forwarded"] = True
            up = Reader(upstream)
            head = up.head()
            if head is None:
                raise Reset("the upstream closed")
            response = Response(head, request.method)
            entry["upstream_status"] = response.status
            entry["status"] = response.status
            kind, length = response.framing()
            pieces = body_pieces(up, kind, length)
            if action == "reset_after":
                for _ in pieces:
                    pass
                raise Reset("the answer is lost")
            if action == "stall_after":
                for_later = list(pieces)
                self._wait(fault.param("seconds"))
                pieces = iter(for_later)
            if action == "corrupt" and kind == "length" and 0 < length <= CORRUPT_LIMIT:
                data = bytearray(b"".join(pieces))
                data[len(data) // 2] ^= 0xFF
                pieces = iter([bytes(data)])
            self._send(sock, head, bps)
            if action in ("cut_body", "stall_body"):
                left = int(fault.param("bytes", 0))
                for piece in pieces:
                    if left <= 0:
                        break
                    part = piece[:left]
                    self._send(sock, part, bps)
                    entry["received"] += len(part)
                    left -= len(part)
                    if len(part) < len(piece):
                        rest = piece[len(part):]
                        break
                else:
                    rest = b""
                if action == "cut_body":
                    # The connection ends early (an EOF before the body's end; `rst`: a reset):
                    # a reset can make the peer's kernel drop what it had not read yet.
                    if fault.param("rst"):
                        time.sleep(0.05)
                        raise Reset("the body is cut")
                    sock.shutdown(socket.SHUT_WR)
                    return False
                self._wait(fault.param("seconds"))
                if rest:
                    self._send(sock, rest, bps)
                    entry["received"] += len(rest)
            for piece in pieces:
                self._send(sock, piece, bps)
                entry["received"] += len(piece)
            if kind == "close":
                return False
            return not (response.closes() or request.wants_close())
        finally:
            try:
                upstream.close()
            except OSError:
                pass

    def _sink(self, sock, request, entry, sink, body):
        """The sink's own S3: multipart uploads and PUTs hashed, never stored."""
        op, key = request.op, request.key
        chunked = request.body_length()[1]
        data_pieces = [unchunk(body)] if chunked else body
        xml = {"Content-Type": "application/xml"}
        with sink.lock:
            if op == "PutObject":
                digest, md5, size = hashlib.sha256(), hashlib.md5(), 0
                for piece in data_pieces:
                    digest.update(piece)
                    md5.update(piece)
                    size += len(piece)
                sink.objects[key] = {"size": size, "sha256": digest.hexdigest(), "parts": 1,
                                     "etag": md5.hexdigest()}
                return self._answer(sock, request, entry, 200, b"",
                                    {"ETag": '"%s"' % md5.hexdigest()})
            if op == "CreateMultipartUpload":
                upload = "sink-" + uuid.uuid4().hex
                sink.uploads[upload] = {"key": key, "parts": {}, "next": 1,
                                        "hasher": hashlib.sha256(), "pending": {},
                                        "broken": False}
                body_xml = ("<InitiateMultipartUploadResult><Bucket>%s</Bucket><Key>%s</Key>"
                            "<UploadId>%s</UploadId></InitiateMultipartUploadResult>" % (
                                escape(request.bucket), escape(key), upload)).encode("utf-8")
                return self._answer(sock, request, entry, 200, body_xml, xml)
            upload = sink.uploads.get(request.query.get("uploadId", ""))
            if op == "UploadPart":
                if upload is None:
                    return self._answer(sock, request, entry, 404, error_body(
                        "NoSuchUpload", "no such upload", request.path), xml)
                data = b"".join(data_pieces)
                etag = hashlib.md5(data).hexdigest()
                number = int(request.query.get("partNumber", "0"))
                sink.take_part(upload, number, data, etag)
                return self._answer(sock, request, entry, 200, b"", {"ETag": '"%s"' % etag})
            if op == "ListParts":
                if upload is None:
                    return self._answer(sock, request, entry, 404, error_body(
                        "NoSuchUpload", "no such upload", request.path), xml)
                parts = "".join(
                    "<Part><PartNumber>%d</PartNumber><ETag>\"%s\"</ETag><Size>%d</Size></Part>"
                    % (n, p["etag"], p["size"]) for n, p in sorted(upload["parts"].items()))
                body_xml = ("<ListPartsResult><Bucket>%s</Bucket><Key>%s</Key><UploadId>%s"
                            "</UploadId><IsTruncated>false</IsTruncated>%s</ListPartsResult>" % (
                                escape(request.bucket), escape(key),
                                request.query.get("uploadId", ""), parts)).encode("utf-8")
                return self._answer(sock, request, entry, 200, body_xml, xml)
            if op == "AbortMultipartUpload":
                sink.uploads.pop(request.query.get("uploadId", ""), None)
                return self._answer(sock, request, entry, 204, b"")
            if op == "CompleteMultipartUpload":
                if upload is None:
                    return self._answer(sock, request, entry, 404, error_body(
                        "NoSuchUpload", "no such upload", request.path), xml)
                listed = [int(n) for n in re.findall(
                    rb"<PartNumber>\s*(\d+)\s*</PartNumber>", b"".join(data_pieces))]
                missing = [n for n in listed if n not in upload["parts"]]
                if missing:
                    return self._answer(sock, request, entry, 400, error_body(
                        "InvalidPart", "parts %s were never uploaded" % missing, request.path),
                        xml)
                size = sum(upload["parts"][n]["size"] for n in listed)
                md5s = b"".join(bytes.fromhex(upload["parts"][n]["etag"]) for n in listed)
                etag = "%s-%d" % (hashlib.md5(md5s).hexdigest(), len(listed))
                sink.objects[key] = {"size": size, "parts": len(listed), "etag": etag,
                                     "sha256": sink.whole_hash(upload, listed)}
                sink.uploads.pop(request.query.get("uploadId", ""), None)
                body_xml = ("<CompleteMultipartUploadResult><Bucket>%s</Bucket><Key>%s</Key>"
                            "<ETag>\"%s\"</ETag></CompleteMultipartUploadResult>" % (
                                escape(request.bucket), escape(key), etag)).encode("utf-8")
                return self._answer(sock, request, entry, 200, body_xml, xml)
            if op in ("HeadObject", "GetObject"):
                found = sink.objects.get(key)
                if found is None:
                    return self._answer(sock, request, entry, 404, b"" if op == "HeadObject"
                                        else error_body("NoSuchKey", "no such key", request.path))
                headers = {"ETag": '"%s"' % found["etag"], "Content-Length": str(found["size"])}
                if op == "HeadObject":
                    self._send(sock, answer_bytes(200, b"", headers))
                    entry["status"] = 200
                    return not request.wants_close()
                return self._answer(sock, request, entry, 501, error_body(
                    "NotImplemented", "a sinked object keeps no bytes", request.path), xml)
        return self._answer(sock, request, entry, 400, error_body(
            "InvalidRequest", "the sink does not take " + op, request.path), xml)

    # -- control ----------------------------------------------------------------------------

    def _control(self, sock, reader, request):
        body = b"".join(self._read_body(reader, request))
        if request.method == "GET" and request.path.endswith("/log"):
            data = json.dumps(self.requests()).encode("utf-8")
        elif request.method == "GET":
            data = json.dumps({"state": self.state(), "held": self.held(),
                               "faults": [f.describe() for f in self.faults]}).encode("utf-8")
        else:
            try:
                command = json.loads(body.decode("utf-8") or "{}")
                self.steer(command)
                data = json.dumps({"state": self.state(), "held": self.held()}).encode("utf-8")
            except (ValueError, TypeError, KeyError) as e:
                sock.sendall(answer_bytes(400, json.dumps({"error": str(e)}).encode("utf-8"),
                                          {"Content-Type": "application/json"}))
                return
        sock.sendall(answer_bytes(200, data, {"Content-Type": "application/json"}))

    def steer(self, command):
        """One control command (the dict `POST /__azchaos/` takes)."""
        if command.get("clear"):
            self.clear()
        for rule in ([command["add"]] if isinstance(command.get("add"), dict)
                     else command.get("add") or []):
            self.add(**rule)
        if command.get("release"):
            self.release()
        if "network" in command:
            self.network(command["network"])
        if "flap" in command:
            if command["flap"]:
                up, down = command["flap"][:2]
                self.flap(float(up), float(down))
            else:
                self.stop_flap()
        if "clock_offset" in command:
            self.clock_offset = int(command["clock_offset"])
        if "quota" in command:
            bucket, quota = command["quota"][:2]
            self.set_quota(bucket, quota)
        if command.get("clear_log"):
            self.clear_log()


def control(url, command):
    """Steers the proxy at `url` from another process: the command dict, its answer."""
    import urllib.request  # noqa: PLC0415 - the helper of other processes only

    request = urllib.request.Request(url.rstrip("/") + CONTROL + "/", method="POST",
                                     data=json.dumps(command).encode("utf-8"))
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.loads(response.read().decode("utf-8"))


# ---------------------------------------------------------------------------------------------
# The iroh lane and DNS
# ---------------------------------------------------------------------------------------------


class UdpSink:
    """A UDP port that swallows every datagram and never answers: an iroh lane whose node
    vanished (QUIC hears nothing back). `datagrams` counts what arrived."""

    def __init__(self, host="127.0.0.1", port=0):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.bind((host, port))
        self.sock.settimeout(0.2)
        self.address = self.sock.getsockname()
        self.datagrams = 0
        self._stop = threading.Event()

    def start(self):
        threading.Thread(target=self._run, name="azchaos-udp", daemon=True).start()
        return self

    def _run(self):
        while not self._stop.is_set():
            try:
                self.sock.recvfrom(65536)
                self.datagrams += 1
            except (socket.timeout, TimeoutError):
                continue
            except OSError:
                return

    def stop(self):
        self._stop.set()
        self.sock.close()


class SlowDns:
    """A DNS server answering A queries for `names` ({"node.azchaos.test.": "127.0.0.1"}) after
    `delay` seconds, NXDOMAIN for the rest; `drop`: never answers (DNS down). For a slow-DNS
    E2E, /etc/resolver/<domain> (macOS, root) names it: `nameserver 127.0.0.1` and `port <P>`."""

    def __init__(self, names, delay=0.0, host="127.0.0.1", port=0, drop=False):
        self.names = {k.lower().rstrip(".") + ".": v for k, v in names.items()}
        self.delay = delay
        self.drop = drop
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.bind((host, port))
        self.sock.settimeout(0.2)
        self.port = self.sock.getsockname()[1]
        self.queries = []
        self._stop = threading.Event()

    def start(self):
        threading.Thread(target=self._run, name="azchaos-dns", daemon=True).start()
        return self

    def stop(self):
        self._stop.set()
        self.sock.close()

    def _run(self):
        while not self._stop.is_set():
            try:
                query, peer = self.sock.recvfrom(512)
            except (socket.timeout, TimeoutError):
                continue
            except OSError:
                return
            threading.Thread(target=self._answer, args=(query, peer), daemon=True).start()

    def _answer(self, query, peer):
        try:
            name, end = self._name(query, 12)
        except (IndexError, ValueError):
            return
        self.queries.append(name)
        if self.drop:
            return
        if self.delay:
            time.sleep(self.delay)
        qtype = struct.unpack(">H", query[end:end + 2])[0]
        question = query[12:end + 4]
        ip = self.names.get(name.lower())
        header_id = query[:2]
        if ip and qtype == 1:
            flags, answers = 0x8180, 1
            record = (b"\xc0\x0c" + struct.pack(">HHIH", 1, 1, 30, 4)
                      + socket.inet_aton(ip))
        else:
            flags, answers, record = (0x8180 if ip else 0x8183), 0, b""
        reply = header_id + struct.pack(">HHHHH", flags, 1, answers, 0, 0) + question + record
        try:
            self.sock.sendto(reply, peer)
        except OSError:
            pass

    @staticmethod
    def _name(packet, at):
        labels = []
        while True:
            length = packet[at]
            at += 1
            if length == 0:
                break
            labels.append(packet[at:at + length].decode("ascii"))
            at += length
        return ".".join(labels) + ".", at


# ---------------------------------------------------------------------------------------------
# The command line
# ---------------------------------------------------------------------------------------------


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], usage=USAGE)
    sub = parser.add_subparsers(dest="command", required=True)
    proxy = sub.add_parser("proxy")
    proxy.add_argument("--upstream", required=True)
    proxy.add_argument("--host", default="127.0.0.1")
    proxy.add_argument("--port", type=int, default=0)
    proxy.add_argument("--rules", help="a JSON file with a list of rules")
    proxy.add_argument("--log", help="append every request as a JSON line to this file")
    proxy.add_argument("--verbose", action="store_true")
    udp = sub.add_parser("udp-sink")
    udp.add_argument("--port", type=int, default=0)
    dns = sub.add_parser("slow-dns")
    dns.add_argument("--name", action="append", default=[], help="NAME=IP (repeatable)")
    dns.add_argument("--delay", type=float, default=5.0)
    dns.add_argument("--port", type=int, default=0)
    args = parser.parse_args(argv)
    if args.command == "proxy":
        running = ChaosProxy(args.upstream, args.host, args.port, args.log, args.verbose).start()
        if args.rules:
            with open(args.rules, "r", encoding="utf-8") as f:
                for rule in json.load(f):
                    running.add(**rule)
        print("azchaos proxy %s -> %s:%d (steer: %s%s/)" % (
            running.url, running.upstream[0], running.upstream[1], running.url, CONTROL),
            flush=True)
    elif args.command == "udp-sink":
        running = UdpSink(port=args.port).start()
        print("azchaos udp sink %s:%d" % running.address, flush=True)
    else:
        names = dict(n.split("=", 1) for n in args.name)
        running = SlowDns(names, args.delay, port=args.port).start()
        print("azchaos slow dns 127.0.0.1:%d (%.1f s)" % (running.port, args.delay), flush=True)
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        running.stop()
    return 0


if __name__ == "__main__":
    sys.exit(main())
