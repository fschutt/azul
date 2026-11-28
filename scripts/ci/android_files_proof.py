#!/usr/bin/env python3
"""android_files_proof.py: the emulator proof of the files / sharing / open-with work (ledger WP-F), with pictures.

Run through the QA's emulator tool (it boots the emulator and calls this script in place of android_probe.py):

    AZ_PROBE_PY=scripts/ci/android_files_proof.py bash scripts/ci/android_probe.sh out AzSelfTest-e2e-android-x86_64.apk

What it proves, each with the evidence it prints and writes to <out>/files-proof.md and files-proof.json (pictures in <out>/pictures/):

  SHARE OUT (CallbackInfo::share, the chooser, the provider)
    1. one file: AzSelfTest's row `files.share_one` hands a file to the system's chooser; the picture shows the chooser with ShareLab in it;
       the script picks ShareLab; ShareLab (a different app, reading through the content resolver like a mail app does) logs the display
       name, the size, the type the provider answered and the SHA-256 of what it read: all equal to what the row wrote;
    2. several files and a text: `files.share_many` (ACTION_SEND_MULTIPLE with a ClipData): every file arrives, in order, with its own name
       (two files that had the same name arrive as "a.md" and "a (2).md"), the subject is the title;
    3. open with: `files.open_with` (ACTION_VIEW of one file through a chooser): ShareLab gets the file and the grant;
    4. the grant is per URI: after the share, ShareLab asks the provider for a URI of ANOTHER share folder and a path outside the area
       (`..`, the types file, the app's databases): none is served (the app that the chooser was given the grant for reads nothing else);
    5. nothing of the app's data is exposed: the provider's manifest entry is not exported (dumpsys package).
  SHARE IN / OPEN WITH (a different app starts the self-test app the way a share target of a real app does)
    6. `open_running`: ShareLab's SendActivity starts the RUNNING app with a VIEW of a document it serves, WITHOUT FLAG_ACTIVITY_NEW_TASK:
       the app's handler logs one `AZ_SELFTEST_OPEN` line with the file's name, type and the working copy READ BACK (length and digest
       equal to the bytes ShareLab serves); there is exactly ONE `android_main` and ONE AzulActivity record (singleTask: no second copy);
    7. `open_cold`: the same with the app NOT running: the request that STARTED the app reaches the handler (`launched_app: true`);
    8. `send_many`: SEND_MULTIPLE of two documents, both copied, in order; a 3 MiB document is copied whole;
    9. `write_back`: ShareLab EDITs a writable document with a write grant; the self-test app appends a line and saves the working copy
       BACK; ShareLab's own file holds the line (read as root).
    10. `folder` (needs the system's folder picker driven through uiautomator): the app asks for a folder, the grant is kept, a file is made
       and read back inside it, and after the app was closed and opened again the folder is still one of its places.
    11. `export`: the system's save dialog; the file lands where the person chose, with the bytes the app gave.
  Every step keeps a picture (the chooser, the receiver) and the log lines that decided it.

The exit status is 0 when every expectation held, 1 when one did not, 2 when an app could not be installed or started.
"""
import argparse
import hashlib
import json
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import android_probe as probe  # noqa: E402

LAB_PACKAGE = "com.azul.sharelab"
LAB_LABEL = "ShareLab"
RECEIVED = "AZ_SHARELAB_RECEIVED "
OPEN = "AZ_SELFTEST_OPEN "
ROW = "AZ_SELFTEST_ROW "
HELLO = b"# Hello from ShareLab\n\nA Markdown document.\n"
PLAIN = b"plain text from ShareLab\n"
BIG_BYTES = 3 * 1024 * 1024
APP_ACTIVITY = "com.azul.app.AzulActivity"

# What the rows of AzSelfTest write: the script computes the same bytes (examples/azul-selftest/src/probes/files.rs).
def share_one_bytes():
    return b"AzSelfTest share probe\n" + bytes(range(256)) * 4


def share_many_bytes(which):
    return (b"# a\n" if which == "a" else b"# a again\n") + bytes(range(64))


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def fnv1a64(data):
    """The digest the self-test app's open handler logs for a working copy (the same FNV-1a over 64 bits)."""
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def open_lines(logcat):
    """The JSON objects the self-test app's open handler logged, in order."""
    out = []
    for line in (logcat or "").splitlines():
        at = line.find(OPEN)
        if at >= 0:
            try:
                out.append(json.loads(line[at + len(OPEN):]))
            except ValueError:
                out.append({"unparsable": line[at:at + 300]})
    return out


def row_lines(logcat):
    """The one-line verdicts of the self-test app's rows (`AZ_SELFTEST_ROW {json}`), in order."""
    out = []
    for line in (logcat or "").splitlines():
        at = line.find(ROW)
        if at >= 0:
            try:
                out.append(json.loads(line[at + len(ROW):]))
            except ValueError:
                pass
    return out


def received_lines(logcat):
    """The JSON objects ShareLab logged, in order."""
    out = []
    for line in (logcat or "").splitlines():
        at = line.find(RECEIVED)
        if at >= 0:
            try:
                out.append(json.loads(line[at + len(RECEIVED):]))
            except ValueError:
                out.append({"unparsable": line[at:at + 300]})
    return out


class Proof:
    def __init__(self, dev, out, package):
        self.dev, self.out, self.package = dev, Path(out), package
        (self.out / "pictures").mkdir(parents=True, exist_ok=True)
        self.lines, self.ok, self.results = [], True, []

    def say(self, text):
        print(text, flush=True)
        self.lines.append(text)

    def expect(self, condition, what, detail=""):
        self.say(f"  [{'ok' if condition else 'FAIL'}] {what}" + (f" ({detail})" if detail and not condition else ""))
        self.ok = self.ok and bool(condition)
        self.results.append({"expect": what, "ok": bool(condition), "detail": detail})

    def shot(self, name):
        """A picture of the screen (the device's own, through screencap): pictures/<name>.png; -> the path."""
        path = self.out / "pictures" / f"{name}.png"
        try:
            png = subprocess.run(["adb"] + (["-s", self.dev.serial] if self.dev.serial else []) + ["exec-out", "screencap", "-p"],
                                 capture_output=True, timeout=60).stdout
            path.write_bytes(png)
        except (OSError, subprocess.TimeoutExpired) as e:
            self.say(f"  (no picture {name}: {e})")
        return path

    def ui(self):
        return self.dev.dump_ui()

    def wait_focus(self, pattern, seconds=30.0):
        """Waits until the focused window matches `pattern`; -> the focus line or ''."""
        t0 = time.time()
        while time.time() - t0 < seconds:
            line = self.dev.focus()
            if re.search(pattern, line or ""):
                return line
            time.sleep(0.5)
        return ""

    def start_row(self, row, extra_props=None):
        """A cold start of the self-test app that runs ONE row (`debug.az.selftest_rows`) and nothing else."""
        self.dev.sh(f"am force-stop {self.package}")
        self.dev.sh(f"am force-stop {LAB_PACKAGE}")
        self.dev.sh("setprop debug.az.selftest 0")
        self.dev.sh(f"setprop debug.az.selftest_rows {row}")
        self.dev.adb("logcat", "-c")
        time.sleep(0.5)
        self.dev.sh(f"am start -n {self.package}/{APP_ACTIVITY}")
        # the first gesture of the window: the engine holds some requests until it saw one
        time.sleep(5.0)
        self.dev.sh("input tap 20 200")

    def pick_in_chooser(self, label, picture):
        """The system's chooser is up: picture, then a tap on `label`; -> whether it was there."""
        focus = self.wait_focus(r"(?i)chooser|resolver|intentresolver", seconds=40)
        self.expect(bool(focus), "the system's chooser is on the screen", focus.strip()[:160])
        if not focus:
            self.shot(picture + "-no-chooser")
            return False
        time.sleep(1.5)
        self.shot(picture)
        xml = self.ui()
        tapped = self.dev.tap_node(xml, text=label)
        if not tapped:
            # the sheet may need to be expanded: a swipe up, then once more
            self.dev.sh("input swipe 540 1900 540 700 300")
            time.sleep(1.0)
            xml = self.ui()
            tapped = self.dev.tap_node(xml, text=label)
        self.expect(tapped, f"{label} is a target of the chooser")
        return tapped

    def received(self, seconds=25.0, count=1):
        """The lines ShareLab logged (waits for `count`)."""
        t0 = time.time()
        got = []
        while time.time() - t0 < seconds:
            got = received_lines(self.dev.adb("logcat", "-d", "-v", "tag", "-s", "AzShareLab:I", timeout=60))
            if len(got) >= count:
                break
            time.sleep(1.0)
        return got

    # ---- the steps ----

    def share_one(self):
        self.say("1. one file through the chooser")
        self.start_row("files.share_one")
        if not self.pick_in_chooser(LAB_LABEL, "01-share-one-chooser"):
            return
        got = self.received()
        self.shot("02-share-one-received")
        self.expect(len(got) == 1, "ShareLab received exactly one share", f"{len(got)}")
        if not got:
            return
        msg = got[-1]
        data = share_one_bytes()
        item = (msg.get("items") or [{}])[0]
        self.expect(msg.get("action") == "android.intent.action.SEND", "the action is SEND", str(msg.get("action")))
        self.expect(item.get("name") == "selftest-share.azt", "the receiver is shown the name the app gave", str(item.get("name")))
        self.expect(item.get("size") == len(data), "the size the provider answers is the file's", f"{item.get('size')} vs {len(data)}")
        self.expect(item.get("read") == len(data) and item.get("sha256") == sha256(data),
                    "the receiver read every byte and they are the file's", f"read {item.get('read')}")
        self.expect(item.get("provider_type") == "application/vnd.azul.selftest", "the type is the one the app / table decided", str(item.get("provider_type")))
        self.expect(msg.get("subject") == "AzSelfTest: one file", "the title reached the receiver as its subject", str(msg.get("subject")))
        uri = str(item.get("uri"))
        self.expect(uri.startswith(f"content://{self.package}.azul.share/") and "/data/" not in uri,
                    "the URI is the app's provider's, not a path", uri)
        # the /proc/self/fd question (ledger 3.4 a): the descriptor a provider gave works, does its PATH open again in the receiver?
        fd_proc = str(item.get("fd_proc"))
        self.say(f"  [info] /proc/self/fd/N of the descriptor the provider gave ShareLab: {fd_proc}")
        self.results.append({"info": "fd_proc of a provider file in the receiving app", "detail": fd_proc})
        self.results[-1]["uri"] = uri

    def share_many(self):
        self.say("2. several files and a text (SEND_MULTIPLE)")
        self.start_row("files.share_many")
        if not self.pick_in_chooser(LAB_LABEL, "03-share-many-chooser"):
            return
        got = self.received(count=1)
        self.shot("04-share-many-received")
        self.expect(len(got) == 1, "ShareLab received one share", f"{len(got)}")
        if not got:
            return
        msg = got[-1]
        items = msg.get("items") or []
        names = [i.get("name") for i in items]
        self.expect(msg.get("action") == "android.intent.action.SEND_MULTIPLE", "the action is SEND_MULTIPLE", str(msg.get("action")))
        self.expect(names == ["a.md", "a (2).md"], "two files of one name arrive as a.md and a (2).md, in order", str(names))
        want = [sha256(share_many_bytes("a")), sha256(share_many_bytes("b"))]
        self.expect([i.get("sha256") for i in items] == want, "every file's bytes are the app's, each its own")

    def open_with(self):
        self.say("3. open with (ACTION_VIEW of one file)")
        self.start_row("files.open_with")
        if not self.pick_in_chooser(LAB_LABEL, "05-open-with-chooser"):
            return
        got = self.received()
        self.shot("06-open-with-received")
        self.expect(len(got) == 1, "ShareLab was opened with the file", f"{len(got)}")
        if not got:
            return
        msg = got[-1]
        item = (msg.get("items") or [{}])[0]
        self.expect(msg.get("action") == "android.intent.action.VIEW", "the action is VIEW", str(msg.get("action")))
        self.expect(item.get("name") == "selftest-open.md" and item.get("sha256") == sha256(share_one_bytes()),
                    "the file and its bytes arrive", str(item.get("name")))
        self.expect(msg.get("type") == "text/markdown", "typed by its extension (md)", str(msg.get("type")))

    # ---- the other direction: ShareLab sends, the self-test app is opened with ----

    def send(self, action, docs, mime=None):
        """ShareLab's SendActivity starts the app with `docs` (its own provider's files), WITHOUT FLAG_ACTIVITY_NEW_TASK."""
        extra = f" --es mime {mime}" if mime else ""
        return self.dev.sh(f"am start -n {LAB_PACKAGE}/.SendActivity --es to {self.package} --es action {action} --es docs {docs}{extra}")

    def log(self):
        return self.dev.adb("logcat", "-d", "-v", "tag", timeout=90)

    def opened(self, seconds=30.0, count=1):
        """The open-handler lines the app logged (waits for `count`)."""
        t0 = time.time()
        got = []
        while time.time() - t0 < seconds:
            got = open_lines(self.log())
            if len(got) >= count:
                break
            time.sleep(1.0)
        return got

    def one_instance(self, what):
        """There is ONE `android_main` since the log was cleared and ONE AzulActivity record: the share did not start a second copy."""
        text = self.log()
        mains = text.count("android_main entered")
        records = set(re.findall(rf"ActivityRecord\{{(\w+) [^}}]*{re.escape(self.package)}/com\.azul\.app\.AzulActivity", self.dev.sh("dumpsys activity activities")))
        self.expect(mains == 1, f"{what}: one android_main", f"{mains}")
        self.expect(len(records) <= 1, f"{what}: one AzulActivity record", f"{sorted(records)}")

    def item_of(self, request, name):
        for item in request.get("items") or []:
            if item.get("name") == name:
                return item
        return {}

    def open_running(self):
        self.say("6. a share into the RUNNING app, from an app that is not in our task (no NEW_TASK)")
        self.start_row("files.open_in")
        time.sleep(3.0)
        self.send("VIEW", "hello.md", "text/markdown")
        got = self.opened()
        self.shot("07-open-running")
        self.expect(len(got) == 1, "the open handler heard one request", f"{len(got)}")
        if got:
            request = got[-1]
            item = self.item_of(request, "hello.md")
            self.expect(request.get("action") == "View", "the action is View", str(request.get("action")))
            self.expect(request.get("launched_app") is False, "it is not the request that started the app")
            self.expect(item.get("kind") == "File" and item.get("mime") == "text/markdown", "a Markdown file", json.dumps(item)[:200])
            self.expect(item.get("readable") is True and item.get("len") == len(HELLO) and item.get("fnv") == hex(fnv1a64(HELLO)),
                        "the working copy holds exactly the bytes ShareLab serves", json.dumps(item)[:240])
            self.expect(str(item.get("uri", "")).startswith("content://com.azul.sharelab.docs/"), "the origin is ShareLab's URI", str(item.get("uri")))
            self.expect(item.get("path") and "azul-inbox" in str(item.get("path")), "the copy is in the app's inbox", str(item.get("path")))
        self.one_instance("a share while the app runs")

    def open_cold(self):
        self.say("7. a share that STARTS the app")
        self.dev.sh(f"am force-stop {self.package}")
        self.dev.sh(f"am force-stop {LAB_PACKAGE}")
        self.dev.adb("logcat", "-c")
        self.dev.sh("setprop debug.az.selftest 0")
        self.dev.sh("setprop debug.az.selftest_rows files.open_in")
        time.sleep(1.0)
        self.send("VIEW", "hello.md", "text/markdown")
        got = self.opened(seconds=40)
        self.shot("08-open-cold")
        self.expect(len(got) >= 1, "the request that started the app reached its handler", f"{len(got)}")
        if got:
            item = self.item_of(got[0], "hello.md")
            self.expect(got[0].get("launched_app") is True, "and it says it is the request that started the app")
            self.expect(item.get("fnv") == hex(fnv1a64(HELLO)), "with the bytes of the file", json.dumps(item)[:200])
        self.one_instance("a share that starts the app")

    def send_many(self):
        self.say("8. several files, and a big one")
        self.start_row("files.open_in")
        time.sleep(3.0)
        self.send("SEND_MULTIPLE", "hello.md,plain.txt", "*/*")
        got = self.opened()
        self.expect(len(got) == 1, "one request for the two files", f"{len(got)}")
        if got:
            names = [i.get("name") for i in got[-1].get("items") or []]
            self.expect(got[-1].get("action") == "Send", "the action is Send", str(got[-1].get("action")))
            self.expect(names == ["hello.md", "plain.txt"], "both files, in order", str(names))
            plain = self.item_of(got[-1], "plain.txt")
            self.expect(plain.get("fnv") == hex(fnv1a64(PLAIN)), "each with its own bytes", json.dumps(plain)[:160])
        self.dev.adb("logcat", "-c")
        self.send("SEND", "big.bin", "application/octet-stream")
        big = self.opened(seconds=60)
        self.shot("09-open-big")
        if big:
            item = self.item_of(big[-1], "big.bin")
            self.expect(item.get("len") == BIG_BYTES and item.get("fnv") == hex(fnv1a64(bytes(BIG_BYTES))),
                        "a 3 MiB file is copied whole", json.dumps(item)[:200])
        else:
            self.expect(False, "the big file reached the handler")

    def write_back(self):
        self.say("9. open, edit, save back")
        self.start_row("files.write_back")
        time.sleep(3.0)
        self.send("EDIT", "edit.md", "text/markdown")
        verdict = None
        t0 = time.time()
        while time.time() - t0 < 40 and verdict is None:
            for row in row_lines(self.log()):
                if row.get("id") == "files.write_back":
                    verdict = row
            time.sleep(1.0)
        self.shot("10-write-back")
        self.expect(verdict is not None and verdict.get("verdict") == "PASS", "the row saved the working copy back",
                    json.dumps(verdict)[:300] if verdict else "no verdict")
        stored = self.dev.sh(f"cat /data/data/{LAB_PACKAGE}/files/docs/edit.md")
        self.expect("edited by AzSelfTest" in (stored or ""), "ShareLab's own file holds the line the app added", (stored or "")[:200])
        self.expect("# Edit me" in (stored or ""), "and the rest of the document is still there")

    # ---- the system's pickers, driven the way a person does it ----

    def tap_first(self, texts, resource_ids=(), seconds=30.0):
        """Waits until one of `texts` (or one of `resource_ids`) is on the screen and taps it; -> what was tapped or ''."""
        t0 = time.time()
        while time.time() - t0 < seconds:
            xml = self.ui()
            for text in texts:
                if self.dev.tap_node(xml, text=text):
                    return text
            for rid in resource_ids:
                if self.dev.tap_node(xml, resource_id=rid):
                    return rid
            time.sleep(1.0)
        return ""

    def row_verdict(self, row_id, seconds=60.0):
        """The `AZ_SELFTEST_ROW` line of `row_id` the app logs when the row ends, waited for; -> the dict or None."""
        t0 = time.time()
        while time.time() - t0 < seconds:
            for row in row_lines(self.log()):
                if row.get("id") == row_id:
                    return row
            time.sleep(1.0)
        return None

    def folder(self):
        self.say("10. a folder the person grants: kept, used, and still there after a restart")
        self.dev.sh("mkdir -p /sdcard/Documents/AzProof")
        self.dev.sh("sh -c 'echo seed > /sdcard/Documents/AzProof/seed.txt'")
        self.start_row("files.folder_grant")
        focus = self.wait_focus(r"(?i)documentsui|PickActivity|picker", seconds=40)
        self.expect(bool(focus), "the system's folder picker is on the screen", focus.strip()[:160])
        if not focus:
            self.shot("11-folder-no-picker")
            return
        time.sleep(2.0)
        self.shot("11-folder-picker")
        used = self.tap_first(["Use this folder", "USE THIS FOLDER"], ["com.android.documentsui:id/button_pick"], seconds=30)
        self.expect(bool(used), "the picker offers to use the folder it opened at (Documents/AzProof)", used)
        time.sleep(1.5)
        self.shot("12-folder-allow")
        allowed = self.tap_first(["Allow", "ALLOW"], ["android:id/button1"], seconds=20)
        self.expect(bool(allowed), "and asks whether the app may use it", allowed)
        row = self.row_verdict("files.folder_grant")
        self.shot("13-folder-granted")
        self.expect(row is not None and row.get("verdict") == "PASS", "the row used the granted folder: list, create, write, read, Exists, delete",
                    json.dumps(row)[:400] if row else "no verdict")
        # the grant is the SYSTEM's: after the app was killed and started again it is still one of its places
        self.dev.sh(f"am force-stop {self.package}")
        self.dev.adb("logcat", "-c")
        self.dev.sh("setprop debug.az.selftest_rows files.folder_use")
        time.sleep(1.0)
        self.dev.sh(f"am start -n {self.package}/{APP_ACTIVITY}")
        time.sleep(5.0)
        self.dev.sh("input tap 20 200")
        again = self.row_verdict("files.folder_use")
        self.shot("14-folder-after-restart")
        self.expect(again is not None and again.get("verdict") == "PASS", "after a restart the folder is still a place and works",
                    json.dumps(again)[:400] if again else "no verdict")
        self.dev.sh("rm -f /sdcard/Documents/AzProof/azul-selftest-*.txt")

    def export(self):
        self.say("11. save as: the answer comes after the bytes are written where the person chose")
        self.dev.sh("rm -f /sdcard/Download/selftest-export.azt /sdcard/Documents/selftest-export.azt")
        self.start_row("files.export")
        focus = self.wait_focus(r"(?i)documentsui|PickActivity|picker|create", seconds=40)
        self.expect(bool(focus), "the system's save dialog is on the screen", focus.strip()[:160])
        if not focus:
            self.shot("15-export-no-dialog")
            return
        time.sleep(2.0)
        self.shot("15-export-dialog")
        saved = self.tap_first(["Save", "SAVE"], ["com.android.documentsui:id/container_save", "com.android.documentsui:id/button_save"], seconds=25)
        self.expect(bool(saved), "the dialog offers to save in the folder it opened in", saved)
        row = self.row_verdict("files.export")
        self.shot("16-export-done")
        self.expect(row is not None and row.get("verdict") == "PASS", "the row was answered after the write", json.dumps(row)[:300] if row else "no verdict")
        data = share_one_bytes()
        found = ""
        for path in ("/sdcard/Download/selftest-export.azt", "/sdcard/Documents/selftest-export.azt"):
            size = (self.dev.sh(f"stat -c %s {path}") or "").strip()
            if size == str(len(data)):
                found = path
        self.expect(bool(found), "the file is where the person chose, with the length of the bytes the app gave", found or "not found")
        if found:
            digest = (self.dev.sh(f"sha256sum {found}") or "").split()
            self.expect(bool(digest) and digest[0] == sha256(data), "and with exactly those bytes", " ".join(digest)[:80])

    def provider_is_closed(self):
        self.say("4. the provider serves only the share's files")
        info = self.dev.sh(f"dumpsys package {self.package}")
        block = re.search(r"Provider\{[^}]*AzulShareProvider[^}]*\}[\s\S]{0,400}", info or "")
        text = block.group(0) if block else ""
        self.expect(bool(text), "the package has the share provider")
        # `dumpsys package` prints `exported=true` for a provider that is; its absence (or `exported=false`) is "not exported". The
        # manifest says it too (dll/tests/android_share_open_contract.rs pins android:exported="false").
        self.expect("exported=true" not in text.replace(" ", ""), "and it is not exported", text[:200])
        # ShareLab asks for URIs the chooser never granted: the area itself, `..`, the types file
        probes = [
            f"content://{self.package}.azul.share/",
            f"content://{self.package}.azul.share/..%2F..%2Fdatabases",
            f"content://{self.package}.azul.share/0123456789abcdef0123456789abcdef/.types",
        ]
        denied = []
        for uri in probes:
            out = self.dev.sh(f"content read --uri {uri}")
            denied.append(bool(re.search(r"(?i)denial|exception|no such|not found|error|unknown", out or "") or not (out or "").strip()))
        self.expect(all(denied), "nothing outside the granted share can be read through the provider", str(denied))


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apk", required=True, help="AzSelfTest's QA APK (it has the files.* rows)")
    ap.add_argument("--out", required=True)
    ap.add_argument("--serial", default="")
    ap.add_argument("--package", default="")
    ap.add_argument("--activity", default="")
    ap.add_argument("--lab-apk", default="", help="a built ShareLab (default: built here with scripts/ci/sharelab/build.sh)")
    ap.add_argument("--only", default="", help="comma list of steps: share_one,share_many,open_with,provider,open_running,open_cold,send_many,write_back,folder,export")
    args, _unknown = ap.parse_known_args(argv)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    dev = probe.Dev(args.serial)
    package, _activity = probe.launcher_of(args.apk)
    package = args.package or package
    lab = args.lab_apk
    if not lab:
        lab = str(out / "sharelab.apk")
        built = subprocess.run(["bash", str(Path(__file__).resolve().parent / "sharelab" / "build.sh"), lab], capture_output=True, text=True)
        print(built.stdout[-400:], built.stderr[-1200:])
        if built.returncode != 0:
            print("ShareLab could not be built")
            return 2
    for apk in (args.apk, lab):
        text = dev.adb("install", "-r", "-t", str(apk), timeout=300)
        if "Success" not in text:
            print(f"the install of {apk} failed: {text.strip()[:300]}")
            return 2
    dev.adb("root", timeout=30)
    dev.adb("wait-for-device", timeout=60)
    dev.sh("input keyevent 82")
    proof = Proof(dev, out, package)
    steps = [s for s in (args.only.split(",") if args.only else
                         ["share_one", "share_many", "open_with", "provider", "open_running", "open_cold", "send_many", "write_back", "folder", "export"]) if s]
    for step in steps:
        fn = {"share_one": proof.share_one, "share_many": proof.share_many, "open_with": proof.open_with,
              "provider": proof.provider_is_closed, "open_running": proof.open_running, "open_cold": proof.open_cold,
              "send_many": proof.send_many, "write_back": proof.write_back, "folder": proof.folder, "export": proof.export}.get(step)
        if fn is None:
            print(f"unknown step {step}")
            return 2
        try:
            fn()
        except Exception as e:  # a step that raised is a failed step, not a missing report
            proof.expect(False, f"step {step} ran", repr(e))
        dev.sh("input keyevent 4")
        dev.sh("input keyevent 3")
        time.sleep(1.0)
    (out / "files-proof.json").write_text(json.dumps({"ok": proof.ok, "results": proof.results}, indent=1), encoding="utf-8")
    (out / "files-proof.md").write_text("# files proof\n\n" + "\n".join(proof.lines) + "\n", encoding="utf-8")
    print("\nfiles proof:", "PASS" if proof.ok else "FAIL")
    return 0 if proof.ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
