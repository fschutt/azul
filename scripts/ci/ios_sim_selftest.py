#!/usr/bin/env python3
"""ios_sim_selftest.py: run AzSelfTest on an iOS Simulator, feed it what a Simulator can be fed and read what it found
(standard library only; ONE macOS job, announced to QA before it is dispatched).

    python3 scripts/ci/ios_sim_selftest.py --app AzSelfTest-e2e-ios-sim.app [--device "iPhone 16 Pro"] [--bundle <id>]
                                           [--touch] [--seconds 180] [--out ios-sim-selftest]

What it does, in this order (nothing is erased and nothing is uninstalled; only this app's privacy decisions are reset):
  1. finds the device (by name or UDID among the available ones), boots it when it is shut down;
  2. installs the app, resets ITS privacy decisions (`simctl privacy reset all <bundle>`), enrols Face ID / Touch ID, sets a
     location and the light appearance;
  3. RUN "fresh": launches the app unattended (`AZ_SELFTEST=1`) with `AZ_SELFTEST_FED=1`, which says that this script answers
     every `AZ_SELFTEST_AWAIT <key>` line the app prints while a row waits for a person (the biometric sheet: a face that
     matches). It saves two screenshots (a few seconds in, and at the end: a system permission sheet that nobody answered
     would be on them) and lists every console line that says the app asked for a permission: a fresh install must ask for
     NOTHING by itself, so that list must be empty;
  4. RUN "granted": `simctl privacy grant location`, a new location, the same launch: the geolocation row must now pass;
  5. RUN "revoked": `simctl privacy revoke location`: the same row must say DENIED, and nothing may fail;
  6. prints one table per run and the findings; the exit code is 1 when a row FAILED, when an expectation below is not met or
     when the app ended without a report (a crash: the last console lines are printed).

What a Simulator cannot do (it has no accelerometer, gyroscope, magnetometer, barometer, camera or Taptic Engine): those rows
must say UNAVAILABLE here, and are proven only by `scripts/ci/ios_phone_selftest.py` on a real iPhone. Nothing here pretends
otherwise: an expectation lists the verdicts a row may have on a Simulator, and a row outside the list is a finding.
"""
import argparse
import json
import os
import plistlib
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ios_sim_feed as feed  # noqa: E402
from ios_phone_selftest import parse_reports, rows_by_id  # noqa: E402

AWAIT = re.compile(r"AZ_SELFTEST_AWAIT\s+(\S+)")

# Console lines that say the app asked the system for a permission (the shell's own logs and the frameworks'): none may
# appear in a fresh unattended run.
ASKED = re.compile(
    r"(requesting [a-z ]+ access|OS prompt|requestWhenInUseAuthorization|requestAlwaysAuthorization|requestAuthorization|"
    r"requestAccessForMediaType|permission request)",
    re.IGNORECASE,
)

# What a row may say on a Simulator, by run. Rows not listed here are only held to "not FAIL". `*` = any run.
SIMULATOR_HAS_NO = ("PASS", "UNAVAILABLE")  # a sensor it does not have may say UNAVAILABLE (a PASS would be a surprise worth reading)
EXPECT = {
    "device.capabilities": {"*": {"PASS"}},
    "device.network": {"*": {"PASS"}},
    "security.keyring": {"*": {"PASS"}},
    "security.biometric_kind": {"*": {"PASS"}},
    "security.biometric_prompt": {"*": {"PASS"}},
    "input.clipboard": {"*": {"PASS"}},
    "system.permission_state": {"*": {"PASS"}},
    "system.settings": {"*": {"PASS"}},
    "sensors.accelerometer": {"*": set(SIMULATOR_HAS_NO)},
    "sensors.gyroscope": {"*": set(SIMULATOR_HAS_NO)},
    "sensors.magnetometer": {"*": set(SIMULATOR_HAS_NO)},
    "sensors.barometer": {"*": set(SIMULATOR_HAS_NO)},
    "sensors.geolocation": {
        "fresh": {"DENIED", "NEEDS-HUMAN"},
        "granted": {"PASS"},
        "revoked": {"DENIED"},
    },
}

# What the display-features row must say on a device, found by the start of its name (the cutout table's classes: an iPhone 16 Pro has
# one Dynamic Island pill, an iPhone SE has no cutout and a Touch ID button): substrings of the row's detail. A device in no entry is
# held to nothing, and a build without the row (it may be older than the row) is not blamed for it.
DISPLAY_EXPECT = {
    "iPhone 16 Pro": ("cutouts=1", "Pill", "fingerprint=None"),
    "iPhone SE": ("cutouts=0", "fingerprint=Front"),
}

# Where the Simulator is told it is (Tuebingen), and then somewhere else, so that a fix that did not follow is seen.
LOCATIONS = {"fresh": (48.5216, 9.0576), "granted": (48.5216, 9.0576), "revoked": (48.7758, 9.1829)}


def parse_devices(text):
    """`simctl list devices available -j` -> [{name, udid, state, runtime}] of the iOS devices, newest runtime first."""
    out = []
    for runtime, devices in json.loads(text).get("devices", {}).items():
        if "iOS" not in runtime:
            continue
        for d in devices:
            if d.get("isAvailable", True):
                out.append({"name": d.get("name", ""), "udid": d.get("udid", ""), "state": d.get("state", ""), "runtime": runtime})
    out.sort(key=lambda d: d["runtime"], reverse=True)
    return out


def pick_device(devices, wanted):
    """The device whose UDID is `wanted`, else the first (newest runtime, a booted one before a shut down one) whose name is."""
    for d in devices:
        if d["udid"] == wanted:
            return d
    named = [d for d in devices if d["name"] == wanted]
    named.sort(key=lambda d: d["state"] != "Booted")
    return named[0] if named else None


def parse_device_types(text):
    """`simctl list devicetypes -j` -> [{name, identifier}]."""
    return [{"name": d.get("name", ""), "identifier": d.get("identifier", "")} for d in json.loads(text).get("devicetypes", [])]


def parse_runtimes(text):
    """`simctl list runtimes -j` -> the available iOS runtimes [{identifier, version}], newest first."""
    out = []
    for r in json.loads(text).get("runtimes", []):
        if r.get("isAvailable", True) and str(r.get("name", r.get("identifier", ""))).startswith("iOS"):
            out.append({"identifier": r.get("identifier", ""), "version": str(r.get("version", "0"))})
    out.sort(key=lambda r: [int(x) if x.isdigit() else 0 for x in r["version"].split(".")], reverse=True)
    return out


def newest_plain_iphone(devices):
    """The device named "iPhone <number>" with the largest number (a booted one first among equals): the one every runner has."""
    plain = [d for d in devices if re.fullmatch(r"iPhone \d+", d["name"])]
    plain.sort(key=lambda d: (-int(d["name"].split()[1]), d["state"] != "Booted"))
    return plain[0] if plain else None


def resolve_device(wanted, devices, types, runtimes):
    """-> (action, value, note): ("use", device, "") the device exists; ("create", (name, type id, runtime id), note) the
    device TYPE exists on this runner and no device of it does (a Simulator is made from it; it stays there); ("use",
    device, note) neither exists: the newest plain iPhone stands in, and the note says so (the report never claims a
    model it did not run on); ("none", None, note) there is no iPhone at all."""
    found = pick_device(devices, wanted)
    if found is not None:
        return "use", found, ""
    kind = next((t for t in types if t["name"] == wanted), None)
    if kind is not None and runtimes:
        return "create", (f"AzSelfTest {wanted}", kind["identifier"], runtimes[0]["identifier"]), f"{wanted}: made from its device type"
    stand_in = newest_plain_iphone(devices)
    if stand_in is None:
        return "none", None, f"no iOS Simulator named {wanted!r} and no plain iPhone to stand in"
    return "use", stand_in, f"{wanted!r} is not on this machine (no device, no device type): ran on {stand_in['name']} instead"


def launch_env(fed=True, extra=None):
    """The environment of the app as `simctl launch` passes it: every variable prefixed SIMCTL_CHILD_."""
    env = {"AZ_SELFTEST": "1", "AZ_LOG": "info", "RUST_BACKTRACE": "1"}
    if fed:
        env["AZ_SELFTEST_FED"] = "1"
    env.update(extra or {})
    return {f"SIMCTL_CHILD_{k}": v for k, v in env.items()}


def new_awaits(text, seen):
    """The `AZ_SELFTEST_AWAIT <key>` lines of the console that were not answered yet, in order; `seen` counts per key."""
    found = AWAIT.findall(text)
    fresh = []
    for key in dict.fromkeys(found):
        total = found.count(key)
        fresh += [key] * (total - seen.get(key, 0))
        seen[key] = total
    return fresh


def feed_for(key, udid, touch=False):
    """The commands that answer an await key; None for a key this script does not know (the row then times out)."""
    if key == "biometric.match":
        return feed.match_commands(udid, True, touch=touch)
    if key == "biometric.nomatch":
        return feed.match_commands(udid, False, touch=touch)
    if key == "biometric.enroll":
        return feed.enrollment_commands(udid, True)
    if key == "biometric.unenroll":
        return feed.enrollment_commands(udid, False)
    if key.startswith("location."):
        try:
            lat, lon = key[len("location."):].split(",")
            return feed.location_commands(udid, lat, lon)
        except ValueError:
            return None
    return None


def asked_lines(text):
    """The console lines that say the app asked the system for a permission."""
    return [line.strip() for line in text.splitlines() if ASKED.search(line)]


def check_rows(report, run):
    """-> [str]: what is wrong in one run's report (a FAIL anywhere, a verdict outside the row's expectation)."""
    found = []
    rows = rows_by_id(report)
    for row in report.get("rows", []):
        if row.get("verdict") == "FAIL":
            found.append(f"[{run}] FAIL {row.get('id')}: {row.get('detail')}")
    for rid, by_run in EXPECT.items():
        allowed = by_run.get(run) or by_run.get("*")
        row = rows.get(rid)
        if allowed is None:
            continue
        if row is None:
            found.append(f"[{run}] MISSING {rid}: the app has no such row")
        elif row.get("verdict") not in allowed:
            found.append(f"[{run}] {rid} is {row.get('verdict')} ({row.get('detail')}); on a Simulator it may be {sorted(allowed)}")
    return found


def check_display(report, device_name):
    """-> [str]: what the display-features row says that is not what this device has (see DISPLAY_EXPECT)."""
    # a Simulator this script made from a device type is named "AzSelfTest <model>"
    model_name = device_name.removeprefix("AzSelfTest ")
    wanted = next((needles for model, needles in DISPLAY_EXPECT.items() if model_name.startswith(model)), None)
    row = rows_by_id(report).get("system.display_features")
    if wanted is None or row is None:
        return []
    if row.get("verdict") != "PASS":
        return [f"system.display_features is {row.get('verdict')} on {device_name} ({row.get('detail')}): the shell reports no display features"]
    missing = [needle for needle in wanted if needle not in row.get("detail", "")]
    return [f"system.display_features on {device_name} lacks {missing}: {row.get('detail')}"] if missing else []


def sh(argv, timeout=120, check=False):
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as e:
        return 127, f"{type(e).__name__}: {e}"
    if check and p.returncode != 0:
        raise RuntimeError(f"{' '.join(argv)} -> {p.returncode}: {(p.stdout + p.stderr).strip()[-400:]}")
    return p.returncode, (p.stdout + p.stderr).strip()


def bundle_id_of(app):
    try:
        with open(Path(app) / "Info.plist", "rb") as f:
            return plistlib.load(f).get("CFBundleIdentifier")
    except (OSError, plistlib.InvalidFileException):
        return None


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def screenshot(udid, path):
    sh(["xcrun", "simctl", "io", udid, "screenshot", str(path)], timeout=60)


def run_once(udid, bundle, name, out, seconds, touch):
    """One launch: the console, the report (or None), the lines that asked for a permission."""
    stdout = out / f"{name}.stdout.txt"
    stderr = out / f"{name}.stderr.txt"
    for f in (stdout, stderr):
        f.write_text("", encoding="utf-8")
    env = dict(os.environ, **launch_env())
    p = subprocess.run(
        ["xcrun", "simctl", "launch", "--terminate-running-process", f"--stdout={stdout}", f"--stderr={stderr}", udid, bundle],
        capture_output=True, text=True, timeout=60, env=env)
    launched = (p.stdout + p.stderr).strip()
    m = re.search(r":\s*(\d+)\s*$", launched)
    pid = int(m.group(1)) if m else None
    if p.returncode != 0:
        return {"console": launched, "report": None, "asked": [], "why": f"launch failed: {launched}"}
    seen, fed_keys, shot = {}, [], False
    t0 = time.time()
    text, report, why = "", None, ""
    while time.time() - t0 < seconds:
        time.sleep(0.5)
        text = stdout.read_text(encoding="utf-8", errors="replace") + "\n" + stderr.read_text(encoding="utf-8", errors="replace")
        if not shot and time.time() - t0 > 4:
            screenshot(udid, out / f"{name}-4s.png")
            shot = True
        for key in new_awaits(text, seen):
            time.sleep(2.0)  # the sheet must be up before the face is shown to it
            commands = feed_for(key, udid, touch)
            if commands is None:
                fed_keys.append(f"{key}: no feed for this key")
                continue
            ok, log = feed.run(commands)
            fed_keys.append(f"{key}: {'fed' if ok else 'FEED FAILED ' + str(log[-1][1:])}")
        reports = parse_reports(text)
        if reports:
            report = reports[-1]
            break
        if pid is not None and not alive(pid):
            why = "the app ended without a report (a crash?)"
            break
    if report is None and not why:
        why = f"no report in {seconds} s"
    screenshot(udid, out / f"{name}-end.png")
    (out / f"{name}.console.txt").write_text(text, encoding="utf-8")
    return {"console": text, "report": report, "asked": asked_lines(text), "why": why, "fed": fed_keys}


def table(report):
    width = max((len(r.get("id", "")) for r in report.get("rows", [])), default=10)
    return [f"  {r.get('verdict', '?'):<12} {r.get('id', ''):<{width}}  {r.get('detail', '')}" for r in report.get("rows", [])]


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--app", required=True, help="the AzSelfTest.app built for the Simulator (a folder)")
    ap.add_argument("--device", default="iPhone 16 Pro", help="a name or a UDID among the available Simulators")
    ap.add_argument("--bundle", help="the bundle identifier (default: the app's Info.plist)")
    ap.add_argument("--touch", action="store_true", help="the device has Touch ID (the notifications of a finger, not a face)")
    ap.add_argument("--seconds", type=int, default=180, help="how long one run may take")
    ap.add_argument("--out", default="ios-sim-selftest", help="where the files are written")
    args = ap.parse_args(argv)
    app = Path(args.app)
    if not app.is_dir():
        print(f"{app} is not an app bundle (a folder ending in .app)")
        return 2
    bundle = args.bundle or bundle_id_of(app)
    if not bundle:
        print(f"{app}/Info.plist names no CFBundleIdentifier: pass --bundle")
        return 2
    code, listing = sh(["xcrun", "simctl", "list", "devices", "available", "-j"], timeout=60)
    if code != 0:
        print(f"`xcrun simctl` does not run here: this needs a Mac with Xcode\n{listing}")
        return 2
    devices = parse_devices(listing)
    _, types_text = sh(["xcrun", "simctl", "list", "devicetypes", "-j"], timeout=60)
    _, runtimes_text = sh(["xcrun", "simctl", "list", "runtimes", "-j"], timeout=60)
    try:
        types, runtimes = parse_device_types(types_text), parse_runtimes(runtimes_text)
    except (ValueError, KeyError):
        types, runtimes = [], []
    action, value, note = resolve_device(args.device, devices, types, runtimes)
    if action == "create":
        name, type_id, runtime_id = value
        existing = pick_device(devices, name)
        if existing is None:
            code, created = sh(["xcrun", "simctl", "create", name, type_id, runtime_id], timeout=120)
            if code != 0:
                print(f"could not make the Simulator {name!r}: {created}")
                return 2
            devices = parse_devices(sh(["xcrun", "simctl", "list", "devices", "available", "-j"], timeout=60)[1])
            existing = pick_device(devices, created.strip().splitlines()[-1])
        value = existing
    if value is None:
        print(note)
        return 2
    device = value
    udid = device["udid"]
    if note:
        print(f"note: {note}", flush=True)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    print(f"device {device['name']} {udid} ({device['runtime']}, {device['state']}); app {bundle}", flush=True)
    if device["state"] != "Booted":
        sh(["xcrun", "simctl", "boot", udid], check=True)
        sh(["xcrun", "simctl", "bootstatus", udid, "-b"], timeout=300)
    sh(["xcrun", "simctl", "install", udid, str(app)], timeout=300, check=True)
    findings, summary = [], []
    plan = [
        ("fresh", [feed.privacy_commands(udid, "reset", "all", bundle), feed.enrollment_commands(udid, True),
                   feed.appearance_commands(udid, "light")]),
        ("granted", [feed.privacy_commands(udid, "grant", "location", bundle)]),
        ("revoked", [feed.privacy_commands(udid, "revoke", "location", bundle)]),
    ]
    for name, setup in plan:
        for commands in setup + [feed.location_commands(udid, *LOCATIONS[name])]:
            ok, log = feed.run(commands)
            if not ok:
                findings.append(f"[{name}] setup failed: {log[-1][0]} -> {log[-1][1]}: {log[-1][2]}")
        print(f"\n================ run {name} ================", flush=True)
        result = run_once(udid, bundle, name, out, args.seconds, args.touch)
        for line in result.get("fed", []):
            print(f"  fed {line}")
        if result["report"] is None:
            findings.append(f"[{name}] {result['why']}: {' | '.join(result['console'].splitlines()[-6:])}")
            continue
        print("\n".join(table(result["report"])))
        (out / f"{name}.report.json").write_text(json.dumps(result["report"], indent=2), encoding="utf-8")
        findings += check_rows(result["report"], name)
        findings += [f"[{name}] {f}" for f in check_display(result["report"], device["name"])]
        if name == "fresh" and result["asked"]:
            findings.append("[fresh] the app asked the system for a permission by itself on a fresh install: "
                            + " | ".join(result["asked"][:4]))
        summary.append(f"{name}: {len(result['report'].get('rows', []))} rows")
    print("\n================ summary ================")
    if note:
        summary.append(f"device: {note}")
    print("; ".join(summary))
    if findings:
        print("\nFINDINGS:\n  " + "\n  ".join(findings))
    else:
        print("no findings")
    (out / "summary.txt").write_text("\n".join(summary + findings) + "\n", encoding="utf-8")
    print(f"files: {out}/ (console, report and screenshot of every run)")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
