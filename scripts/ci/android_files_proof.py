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
APP_ACTIVITY = "com.azul.app.AzulActivity"

# What the rows of AzSelfTest write: the script computes the same bytes (examples/azul-selftest/src/probes/files.rs).
def share_one_bytes():
    return b"AzSelfTest share probe\n" + bytes(range(256)) * 4


def share_many_bytes(which):
    return (b"# a\n" if which == "a" else b"# a again\n") + bytes(range(64))


def sha256(data):
    return hashlib.sha256(data).hexdigest()


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
    ap.add_argument("--only", default="", help="comma list of steps: share_one,share_many,open_with,provider")
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
    steps = [s for s in (args.only.split(",") if args.only else ["share_one", "share_many", "open_with", "provider"]) if s]
    for step in steps:
        fn = {"share_one": proof.share_one, "share_many": proof.share_many, "open_with": proof.open_with,
              "provider": proof.provider_is_closed}.get(step)
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
