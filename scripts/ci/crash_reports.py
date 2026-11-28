#!/usr/bin/env python3
"""crash_reports.py: which of the reports the host wrote during a boot test are the APP's.

The iOS Simulator runs on the Mac that hosts it, so the reports of ITS system daemons land in the same ~/Library/Logs/DiagnosticReports as
the reports of the app under test. "A file newer than the marker" is therefore not "the app crashed": run 46118639960 of 2026-11-28 failed
AzMail, alive twice, on the screen and drawn, on
    STExtractionService_2026-11-28-073050_sjc20-bb660-5c7bb040-b3a6-467e-b17b-9319b7a1c21e-C6D928107DAB.diag
which is the disk-writes report of the Simulator's Screen Time extraction daemon (Command: STExtractionService, Identifier:
com.apple.STExtractionService), written while the app was starting.

A report is the APP's when
  1. the process name its file name starts with is the app's executable or a helper of it: the executable followed by a separator
     (`AzMail-2026-11-28-073050.ips`, `AzMail_2026-11-28-073050_host.diag`, `AzMail Helper-2026-11-28-073050.ips`; `AzMailer-...` is
     another process), or
  2. the process lines at the top of the file name the app: the FIRST `Process:`, `Command:`, `Identifier:` and `Path:` line of a text
     report (a dump that lists several processes is about the first), the header line of an .ips ("app_name", "bundleID", "name")
     and its "procName", "procPath" and bundleInfo's "CFBundleIdentifier": the executable, a helper of it, `<executable>.app/` in a
     path, or the app's bundle id. Other lines of a report ("On Behalf Of", a frame, a thread or an image name) can name the app
     without being its process and are not read.
Every other report is a SYSTEM report: copied to crash-system/ and named in an information line, never part of the verdict. The app's own
reports of every kind (a crash, a hang, a resource event of its process) stay a verdict: copied to crash/.

    crash_reports.py --exe AzMail --id com.azul.AzMail --since <marker file> --out <dir> [--wait 20] [--shell] <reports dir> [...]

--since is a file (the boot test touches one before it launches the app): the reports whose modification time is newer than it count,
looked for in the folders below the given ones as well (macOS moves old reports to Retired/). --wait N waits up to N seconds for the
app's report to appear (a crash report is written a few seconds after the crash), polling every 2 s; reports of other processes never end
the wait. --shell prints the answer as shell assignments for `eval`: crash_app (the first report of the app, empty for none), crash_other
(the names of the system reports, separated by ", ") and crash_extra (the JSON {"system_reports": [...]} for smoke_verdict.py --extra).
The information lines go to stderr. Copies only: nothing is moved or deleted.
"""
import argparse
import json
import os
import re
import shlex
import shutil
import sys
import time
from pathlib import Path

HEAD_BYTES = 16384
SEPARATORS = " -_."
STAMP = re.compile(r"^(?P<name>.+?)[-_ ]\d{4}-\d{2}-\d{2}-\d{6}")
LINE = re.compile(r"^\s*(?P<key>Process|Command|Identifier|Path):\s*(?P<value>.+?)\s*$")
HEADER_FIELD = re.compile(r'"(?P<key>app_name|bundleID|name)"\s*:\s*"(?P<value>[^"]*)"')
BODY_FIELD = re.compile(r'"(?P<key>procName|procPath)"\s*:\s*"(?P<value>[^"]*)"')
BUNDLE_INFO = re.compile(r'"bundleInfo"\s*:\s*\{[^}]*?"CFBundleIdentifier"\s*:\s*"(?P<value>[^"]*)"')
PID_SUFFIX = re.compile(r"\s*\[\d+\]$")


def process_name(file_name):
    """The process a report's file name starts with: what comes before the date stamp (`AzMail-2026-11-28-073050.ips`,
    `STExtractionService_2026-11-28-073050_host.diag`); a name without a stamp: up to the first dot. A leading dot is the report being written
    (the host writes `.<name>` first and renames it: run 46123227484 saw `.STExtractionService_2026-11-28-075645_...diag`): it names the same process."""
    file_name = file_name.lstrip(".")
    m = STAMP.match(file_name)
    return m.group("name") if m else file_name.split(".")[0]


def is_the_app_or_a_helper(name, exe):
    """`name` is the executable or starts with it and a separator (a helper process of the app); an empty executable names nothing."""
    if not exe or not name:
        return False
    return name == exe or (name.startswith(exe) and name[len(exe)] in SEPARATORS)


def head_facts(head):
    """[(key, value)] of the process lines at the top of a report's text: the first of each key of a text report, the header line and the
    process fields of an .ips (its other "name" fields are threads and images)."""
    pairs, seen = [], set()
    lines = head.splitlines()
    for line in lines:
        m = LINE.match(line)
        if m and m.group("key") not in seen:
            seen.add(m.group("key"))
            pairs.append((m.group("key"), m.group("value")))
    if lines and lines[0].lstrip().startswith("{"):
        pairs.extend((m.group("key"), m.group("value")) for m in HEADER_FIELD.finditer(lines[0]))
        pairs.extend((m.group("key"), m.group("value")) for m in BODY_FIELD.finditer(head))
        m = BUNDLE_INFO.search(head)
        if m:
            pairs.append(("CFBundleIdentifier", m.group("value")))
    return pairs


def head_names_the_app(path, exe, bundle_id):
    """The process lines at the top of the report name the app (the second way of the module docstring)."""
    try:
        with open(path, "rb") as f:
            head = f.read(HEAD_BYTES).decode("utf-8", "replace")
    except OSError:
        return False
    for key, value in head_facts(head):
        value = PID_SUFFIX.sub("", value)
        if key in ("Identifier", "bundleID", "CFBundleIdentifier"):
            if bundle_id and value.casefold() == bundle_id.casefold():
                return True
        elif key in ("Path", "procPath"):
            if exe and f"/{exe}.app/" in value:
                return True
        elif is_the_app_or_a_helper(value, exe):
            return True
    return False


def is_the_apps(path, exe, bundle_id):
    path = Path(path)
    return is_the_app_or_a_helper(process_name(path.name), exe) or head_names_the_app(path, exe, bundle_id)


def reports_since(directories, marker):
    """The files in `directories` (and below them) modified after `marker` (a file), oldest name first; like `find -newer`."""
    since = os.stat(marker).st_mtime
    found = []
    for directory in directories:
        for root, _dirs, files in os.walk(directory):
            for name in files:
                path = Path(root) / name
                try:
                    if path.stat().st_mtime > since:
                        found.append(path)
                except OSError:
                    pass
    return sorted(found, key=lambda p: (p.name, str(p)))


def classify(directories, marker, exe, bundle_id):
    """-> (the app's reports, the system reports), both as lists of paths."""
    mine, other = [], []
    for path in reports_since(directories, marker):
        (mine if is_the_apps(path, exe, bundle_id) else other).append(path)
    return mine, other


def wait_for_the_apps(directories, marker, exe, bundle_id, seconds, sleep=time.sleep, clock=time.monotonic, step=2.0):
    """Poll until a report of the APP is there or `seconds` have passed; reports of other processes do not end the wait."""
    end = clock() + seconds
    while True:
        mine, _ = classify(directories, marker, exe, bundle_id)
        if mine or clock() >= end:
            return
        sleep(step)


def collect(directories, marker, exe, bundle_id, out):
    """Copy the app's reports to <out>/crash/ and the system reports to <out>/crash-system/ (copies, with their times; one that vanished
    meanwhile, macOS retires old reports, is still named). -> (names of the app's, names of the system reports), each name once."""
    mine, other = classify(directories, marker, exe, bundle_id)
    for paths, folder in ((mine, "crash"), (other, "crash-system")):
        if paths:
            (Path(out) / folder).mkdir(parents=True, exist_ok=True)
        for path in paths:
            try:
                shutil.copy2(path, Path(out) / folder / path.name)
            except OSError as e:
                print(f"{path}: not copied ({e})", file=sys.stderr)
    return list(dict.fromkeys(p.name for p in mine)), list(dict.fromkeys(p.name for p in other))


def shell_assignments(mine, other):
    extra = json.dumps({"system_reports": other}) if other else "{}"
    return "\n".join([f"crash_app={shlex.quote(mine[0] if mine else '')}",
                      f"crash_other={shlex.quote(', '.join(other))}",
                      f"crash_extra={shlex.quote(extra)}"])


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", required=True, help="the app's executable (CFBundleExecutable)")
    ap.add_argument("--id", default="", help="the app's bundle id")
    ap.add_argument("--since", required=True, help="a file; the reports newer than it count")
    ap.add_argument("--out", required=True, help="the folder that gets crash/ and crash-system/")
    ap.add_argument("--wait", type=float, default=0.0, help="seconds to wait for a report of the app")
    ap.add_argument("--shell", action="store_true", help="print crash_app, crash_other and crash_extra as shell assignments")
    ap.add_argument("directories", nargs="+")
    a = ap.parse_args(argv)
    if not os.path.exists(a.since):
        ap.error(f"--since: no file {a.since}")
    if a.wait > 0:
        wait_for_the_apps(a.directories, a.since, a.exe, a.id, a.wait)
    mine, other = collect(a.directories, a.since, a.exe, a.id, a.out)
    print("reports of the app: " + (", ".join(mine) if mine else "none"), file=sys.stderr)
    if other:
        print("reports of other processes while the app ran (system daemons of the Simulator, not the app's, not part of the verdict): "
              + ", ".join(other), file=sys.stderr)
    if a.shell:
        print(shell_assignments(mine, other))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
