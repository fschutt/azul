#!/usr/bin/env python3
"""An azul app run headless with its debug server, driven over HTTP.

    with AzulHeadless() as az:
        az.resize(760, 1100)
        az.mount("<body><p class='x'>hi</p></body>", css="p { color: red }")
        nodes = az.all_nodes_layout()     # [{node_id, tag, id, classes, rect}]
        png = az.screenshot()             # PNG bytes of the window

The app is any prebuilt azul binary whose dylib has the debug server
(`AZ_DEBUG=<port>`), started with `AZ_BACKEND=headless` so no window opens. The
`mount` op replaces the app's own DOM with the given markup, parsed by azul's
XML loader (layout/src/e2e/full.rs `DebugEvent::Mount`). The ops are documented
in scripts/ideas/DEBUG_API.md.

Defaults: AZUL_APP (else target/release/AzPaint - a small app with no <video>)
and AZUL_LIB_DIR (else target/azul-lib), both relative to the repo root that
holds this file, or to AZUL_ROOT.

Memory: the app is killed the moment its resident set passes `cap_mb`
(default 1500) or after `seconds` (default 120) - a 2026-09-30 kernel panic on
an 8 GB Mac came from an app that reached 17.7 GB. A caller that needs longer
restarts the app between batches (`AzulHeadless.restart()`), which also keeps
one document's leftovers from reaching the next. AZUL_CAPPED_RUNNER may name
a wrapper script (`run_capped.sh --cap-mb N --seconds S --log F -- cmd...`)
to use instead of the built-in watchdog.
"""

import base64
import json
import os
import signal
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.environ.get("AZUL_ROOT") or os.path.dirname(os.path.dirname(HERE))


class AzulError(RuntimeError):
    pass


def _free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def _rss_mb(pid):
    try:
        out = subprocess.check_output(["ps", "-o", "rss=", "-p", str(pid)], text=True, timeout=5)
        return int(out.strip() or 0) // 1024
    except Exception:  # noqa: BLE001 - the process is gone
        return 0


class AzulHeadless:
    def __init__(self, app=None, lib_dir=None, port=None, log_path=None, cap_mb=1500, seconds=120):
        self.app = app or os.environ.get("AZUL_APP") or os.path.join(REPO, "target", "release", "AzPaint")
        self.lib_dir = lib_dir or os.environ.get("AZUL_LIB_DIR") or os.path.join(REPO, "target", "azul-lib")
        if not os.access(self.app, os.X_OK):
            raise AzulError("no azul app at %s (set AZUL_APP)" % self.app)
        self.log_path = log_path or os.devnull
        self.cap_mb = cap_mb
        self.seconds = seconds
        self.capped = None
        self.proc = None
        self.log = None
        self._watchdog = None
        self._start(port)

    def _start(self, port=None):
        self.port = port or _free_port()
        self.base = "http://127.0.0.1:%d/" % self.port
        env = dict(os.environ)
        env["AZ_BACKEND"] = "headless"
        env["AZ_DEBUG"] = str(self.port)
        for var in ("DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"):
            env[var] = self.lib_dir + (os.pathsep + env[var] if env.get(var) else "")
        self.log = open(self.log_path, "ab")
        runner = os.environ.get("AZUL_CAPPED_RUNNER")
        if runner:
            cmd = [runner, "--cap-mb", str(self.cap_mb), "--seconds", str(self.seconds),
                   "--log", self.log_path, "--", "env", "AZ_BACKEND=headless", "AZ_DEBUG=%d" % self.port, self.app]
            self.proc = subprocess.Popen(cmd, env=env, start_new_session=True,
                                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        else:
            self.proc = subprocess.Popen([self.app], env=env, start_new_session=True,
                                         stdout=self.log, stderr=subprocess.STDOUT)
            self.capped = None
            self._watchdog = threading.Thread(target=self._watch, args=(self.proc, time.time()), daemon=True)
            self._watchdog.start()
        deadline = time.time() + 60
        while True:
            if self.proc.poll() is not None:
                raise AzulError("%s exited with %s before its debug server answered (%s)"
                                % (self.app, self.proc.returncode, self.capped or "see " + self.log_path))
            try:
                with urllib.request.urlopen(self.base, timeout=2) as r:
                    r.read(16)
                break
            except (urllib.error.URLError, ConnectionError, socket.timeout, OSError):
                if time.time() > deadline:
                    self.close()
                    raise AzulError("the debug server on port %d did not answer within 60 s" % self.port)
                time.sleep(0.2)

    def _watch(self, proc, started):
        """Kill the app past the memory cap or the time limit (run_capped.sh's rule)."""
        while proc.poll() is None:
            rss = _rss_mb(proc.pid)
            if rss > self.cap_mb:
                self.capped = "CAPPED %d MB" % rss
                self._kill_group(proc)
                return
            if time.time() - started > self.seconds:
                self.capped = "TIMEOUT %d s" % self.seconds
                self._kill_group(proc)
                return
            time.sleep(0.5)

    @staticmethod
    def _kill_group(proc):
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass

    def alive(self):
        return self.proc is not None and self.proc.poll() is None

    def restart(self):
        """A fresh app process (same port range, same log): between batches."""
        self.close()
        self._start()

    def op(self, op, timeout=120, **params):
        if not self.alive():
            raise AzulError("the app is not running (%s)" % (self.capped or "exited"))
        body = dict(params)
        body["op"] = op
        req = urllib.request.Request(self.base, data=json.dumps(body).encode("utf-8"), method="POST",
                                     headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=timeout) as r:
            resp = json.loads(r.read().decode("utf-8"))
        if resp.get("status") != "ok":
            raise AzulError("%s: %s" % (op, resp.get("message")))
        return resp.get("data", {}).get("value") if isinstance(resp.get("data"), dict) else None

    def resize(self, width, height):
        self.op("resize", width=float(width), height=float(height))
        self.op("wait_frame")

    def mount(self, html, css=""):
        # A document mounted over another lays out with the first one's
        # leftovers (scripts/REFCI_2026_09_30.md): always start from the app's DOM.
        self.op("unmount")
        self.op("wait_frame")
        self.op("mount", html=html, css=css)
        self.op("wait_frame")

    def all_nodes_layout(self):
        return (self.op("get_all_nodes_layout") or {}).get("nodes", [])

    def screenshot(self):
        value = self.op("take_screenshot") or {}
        data = value.get("data", "")
        if "," in data:
            data = data.split(",", 1)[1]
        return base64.b64decode(data)

    def close(self):
        if self.proc is not None and self.proc.poll() is None:
            try:
                self.op("close", timeout=5)
            except Exception:  # noqa: BLE001 - it may already be going
                pass
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self._kill_group(self.proc)
                self.proc.wait()
        if self.log:
            self.log.close()
            self.log = None

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()


if __name__ == "__main__":
    with AzulHeadless() as az:
        az.resize(400, 300)
        az.mount(sys.argv[1] if len(sys.argv) > 1 else "<body><div class='x' style='width:100px;height:20px'></div></body>")
        print(json.dumps(az.all_nodes_layout(), indent=1))
        print("rss MB", _rss_mb(az.proc.pid), file=sys.stderr)
