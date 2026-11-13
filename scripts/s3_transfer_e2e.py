#!/usr/bin/env python3
"""End-to-end run of the apps' S3 transfers against the local S3 test server
(examples/azul-drive/scripts/s3_server.py) - no cloud, no token server.

The client is `s3_transfer` (examples/azul-bridge/examples/s3_transfer.rs): azul-storage's S3
drive (parts of 16 MiB four at once, a file's upload resumable after a kill, ranged downloads
four at once that resume) behind azcloud-kit's failover, over the bridge's HTTP client.

    cargo build -p azul-bridge --example s3_transfer
    python3 scripts/s3_transfer_e2e.py [--bin PATH] [--size-mib 64] [--keep]

Steps (each on servers of its own, under one temporary folder):

  upload_resume   a 64 MiB upload in 4 MiB parts; the server holds the parts after the 8th,
                  the client is killed (SIGKILL), the server lets go; the next run of the same
                  upload resumes it (no new CreateMultipartUpload, one ListParts, only the parts
                  that are missing) and the object's bytes are the file's (SHA-256)
  race            two clients write the same key with If-None-Match: * at the same moment -
                  one PUT and one streamed multipart upload, several rounds: exactly one wins
                  (exit 0), the other is told it lost (exit 3), the object is the winner's
  download_resume a 64 MiB download; the server holds the ranges after the 6th, the client is
                  killed, the next run fetches only the ranges it still lacks; the bytes match
                  and no hidden part file is left
  node_killed     two nodes over one store; the client downloads through node A with node B
                  in its node list; A is killed (SIGKILL) half way; the download finishes
                  through B and its bytes match
  hint            node A answers 503 with x-azlin-alt-endpoints naming node B (Retry-After: 0);
                  the same request goes to B and the write lands
  dns_down        DNS down from the start: the block endpoint and the node have names that never
                  resolve (`.invalid`, RFC 6761, as azctl chaos --no-dns does); the node list
                  carries the node's address (layer 4: the block host at every node's address).
                  A big upload and its download complete at 127.0.0.1, every request still
                  naming its host, and the bytes match - HTTPS (here HTTP) by IP, iroh off

Exit code 0 when every step passed.
"""

import argparse
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SERVER_DIR = os.path.join(HERE, "..", "examples", "azul-drive", "scripts")
sys.path.insert(0, SERVER_DIR)

import s3_server  # noqa: E402

BUCKET = "azdrive"
ACCESS = s3_server.DEFAULT_ACCESS_KEY
SECRET = s3_server.DEFAULT_SECRET_KEY
MIB = 1024 * 1024
PART = 4 * MIB


class Failure(Exception):
    pass


def expect(cond, message):
    if not cond:
        raise Failure(message)


def repo_roots():
    """This checkout, and the main checkout when this is a git worktree."""
    repo = os.path.abspath(os.path.join(HERE, ".."))
    roots = [repo]
    try:
        common = subprocess.run(
            ["git", "-C", repo, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            check=True, capture_output=True, text=True,
        ).stdout.strip()
        main = os.path.dirname(common)
        if main and main not in roots:
            roots.append(main)
    except (OSError, subprocess.CalledProcessError):
        pass
    return roots


def find_binary(explicit):
    candidates = [explicit] if explicit else []
    if os.environ.get("S3_TRANSFER_BIN"):
        candidates.append(os.environ["S3_TRANSFER_BIN"])
    for root in repo_roots():
        for profile in ("release", "debug"):
            candidates.append(os.path.join(root, "target", profile, "examples", "s3_transfer"))
    for candidate in candidates:
        if candidate and os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
    return None


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(MIB), b""):
            digest.update(block)
    return digest.hexdigest()


def random_file(path, size):
    with open(path, "wb") as f:
        left = size
        while left:
            n = min(left, 4 * MIB)
            f.write(os.urandom(n))
            left -= n
    return sha256_file(path)


def wait_for(what, check, timeout=60.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if check():
            return
        time.sleep(0.05)
    raise Failure("timed out waiting for %s" % what)


def ops(requests, op, status=None):
    return [r for r in requests if r.get("op") == op and (status is None or r.get("status") == status)]


class Run:
    def __init__(self, binary, base):
        self.binary = binary
        self.base = base

    def args(self, command, rest, endpoint, nodes=(), resume=None):
        out = [self.binary, command] + list(rest) + [
            "--endpoint", endpoint, "--bucket", BUCKET, "--access-key", ACCESS,
            "--secret-key", SECRET, "--part-size", str(PART), "--parallel", "4",
        ]
        for node in nodes:
            out += ["--node", node]
        if resume:
            out += ["--resume", resume]
        return out

    def start(self, command, rest, endpoint, **kw):
        return subprocess.Popen(self.args(command, rest, endpoint, **kw),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def call(self, command, rest, endpoint, timeout=300, **kw):
        done = subprocess.run(self.args(command, rest, endpoint, **kw), capture_output=True,
                              text=True, timeout=timeout)
        line = (done.stdout.strip().splitlines() or ["{}"])[-1]
        try:
            answer = json.loads(line)
        except ValueError:
            answer = {"raw": done.stdout, "stderr": done.stderr}
        return done.returncode, answer


class Node:
    """A server in a process of its own (it can be killed), over the store at `root`."""

    def __init__(self, root, log_path, *flags):
        self.log_path = log_path
        self.process = subprocess.Popen(
            [sys.executable, os.path.join(SERVER_DIR, "s3_server.py"), "--root", root,
             "--port", "0", "--bucket", BUCKET, "--log", log_path] + list(flags),
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        line = self.process.stdout.readline().strip()
        if not line.startswith("S3_SERVER_URL "):
            self.kill()
            raise Failure("the node did not start: %r" % line)
        self.url = line.split(" ", 1)[1]

    def requests(self):
        try:
            with open(self.log_path, encoding="utf-8") as f:
                return [json.loads(line) for line in f if line.strip()]
        except OSError:
            return []

    def kill(self):
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGKILL)
            self.process.wait(timeout=10)


def kill(process):
    if process.poll() is None:
        process.send_signal(signal.SIGKILL)
        process.wait(timeout=10)


def get_object(url, key):
    status, _, body = s3_server.Client(url, ACCESS, SECRET).request("GET", BUCKET, key)
    return status, body


def upload_resume(run, base, size):
    root = os.path.join(base, "upload-store")
    server = s3_server.start(root, access_key=ACCESS, secret_key=SECRET, buckets=[BUCKET])
    try:
        source = os.path.join(base, "upload.bin")
        digest = random_file(source, size)
        resume = os.path.join(base, "upload-resume")
        parts = size // PART
        half = parts // 2
        server.hold_parts_after(half)
        client = run.start("put", [source, "big.bin"], server.url, resume=resume)
        try:
            wait_for("%d parts" % half, lambda: len(ops(server.requests(), "UploadPart", 200)) >= half
                     and server.held() > 0)
        finally:
            kill(client)
        states = [n for n in os.listdir(resume) if n.endswith(".json")]
        expect(len(states) == 1, "the killed upload left no state file: %s" % states)
        first = server.requests()
        expect(len(ops(first, "CreateMultipartUpload")) == 1, "one upload was started")
        expect(not ops(first, "CompleteMultipartUpload"), "the killed upload was completed")
        server.release()
        time.sleep(1.0)
        server.clear_log()
        code, answer = run.call("put", [source, "big.bin"], server.url, resume=resume)
        expect(code == 0, "the resumed upload failed: %s" % answer)
        second = server.requests()
        sent = len(ops(second, "UploadPart", 200))
        expect(not ops(second, "CreateMultipartUpload"), "the resumed run started a new upload")
        expect(len(ops(second, "ListParts")) == 1, "the resumed run did not ask which parts exist")
        expect(0 < sent < parts, "the resumed run sent %d of %d parts" % (sent, parts))
        expect(len(ops(second, "CompleteMultipartUpload", 200)) == 1, "not completed once")
        status, body = get_object(server.url, "big.bin")
        expect(status == 200 and hashlib.sha256(body).hexdigest() == digest,
               "the object's bytes are not the file's")
        expect(not [n for n in os.listdir(resume) if n.endswith(".json")],
               "a finished upload left its state file")
        return "%d MiB: killed after %d of %d parts, resumed with %d" % (
            size // MIB, len(ops(first, "UploadPart", 200)), parts, sent)
    finally:
        server.stop()


def race(run, base, rounds=5):
    root = os.path.join(base, "race-store")
    server = s3_server.start(root, access_key=ACCESS, secret_key=SECRET, buckets=[BUCKET])
    try:
        files = []
        for name in ("a", "b"):
            path = os.path.join(base, "race-%s.bin" % name)
            random_file(path, 9 * MIB)
            files.append(path)
        for i in range(rounds):
            for streamed in (False, True):
                key = "race/%s-%d" % ("big" if streamed else "small", i)
                if streamed:
                    clients = [run.start("put-file-if-absent", [f, key], server.url) for f in files]
                else:
                    clients = [run.start("put-if-absent", [key, "writer-%s" % w], server.url)
                               for w in ("a", "b")]
                codes = [c.wait(timeout=120) for c in clients]
                expect(sorted(codes) == [0, 3], "%s: exit codes %s, not one winner" % (key, codes))
                winner = codes.index(0)
                status, body = get_object(server.url, key)
                expect(status == 200, "%s was not written" % key)
                if streamed:
                    with open(files[winner], "rb") as f:
                        expect(body == f.read(), "%s is not the winner's file" % key)
                else:
                    expect(body == ("writer-%s" % "ab"[winner]).encode(), "%s is not the winner's" % key)
        return "%d rounds of a PUT and a streamed upload: one winner each" % rounds
    finally:
        server.stop()


def download_resume(run, base, size):
    root = os.path.join(base, "download-store")
    server = s3_server.start(root, access_key=ACCESS, secret_key=SECRET, buckets=[BUCKET])
    try:
        os.makedirs(os.path.join(root, BUCKET), exist_ok=True)
        digest = random_file(os.path.join(root, BUCKET, "big.bin"), size)
        ranges = size // PART
        dest_dir = os.path.join(base, "download-here")
        os.makedirs(dest_dir)
        dest = os.path.join(dest_dir, "big.bin")
        server.hold_gets_after(6)
        client = run.start("get", ["big.bin", dest], server.url)
        try:
            wait_for("6 ranges", lambda: len(ops(server.requests(), "GetObject", 206)) >= 6
                     and server.held() > 0)
        finally:
            kill(client)
        expect(not os.path.exists(dest), "a killed download left a file under its name")
        server.release()
        time.sleep(0.5)
        server.clear_log()
        code, answer = run.call("get", ["big.bin", dest], server.url)
        expect(code == 0, "the resumed download failed: %s" % answer)
        fetched = len(ops(server.requests(), "GetObject", 206))
        expect(0 < fetched < ranges, "the resumed run fetched %d of %d ranges" % (fetched, ranges))
        expect(sha256_file(dest) == digest, "the downloaded bytes are not the object's")
        left = [n for n in os.listdir(dest_dir) if n != "big.bin"]
        expect(not left, "files left beside the download: %s" % left)
        return "%d MiB: killed after 6 of %d ranges, resumed with %d" % (size // MIB, ranges, fetched)
    finally:
        server.stop()


def node_killed(run, base, size):
    root = os.path.join(base, "nodes-store")
    os.makedirs(os.path.join(root, BUCKET), exist_ok=True)
    digest = random_file(os.path.join(root, BUCKET, "big.bin"), size)
    a = Node(root, os.path.join(base, "node-a.jsonl"), "--hold-gets-after", "6")
    b = Node(root, os.path.join(base, "node-b.jsonl"))
    try:
        dest = os.path.join(base, "through-nodes.bin")
        client = run.start("get", ["big.bin", dest], a.url, nodes=[b.url])
        try:
            wait_for("6 ranges from node A",
                     lambda: len(ops(a.requests(), "GetObject", 206)) >= 6)
            time.sleep(0.3)
            a.kill()
            out, err = client.communicate(timeout=300)
        finally:
            kill(client)
        expect(client.returncode == 0, "the download did not survive node A: %s %s" % (out, err))
        expect(sha256_file(dest) == digest, "the bytes through node B are not the object's")
        from_b = len(ops(b.requests(), "GetObject", 206))
        expect(from_b > 0, "nothing came from node B")
        return "%d MiB: node A killed after 6 ranges, %d ranges from node B" % (size // MIB, from_b)
    finally:
        a.kill()
        b.kill()


def hint(run, base):
    root = os.path.join(base, "hint-store")
    a = s3_server.start(root, access_key=ACCESS, secret_key=SECRET, buckets=[BUCKET])
    b = s3_server.start(root, access_key=ACCESS, secret_key=SECRET, buckets=[BUCKET])
    try:
        a.fail_bucket(BUCKET, 503, "ServiceUnavailable", "draining",
                      {"x-azlin-error": "unavailable", "x-azlin-alt-endpoints": b.url,
                       "Retry-After": "0"})
        started = time.time()
        code, answer = run.call("put-if-absent", ["hint/key", "through b"], a.url)
        took = time.time() - started
        expect(code == 0, "the hinted write failed: %s" % answer)
        expect(len(ops(a.requests(), "Fault")) == 1, "node A was asked more than once")
        expect(len(ops(b.requests(), "PutObject", 200)) == 1, "node B did not take the write")
        status, body = get_object(b.url, "hint/key")
        expect((status, body) == (200, b"through b"), "the object is not there")
        return "503 + x-azlin-alt-endpoints: the same request went to node B (%.2f s)" % took
    finally:
        a.stop()
        b.stop()


def dns_down(run, base, size):
    root = os.path.join(base, "dns-store")
    os.makedirs(os.path.join(root, BUCKET), exist_ok=True)
    node = Node(root, os.path.join(base, "node-dns.jsonl"))
    try:
        port = node.url.rstrip("/").rsplit(":", 1)[1]
        block = "http://blk.azlin-dns-test.invalid:%s" % port
        named = "http://n1.azlin-dns-test.invalid:%s,127.0.0.1" % port
        src = os.path.join(base, "dns-up.bin")
        digest = random_file(src, size)
        started = time.time()
        code, answer = run.call("put", [src, "dns/big.bin"], block, nodes=[named])
        expect(code == 0, "the upload with DNS down failed: %s" % answer)
        dest = os.path.join(base, "dns-down.bin")
        code, answer = run.call("get", ["dns/big.bin", dest], block, nodes=[named])
        expect(code == 0, "the download with DNS down failed: %s" % answer)
        took = time.time() - started
        expect(sha256_file(dest) == digest, "the bytes through the addresses are not the file's")
        hosts = sorted({str(r.get("host")) for r in node.requests()})
        expect(hosts and all(h.endswith(".azlin-dns-test.invalid:%s" % port) for h in hosts),
               "the requests did not name their hosts: %s" % hosts)
        return "%d MiB up and down at 127.0.0.1 under %s (%.1f s)" % (size // MIB,
                                                                    ", ".join(hosts), took)
    finally:
        node.kill()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--bin", help="the s3_transfer binary (else S3_TRANSFER_BIN, else target/)")
    parser.add_argument("--size-mib", type=int, default=64, help="the big file's size (MiB)")
    parser.add_argument("--keep", action="store_true", help="keep the temporary folder")
    args = parser.parse_args()
    binary = find_binary(args.bin)
    if binary is None:
        print("FAIL  no s3_transfer binary: cargo build -p azul-bridge --example s3_transfer "
              "(or --bin / S3_TRANSFER_BIN)")
        return 2
    print("client %s" % binary)
    size = max(args.size_mib, 16) * MIB
    base = tempfile.mkdtemp(prefix="s3-transfer-e2e-")
    run = Run(binary, base)
    steps = [
        ("upload_resume", lambda: upload_resume(run, base, size)),
        ("race", lambda: race(run, base)),
        ("download_resume", lambda: download_resume(run, base, size)),
        ("node_killed", lambda: node_killed(run, base, size)),
        ("hint", lambda: hint(run, base)),
        ("dns_down", lambda: dns_down(run, base, size)),
    ]
    failed = 0
    try:
        for name, step in steps:
            started = time.time()
            try:
                detail = step()
                print("PASS  %-16s %s (%.1f s)" % (name, detail, time.time() - started))
            except Exception as e:  # noqa: BLE001 - every failure is reported, the run goes on
                failed += 1
                print("FAIL  %-16s %s" % (name, e))
    finally:
        if args.keep:
            print("kept %s" % base)
        else:
            shutil.rmtree(base, ignore_errors=True)
    print("%d of %d steps passed" % (len(steps) - failed, len(steps)))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
