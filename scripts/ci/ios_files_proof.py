#!/usr/bin/env python3
"""ios_files_proof.py: the iOS Simulator proof of the share / open-with work (ledger WP-F), with pictures (standard library only).

    python3 scripts/ci/ios_files_proof.py --app AzSelfTest-e2e-ios-sim.app [--device "iPhone 16 Pro"] [--out ios-files]

ONE macOS job (scripts/ci/ios_files_job.sh builds the app first). What it shows, each with the console lines and the picture that
decided it, written to <out>/files-proof.md, files-proof.json and <out>/pictures/*.png:

  SHARE OUT    the self-test row `files.share_one` writes a file and calls `CallbackInfo::share`: the row's verdict (the report line)
               says the sheet was SHOWN, and the picture shows the system's share sheet (UIActivityViewController) with the file.
               A Simulator cannot be tapped from here, so what a person chooses in the sheet is not driven: the receiving side of a
               share is proven on Android (ShareLab); here the proof is that the sheet is up with the file.
  OPEN WITH    the row `files.open_with` (UIDocumentInteractionController's menu of the apps that can open the file): the verdict and
               the picture.
  LINKS        `simctl openurl azselftest://...` while the app runs, and as the call that STARTS the app: the app's open handler
               (`application:openURL:options:`) logs one `AZ_SELFTEST_OPEN` line per request; the cold start's request says
               `launched_app: true` (the link that started the app is held until the handler is installed, not lost).
  FILES        `simctl openurl file://...md` of a file whose type the app declares (best effort: a Simulator decides by itself which
               app opens a file; the outcome is recorded either way and is not a failure of the job).
  DECLARATIONS what the build wrote into the app's Info.plist (CFBundleDocumentTypes, CFBundleURLTypes, imported types, in-place
               opening): `plutil` of the bundle, checked against the self-test's `[[app.documents]]` of scripts/demo_apps.toml.

The exit status is 0 when every expectation held, 1 when one did not, 2 when the Simulator or the app could not be used.
"""
import argparse
import json
import os
import plistlib
import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ios_sim_selftest as sim  # noqa: E402

OPEN_LINE = "AZ_SELFTEST_OPEN "
REPORT_LINE = "AZ_SELFTEST_REPORT "
LINK = "azselftest://proof/hello?from=simctl"


def open_lines(text):
    """The JSON objects the app's open handler logged, in order."""
    found = []
    for line in (text or "").splitlines():
        at = line.find(OPEN_LINE)
        if at >= 0:
            try:
                found.append(json.loads(line[at + len(OPEN_LINE):]))
            except ValueError:
                found.append({"unparsable": line[at:at + 300]})
    return found


def report_row(text, row_id):
    """The row `row_id` of the last `AZ_SELFTEST_REPORT {json}` of the console: {id, verdict, detail, ...} or None."""
    for line in reversed((text or "").splitlines()):
        at = line.find(REPORT_LINE)
        if at < 0:
            continue
        try:
            report = json.loads(line[at + len(REPORT_LINE):])
        except ValueError:
            continue
        for row in report.get("rows", report.get("results", [])):
            if row.get("id") == row_id:
                return row
    return None


def declarations(app):
    """The document types and URL schemes of the built app's Info.plist: ({type names}, {schemes}, in place)."""
    try:
        info = plistlib.loads((Path(app) / "Info.plist").read_bytes())
    except (OSError, plistlib.InvalidFileException):
        return set(), set(), False
    names = {t.get("CFBundleTypeName") for t in info.get("CFBundleDocumentTypes", [])}
    schemes = {s for u in info.get("CFBundleURLTypes", []) for s in u.get("CFBundleURLSchemes", [])}
    return names, schemes, bool(info.get("LSSupportsOpeningDocumentsInPlace"))


class Proof:
    def __init__(self, udid, bundle, app, out):
        self.udid, self.bundle, self.app, self.out = udid, bundle, app, Path(out)
        (self.out / "pictures").mkdir(parents=True, exist_ok=True)
        self.lines, self.ok, self.results = [], True, []

    def say(self, text):
        print(text, flush=True)
        self.lines.append(text)

    def expect(self, condition, what, detail=""):
        self.say(f"  [{'ok' if condition else 'FAIL'}] {what}" + (f" ({detail})" if detail and not condition else ""))
        self.ok = self.ok and bool(condition)
        self.results.append({"expect": what, "ok": bool(condition), "detail": detail})

    def note(self, what, detail=""):
        self.say(f"  [info] {what}" + (f" ({detail})" if detail else ""))
        self.results.append({"info": what, "detail": detail})

    def shot(self, name):
        path = self.out / "pictures" / f"{name}.png"
        sim.screenshot(self.udid, path)
        return path

    def launch(self, name, env, seconds, open_url=None):
        """Start the app (the console in <name>.stdout.txt), wait `seconds`, return (console, pid)."""
        stdout, stderr = self.out / f"{name}.stdout.txt", self.out / f"{name}.stderr.txt"
        for f in (stdout, stderr):
            f.write_text("", encoding="utf-8")
        full = dict(os.environ, **{f"SIMCTL_CHILD_{k}": v for k, v in env.items()})
        ok, pid, said = sim.launch_app(self.udid, self.bundle, stdout, stderr, full)
        if not ok:
            self.expect(False, f"{name}: the app started", said[-200:])
            return "", None
        time.sleep(seconds)
        if open_url:
            sim.sh(["xcrun", "simctl", "openurl", self.udid, open_url], timeout=60)
            time.sleep(6)
        return stdout.read_text(encoding="utf-8", errors="replace") + "\n" + stderr.read_text(encoding="utf-8", errors="replace"), pid

    def container_log(self):
        """The open log (`azul-selftest-files/open.log`) in the app's data container."""
        code, container = sim.sh(["xcrun", "simctl", "get_app_container", self.udid, self.bundle, "data"], timeout=60)
        if code != 0:
            return ""
        path = Path(container.strip().splitlines()[-1]) / "azul-selftest-files" / "open.log"
        try:
            return path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            return ""

    def stop(self):
        sim.sh(["xcrun", "simctl", "terminate", self.udid, self.bundle], timeout=30)

    # ---- the steps ----

    def declarations_step(self):
        self.say("0. what the build declared (Info.plist of the app)")
        names, schemes, in_place = declarations(self.app)
        self.note("document types", ", ".join(sorted(n for n in names if n)))
        self.expect("azselftest" in schemes, "the app declares the URL scheme azselftest", str(sorted(schemes)))
        self.expect(len(names) >= 3, "the app declares its document types", str(sorted(n for n in names if n)))
        self.expect(in_place, "an editor type sets LSSupportsOpeningDocumentsInPlace")
        _, plist = sim.sh(["plutil", "-p", str(Path(self.app) / "Info.plist")], timeout=30)
        (self.out / "Info.plist.txt").write_text(plist, encoding="utf-8")

    def row_step(self, number, title, row, picture, expect_word):
        self.say(f"{number}. {title}")
        console, _pid = self.launch(f"row-{row.replace('.', '-')}", {"AZ_SELFTEST_ROWS": row, "AZ_LOG": "info"}, seconds=14)
        self.shot(picture)
        found = report_row(console, row)
        verdict = (found or {}).get("verdict") or (found or {}).get("code") or ""
        detail = (found or {}).get("detail", "")
        self.expect(found is not None, f"the report has the row {row}", console[-300:].replace("\n", " | ") if found is None else "")
        self.expect(str(verdict).upper().startswith("PASS"), f"{row} says the system's sheet was shown", f"{verdict}: {detail}")
        self.expect(expect_word in str(detail), f"the detail names what was shown ({expect_word!r})", str(detail))
        self.stop()

    def link_step(self):
        self.say("3. a link while the app runs, and as the call that starts it")
        # running: the app is up (an idle list, no row), the link arrives through application:openURL:options:
        console, _ = self.launch("link-running", {"AZ_LOG": "info"}, seconds=10, open_url=LINK)
        self.shot("03-link-running")
        got = open_lines(console)
        self.expect(len(got) >= 1, "the open handler heard the link of a running app", f"{len(got)} line(s)")
        if got:
            item = (got[-1].get("items") or [{}])[0]
            self.expect(item.get("kind") == "Url" and item.get("text") == LINK, "the item is the link, as sent", json.dumps(item)[:200])
            self.expect(got[-1].get("launched_app") is False, "a request of a running app is not the one that started it")
        self.stop()
        time.sleep(2)
        # cold: `simctl openurl` of a custom scheme starts the app; the request is held until the handler exists
        sim.sh(["xcrun", "simctl", "openurl", self.udid, LINK + "&cold=1"], timeout=60)
        time.sleep(14)
        self.shot("04-link-cold-start")
        # the app was started by the system, not by `simctl launch`: nobody captured its console. Its open handler also appends every
        # request to a file next to the files of the rows, in the app's container: that is what is read here
        text = self.container_log()
        (self.out / "cold.open.log").write_text(text, encoding="utf-8")
        cold = [json.loads(line) for line in text.splitlines() if line.strip().startswith("{")]
        cold = [c for c in cold if any(i.get("text", "").endswith("cold=1") for i in c.get("items", []))]
        self.expect(len(cold) >= 1, "the link that STARTED the app reached its open handler", f"{len(cold)} line(s)")
        if cold:
            self.expect(cold[-1].get("launched_app") is True, "and it says it is the request that started the app")
        self.stop()

    def file_step(self):
        self.say("5. a file of a declared type opened from outside (best effort on a Simulator)")
        sample = self.out / "proof-open.md"
        sample.write_bytes(b"# opened from outside\n\nA Markdown file the Simulator is asked to open.\n")
        console, _ = self.launch("file-running", {"AZ_LOG": "info"}, seconds=10, open_url=sample.resolve().as_uri())
        self.shot("05-file-open")
        got = open_lines(console)
        files = [i for r in got for i in (r.get("items") or []) if i.get("kind") == "File"]
        if files:
            self.expect(bool(files[0].get("readable")), "the file reached the app with a readable working copy", json.dumps(files[0])[:240])
        else:
            self.note("the Simulator did not hand the file to the app (it chose another app or none)", f"{len(got)} request(s) logged")
        self.stop()


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--app", required=True, help="AzSelfTest.app built for the Simulator (a folder)")
    ap.add_argument("--device", default="iPhone 16 Pro")
    ap.add_argument("--out", default="ios-files")
    args = ap.parse_args(argv)
    app = Path(args.app)
    bundle = sim.bundle_id_of(app)
    if not app.is_dir() or not bundle:
        print(f"{app} is not an app bundle with a CFBundleIdentifier")
        return 2
    code, listing = sim.sh(["xcrun", "simctl", "list", "devices", "available", "-j"], timeout=60)
    if code != 0:
        print(f"`xcrun simctl` does not run here: this needs a Mac with Xcode\n{listing}")
        return 2
    devices = sim.parse_devices(listing)
    _, types_text = sim.sh(["xcrun", "simctl", "list", "devicetypes", "-j"], timeout=60)
    _, runtimes_text = sim.sh(["xcrun", "simctl", "list", "runtimes", "-j"], timeout=60)
    try:
        types, runtimes = sim.parse_device_types(types_text), sim.parse_runtimes(runtimes_text)
    except (ValueError, KeyError):
        types, runtimes = [], []
    action, value, note = sim.resolve_device(args.device, [], types, runtimes)
    if action != "create":
        action, value, note = sim.resolve_device(args.device, devices, types, runtimes)
    if action == "create":
        name, type_id, runtime_id = value
        code, created = sim.sh(["xcrun", "simctl", "create", name, type_id, runtime_id], timeout=120)
        if code != 0:
            stand_in = sim.newest_plain_iphone(devices)
            value = stand_in
            note = f"could not make {name!r} ({created[-160:]}): ran on {stand_in['name'] if stand_in else 'nothing'} instead"
        else:
            value = sim.pick_device(sim.parse_devices(sim.sh(["xcrun", "simctl", "list", "devices", "available", "-j"], timeout=60)[1]),
                                    created.strip().splitlines()[-1])
    if value is None:
        print(note or "no Simulator")
        return 2
    udid = value["udid"]
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    print(f"device {value['name']} {udid} ({value['runtime']}); app {bundle}", flush=True)
    ok, why = sim.boot_device(udid)
    if not ok:
        print(why)
        return 2
    code, said = sim.sh(["xcrun", "simctl", "install", udid, str(app)], timeout=300)
    if code != 0:
        print(f"simctl install failed: {said[-300:]}")
        return 2
    proof = Proof(udid, bundle, app, out)
    try:
        for step in (
            proof.declarations_step,
            lambda: proof.row_step(1, "share out: one file through the system's share sheet", "files.share_one", "01-share-sheet", "one file"),
            lambda: proof.row_step(2, "open with: the system's menu of the apps that can open the file", "files.open_with", "02-open-with", "open with"),
            proof.link_step,
            proof.file_step,
        ):
            try:
                step()
            except Exception as e:  # a step that raised is a failed step, not a missing report
                proof.expect(False, "a step ran to its end", repr(e))
            sim.sh(["xcrun", "simctl", "terminate", udid, bundle], timeout=30)
    finally:
        sim.sh(["xcrun", "simctl", "shutdown", udid], timeout=120)
    (out / "files-proof.json").write_text(json.dumps({"ok": proof.ok, "device": value["name"], "results": proof.results}, indent=1), encoding="utf-8")
    (out / "files-proof.md").write_text("# iOS files proof (Simulator)\n\n" + "\n".join(proof.lines) + "\n", encoding="utf-8")
    print("\nios files proof:", "PASS" if proof.ok else "FAIL")
    return 0 if proof.ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
