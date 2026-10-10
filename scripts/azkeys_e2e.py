#!/usr/bin/env python3
"""AzKeys end to end over the debug server (headless).

    1. starts AzKeys with --sample on an empty, temporary data folder: the sample vault is made
       (Argon2id + XChaCha20-Poly1305) and opens; the vault FILE on disk is checked: an
       azkeys-vault envelope that holds neither an item title nor a password in plain text;
    2. searches ("codehost" finds one item), selects it, reads its one-time code (six digits),
       copies its password (the clipboard countdown starts);
    3. locks the vault (the toolbar), tries a wrong master password (the message counts the
       attempts down), unlocks with the right one;
    4. adds a login (title, user name, password), saves it, locks, unlocks and finds it again
       (it went through the vault file);
    5. opens the generator (a 20-character password) and the audit (weak / reused / old counts);
    6. takes screenshots: the vault (light), the wrong-password unlock screen, the audit, the
       vault in dark mode.

Usage (from the azul repository, after building libazul with the debug server and AzKeys):

    python3 scripts/azkeys_e2e.py [--bin target/release/AzKeys] [--debug-port 8781]
        [--timeout 180] [--out <dir>] [--keep]

`AZKEYS_BIN` also names the binary. Run it through the capped runner on a small machine
(scripts/waves/tools/run_capped.sh). Never run while an app is running on the same port.
"""

import argparse
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

# The user's shared Azlin config (~/.azlin/config.json) stays out of the test: the app
# reads and writes one of its own (scripts/azlin_e2e.py does the same for the others).
if not os.environ.get("AZLIN_CONFIG"):
    os.environ["AZLIN_CONFIG"] = os.path.join(
        tempfile.mkdtemp(prefix="azlin-e2e-config-"), "config.json")

HERE = os.path.dirname(os.path.abspath(__file__))
PREFIX = "__azkeys_"
SAMPLE_PASSWORD = "sample"


def log(line):
    print("[azkeys] %s" % line, flush=True)


class Failure(Exception):
    pass


def repo_roots():
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
    if os.environ.get("AZKEYS_BIN"):
        candidates.append(os.environ["AZKEYS_BIN"])
    for root in repo_roots():
        for sub in ("release", "debug"):
            candidates.append(os.path.join(root, "target", sub, "AzKeys"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no AzKeys binary; pass --bin or set AZKEYS_BIN (tried %s)" % candidates)


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for v in value:
            yield from strings(v)
    elif isinstance(value, dict):
        for v in value.values():
            yield from strings(v)


def sel(name):
    """The CSS selector of one of AzKeys' ids (ids.rs: every id carries the prefix)."""
    return "#%s%s" % (PREFIX, name)


class App:
    """AzKeys under its debug server, on the data folder `data`."""

    def __init__(self, binary, port, data, logs, deadline, run, extra=()):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "azkeys-%d.stdout" % run)
        self.err_path = os.path.join(logs, "azkeys-%d.stderr" % run)
        env = dict(os.environ)
        env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(port)})
        self.process = subprocess.Popen(
            [binary, "--data-dir", data, "--size", "1200x760"] + list(extra), env=env,
            stdin=subprocess.DEVNULL,
            stdout=open(self.out_path, "wb"), stderr=open(self.err_path, "wb"),
        )

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()

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

    def texts(self):
        return list(strings(self.op("get_node_hierarchy")))

    def shows(self, text):
        return any(text in t for t in self.texts())

    def printed(self, key, pattern=r".+"):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^%s (%s)$" % (re.escape(key), pattern), text, re.M)

    def locks(self):
        """How often the vault locked (`AZKEYS_LOCKED` lines)."""
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                return len(re.findall(r"^AZKEYS_LOCKED$", f.read(), re.M))
        except OSError:
            return 0

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("AzKeys exited (%s) while waiting for %s" % (self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def key(self, key, shift=False, primary=False):
        ctrl = meta = False
        if primary:
            if sys.platform == "darwin":
                meta = True
            else:
                ctrl = True
        mods = {"shift": shift, "ctrl": ctrl, "alt": False, "meta": meta}
        self.must("key_down", key=key, modifiers=mods)
        # A tap of the chord: the key and its modifiers come up together (an op's modifiers are
        # the whole modifier state - the chord's would stay held for every later op).
        self.must("key_up", key=key, modifiers={"shift": False, "ctrl": False, "alt": False, "meta": False})
        self.frame(2)

    def type(self, text):
        self.must("text_input", text=text)
        self.frame(2)

    def click(self, selector):
        self.must("click", selector=selector)
        self.frame(2)

    def type_into(self, selector, text):
        """Types into a text field: focus it by its id, let a redraw settle, focus again, type."""
        self.must("focus_node", selector=selector)
        self.frame(1)
        time.sleep(0.2)
        self.must("focus_node", selector=selector)
        self.frame(1)
        self.type(text)

    def screenshot(self, path):
        self.must("wait_settled")
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure("take_screenshot returned no PNG: %s" % json.dumps(value)[:200])
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


def count(app, key, pattern=r".+"):
    return len(app.printed(key, pattern))


def unlock(app, password):
    """Types the master password and presses Unlock."""
    app.until("the unlock screen", lambda: app.shows("Unlock"))
    app.type_into(sel("unlock-password"), password)
    app.click(sel("unlock-button"))


def check_vault_file(data, vault_id):
    path = os.path.join(data, "keys", "vaults", "%s.azkv" % vault_id)
    with open(path, "r", encoding="utf-8") as f:
        text = f.read()
    envelope = json.loads(text)
    if envelope.get("format") != "azkeys-vault" or envelope.get("version") != 1:
        raise Failure("%s is no azkeys-vault v1 envelope: %s" % (path, text[:200]))
    if envelope.get("kdf", {}).get("algorithm") != "argon2id":
        raise Failure("%s does not derive with Argon2id" % path)
    for plain in ("CodeHost", "q7#Rt!vW2mZp8&Lk", "dev@example.org", SAMPLE_PASSWORD + '"'):
        if plain in text:
            raise Failure("%s holds %r in plain text" % (path, plain))
    log("vault file %s: sealed, %d bytes" % (path, len(text)))


def run(args):
    binary = find_binary(args.bin)
    out = args.out or tempfile.mkdtemp(prefix="azkeys-e2e-")
    os.makedirs(out, exist_ok=True)
    data = tempfile.mkdtemp(prefix="azkeys-data-")
    deadline = time.time() + args.timeout
    log("binary %s, data %s, out %s" % (binary, data, out))
    app = App(binary, args.debug_port, data, out, deadline, 1, extra=["--sample"])
    try:
        # 1. The sample vault is made and opens; its file is sealed.
        vault_id = app.until("the sample vault", lambda: (app.printed("AZKEYS_CREATED", r"[0-9a-f-]+") or [None])[-1])
        app.until("the vault screen", lambda: app.shows("All items ("))
        check_vault_file(data, vault_id)
        app.screenshot(os.path.join(out, "vault-flat-light.png"))

        # 2. Search, select, the one-time code, copy.
        # Every word must match: "codehost" alone also finds the sample's secure note
        # "CodeHost recovery codes" (the sample got its 11 notes in db13c2ce8).
        app.type_into(sel("list-search"), "codehost example")
        app.until("one item for 'codehost example'",
                  lambda: app.printed("AZKEYS_VIEW", r"\d+")[-1:] == ["1"])
        app.click(sel("row-0"))
        app.until("CodeHost selected", lambda: "CodeHost (example)" in app.printed("AZKEYS_SELECTED"))
        code = app.until("a one-time code", lambda: [t for t in app.texts() if re.fullmatch(r"\d{3} \d{3}", t)])
        log("one-time code shown: %s" % code[0])
        app.click(sel("copy-password"))
        app.until("the copy", lambda: "password of CodeHost (example)" in app.printed("AZKEYS_COPIED"))
        app.until("the clipboard countdown", lambda: app.shows("Clipboard clears in"))

        # 3. Lock, a wrong password, the right one.
        app.click(sel("toolbar-lock"))
        app.until("the lock", lambda: app.locks() >= 1 and app.shows("Unlock"))
        unlock(app, "not the password")
        app.until("the wrong password", lambda: app.printed("AZKEYS_WRONG_PASSWORD", r"\d+"))
        app.until("the attempts message", lambda: app.shows("2 attempts left"))
        app.screenshot(os.path.join(out, "unlock-wrong-password.png"))
        unlocked = count(app, "AZKEYS_UNLOCKED")
        unlock(app, SAMPLE_PASSWORD)
        app.until("the unlock", lambda: count(app, "AZKEYS_UNLOCKED") > unlocked)
        log("locked, refused a wrong password, unlocked")

        # 4. A new login goes through the vault file.
        saves = count(app, "AZKEYS_SAVED")
        app.click(sel("toolbar-new"))
        app.until("the new-item form", lambda: app.shows("New login"))
        app.type_into(sel("edit-title"), "E2E site")
        app.type_into(sel("edit-username"), "e2e@example.org")
        app.type_into(sel("edit-password"), "e2e-Pa55word-xyz!")
        app.click(sel("edit-save"))
        app.until("the item saved", lambda: "E2E site" in app.printed("AZKEYS_ITEM_SAVED"))
        app.until("the vault written", lambda: count(app, "AZKEYS_SAVED") > saves)
        app.click(sel("toolbar-lock"))
        unlocked = count(app, "AZKEYS_UNLOCKED")
        unlock(app, SAMPLE_PASSWORD)
        app.until("the unlock", lambda: count(app, "AZKEYS_UNLOCKED") > unlocked)
        app.type_into(sel("list-search"), "E2E")
        app.until("the new login after a lock", lambda: app.printed("AZKEYS_VIEW", r"\d+")[-1:] == ["1"])
        log("a new login survived a lock")

        # 5. The generator and the audit.
        app.click(sel("toolbar-generator"))
        app.until("a generated password", lambda: [t for t in app.texts() if len(t) == 20 and " " not in t])
        app.click(sel("toolbar-audit"))
        app.until("the audit", lambda: app.shows("Security audit"))
        app.screenshot(os.path.join(out, "audit.png"))

        # 6. Dark mode.
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.screenshot(os.path.join(out, "vault-dark.png"))
        log("PASS")
        return 0
    except Failure as e:
        log("FAIL: %s" % e)
        log("stdout:\n%s" % open(app.out_path, errors="replace").read()[-3000:])
        log("stderr:\n%s" % open(app.err_path, errors="replace").read()[-3000:])
        return 1
    finally:
        app.stop()
        if not args.keep:
            shutil.rmtree(data, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8781)
    parser.add_argument("--timeout", type=float, default=180.0)
    parser.add_argument("--out")
    parser.add_argument("--keep", action="store_true")
    sys.exit(run(parser.parse_args()))


if __name__ == "__main__":
    main()
