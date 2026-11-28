#!/usr/bin/env python3
"""android_azmail.py: AzMail on an Android emulator, the way the user asked to see it ("build AzMail for mobile, how that looks?"):

    android_azmail.py --out DIR --apk AzMail-android-x86_64.apk [--data azmail-data-parent-folder] [--tag portrait]

1. installs the APK and starts it (a cold start); the first picture, light and dark, and the screen's accessibility dump (uiautomator);
2. with --data (the output of scripts/ci/azmail_sample_data.py: the folder `azmail-data/`, the files `--sample` writes on a desktop) the account
   folder is put, as root, where the first start said its mail folder is (the line `[azmail] N account(s) in <folder>` of its log), owned by the app,
   and the app is started again: a phone app has no command line, hence no `--sample`, and it reads AZMAIL_DATA with std::env::var, which a phone's
   `debug.az.env` does not reach (the engine's environment reader answers from it, the process environment does not hold it);
3. best effort, each step a picture and a dump whatever happens: a tap on the first message of the list (the reading pane), a tap on the
   `New E-mail` command (compose; no BACK in between: it leaves the app on a phone). A step whose label the dump does not show says so; nothing aborts the rest.

Everything lands in DIR: shots/NN-<name>.png (the device's own pixels), ui/<name>.xml (the dumps), azmail.log (the app's lines of logcat), steps.json
(what each step did and found). Exit 0 whenever the app ran; 1 when the APK did not install. Nothing here judges the app: the pictures are the answer."""
import argparse
import json
import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mobile_qa as q  # noqa: E402

# the sample messages' subjects (examples/azul-mail/scripts/sample_mail/INBOX), most telling first
SUBJECTS = ("Garden Weekly", "Re: Garden plan", "Seed invoice", "Grüße aus München")
NEW_MAIL = ("New E-mail", "New Email", "New Message")


def find_label(xml_text, wanted):
    """(label, x, y) of the first node of a uiautomator dump whose text or content description holds one of `wanted` (the exact label first,
    then a substring), or None. The centre is in device pixels."""
    nodes = q.parse_bounds(xml_text)
    for want in wanted:
        for label, x, y in nodes:
            if label == want and x > 0 and y > 0:
                return label, x, y
    for want in wanted:
        for label, x, y in nodes:
            if want.lower() in label.lower() and x > 0 and y > 0:
                return label, x, y
    return None


def package_and_activity(apk):
    """(package, launchable activity) of an APK from its manifest (aapt2 of the SDK's build-tools), or ('', '')."""
    import shutil
    import subprocess
    aapt = shutil.which("aapt2") or next(iter(sorted(Path("/usr/local/lib/android/sdk/build-tools").glob("*/aapt2"))), None)
    if not aapt:
        return "", ""
    text = subprocess.run([str(aapt), "dump", "badging", str(apk)], capture_output=True, text=True).stdout
    pkg = re.search(r"package: name='([^']*)'", text)
    act = re.search(r"launchable-activity: name='([^']*)'", text)
    return (pkg.group(1) if pkg else ""), (act.group(1) if act else "")


def mail_folder_of(log_text, package):
    """The AzMail folder the app said it uses: the last `[azmail] N account(s) in <folder>` line of its log, else the app's files directory + AzMail."""
    found = re.findall(r"\[azmail\] \d+ account\(s\) in (\S+)", log_text)
    return found[-1] if found else f"/data/user/0/{package}/files/AzMail"


def seed(dev, data_dir, folder):
    """Puts the account folders of `data_dir` (azmail-data/<address>/) into the AzMail folder `folder` of the app and makes them the app's own. The
    emulator's adbd runs as root (AOSP images). -> (folder, note)"""
    pkg = dev.package
    dev.adb("root", timeout=60)
    time.sleep(3.0)
    dev.adb("wait-for-device", timeout=60)
    uid = dev.sh(f"stat -c %u /data/user/0/{pkg}").strip()
    if not uid.isdigit():
        return None, f"no data directory for {pkg} (stat said {uid!r})"
    dev.sh(f"mkdir -p {folder}")
    notes = []
    for account in sorted(p for p in Path(data_dir).iterdir() if p.is_dir()):
        pushed = dev.adb("push", str(account), folder + "/", timeout=180)
        notes.append(pushed.strip().splitlines()[-1][:100] if pushed.strip() else "no output")
    dev.sh(f"chown -R {uid}:{uid} {folder}")
    dev.sh(f"restorecon -R {folder}")
    listing = dev.sh(f"ls {folder}").strip()
    return folder, f"pushed ({'; '.join(notes)}); {folder} holds: {listing[:80]}"


def dump_ui(dev, out_dir, name):
    """uiautomator's dump of the screen -> ui/<name>.xml; returns its text."""
    dev.sh("uiautomator dump /sdcard/azmail-ui.xml", timeout=60)
    xml = dev.sh("cat /sdcard/azmail-ui.xml", timeout=30)
    ui = Path(out_dir) / "ui"
    ui.mkdir(parents=True, exist_ok=True)
    (ui / f"{name}.xml").write_text(xml, encoding="utf-8")
    return xml


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--apk", required=True)
    ap.add_argument("--data", default="", help="the folder azmail-data produced by azmail_sample_data.py")
    ap.add_argument("--tag", default="", help="a word that goes into the picture names (portrait, landscape)")
    ap.add_argument("--wait", type=float, default=35.0, help="seconds to wait after a start before the first picture")
    ap.add_argument("--serial", default="")
    args = ap.parse_args(argv)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    run = q.Run(args.out, "android", {"label": "AzMail"})
    pkg, act = package_and_activity(args.apk)
    pkg = pkg or "com.azul.AzMail"
    dev = q.Android(run, pkg, act, args.serial)
    steps = []
    shots = out / "shots"
    shots.mkdir(parents=True, exist_ok=True)
    counter = {"n": 0}

    def shot(name):
        """The device's own picture (adb screencap -p) -> shots/NN-<name>.png; a failed capture is a note, never a crash. -> the file name or ''."""
        counter["n"] += 1
        fname = f"{counter['n']:02d}-{name}.png"
        data = dev.adb("exec-out", "screencap", "-p", binary=True, timeout=60)
        if data[:4] != b"\x89PNG":
            step(f"picture-{name}", False, f"screencap gave {len(data)} bytes, not a PNG")
            return ""
        (shots / fname).write_bytes(data)
        return fname

    def step(name, ok, note):
        steps.append({"step": name, "ok": ok, "note": note})
        print(f"[{'ok' if ok else '--'}] {name}: {note}")

    installed = dev.adb("install", "-r", "-t", str(args.apk), timeout=300)
    if "Success" not in installed:
        step("install", False, installed.strip()[-200:])
        (out / "steps.json").write_text(json.dumps(steps, indent=1), encoding="utf-8")
        return 1
    step("install", True, f"{pkg} / {act or '?'}")
    dev.sh("setprop debug.az.e2e ''")
    tag = f"-{args.tag}" if args.tag else ""

    def start(note):
        dev.sh(f"am force-stop {pkg}")
        dev.adb("logcat", "-c")
        dev.launch(cold=True)
        time.sleep(args.wait)
        alive = dev.sh(f"pidof {pkg}").strip() != ""
        step(note, alive, "started (the process is alive)" if alive else "the process is not alive")

    def app_log():
        return dev.adb("logcat", "-d", "-v", "threadtime", "RustStdoutStderr:I", "AndroidRuntime:E", "DEBUG:I", "*:S", timeout=120)

    # 1. the start with no data: what a person sees first
    start("start-empty")
    first_log = app_log()
    fname = shot(f"azmail-first{tag}")
    step("picture-first", bool(fname), fname or "no picture")
    xml = dump_ui(dev, out, f"first{tag}")
    step("dump-first", bool(xml), f"{len(xml)} bytes, {len(q.parse_bounds(xml))} labelled nodes")
    dev.sh("cmd uimode night yes")
    time.sleep(8.0)
    shot(f"azmail-first-dark{tag}")
    dev.sh("cmd uimode night no")
    time.sleep(4.0)

    # 2. the mailbox: the sample account's files, AZMAIL_DATA
    if args.data:
        folder = mail_folder_of(first_log, pkg)
        folder, note = seed(dev, args.data, folder)
        step("seed", bool(folder), note or "")
        if folder:
            start("start-with-the-mailbox")
            shot(f"azmail-list{tag}")
            xml = dump_ui(dev, out, f"list{tag}")
            labels = [l for l, _x, _y in q.parse_bounds(xml)]
            step("dump-list", bool(xml), f"{len(labels)} labelled nodes: " + " | ".join(labels[:14])[:300])
            hit = find_label(xml, SUBJECTS)
            if hit:
                dev.tap(hit[1], hit[2])
                time.sleep(8.0)
                shot(f"azmail-reading{tag}")
                dump_ui(dev, out, f"reading{tag}")
                step("open-a-message", True, f"tapped {hit[0]!r} at {hit[1]},{hit[2]}")
                # no BACK here: on a phone the reading pane is not a screen of its own, BACK left the app (the first run's compose step then saw the launcher)
            else:
                step("open-a-message", False, "none of the sample subjects is in the dump (the mailbox did not load, or the list shows other labels)")
            xml = dump_ui(dev, out, f"before-compose{tag}")
            hit = find_label(xml, NEW_MAIL)
            if hit:
                dev.tap(hit[1], hit[2])
                time.sleep(8.0)
                shot(f"azmail-compose{tag}")
                dump_ui(dev, out, f"compose{tag}")
                step("compose", True, f"tapped {hit[0]!r} at {hit[1]},{hit[2]}")
            else:
                step("compose", False, "no New E-mail command in the dump")
            dev.sh("cmd uimode night yes")
            time.sleep(8.0)
            shot(f"azmail-dark{tag}")
            dev.sh("cmd uimode night no")
    log = app_log()
    (out / "azmail-first-start.log").write_text(first_log, encoding="utf-8")
    (out / "azmail.log").write_text(log, encoding="utf-8")
    step("log", True, f"{len(log.splitlines())} lines; [azmail] lines: " + " | ".join(l.split("]", 1)[-1].strip()[:90] for l in log.splitlines() if "[azmail]" in l)[:400])
    dev.sh(f"am force-stop {pkg}")
    (out / "steps.json").write_text(json.dumps(steps, indent=1), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
