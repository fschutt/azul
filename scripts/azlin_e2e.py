#!/usr/bin/env python3
"""Shared driver for the Azlin apps' headless E2E scripts (azcalculator_e2e.py,
azcontacts_e2e.py): start an app with AZ_BACKEND=headless and AZ_DEBUG=<port>,
talk to its debug server (ops in layout/src/e2e/full.rs), read the lines it
prints, take screenshots.

Every op that changes state is followed by `frame()` (wait_frame). The app's
stdout is read from a log file; `printed(key)` returns the values of the lines
`<KEY> <value>`, `until(what, check)` polls until `check()` is truthy.
"""

import base64
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))


class Failure(Exception):
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


def find_binary(name, explicit=None, env_var=None):
    candidates = []
    if explicit:
        candidates.append(explicit)
    if env_var and os.environ.get(env_var):
        candidates.append(os.environ[env_var])
    for root in repo_roots():
        for sub in ("release", "debug"):
            candidates.append(os.path.join(root, "target", sub, name))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no %s binary; pass --bin (tried %s)" % (name, candidates))


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for v in value:
            yield from strings(v)
    elif isinstance(value, dict):
        for v in value.values():
            yield from strings(v)


def dicts(value):
    if isinstance(value, dict):
        yield value
        for v in value.values():
            yield from dicts(v)
    elif isinstance(value, list):
        for v in value:
            yield from dicts(v)


def tail(path, lines=40):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return "".join(f.readlines()[-lines:])
    except OSError:
        return "(no output)"


class App:
    """One app under its debug server."""

    def __init__(self, tag, binary, args, port, logs, timeout, extra_env=None):
        self.tag = tag
        self.port = port
        self.deadline = time.time() + timeout
        self.out_path = os.path.join(logs, "%s.stdout" % tag)
        self.err_path = os.path.join(logs, "%s.stderr" % tag)
        env = dict(os.environ)
        env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(port)})
        env.update(extra_env or {})
        self.process = subprocess.Popen(
            [binary] + list(args), env=env, stdin=subprocess.DEVNULL,
            stdout=open(self.out_path, "wb"), stderr=open(self.err_path, "wb"),
        )

    def log(self, line):
        print("[%s] %s" % (self.tag, line), flush=True)

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()

    # ---- the debug server ----

    def op(self, op, **params):
        body = {"op": op}
        body.update(params)
        request = urllib.request.Request(
            "http://127.0.0.1:%d/" % self.port,
            data=json.dumps(body).encode("utf-8"), method="POST",
        )
        with urllib.request.urlopen(request, timeout=15) as response:
            return json.loads(response.read().decode("utf-8") or "{}")

    def must(self, op, **params):
        answer = self.op(op, **params)
        if isinstance(answer, dict) and answer.get("status") == "error":
            raise Failure("%s %s failed: %s" % (op, json.dumps(params), json.dumps(answer)[:300]))
        return answer

    def value(self, op, **params):
        answer = self.must(op, **params)
        data = answer.get("data") if isinstance(answer, dict) else None
        if isinstance(data, dict) and "value" in data:
            return data["value"]
        return data if data is not None else answer

    def frame(self, n=1):
        for _ in range(n):
            self.must("wait_frame")

    # ---- what the window shows ----

    def texts(self):
        return list(strings(self.op("get_node_hierarchy")))

    def shows(self, text):
        return any(text in t for t in self.texts())

    def has_id(self, node_id):
        answer = self.op("get_node_layout", selector="#%s" % node_id)
        if not isinstance(answer, dict) or answer.get("status") == "error":
            return False
        data = answer.get("data") or {}
        value = data.get("value") if isinstance(data, dict) else None
        return isinstance(value, dict) and value.get("node_id") is not None

    def rect(self, node_id):
        value = self.value("get_node_layout", selector="#%s" % node_id)
        return value.get("rect") or {}

    # ---- input ----

    def click(self, selector=None, text=None, frames=2):
        if selector:
            self.must("click", selector=selector)
        else:
            self.must("click", text=text)
        self.frame(frames)

    def key(self, key, shift=False, ctrl=False, alt=False, meta=False, frames=2):
        mods = {"shift": shift, "ctrl": ctrl, "alt": alt, "meta": meta}
        self.must("key_down", key=key, modifiers=mods)
        self.must("key_up", key=key, modifiers=mods)
        self.frame(frames)

    def type_keys(self, keys):
        """keys: a list of key names or (name, {"shift": True}) pairs."""
        for k in keys:
            if isinstance(k, tuple):
                self.key(k[0], **k[1])
            else:
                self.key(k)

    def text_input(self, selector, text):
        self.must("focus_node", selector=selector)
        self.frame()
        self.must("text_input", text=text)
        self.frame(2)

    # ---- stdout ----

    def printed(self, key, pattern=r".*"):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^%s (%s)$" % (re.escape(key), pattern), text, re.M)

    def last(self, key):
        values = self.printed(key)
        return values[-1] if values else None

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("%s exited (%s) while waiting for %s" % (self.tag, self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def expect_line(self, key, expected, what=None):
        """Waits until the last `<key> ...` line equals `expected`."""
        def check():
            return self.last(key) == expected
        try:
            self.until(what or "%s %r" % (key, expected), check)
        except Failure:
            raise Failure("%s: expected %r, last %r" % (what or key, expected, self.last(key)))

    def screenshot(self, path):
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure("take_screenshot returned no PNG: %s" % json.dumps(value)[:200])
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        self.log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


def run(tag, body, argv=None, default_port=8781, binary_name=None, binary_env=None):
    """Parses the common switches, starts nothing itself: `body(args, logs, out)`
    does the work and returns True. Exit code 0 on success."""
    import argparse

    parser = argparse.ArgumentParser(description=sys.modules["__main__"].__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=default_port)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--out")
    parser.add_argument("--keep", action="store_true", help="keep the data folder and logs")
    args = parser.parse_args(argv)
    logs = tempfile.mkdtemp(prefix="%s-e2e-" % tag)
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    passed = False
    try:
        passed = bool(body(args, logs, out))
    except Failure as e:
        print("[%s] FAIL: %s" % (tag, e), flush=True)
        passed = False
    finally:
        print("[%s] logs and screenshots in %s" % (tag, logs), flush=True)
    sys.exit(0 if passed else 1)
