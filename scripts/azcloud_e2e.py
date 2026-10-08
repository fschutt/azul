#!/usr/bin/env python3
"""End-to-end run of the `azcloud` CLI (examples/azcloud-api) against a running local Azlin
cluster: `azctl dev up --processes` (or `--vms`) of azul-apps/iso.

    cd examples/azcloud-api && cargo build --release        # the azcloud binary
    AZLIN_TOKEN_URL=http://127.0.0.1:8081 python3 scripts/azcloud_e2e.py

What it checks, on two simulated machines A and B (each its own HOME, state folder, data root,
.azlin folder and shared config, all under one temporary folder; the user's real ~/.azlin and
data root are never read or written):

  config     `azcloud config` names the token server's source (the environment here) and lists
             the built-in defaults still in use - the hard-coding report
  signup     a drive at the token server; `refresh` (POST /v1/drives/{id}/credentials) works
  files      a 1 MiB and a 50 MiB file up and down, compared by SHA-256 (and azcloud's BLAKE3)
  sync       a folder synced twice: the second run uploads nothing and writes no index; then an
             edit, an addition, a delete; machine B joins (invite / join) and gets the same tree
  azlin      the Azlin tree (data root + .azlin) from A to B: app files travel, caches / logs /
             locks / secret-shaped files / .azlin/cache never do, config.json arrives without A's
             endpoints and keeps B's; concurrent edits converge; a clash keeps both versions
  transport  https forced; iroh forced and auto (when the node's iroh id is known: --iroh-node /
             --iroh-addr, or discovered from the dev state's node admin port like `azctl test
             client --iroh`); a dead iroh address falls back to https and is remembered

Nothing is hard-coded without a way around it: the token server is --token-url, else
AZLIN_TOKEN_URL, else AZLIN_TOKEN_SERVER, else http://127.0.0.1:8081 (printed as a DEFAULT);
the binary is --bin, else AZCLOUD_BIN, else examples/azcloud-api/target/release/azcloud of this
checkout or the main one. Exit code 0 when every step passed (skipped steps say why).
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_TOKEN_URL = "http://127.0.0.1:8081"
MIB = 1024 * 1024


class Failure(Exception):
    pass


class Skip(Exception):
    pass


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
    candidates = []
    if explicit:
        candidates.append(explicit)
    if os.environ.get("AZCLOUD_BIN"):
        candidates.append(os.environ["AZCLOUD_BIN"])
    for root in repo_roots():
        for sub in ("release", "debug"):
            candidates.append(os.path.join(root, "examples", "azcloud-api", "target", sub,
                                           "azcloud"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no azcloud binary: `cd examples/azcloud-api && cargo build --release`, "
                  "or pass --bin / AZCLOUD_BIN (tried %s)" % candidates)


def token_url_of(args):
    """The token server and where it came from: nothing here is a silent default."""
    if args.token_url:
        return args.token_url.rstrip("/"), "flag --token-url"
    for var in ("AZLIN_TOKEN_URL", "AZLIN_TOKEN_SERVER"):
        if os.environ.get(var, "").strip():
            return os.environ[var].strip().rstrip("/"), "environment " + var
    return DEFAULT_TOKEN_URL, "DEFAULT (set AZLIN_TOKEN_URL or --token-url)"


def http_json(url, token=None, timeout=10):
    req = urllib.request.Request(url)
    if token:
        req.add_header("authorization", "Bearer " + token)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode() or "null")


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(MIB), b""):
            h.update(chunk)
    return h.hexdigest()


def write_file(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)


def read_file(path):
    try:
        with open(path, "rb") as f:
            return f.read()
    except FileNotFoundError:
        return None


def tree(root, skip_top=(".azlin",)):
    """Every file under root as relpath -> sha256 (the folder's own bookkeeping left out)."""
    out = {}
    if not os.path.isdir(root):
        return out
    for dirpath, dirnames, filenames in os.walk(root):
        rel_dir = os.path.relpath(dirpath, root)
        if rel_dir == ".":
            dirnames[:] = [d for d in dirnames if d not in skip_top]
        for name in filenames:
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, root).replace(os.sep, "/")
            out[rel] = sha256_file(path)
    return out


class Machine:
    """One simulated computer: its own HOME and every Azlin folder inside it."""

    def __init__(self, name, base, binary, token_url):
        self.name = name
        self.binary = binary
        self.root = os.path.join(base, name)
        self.home = os.path.join(self.root, "home")
        self.state = os.path.join(self.root, "state")
        self.data = os.path.join(self.home, "AzlinData")
        self.azlin_home = os.path.join(self.home, ".azlin")
        self.config = os.path.join(self.azlin_home, "config.json")
        self.work = os.path.join(self.root, "work")
        for d in (self.home, self.work, self.data, self.azlin_home):
            os.makedirs(d, exist_ok=True)
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(("AZLIN_", "AZCLOUD_", "AZMEET_", "XDG_"))}
        env.update({
            "HOME": self.home,
            "AZCLOUD_HOME": self.state,
            "AZLIN_DATA": self.data,
            "AZLIN_HOME": self.azlin_home,
            "AZLIN_CONFIG": self.config,
            "AZLIN_TOKEN_URL": token_url,
            "AZCLOUD_DEVICE": "machine-" + name.lower(),
        })
        self.env = env

    def az(self, *args, check=True, timeout=900):
        cmd = [self.binary, "--json"] + [str(a) for a in args]
        started = time.time()
        r = subprocess.run(cmd, env=self.env, capture_output=True, text=True, timeout=timeout)
        took = time.time() - started
        try:
            out = json.loads(r.stdout) if r.stdout.strip() else {}
        except json.JSONDecodeError:
            raise Failure("%s: azcloud %s printed no JSON:\n%s\n%s"
                          % (self.name, " ".join(args), r.stdout[-2000:], r.stderr[-2000:]))
        out["_exit"] = r.returncode
        out["_seconds"] = round(took, 2)
        if check and (r.returncode != 0 or out.get("ok") is False):
            raise Failure("%s: azcloud %s failed (exit %d): %s\n%s"
                          % (self.name, " ".join(args), r.returncode, out.get("error"),
                             r.stderr[-3000:]))
        return out

    def config_json(self):
        data = read_file(self.config)
        return json.loads(data) if data else {}

    def write_config(self, value):
        write_file(self.config, (json.dumps(value, indent=2, sort_keys=True) + "\n").encode())


def expect(cond, message):
    if not cond:
        raise Failure(message)


class Run:
    def __init__(self):
        self.results = []

    def step(self, name, fn):
        started = time.time()
        try:
            detail = fn() or ""
            self.results.append(("PASS", name, detail))
            print("PASS  %-34s %s (%.1f s)" % (name, detail, time.time() - started), flush=True)
        except Skip as e:
            self.results.append(("SKIP", name, str(e)))
            print("SKIP  %-34s %s" % (name, e), flush=True)
        except Failure as e:
            self.results.append(("FAIL", name, str(e)))
            print("FAIL  %-34s %s" % (name, e), flush=True)
        except Exception as e:  # noqa: BLE001 - a crash of a step is its failure
            self.results.append(("FAIL", name, "%s: %s" % (type(e).__name__, e)))
            print("FAIL  %-34s %s: %s" % (name, type(e).__name__, e), flush=True)

    def failed(self):
        return [r for r in self.results if r[0] == "FAIL"]


def dev_state_dir(args):
    if args.dev_state:
        return args.dev_state
    if os.environ.get("AZLIN_DEV_STATE"):
        return os.environ["AZLIN_DEV_STATE"]
    for root in repo_roots():
        candidate = os.path.join(os.path.dirname(root), "azul-apps", "iso", "dev", "state")
        if os.path.isdir(candidate):
            return candidate
    return None


def iroh_target(args):
    """The node to dial over iroh, as (node id, direct address or None, relay): the flags, else the
    first node of the dev state that reports an iroh id on /admin/status (azctl test client --iroh
    does the same). A process-mode node is dialled directly (relay "off"); a VM's UDP sockets are
    inside the guest, so it is reached through the relay it homes on (AZMEET_RELAY, the local
    stack's). (None, why) when none."""
    relay = args.relay or os.environ.get("AZMEET_RELAY") or "off"
    if args.iroh_node:
        return (args.iroh_node, args.iroh_addr, relay), "flags --iroh-node/--iroh-addr"
    if os.environ.get("AZLIN_IROH_NODE"):
        return ((os.environ["AZLIN_IROH_NODE"], os.environ.get("AZLIN_IROH_ADDR"), relay),
                "environment AZLIN_IROH_NODE")
    state = dev_state_dir(args)
    if not state:
        return None, "no dev state found (--dev-state, AZLIN_DEV_STATE)"
    dev_json = os.path.join(state, "dev.json")
    try:
        with open(dev_json) as f:
            dev = json.load(f)
    except (OSError, ValueError) as e:
        return None, "cannot read %s (%s)" % (dev_json, e)
    host = args.admin_host
    for node in dev.get("nodes", []):
        url = "http://%s:%s/admin/status" % (host, node.get("admin"))
        try:
            status = http_json(url, node.get("admin_token"))
        except (urllib.error.URLError, OSError, ValueError):
            continue
        node_id = status.get("iroh_id") if isinstance(status, dict) else None
        if not node_id:
            continue
        if dev.get("mode") == "vms":
            if relay == "off":
                return None, ("the dev state's nodes are VMs, reachable over iroh only through a "
                              "relay: set AZMEET_RELAY or --relay")
            return (node_id, None, relay), "dev state %s (node %s, VM, via relay %s)" % (
                dev_json, node.get("name"), relay)
        addrs = [a for a in status.get("iroh_addrs") or [] if "." in a]
        addr = None
        if addrs:
            addr = "%s:%s" % (host, addrs[0].rsplit(":", 1)[-1])
        return (node_id, addr, "off"), "dev state %s (node %s)" % (dev_json, node.get("name"))
    return None, ("no node of %s reports an iroh id: build azinit with --features dev,iroh "
                  "and start it with AZLIN_AZINIT=<that binary> azctl dev up" % dev_json)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--bin", help="the azcloud binary (else AZCLOUD_BIN, else target/release)")
    parser.add_argument("--token-url", help="the token server (else AZLIN_TOKEN_URL)")
    parser.add_argument("--big-mib", type=int, default=50, help="the big file's size (50)")
    parser.add_argument("--iroh-node", help="the node's iroh id (else discovered)")
    parser.add_argument("--iroh-addr", help="its UDP socket ip:port")
    parser.add_argument("--relay", help="the iroh relay to dial it through (else AZMEET_RELAY; "
                                        "process-mode nodes are dialled directly)")
    parser.add_argument("--dev-state", help="azctl's dev state folder (else AZLIN_DEV_STATE)")
    parser.add_argument("--admin-host", default="127.0.0.1",
                        help="where the nodes' admin ports listen (127.0.0.1)")
    parser.add_argument("--require-iroh", action="store_true",
                        help="fail (instead of skip) when no iroh node is known")
    parser.add_argument("--skip", default="",
                        help="comma-separated steps to leave out: files,sync,azlin,transport")
    parser.add_argument("--keep", action="store_true", help="keep the temporary folder")
    args = parser.parse_args()
    skip = {s.strip() for s in args.skip.split(",") if s.strip()}

    binary = find_binary(args.bin)
    token_url, token_source = token_url_of(args)
    print("azcloud      %s" % binary)
    print("token server %s (%s)" % (token_url, token_source))
    try:
        tiers = http_json(token_url + "/v1/tiers")
        print("             answers: %d tiers" % len(tiers.get("tiers", [])))
    except (urllib.error.URLError, OSError, ValueError) as e:
        print("FAIL  the token server at %s does not answer (%s): `azctl dev up --processes` "
              "first" % (token_url, e))
        return 1

    base = tempfile.mkdtemp(prefix="azcloud-e2e-")
    print("sandbox      %s" % base)
    a = Machine("A", base, binary, token_url)
    b = Machine("B", base, binary, token_url)
    # Each machine's shared config: the local profile, plus endpoints that must stay its own.
    a.write_config({"currentTheme": "flora:green", "mode": "dark",
                    "endpoints": {"profile": "local", "token": token_url}})
    b.write_config({"endpoints": {"profile": "local", "meet": "http://127.0.0.1:18790"}})
    run = Run()
    ctx = {}

    def config_step():
        c = a.az("config")
        tok = c["endpoints"]["token"]
        expect(tok["value"] == token_url, "token %r, not %r" % (tok["value"], token_url))
        expect(tok["kind"] == "env", "the token server should come from the environment: %s"
               % tok["source"])
        rejected = [(k, v["rejected"]) for k, v in c["endpoints"].items() if v["rejected"]]
        expect(not rejected, "rejected endpoint values: %s" % rejected)
        ctx["defaults"] = c.get("defaults_in_use", [])
        for line in ctx["defaults"]:
            print("      built-in default in use: %s" % line)
        return "token from %s; %d built-in defaults in use" % (tok["source"],
                                                              len(ctx["defaults"]))

    def signup_step():
        s = a.az("signup", "--name", "azcloud e2e", "--tier", "100GB")
        ctx["drive"] = s["drive"]
        expect(s["drive"].startswith("d_"), "drive id %r" % s["drive"])
        st = a.az("status")
        expect(st["drive"] == s["drive"], "status names another drive")
        before = st["expires_at"]
        r = a.az("refresh")
        expect(r["drive"] == s["drive"], "refresh answered for another drive")
        info = a.az("info")
        expect(info.get("id") == s["drive"], "info: %s" % info)
        return "drive %s, bucket %s, %d nodes; refresh ok (was %s, now %s); info %s" % (
            s["drive"], s["bucket"], s["nodes"], before, r["expires_at"], info.get("tier"))

    def up_down(machine, size, key, *flags):
        src = os.path.join(machine.work, key.replace("/", "_"))
        write_file(src, os.urandom(size))
        up = machine.az("up", src, "--key", key, *flags)
        dst = src + ".back"
        down = machine.az("down", key, dst, *flags)
        expect(sha256_file(src) == sha256_file(dst), "%s: the download differs" % key)
        expect(up["blake3"] == down["blake3"], "%s: BLAKE3 up %s, down %s"
               % (key, up["blake3"], down["blake3"]))
        expect(up["bytes"] == size and down["bytes"] == size, "%s: sizes" % key)
        return up, down

    def files_step():
        if "files" in skip:
            raise Skip("--skip files")
        up1, _ = up_down(a, MIB, "e2e/1m.bin")
        upb, downb = up_down(a, args.big_mib * MIB, "e2e/big.bin")
        listing = a.az("ls", "e2e/")
        keys = {o["key"]: o["size"] for o in listing["objects"]}
        expect(keys.get("e2e/1m.bin") == MIB, "ls: %s" % keys)
        expect(keys.get("e2e/big.bin") == args.big_mib * MIB, "ls: %s" % keys)
        # A presigned link reads the object without any credentials.
        link = a.az("share", "e2e/1m.bin", "--expires", "600")
        with urllib.request.urlopen(link["url"], timeout=60) as r:
            shared = r.read()
        expect(hashlib.sha256(shared).hexdigest()
               == sha256_file(os.path.join(a.work, "e2e_1m.bin")),
               "the presigned link read other bytes")
        return "1 MiB and %d MiB match (over %s; big up %.1f s, down %.1f s); share link ok" % (
            args.big_mib, up1["transport"], upb["_seconds"], downb["_seconds"])

    def make_sync_folder(root):
        for i in range(25):
            write_file(os.path.join(root, "dir%d" % (i % 4), "sub", "f%02d.txt" % i),
                       ("file %d\n" % i).encode() * (i + 1))
        write_file(os.path.join(root, "big", "three.bin"), os.urandom(3 * MIB))
        write_file(os.path.join(root, "scratch.tmp"), b"never synced")
        write_file(os.path.join(root, ".DS_Store"), b"never synced")

    def sync_report(out, i=0):
        return out["reports"][i]

    def sync_step():
        if "sync" in skip:
            raise Skip("--skip sync")
        src = os.path.join(a.work, "sync-src")
        make_sync_folder(src)
        first = sync_report(a.az("sync", src, "--prefix", "e2e/sync/"))
        expect(first["files_up"] == 26, "first sync: %d files up, not 26" % first["files_up"])
        expect(first["index_written"], "first sync wrote no index")
        second = sync_report(a.az("sync", src, "--prefix", "e2e/sync/"))
        expect(second["files_up"] == 0 and second["blobs_up"] == 0 and second["bytes_up"] == 0,
               "the second sync uploaded: %s" % second)
        expect(not second["index_written"], "the second sync wrote the index")
        # An edit, an addition, a delete.
        write_file(os.path.join(src, "dir0", "sub", "f00.txt"), b"edited on A\n")
        write_file(os.path.join(src, "new.txt"), b"new on A\n")
        os.remove(os.path.join(src, "dir1", "sub", "f01.txt"))
        third = sync_report(a.az("sync", src, "--prefix", "e2e/sync/"))
        expect(third["files_up"] == 2 and third["deleted_there"] == 1,
               "third sync: %d up, %d deleted" % (third["files_up"], third["deleted_there"]))
        # Machine B joins with a code of its own and gets the same tree.
        code = os.path.join(a.root, "join-code.txt")
        a.az("invite", "--out", code)
        joined = b.az("join", "--code-file", code)
        expect(joined["drive"] == ctx["drive"], "B joined another drive")
        dst = os.path.join(b.work, "sync-dst")
        got = sync_report(b.az("sync", dst, "--prefix", "e2e/sync/"))
        want = {k: v for k, v in tree(src).items() if k not in ("scratch.tmp", ".DS_Store")}
        expect(tree(dst) == want, "B's tree differs: %s" % sorted(set(tree(dst)) ^ set(want)))
        again = sync_report(b.az("sync", dst, "--prefix", "e2e/sync/"))
        expect(again["files_up"] == 0 and not again["index_written"], "B's second sync: %s"
               % again)
        gc = a.az("gc", "e2e/sync/", "--dry-run", "--grace-hours", "0")["reports"][0]
        expect(gc["deleted"] >= 1, "the edit and the delete left no unreferenced blob: %s" % gc)
        expect(gc["referenced"] >= 26, "gc: %s" % gc)
        return ("26 up, then 0 up and no index write; edit/add/delete travelled; "
                "B joined and got %d files" % got["files_down"])

    def azlin_step():
        if "azlin" in skip:
            raise Skip("--skip azlin")
        if "drive" not in ctx:
            raise Failure("no drive (the signup step failed)")
        if not os.path.exists(os.path.join(b.state, "azlin.json")):
            code = os.path.join(a.root, "join-code-azlin.txt")
            a.az("invite", "--out", code)
            b.az("join", "--code-file", code)
        # A's Azlin tree: app files that travel and files that never do.
        travels = {
            "calculator/history.jsonl": b"1+1=2\n",
            "notes/Notes/4b07f9bf-bdc8-41a2-bce8-e5e8d7887940.md": b"# A note\n",
            "contacts/0d5c1b3e-27c4-4c7e-9a43-111111111111.vcf": b"BEGIN:VCARD\nEND:VCARD\n",
            "meet/settings.json": b'{"name": "A"}\n',
            "notes/.history/4b07f9bf.json": b"[]\n",
        }
        never = {
            "music/cache/thumb.jpg": b"jpg",
            "term/session.log": b"log",
            "mail/dkim.pem": b"-----BEGIN PRIVATE KEY-----",
            "notes/state.lock": b"lock",
            "sheets/.~lock.Book1.xlsx#": b"lock",
            ".azlin/cache": b"azlin-cache 1 0000000000000000\n- a-only-marker\n",
            "notes/.DS_Store": b"finder",
        }
        for key, data in list(travels.items()) + list(never.items()):
            write_file(os.path.join(a.data, key), data)
        write_file(os.path.join(a.azlin_home, "themes", "mine.css"), b"/* A's rice */\n")
        sa = a.az("sync", "--azlin")
        sb = b.az("sync", "--azlin")
        for key, data in travels.items():
            expect(read_file(os.path.join(b.data, key)) == data, "B lacks %s" % key)
        for key in never:
            if key == ".azlin/cache":
                continue  # B's own manifest is its own; A's never travels (checked below)
            expect(read_file(os.path.join(b.data, key)) is None, "%s travelled to B" % key)
        manifest = read_file(os.path.join(b.data, ".azlin", "cache")) or b""
        expect(b"a-only-marker" not in manifest, "A's .azlin/cache reached B")
        expect(b"calculator/history.jsonl" in manifest,
               "B's own manifest does not record the files the sync wrote: %r" % manifest[:300])
        expect(read_file(os.path.join(b.azlin_home, "themes", "mine.css")) is not None,
               ".azlin/themes did not travel")
        cfg = b.config_json()
        expect(cfg.get("currentTheme") == "flora:green" and cfg.get("mode") == "dark",
               "B's config did not get A's look: %s" % cfg)
        expect(cfg.get("endpoints", {}).get("meet") == "http://127.0.0.1:18790",
               "B lost its own endpoints: %s" % cfg)
        expect("token" not in cfg.get("endpoints", {}), "A's endpoints reached B: %s" % cfg)
        # The drive itself never holds an endpoints section.
        for prefix in ("azlin/config/", "azlin/data/"):
            for obj in a.az("ls", prefix)["objects"]:
                path = os.path.join(a.work, "probe.bin")
                a.az("down", obj["key"], path)
                data = read_file(path) or b""
                expect(b'"endpoints"' not in data and token_url.encode() not in data,
                       "%s holds an endpoints section" % obj["key"])
        # Concurrent changes on both machines converge.
        write_file(os.path.join(b.data, "notes/Notes/4b07f9bf-bdc8-41a2-bce8-e5e8d7887940.md"),
                   b"# A note, edited on B\n")
        cb = b.config_json()
        cb["mode"] = "light"
        b.write_config(cb)
        ca = a.config_json()
        ca["currentTheme"] = "flat"
        a.write_config(ca)
        write_file(os.path.join(a.data, "contacts/new.vcf"), b"BEGIN:VCARD\nFN:New\nEND:VCARD\n")
        a.az("sync", "--azlin")
        b.az("sync", "--azlin")
        a.az("sync", "--azlin")
        def synced(m):
            return {k: v for k, v in tree(m.data).items() if k not in never}

        expect(synced(a) == synced(b), "the data trees differ: %s"
               % sorted(set(synced(a).items()) ^ set(synced(b).items())))
        for m in (a, b):
            c = m.config_json()
            expect(c.get("currentTheme") == "flat" and c.get("mode") == "light",
                   "%s config did not converge: %s" % (m.name, c))
        expect(a.config_json()["endpoints"].get("token") == token_url, "A lost its endpoints")
        expect(b.config_json()["endpoints"].get("meet") == "http://127.0.0.1:18790",
               "B lost its endpoints")
        # A clash keeps both versions on both machines.
        write_file(os.path.join(a.data, "calculator/history.jsonl"), b"2+2=4 (A)\n")
        write_file(os.path.join(b.data, "calculator/history.jsonl"), b"3+3=6 (B)\n")
        a.az("sync", "--azlin")
        clash = b.az("sync", "--azlin")
        conflicts = [c for r in clash["reports"] for c in r["conflicts"]]
        expect(len(conflicts) == 1, "conflicts: %s" % conflicts)
        a.az("sync", "--azlin")
        expect(synced(a) == synced(b), "after the clash the trees differ")
        copies = [k for k in synced(a) if k.startswith("calculator/history (conflict machine-b")]
        expect(len(copies) == 1, "no conflict copy on A: %s" % sorted(synced(a)))
        return "A -> B: %d app files, never: %d kinds; config merged per key; clash kept as %s" % (
            len(travels), len(never), copies[0])

    def transport_step():
        if "transport" in skip:
            raise Skip("--skip transport")
        t = a.az("transport", "--transport", "https")
        expect(t["lane"] == "https", "forced https: %s" % t)
        up, down = up_down(a, MIB, "e2e/https.bin", "--transport", "https")
        expect(up["transport"] == "https" and down["transport"] == "https", "not over https")
        target, why = iroh_target(args)
        if not target:
            if args.require_iroh:
                raise Failure("iroh: " + why)
            raise Skip("https ok; iroh skipped: " + why)
        node, addr, relay = target
        iroh_flags = ["--iroh-node", node, "--relay", relay]
        if addr:
            iroh_flags += ["--iroh-addr", addr]
        t = a.az("transport", "--transport", "iroh", *iroh_flags)
        expect(t["lane"] == "iroh", "forced iroh: %s" % t)
        up, down = up_down(a, MIB, "e2e/iroh.bin", "--transport", "iroh", *iroh_flags)
        expect(up["transport"] == "iroh" and down["transport"] == "iroh", "not over iroh")
        upb, downb = up_down(a, args.big_mib * MIB, "e2e/iroh-big.bin", "--transport", "iroh",
                             *iroh_flags)
        auto = a.az("transport", *iroh_flags)
        expect(auto["lane"] == "iroh", "auto with a known node should take iroh: %s" % auto)
        # A dead iroh address: auto falls back to https and remembers it.
        dead = ["--iroh-node", node, "--iroh-addr", "127.0.0.1:9", "--relay", "off"]
        memory = os.path.join(a.state, "transport.json")
        if os.path.exists(memory):
            os.remove(memory)
        fell = a.az("transport", *dead)
        expect(fell["lane"] == "https" and "iroh failed" in fell["reason"],
               "a dead iroh address: %s" % fell)
        remembered = a.az("transport", *dead)
        expect(remembered["lane"] == "https" and not remembered["probed"],
               "the failure was not remembered: %s" % remembered)
        return "https and iroh (%s, %d MiB up %.1f s / down %.1f s); fallback remembered" % (
            why, args.big_mib, upb["_seconds"], downb["_seconds"])

    run.step("config: endpoints and sources", config_step)
    run.step("signup + refresh", signup_step)
    run.step("files: 1 MiB + big up/down", files_step)
    run.step("sync twice, edit, B joins", sync_step)
    run.step("azlin tree A <-> B", azlin_step)
    run.step("transports https / iroh", transport_step)

    print()
    for status, name, detail in run.results:
        print("%-4s  %s" % (status, name))
    if ctx.get("defaults"):
        print("\nbuilt-in defaults the run relied on (configure them to remove the default):")
        for line in ctx["defaults"]:
            print("  " + line)
    if args.keep or run.failed():
        print("\nsandbox kept: %s" % base)
    else:
        shutil.rmtree(base, ignore_errors=True)
    return 1 if run.failed() else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Failure as e:
        print("FAIL  %s" % e)
        sys.exit(1)
