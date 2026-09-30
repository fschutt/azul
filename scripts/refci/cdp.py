#!/usr/bin/env python3
"""A headless Chrome driven over the DevTools protocol, standard library only.

Chrome is started with `--remote-debugging-pipe`: it reads protocol messages on
file descriptor 3 and writes on 4, each a JSON object ended by a NUL byte. That
avoids a WebSocket client (not in the standard library) and a TCP port.
POSIX only (macOS, Linux). The repo's Node client is scripts/e2e-web/lib/cdp.mjs.

    with Chrome() as chrome:
        page = chrome.open("file:///tmp/x.html", width=760, height=1100)
        boxes = page.evaluate("document.title")
        png = page.screenshot(full_page=True)
        page.close()
"""

import base64
import json
import os
import shutil
import subprocess
import tempfile
import time

CANDIDATES = [
    os.environ.get("CHROME", ""),
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "chrome",
]


def find_chrome():
    for c in CANDIDATES:
        if not c:
            continue
        if os.path.isabs(c) and os.access(c, os.X_OK):
            return c
        found = shutil.which(c)
        if found:
            return found
    raise RuntimeError("no Chrome found; set CHROME=<path to the binary>")


class CdpError(RuntimeError):
    pass


def _high(fd):
    """`fd` moved to a close-on-exec descriptor numbered 10 or higher."""
    import fcntl

    high = fcntl.fcntl(fd, fcntl.F_DUPFD_CLOEXEC, 10)
    os.close(fd)
    return high


class Chrome:
    def __init__(self, binary=None, extra_args=()):
        self.binary = binary or find_chrome()
        self.profile = tempfile.mkdtemp(prefix="refci-chrome-")
        # Every pipe end is close-on-exec and numbered 10 or higher, so the two
        # dup2() calls below never copy an fd onto itself (which would keep its
        # close-on-exec flag) and only fds 3 and 4 survive into Chrome.
        to_chrome_r, self._to_chrome = (_high(fd) for fd in os.pipe())
        self._from_chrome, from_chrome_w = (_high(fd) for fd in os.pipe())

        def wire():  # runs in the child: the pipe ends become fds 3 and 4
            os.dup2(to_chrome_r, 3)
            os.dup2(from_chrome_w, 4)

        args = [
            self.binary, "--headless=new", "--remote-debugging-pipe",
            "--user-data-dir=" + self.profile, "--no-first-run", "--no-default-browser-check",
            "--disable-gpu", "--hide-scrollbars", "--force-color-profile=srgb",
            "--disable-extensions", "--mute-audio", "--allow-file-access-from-files",
            # Nothing leaves the machine: every host name fails to resolve.
            "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE localhost",
        ] + list(extra_args) + ["about:blank"]
        self.proc = subprocess.Popen(args, close_fds=False, preexec_fn=wire,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.close(to_chrome_r)
        os.close(from_chrome_w)
        self._next_id = 0
        self._buf = b""
        self._events = []
        self.version = self.call("Browser.getVersion")

    # -- transport -------------------------------------------------------
    def _send(self, msg):
        data = json.dumps(msg).encode("utf-8") + b"\0"
        while data:
            n = os.write(self._to_chrome, data)
            data = data[n:]

    def _recv(self, timeout):
        deadline = time.time() + timeout
        while b"\0" not in self._buf:
            if time.time() > deadline:
                raise CdpError("Chrome did not answer within %.0f s" % timeout)
            chunk = os.read(self._from_chrome, 1 << 20)
            if not chunk:
                raise CdpError("Chrome closed the pipe (exit code %s)" % self.proc.poll())
            self._buf += chunk
        msg, self._buf = self._buf.split(b"\0", 1)
        return json.loads(msg.decode("utf-8"))

    def call(self, method, params=None, session=None, timeout=60):
        self._next_id += 1
        mid = self._next_id
        msg = {"id": mid, "method": method, "params": params or {}}
        if session:
            msg["sessionId"] = session
        self._send(msg)
        while True:
            m = self._recv(timeout)
            if m.get("id") == mid:
                if "error" in m:
                    raise CdpError("%s: %s" % (method, m["error"].get("message")))
                return m.get("result", {})
            self._events.append(m)

    def wait_event(self, method, session=None, timeout=60):
        for i, e in enumerate(self._events):
            if e.get("method") == method and e.get("sessionId") == session:
                return self._events.pop(i)
        deadline = time.time() + timeout
        while True:
            m = self._recv(max(0.1, deadline - time.time()))
            if m.get("method") == method and m.get("sessionId") == session:
                return m
            self._events.append(m)

    # -- pages -----------------------------------------------------------
    def open(self, url, width=800, height=600):
        target = self.call("Target.createTarget", {"url": "about:blank"})["targetId"]
        session = self.call("Target.attachToTarget", {"targetId": target, "flatten": True})["sessionId"]
        page = Page(self, target, session)
        page.call("Page.enable")
        page.call("Emulation.setDeviceMetricsOverride",
                  {"width": width, "height": height, "deviceScaleFactor": 1, "mobile": False})
        page.call("Page.navigate", {"url": url})
        self.wait_event("Page.loadEventFired", session)
        return page

    def close(self):
        try:
            self.call("Browser.close", timeout=5)
        except Exception:  # noqa: BLE001 - already gone
            pass
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
        for fd in (self._to_chrome, self._from_chrome):
            try:
                os.close(fd)
            except OSError:
                pass
        shutil.rmtree(self.profile, ignore_errors=True)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()


class Page:
    def __init__(self, chrome, target, session):
        self.chrome = chrome
        self.target = target
        self.session = session

    def call(self, method, params=None, timeout=60):
        return self.chrome.call(method, params, session=self.session, timeout=timeout)

    def evaluate(self, expression):
        r = self.call("Runtime.evaluate", {"expression": expression, "returnByValue": True})
        if "exceptionDetails" in r:
            raise CdpError("evaluate: %s" % r["exceptionDetails"].get("text"))
        return r["result"].get("value")

    def screenshot(self, full_page=False):
        params = {"format": "png"}
        if full_page:
            w, h = self.evaluate("[document.documentElement.scrollWidth, document.documentElement.scrollHeight]")
            params["captureBeyondViewport"] = True
            params["clip"] = {"x": 0, "y": 0, "width": w, "height": h, "scale": 1}
        return base64.b64decode(self.call("Page.captureScreenshot", params)["data"])

    def close(self):
        self.chrome.call("Target.closeTarget", {"targetId": self.target})
