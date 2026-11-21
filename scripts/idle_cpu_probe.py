#!/usr/bin/env python3
"""Idle-CPU probe for the azul example apps (IDLE_CPU, 2026-09-30).

Launches each app binary, lets it settle, samples its CPU usage with
`ps -o %cpu=` once a second, kills it and prints a table. Never touches the
mouse or the keyboard: an idle app must stay idle on its own.

    python3 scripts/idle_cpu_probe.py                 # the default app set
    python3 scripts/idle_cpu_probe.py target/release/AzWidgets --sample
    python3 scripts/idle_cpu_probe.py --warmup 8 --seconds 10 --sample

Targets after the IDLE_CPU work: every idle app < 0.1 %CPU, AzWidgets (three
visible spinners) well under 10 %CPU.

`--sample` (macOS) additionally runs `sample <pid> 3` and prints the 15 azul
frames of the MAIN thread with the most samples (inclusive counts), which is
where an idle app's cost shows up (display link ticks, timers, display-list
rebuilds, screenshot encodes).

Python 3 standard library only.
"""

import argparse
import os
import re
import signal
import subprocess
import sys
import time

DEFAULT_APPS = [
    "AzWidgets",
    "AzBuilder",
    "AzCalendar",
    "AzMaps",
    "AzPaint",
    "AzReview",
]


def parse_args(argv):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument(
        "binaries",
        nargs="*",
        help="app binaries (default: target/release/{%s})" % ",".join(DEFAULT_APPS),
    )
    p.add_argument("--warmup", type=float, default=8.0, help="seconds to settle (default 8)")
    p.add_argument("--seconds", type=int, default=10, help="samples, one per second (default 10)")
    p.add_argument(
        "--sample",
        action="store_true",
        help="also run macOS `sample` for 3 s and print the top 15 azul main-thread frames",
    )
    p.add_argument(
        "--lib-dir",
        default="target/azul-lib",
        help="DYLD_LIBRARY_PATH for the apps (default target/azul-lib)",
    )
    return p.parse_args(argv)


def cpu_percent(pid):
    """`ps -o %cpu=` for `pid`, or None once the process is gone."""
    try:
        out = subprocess.run(
            ["ps", "-o", "%cpu=", "-p", str(pid)],
            capture_output=True,
            text=True,
            check=False,
        ).stdout.strip()
    except OSError:
        return None
    if not out:
        return None
    try:
        return float(out.replace(",", "."))
    except ValueError:
        return None


# One call-graph line of `sample`: indentation / tree glyphs, the sample
# count, the symbol, then `(in <image>)`.
SAMPLE_LINE = re.compile(r"^[\s+!:|]*(\d+)\s+(.*?)\s+\(in ([^)]+)\)")


def main_thread_block(text):
    """The call-graph lines of the main thread in a `sample` report."""
    lines = text.splitlines()
    start = None
    for i, line in enumerate(lines):
        if "com.apple.main-thread" in line or re.search(r"Thread_\d+\s+Main Thread", line):
            start = i + 1
            break
    if start is None:
        return []
    block = []
    for line in lines[start:]:
        stripped = line.strip()
        # The next thread starts at the same (shallow) indentation with a
        # thread-count header; the report's summary sections are unindented.
        if re.match(r"^\s{0,4}\d+\s+Thread_", line) or (stripped and not line[:1].isspace()):
            break
        block.append(line)
    return block


def is_azul_frame(symbol, image):
    image_l = image.lower()
    return (
        "azul" in image_l
        or symbol.startswith("azul")
        or "azul_" in symbol
        or "::" in symbol and ("layout" in symbol or "shell2" in symbol)
    )


def top_azul_frames(text, limit=15):
    """The `limit` azul frames of the main thread with the most samples."""
    best = {}
    for line in main_thread_block(text):
        m = SAMPLE_LINE.match(line)
        if not m:
            continue
        count, symbol, image = int(m.group(1)), m.group(2).strip(), m.group(3).strip()
        if not is_azul_frame(symbol, image):
            continue
        # Inclusive counts: a frame that appears in several call paths keeps
        # its largest subtree (summing would double-count recursion).
        best[symbol] = max(best.get(symbol, 0), count)
    return sorted(best.items(), key=lambda kv: kv[1], reverse=True)[:limit]


def run_sample(pid):
    try:
        out = subprocess.run(
            ["sample", str(pid), "3", "-mayDie"],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, str(e)
    report = out.stdout
    # `sample` may write the report to a file and print its path instead.
    m = re.search(r"Sample analysis of process \d+ written to file (\S+)", out.stdout + out.stderr)
    if m and os.path.exists(m.group(1)):
        with open(m.group(1), encoding="utf-8", errors="replace") as f:
            report = f.read()
    return report, None


def stop(proc):
    if proc.poll() is not None:
        return
    try:
        proc.send_signal(signal.SIGTERM)
        proc.wait(timeout=3)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=3)
    except OSError:
        pass


def probe(binary, args):
    env = dict(os.environ)
    lib_dir = os.path.abspath(args.lib_dir)
    env["DYLD_LIBRARY_PATH"] = (
        lib_dir + (os.pathsep + env["DYLD_LIBRARY_PATH"] if env.get("DYLD_LIBRARY_PATH") else "")
    )
    try:
        proc = subprocess.Popen(
            [binary],
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except OSError as e:
        return {"app": os.path.basename(binary), "error": "launch failed: %s" % e}

    result = {"app": os.path.basename(binary)}
    try:
        time.sleep(args.warmup)
        if proc.poll() is not None:
            result["error"] = "exited during warm-up (code %s)" % proc.returncode
            return result
        samples = []
        for _ in range(args.seconds):
            value = cpu_percent(proc.pid)
            if value is None:
                break
            samples.append(value)
            time.sleep(1.0)
        if not samples:
            result["error"] = "no CPU samples (exited?)"
            return result
        result["mean"] = sum(samples) / len(samples)
        result["max"] = max(samples)
        result["samples"] = samples
        if args.sample:
            report, err = run_sample(proc.pid)
            if err:
                result["sample_error"] = err
            elif report:
                result["frames"] = top_azul_frames(report)
        return result
    finally:
        stop(proc)


def main(argv):
    args = parse_args(argv)
    binaries = args.binaries or [os.path.join("target", "release", a) for a in DEFAULT_APPS]
    results = []
    for binary in binaries:
        if not os.path.exists(binary):
            results.append({"app": os.path.basename(binary), "error": "not found: %s" % binary})
            continue
        print("probing %s ..." % binary, file=sys.stderr, flush=True)
        results.append(probe(binary, args))

    print()
    print("%-14s %10s %10s   %s" % ("app", "mean %CPU", "max %CPU", "note"))
    print("-" * 60)
    for r in results:
        if "error" in r:
            print("%-14s %10s %10s   %s" % (r["app"], "-", "-", r["error"]))
        else:
            note = "idle target < 0.1" if r["app"] != "AzWidgets" else "spinners: target < 10"
            print("%-14s %10.2f %10.2f   %s" % (r["app"], r["mean"], r["max"], note))

    for r in results:
        if r.get("frames") or r.get("sample_error"):
            print()
            print("== %s: top azul frames of the main thread (sample, 3 s) ==" % r["app"])
            if r.get("sample_error"):
                print("   sample failed: %s" % r["sample_error"])
            for symbol, count in r.get("frames", []):
                print("   %6d  %s" % (count, symbol))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
