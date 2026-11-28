#!/usr/bin/env python3
"""The font cache proof of the Android shell, on an emulator: miss, hit, a damaged manifest, hit again.

Run by scripts/ci/android_permission_proof.py on the emulator it boots (or alone through the QA's emulator tool:
`AZ_PROBE_PY=scripts/ci/android_font_cache_proof.py bash scripts/ci/android_probe.sh out <apk>`). It starts the app four times with the
engine's startup timeline on (`debug.az.startup 1`) and keeps the `fonts:` lines of each start:

  1. after `pm clear`:             `fonts:cache miss, 0 faces, ...` and later `fonts:cache_written Written, N faces, ...`
  2. a second cold start:          `fonts:cache hit, N faces, check X ms, load Y ms` and `fonts:cache_written Unchanged`; the files of
                                   the cache directory are listed before and after (a hit must not touch them)
  3. after a byte is appended to the manifest: `fonts:cache rebuilt (...)`
  4. a fourth cold start:          a hit again

The cache is `<app cache dir>/rfc/fonts/manifest.bin` (+ `manifest.key`); the app's cache directory is read as root (`adb root`, the AOSP
image allows it). The numbers (check / load ms, faces) go to <out>/font-cache-proof.md for the per-platform cost table.
"""
import re
import sys
import time
from pathlib import Path

FONT_LINE = re.compile(r"fonts:\S+.*")
# A mark made after the table was printed is one line `[startup] late: <mark> at <ms> ms (+<ms> ms, <thread>) <detail>`; in the table
# it is `<mark> <detail>` after the columns. The cache's write comes late whenever the scan outlasts the first frame.
LATE = r"(?: at [\d.]+ ms \([^)]*\))?"


def font_lines(log_text):
    """The `fonts:...` marks of a logcat (the engine's startup timeline entries about the font cache), from the mark's name on, in order.
    The one `[startup-json]` line that repeats the table is left out."""
    lines = []
    for raw in (log_text or "").splitlines():
        if "[startup-json]" in raw:
            continue
        found = FONT_LINE.search(raw)
        if found and "fonts:" in raw:
            lines.append(found.group(0).strip())
    return lines


def verdict_of(lines):
    """-> ('miss' | 'hit' | 'rebuilt' | None, the cache line): what the start's `fonts:cache ...` mark says."""
    for line in lines:
        match = re.match(r"fonts:cache" + LATE + r" (miss|hit|rebuilt)\b", line)
        if match:
            return match.group(1), line
    return None, ""


def numbers_of(line):
    """-> {'faces': n, 'check_ms': x, 'load_ms': y} for what the line states (absent ones are left out)."""
    out = {}
    for key, pattern in (("faces", r"(\d+) faces"), ("check_ms", r"check ([\d.]+) ms"), ("load_ms", r"load ([\d.]+) ms")):
        found = re.search(pattern, line)
        if found:
            out[key] = float(found.group(1)) if "." in found.group(1) else int(found.group(1))
    return out


def written_of(lines):
    """-> 'Written' | 'Unchanged' | ... : the word of the `fonts:cache_written` mark, or None."""
    for line in lines:
        found = re.match(r"fonts:cache_written" + LATE + r" (\w+)", line)
        if found:
            return found.group(1)
    return None


def run_font_scenarios(dev, package, activity, say, expect):
    """The four starts. `dev` is android_probe.Dev; `say(text)` prints a line, `expect(ok, what)` records an expectation.
    -> the list of (n, verdict, cache line, written word, numbers)."""
    cache = f"/data/data/{package}/cache/rfc/fonts"
    dev.adb("root", timeout=30)
    dev.adb("wait-for-device", timeout=60)
    dev.sh("setprop debug.az.startup 1")
    dev.sh("setprop debug.az.selftest 1")          # unattended: the app does its rows and exits, a start is short
    dev.sh("setprop debug.az.selftest_rows ''")
    results = []

    def start(n):
        dev.sh(f"am force-stop {package}")
        time.sleep(1.0)
        dev.adb("logcat", "-c")
        dev.sh(f"am start -n {package}/{activity}")
        time.sleep(20.0)   # the first frame, the font scan and the cache write (a late line)
        text = dev.adb("logcat", "-d", "-v", "threadtime", timeout=120)
        lines = font_lines(text)
        verdict, line = verdict_of(lines)
        written = written_of(lines)
        listing = dev.sh(f"stat -c '%n %s %Y' {cache}/* 2>&1")   # name, bytes, modification time (epoch s)
        say(f"  start {n}: {verdict or 'NO CACHE LINE'} | {line} | written: {written} | numbers {numbers_of(line)}")
        for l in lines:
            say(f"      {l}")
        say("      cache dir: " + " ; ".join(x.strip() for x in listing.splitlines() if x.strip())[:400])
        results.append((n, verdict, line, written, numbers_of(line), listing))
        return verdict, written, listing

    say("== font cache: miss, hit, damaged, hit ==")
    dev.sh(f"am force-stop {package}")
    dev.sh(f"pm clear {package}")
    v1, w1, _ = start(1)
    expect(v1 == "miss", "a fresh install misses the font cache")
    expect(w1 == "Written", "and writes it once the scan is done")
    listing1 = results[-1][5]
    v2, w2, listing2 = start(2)
    expect(v2 == "hit", "the second cold start HITS the font cache")
    expect(w2 == "Unchanged", "and writes nothing")
    expect(listing2 == listing1 and bool(listing2.strip()), "a hit leaves the files of the cache untouched (sizes and modification times)")
    dev.sh(f"sh -c 'echo x >> {cache}/manifest.bin'")
    v3, w3, listing3 = start(3)
    expect(v3 == "rebuilt", "a damaged manifest is rebuilt, not trusted")
    v4, w4, listing4 = start(4)
    expect(v4 == "hit", "and the start after the rebuild hits again")
    expect(w4 == "Unchanged", "without a write")
    expect(listing4 == listing3, "and leaves what the rebuild wrote untouched")
    dev.sh("setprop debug.az.selftest 0")
    return results


def main(argv):
    """Alone: boots nothing, runs against the connected emulator (the QA's tool calls this in place of android_probe.py)."""
    import argparse

    here = Path(__file__).resolve().parent
    sys.path.insert(0, str(here))
    import android_probe as probe

    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apk", default="")
    ap.add_argument("--package", default="")
    ap.add_argument("--activity", default="")
    ap.add_argument("--out", required=True)
    ap.add_argument("--serial", default="")
    args, _ = ap.parse_known_args(argv)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    dev = probe.Dev(args.serial)
    package, activity = args.package, args.activity
    if args.apk:
        if not package or not activity:
            pkg, act = probe.launcher_of(args.apk)
            package, activity = package or pkg, activity or act
        if "Success" not in dev.adb("install", "-r", "-t", str(args.apk), timeout=300):
            print("the install failed")
            return 2
    lines, ok = [], [True]

    def say(text):
        print(text, flush=True)
        lines.append(text)

    def expect(condition, what):
        say(f"  [{'ok' if condition else 'FAIL'}] {what}")
        ok[0] = ok[0] and bool(condition)

    run_font_scenarios(dev, package, activity, say, expect)
    (out / "font-cache-proof.md").write_text("# font cache proof\n\n" + "\n".join(lines) + "\n", encoding="utf-8")
    print("FONT CACHE PROOF: " + ("all held" if ok[0] else "SOMETHING DID NOT HOLD"))
    return 0 if ok[0] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
