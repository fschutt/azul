#!/usr/bin/env python3
"""e2e_matrix.py: the same e2e scenario files on desktop, on the Android emulator and on the iOS Simulator, in one table (standard library only).

    e2e_matrix.py list   [--repo .]                                  the files, their tests, their host app
    e2e_matrix.py desktop --bin AzWidgets=target/debug/AzWidgets [--bin AzWriter=...] --out DIR [--repo .]
    e2e_matrix.py android --apk apks/AzWidgets-e2e-android-x86_64.apk [--apk ...] --out DIR [--serial S] [--repo .]
    e2e_matrix.py ios     --app AzWidgets-e2e-ios-sim.app.zip [--app ...] --udid UDID --out DIR [--repo .]
    e2e_matrix.py merge   DIR [DIR ...] --out DIR [--repo .]         one table from the results of the platforms

Every run writes DIR/e2e-<platform>.json (one record per file and test: {file, test, verdict, detail}) and the raw output of every
file under DIR/raw/<platform>/. `merge` reads those and writes DIR/e2e-matrix.md and e2e-matrix.json: rows = test, columns = desktop /
android / ios, a cell is PASS, SKIP (gate or not-applicable, with the reason), PENDING, XFAIL, FAIL (first failing step and message),
`n/a` (the file has no host app on that platform, with the reason) or `no verdict` (the app ended without saying).

HOW A FILE RUNS. The scenario executor is the engine's own (`azul_layout::e2e`); in an app it is switched on by AZ_E2E=<file>: the app
mounts its normal window, runs the file, prints the cargo-style report and EXITS (the exit is the completion signal). Android: the file
is pushed and its path set as the property `debug.az.e2e`; iOS: SIMCTL_CHILD_AZ_E2E; desktop: the environment variable, the headless
backend. The report is parsed from the app's own output (logcat / the console files / stdout), the same text on all three.

THE HOST APP of a file: the app whose DOM it asserts. examples/azul-widgets/e2e/*.json: AzWidgets. examples/<app folder>/e2e/*.json:
the package of that folder's Cargo.toml (AzClock, ...), which needs `e2e = true` in the manifest to have a QA build. A file that MOUNTS
its own DOM (an op `mount`) runs in AzWidgets everywhere. tests/e2e/azwriter_boot.json: AzWriter. The rest asserts the DOM of a desktop C example
(hello-world, the resume_* family, ...): it has no host on a phone, and the desktop language board runs it, not this matrix.
"""
import argparse
import json
import re
import subprocess
import sys
import time
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

ANSI = re.compile(r"\x1b\[[0-9;]*m")
VERDICT_LINE = re.compile(r"test (?P<name>.+?) \.\.\. (?P<verdict>PASS|FAIL|SKIP|PENDING|XFAIL|XPASS)\b(?P<rest>.*)$")
FAILURE_HEAD = re.compile(r"^---- (?P<name>.+?) \((?P<verdict>FAIL|XPASS)\) ----$")
FAILED_STEP = re.compile(r"^\s*step (?P<index>\d+): (?P<op>\S+) → FAILED: (?P<message>.*)$")
SUMMARY = re.compile(r"test result: .*?(?P<passed>\d+) passed; (?P<failed>\d+) failed")
PLATFORMS = ("desktop", "runner", "android", "ios")
PLATFORM_LABELS = {"desktop": "desktop (the app, headless backend)", "runner": "desktop (in-process runner)", "android": "android", "ios": "ios"}

# the name of the package in an app's Cargo.toml: the host of the scenario files in the app's own examples/<folder>/e2e/
PACKAGE_NAME = re.compile(r'\[package\][^\[]*?^name\s*=\s*"([^"]+)"', re.M | re.S)
# files that assert an app's own DOM and are not in examples/azul-widgets: stem -> the phone app that hosts them (the rest has none)
PHONE_HOST_BY_STEM = {"azwriter_boot": "AzWriter"}
NO_PHONE_HOST_REASON = "asserts the DOM of a desktop C example (tests/e2e/*.sh, the language board), which is not a phone app"
NO_DESKTOP_HOST_REASON = "run by the desktop language board against its C example, not by this matrix"
NO_DESKTOP_BUILD_REASON = ("needs lib.desktop-e2e: no desktop {host} of the desktop QA build was given "
                           "(scripts/ci/desktop_e2e_build.sh: the shipped libazul with the debug server and the e2e executor)")


# ---- discovery ------------------------------------------------------------------------------------------------------------------------

def load_tests(path):
    """The test objects of one scenario file (the file holds one object or a list)."""
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    return data if isinstance(data, list) else [data]


def discover(repo):
    """-> [{file, rel, tests: [{name, only_on, pending, not_applicable, expect, steps}], self_mounted, host}] sorted by path."""
    repo = Path(repo)
    files = sorted(set(repo.glob("tests/e2e/*.json")) | set(repo.glob("examples/*/e2e/*.json")))
    found = []
    for f in files:
        try:
            tests = load_tests(f)
        except (OSError, ValueError):
            continue
        rel = str(f.relative_to(repo))
        mounted = any(step.get("op") == "mount" for t in tests for step in t.get("steps", []))
        found.append({"file": str(f), "rel": rel, "self_mounted": mounted, "host": host_for(rel, mounted, repo),
                      "tests": [{"name": t.get("name", "?"), "only_on": t.get("only_on"), "pending": t.get("pending"),
                                 "not_applicable": t.get("not_applicable"), "expect": t.get("expect"), "steps": len(t.get("steps", []))} for t in tests]})
    return found


def host_for(rel, self_mounted, repo=None):
    """The phone app that hosts a file, or None when it has none. `repo`: the checkout, to read the package of an app's own folder."""
    if rel.startswith("examples/azul-widgets/"):
        return "AzWidgets"
    stem = Path(rel).stem
    if stem in PHONE_HOST_BY_STEM:
        return PHONE_HOST_BY_STEM[stem]
    parts = Path(rel).parts
    if repo is not None and len(parts) > 3 and parts[0] == "examples" and parts[2] == "e2e":
        # examples/<app folder>/e2e/*.json asserts the DOM of the app of that folder: the package its Cargo.toml names (AzClock, ...)
        try:
            named = PACKAGE_NAME.search((Path(repo) / "examples" / parts[1] / "Cargo.toml").read_text(encoding="utf-8"))
        except OSError:
            named = None
        if named:
            return named.group(1)
    return "AzWidgets" if self_mounted else None


# ---- the report an app prints -----------------------------------------------------------------------------------------------------------

def parse_report(text):
    """The cargo-style report of the engine's executor -> {test name: (verdict, detail)}. detail: the SKIP / PENDING reason, or the first
    failing step of a FAIL ('step 3: click -> FAILED: ...'). Tests the text does not mention are absent."""
    results, first_failure, current = {}, {}, None
    for raw in ANSI.sub("", text or "").splitlines():
        line = re.sub(r"^.*?RustStdoutStderr:\s*", "", raw).rstrip()
        m = VERDICT_LINE.search(line)
        if m:
            rest = m.group("rest").strip()
            reason = re.sub(r"^\(\d+ ms\)\s*", "", rest)
            reason = reason[1:-1] if reason.startswith("(") and reason.endswith(")") else reason
            results[m.group("name")] = (m.group("verdict"), reason)
            continue
        h = FAILURE_HEAD.match(line)
        if h:
            current = h.group("name")
            continue
        s = FAILED_STEP.match(line)
        if s and current and current not in first_failure:
            first_failure[current] = f"step {s.group('index')}: {s.group('op')} -> FAILED: {s.group('message')}"
    for name, (verdict, reason) in list(results.items()):
        if verdict in ("FAIL", "XPASS") and name in first_failure:
            results[name] = (verdict, first_failure[name])
    return results


def summary_counts(text):
    """(passed, failed) of the `test result:` line, or None."""
    m = None
    for m in SUMMARY.finditer(ANSI.sub("", text or "")):
        pass
    return (int(m.group("passed")), int(m.group("failed"))) if m else None


def records_for(scn, platform, report_text, started, why_no_verdict=""):
    """One record per test of the file from the text one run printed. A test the report does not name: `no verdict` with the reason."""
    parsed = parse_report(report_text)
    out = []
    for t in scn["tests"]:
        if t["name"] in parsed:
            verdict, detail = parsed[t["name"]]
        else:
            verdict, detail = "NO VERDICT", why_no_verdict or "the app ended without printing a verdict for this test"
        out.append({"file": scn["rel"], "test": t["name"], "platform": platform, "verdict": verdict, "detail": detail})
    return out


def not_applicable(scn, platform, reason):
    return [{"file": scn["rel"], "test": t["name"], "platform": platform, "verdict": "N/A", "detail": reason} for t in scn["tests"]]


# ---- the platform runs -----------------------------------------------------------------------------------------------------------------

def run_desktop(scns, bins, out, timeout=240, azul_doc=""):
    """The desktop run, two columns. `desktop`: the file runs in the host app's binary of the desktop QA build (the shipped libazul with the e2e
    executor, AZ_E2E=<file>, the headless backend) when one is given: the same path the phones take. `runner`: a file that MOUNTS its own DOM also
    runs in-process, headless, in `azul-doc e2e <file> --keep-going --jobs 1` (the CI's harness, `azul_layout::e2e` without a shell). A file
    that asserts the DOM of a desktop C example is run by neither, with the reason."""
    import os
    records = []
    raw_dir = Path(out) / "raw" / "desktop"
    raw_dir.mkdir(parents=True, exist_ok=True)
    for scn in scns:
        host = scn["host"]
        if scn["self_mounted"] and azul_doc:
            # the files that mount their own DOM also run in process, headless, in `azul-doc e2e` (the CI's harness: the `runner` column)
            started = time.time()
            try:
                done = subprocess.run([azul_doc, "e2e", scn["file"], "--keep-going", "--jobs", "1"], capture_output=True, text=True, timeout=timeout)   # one thread: no test of a file waits on another's process-wide state
                text, why = done.stdout + "\n" + done.stderr, f"azul-doc exited with status {done.returncode} without a verdict"
            except subprocess.TimeoutExpired as e:
                text, why = ((e.stdout or b"").decode("utf-8", "replace") if isinstance(e.stdout, bytes) else (e.stdout or "")), f"azul-doc ran longer than {timeout} s"
            (raw_dir / (Path(scn["rel"]).stem + ".runner.txt")).write_text(text, encoding="utf-8")
            records += records_for(scn, "runner", text, started, why)
        if host is None:
            records += not_applicable(scn, "desktop", NO_DESKTOP_HOST_REASON)
            continue
        if host not in bins:
            records += not_applicable(scn, "desktop", NO_DESKTOP_BUILD_REASON.format(host=host))
            continue
        env = dict(os.environ, AZ_E2E=str(Path(scn["file"]).resolve()), AZ_BACKEND="headless", AZ_LOG="off")
        started = time.time()
        try:
            done = subprocess.run([bins[host]], env=env, capture_output=True, text=True, timeout=timeout)
            text, why = done.stdout + "\n" + done.stderr, f"{host} exited with status {done.returncode} without a verdict"
        except subprocess.TimeoutExpired as e:
            text, why = ((e.stdout or b"").decode("utf-8", "replace") if isinstance(e.stdout, bytes) else (e.stdout or "")), f"{host} ran longer than {timeout} s"
        (raw_dir / (Path(scn["rel"]).stem + ".txt")).write_text(text, encoding="utf-8")
        records += records_for(scn, "desktop", text, started, why)
    return records


def apk_identity(apk):
    """(package, activity) of an APK from its manifest through aapt2 (the SDK's build-tools), or ('', '')."""
    import shutil
    aapt = shutil.which("aapt2") or next(iter(sorted(Path("/usr/local/lib/android/sdk/build-tools").glob("*/aapt2"))), None)
    if not aapt:
        return "", ""
    text = subprocess.run([str(aapt), "dump", "badging", str(apk)], capture_output=True, text=True).stdout
    pkg = re.search(r"package: name='([^']*)'", text)
    act = re.search(r"launchable-activity: name='([^']*)'", text)
    return (pkg.group(1) if pkg else ""), (act.group(1) if act else "")


def android_shot_dir(package, stem):
    """Where the engine writes the frames of one scenario file on an Android phone when they are to be pulled: the app's own cache folder (an app
    can always write there; the emulator's adbd runs as root and pulls from it). The path travels in the system property `debug.az.env`
    (`AZ_E2E_SHOT_DIR=<path>`), whose value holds 91 characters at most: the name of a file with a long name is cut to what fits (the frames are
    pulled to a folder of the real name right after the file's run)."""
    base = f"/data/user/0/{package}/cache/"
    room = 91 - len("AZ_E2E_SHOT_DIR=") - len(base)
    return base + stem[:max(room, 1)]


def run_android(scns, apks, out, serial="", timeout=240, shots=""):
    """The Android run: per host app its APK is installed once; per file the scenario is pushed, its path becomes `debug.az.e2e`, the app
    is started cold and the file's report is read from logcat when the process has exited. `shots`: a folder on this machine; the engine's
    frames of each file (AZ_E2E_SHOT_DIR, the app's own cache folder, which the emulator's root adbd reads) are pulled to <shots>/<file stem>/."""
    import mobile_qa as q
    run = q.Run(out, "android", {"label": "e2e matrix"})
    records = []
    raw_dir = Path(out) / "raw" / "android"
    raw_dir.mkdir(parents=True, exist_ok=True)
    by_host = {}
    for apk in apks:
        name = Path(apk).name.split("-")[0]               # AzWidgets-e2e-android-x86_64.apk -> AzWidgets
        by_host[name] = apk
    devices = {}
    for scn in scns:
        host = scn["host"]
        if host is None:
            records += not_applicable(scn, "android", NO_PHONE_HOST_REASON)
            continue
        if host not in by_host:
            records += not_applicable(scn, "android", f"no e2e APK of {host} was given")
            continue
        if host not in devices:
            pkg, act = apk_identity(by_host[host])
            dev = q.Android(run, pkg or f"com.azul.{host}", act, serial)
            install = dev.adb("install", "-r", "-t", str(by_host[host]), timeout=300)
            devices[host] = dev if "Success" in install else None
            if devices[host] is None:
                print(f"install of {host} failed: {install.strip()[-160:]}")
        dev = devices[host]
        if dev is None:
            records += not_applicable(scn, "android", f"the e2e APK of {host} did not install")
            continue
        remote = "/data/local/tmp/az_e2e.json"
        dev.adb("push", scn["file"], remote, timeout=60)
        dev.sh(f"setprop debug.az.e2e {remote}")
        # a relative output path of a scenario (`target/e2e/x.png`) cannot be written in an app's working directory: the window's e2e output folder
        # (AZ_E2E_SHOT_DIR) is where the engine falls back to, the app's own cache folder here
        stem = Path(scn["rel"]).stem
        shot_dev = android_shot_dir(dev.package, stem) if shots else ""
        dev.sh(f"setprop debug.az.env 'AZ_E2E_SHOT_DIR={shot_dev or '/data/user/0/' + dev.package + '/cache'}'")
        dev.sh(f"am force-stop {dev.package}")
        dev.adb("logcat", "-c")
        started = time.time()
        launched = dev.launch(cold=True)
        seen, ended = False, False
        while time.time() - started < timeout:
            alive = dev.alive()
            seen = seen or alive
            if seen and not alive:
                ended = True
                break
            if not seen and time.time() - started > 20:
                break
            time.sleep(1.0)
        text = dev.adb("logcat", "-d", "-v", "raw", "RustStdoutStderr:I", "*:S", timeout=120)
        crash = dev.adb("logcat", "-d", "-b", "crash", "-v", "threadtime", timeout=60)
        raw = text + ("\n== crash buffer\n" + crash if crash.strip() else "")
        if not seen:
            # an app that never showed up leaves no log of its own: what the system says about the start (the `am start` answer and the activity
            # manager's lines of the package: a crash loop that blocks the next start is written there) is what says why
            system = dev.adb("logcat", "-d", "-b", "all", "-v", "threadtime", timeout=60)
            lines = [l for l in system.splitlines() if dev.package in l or "AndroidRuntime" in l or "libc " in l or "DEBUG " in l]
            front = dev.sh("dumpsys window | grep -E 'mCurrentFocus|mFocusedApp'", timeout=60) + dev.sh("dumpsys activity activities | grep -E 'topResumedActivity|mResumedActivity'", timeout=60)
            raw += ("\n== am start\n" + str(launched).strip() + "\n== what has the screen\n" + str(front).strip()
                    + "\n== the system's lines of the package (the last 80)\n" + "\n".join(lines[-80:]) + "\n")
        (raw_dir / (Path(scn["rel"]).stem + ".txt")).write_text(raw, encoding="utf-8")
        dev.sh("am force-stop " + dev.package)
        if shot_dev:
            dest = Path(shots) / stem
            dest.mkdir(parents=True, exist_ok=True)
            dev.adb("root", timeout=60)        # the AOSP emulator images run adbd as root already; a user build says so here and the pull fails
            pulled = dev.adb("pull", shot_dev + "/.", str(dest), timeout=180)
            print(f"{stem}: {len(list(dest.glob('*.png')))} engine frames pulled ({pulled.strip().splitlines()[-1][:120] if pulled.strip() else 'no output'})")
        why = ("the app was still running after the timeout" if not ended and seen else
               "the app never started" if not seen else "the app exited without a verdict" + (" (crash buffer: " + crash.strip().splitlines()[0][:120] + ")" if crash.strip() else ""))
        records += records_for(scn, "android", text, started, why)
    for dev in devices.values():
        if dev is not None:
            dev.sh("setprop debug.az.e2e ''")
    return records


def run_ios(scns, app_zips, udid, out, timeout=240, shots=""):
    """The iOS run: per host app the Simulator app is installed once; per file SIMCTL_CHILD_AZ_E2E names it (the Simulator shares the host's
    files), the app is launched cold and the report is read from its console when the process has exited."""
    import plistlib
    import tempfile
    import mobile_qa as q
    run = q.Run(out, "ios", {"label": "e2e matrix"})
    records = []
    raw_dir = Path(out) / "raw" / "ios"
    raw_dir.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="e2e-matrix-"))
    by_host = {Path(z).name.split("-")[0]: z for z in app_zips}
    devices = {}
    for scn in scns:
        host = scn["host"]
        if host is None:
            records += not_applicable(scn, "ios", NO_PHONE_HOST_REASON)
            continue
        if host not in by_host:
            records += not_applicable(scn, "ios", f"no e2e Simulator app of {host} was given")
            continue
        if host not in devices:
            target = work / host
            target.mkdir(parents=True, exist_ok=True)
            with zipfile.ZipFile(by_host[host]) as z:
                z.extractall(target)
            app = next(iter(sorted(target.glob("*.app"))), None)
            info = plistlib.loads((app / "Info.plist").read_bytes()) if app else {}
            dev = q.IOS(run, udid, info.get("CFBundleIdentifier", f"com.azul.{host}"), info.get("CFBundleExecutable", host), None)
            rc, _o, err = dev.simctl("install", udid, str(app), timeout=240) if app else (1, "", "no .app in the zip")
            devices[host] = dev if rc == 0 else None
            if rc != 0:
                print(f"install of {host} failed: {err.strip()[:160]}")
        dev = devices[host]
        if dev is None:
            records += not_applicable(scn, "ios", f"the e2e Simulator app of {host} did not install")
            continue
        started = time.time()
        # /tmp: the Simulator's host folder; with `shots` one folder per file in it, the engine's frames of that file
        shot_dir = str(Path(shots).resolve() / Path(scn["rel"]).stem) if shots else "/tmp"
        dev.launch(extra_env={"AZ_E2E": str(Path(scn["file"]).resolve()), "AZ_E2E_SHOT_DIR": shot_dir}, cold=True)
        seen, ended = False, False
        while time.time() - started < timeout:
            alive = dev.alive()
            seen = seen or alive
            if seen and not alive:
                ended = True
                break
            if not seen and time.time() - started > 25:
                break
            time.sleep(1.0)
        text = dev.console_text()
        (raw_dir / (Path(scn["rel"]).stem + ".txt")).write_text(text, encoding="utf-8")
        why = ("the app was still running after the timeout" if not ended and seen else
               "the app never started" if not seen else "the app exited without a verdict")
        records += records_for(scn, "ios", text, started, why)
    return records


# ---- the table ---------------------------------------------------------------------------------------------------------------------------

def cell(record):
    """The text of one table cell."""
    if not record:
        return "not run"
    verdict, detail = record["verdict"], record.get("detail", "")
    detail = detail.replace("|", "/").replace("\n", " ")
    if verdict == "PASS":
        return "PASS"
    return f"{verdict}: {detail[:150]}" if detail else verdict


def merge(record_sets, scns):
    """{(file, test): {platform: record}} from the records of the platform runs."""
    table = {}
    for records in record_sets:
        for r in records:
            table.setdefault((r["file"], r["test"]), {})[r["platform"]] = r
    for scn in scns:                                   # every test of every file has a row, a platform nobody ran says `not run`
        for t in scn["tests"]:
            table.setdefault((scn["rel"], t["name"]), {})
    return table


def verdict_class(record):
    """The verdict as the table compares it: a failure of any kind is FAIL, a platform that did not run the test says nothing."""
    if not record or record["verdict"] in ("N/A", "not run"):
        return None
    return "FAIL" if record["verdict"] in ("FAIL", "NO VERDICT", "XPASS") else record["verdict"]


def render_markdown(table, scns):
    """The matrix as Markdown: one row per test with a `same` / `DIFFERENT` mark over the platforms that ran it, the list of the tests whose
    verdicts differ, then a totals table per platform."""
    order = [(s["rel"], t["name"]) for s in scns for t in s["tests"]]
    lines = ["# The same e2e files on desktop, Android and iOS", "", "| file | test | " + " | ".join(PLATFORM_LABELS[p] for p in PLATFORMS) + " | verdicts |",
             "|---|---|" + "---|" * (len(PLATFORMS) + 1)]
    counts = {p: {} for p in PLATFORMS}
    different = []
    for key in order:
        row = table.get(key, {})
        cells = []
        for p in PLATFORMS:
            r = row.get(p)
            cells.append(cell(r))
            verdict = r["verdict"] if r else "not run"
            counts[p][verdict] = counts[p].get(verdict, 0) + 1
        classes = {p: verdict_class(row.get(p)) for p in PLATFORMS}
        ran = {c for c in classes.values() if c}
        agree = len(ran) <= 1
        if not agree:
            different.append((key, row, classes))
        lines.append(f"| {key[0]} | {key[1][:60]} | " + " | ".join(cells) + f" | {'same' if agree else 'DIFFERENT'} |")
    lines += ["", "## Where the verdicts differ", ""]
    if not different:
        lines.append("- nowhere: every test that ran on more than one platform has the same verdict on all of them")
    for key, row, classes in different:
        parts = [f"{p} {row[p]['verdict']}" + (f" ({row[p].get('detail', '')[:110]})" if row[p]["verdict"] != "PASS" and row[p].get("detail") else "")
                 for p in PLATFORMS if classes[p]]
        lines.append(f"- {key[0]} :: {key[1][:70]}: " + ", ".join(parts))
    verdicts = sorted({v for c in counts.values() for v in c})
    lines += ["", "## Totals", "", "| platform | " + " | ".join(verdicts) + " |", "|---|" + "---|" * len(verdicts)]
    for p in PLATFORMS:
        lines.append(f"| {p} | " + " | ".join(str(counts[p].get(v, 0)) for v in verdicts) + " |")
    return "\n".join(lines) + "\n"


def phone_only_failures(table):
    """The rows that FAIL (or have no verdict) on a phone while desktop passes or skips: the ones that need a verdict (scenario fault or engine / shell)."""
    out = []
    for (file, test), row in sorted(table.items()):
        desktop = row.get("desktop", {}).get("verdict")
        for p in ("android", "ios"):
            r = row.get(p)
            if r and r["verdict"] in ("FAIL", "NO VERDICT", "XPASS") and desktop in (None, "PASS", "SKIP", "N/A", "PENDING", "XFAIL"):
                out.append((file, test, p, r["verdict"], r.get("detail", "")))
    return out


# ---- command line ----------------------------------------------------------------------------------------------------------------------

def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--repo", default=".")
    common.add_argument("--out", default="")
    common.add_argument("--only", default="", help="only files whose path contains this text")
    sub.add_parser("list", parents=[common])
    d = sub.add_parser("desktop", parents=[common])
    d.add_argument("--bin", action="append", default=[], help="Host=path of its e2e build")
    d.add_argument("--azul-doc", default="", help="the azul-doc binary: the files that mount their own DOM run in it, in process")
    a = sub.add_parser("android", parents=[common])
    a.add_argument("--apk", action="append", default=[])
    a.add_argument("--serial", default="")
    a.add_argument("--shots", default="", help="a folder: the engine's frames of each file are kept in <shots>/<file stem>/")
    i = sub.add_parser("ios", parents=[common])
    i.add_argument("--app", action="append", default=[])
    i.add_argument("--udid", default="")
    i.add_argument("--shots", default="", help="a folder: the engine's frames of each file are kept in <shots>/<file stem>/")
    m = sub.add_parser("merge", parents=[common])
    m.add_argument("dirs", nargs="+")
    args = ap.parse_args(argv)

    scns = [s for s in discover(args.repo) if args.only in s["rel"]]
    out = Path(args.out) if args.out else Path("demo-out/e2e-matrix")
    out.mkdir(parents=True, exist_ok=True)
    if args.cmd == "list":
        for s in scns:
            print(f"{s['rel']}: {len(s['tests'])} tests, host {s['host'] or '-'}, self-mounted {s['self_mounted']}")
        return 0
    if args.cmd == "merge":
        sets = []
        for d_ in args.dirs:
            for f in sorted(Path(d_).glob("e2e-*.json")):
                if f.name not in ("e2e-matrix.json",):
                    sets.append(json.loads(f.read_text(encoding="utf-8")))
        table = merge(sets, scns)
        (out / "e2e-matrix.md").write_text(render_markdown(table, scns), encoding="utf-8")
        (out / "e2e-matrix.json").write_text(json.dumps([{"file": k[0], "test": k[1], **{p: r for p, r in row.items()}} for k, row in table.items()], indent=1), encoding="utf-8")
        print(render_markdown(table, scns))
        for file, test, p, verdict, detail in phone_only_failures(table):
            print(f"PHONE-ONLY {verdict}: {p}: {file} :: {test[:60]}: {detail[:160]}")
        return 0
    if args.cmd == "desktop":
        bins = dict(b.split("=", 1) for b in args.bin)
        records = run_desktop(scns, bins, out, azul_doc=args.azul_doc)
    elif args.cmd == "android":
        records = run_android(scns, args.apk, out, args.serial, shots=args.shots)
    else:
        records = run_ios(scns, args.app, args.udid, out, shots=args.shots)
    (out / f"e2e-{args.cmd}.json").write_text(json.dumps(records, indent=1), encoding="utf-8")
    verdicts = {}
    for r in records:
        verdicts[r["verdict"]] = verdicts.get(r["verdict"], 0) + 1
    print(f"{args.cmd}: {len(records)} tests: " + ", ".join(f"{v} {n}" for v, n in sorted(verdicts.items())))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
