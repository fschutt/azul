#!/usr/bin/env python3
"""Sample the CPU use of running processes: `ps -o %cpu=` once a second.

Used by `two-clients.mjs --cpu` while two AzMeet clients are in a call, and
usable on its own:

    python3 examples/azul-meet/scripts/cpu_sample.py --pid Ada=1234 --pid Ben=1235 \
        [--seconds 15] [--interval 1] [--json]

Prints one line per process with the mean and the max %CPU over the samples
(100 % = one core), and with --json one JSON object on the last line for
scripts. A process that exits during the sampling keeps the samples it had.
Standard library only.
"""

import argparse
import json
import subprocess
import sys
import time

def sample_cpu(pid):
    """%CPU of `pid` right now, or None when the process is gone."""
    try:
        out = subprocess.run(
            ["ps", "-o", "%cpu=", "-p", str(pid)],
            capture_output=True,
            text=True,
            timeout=5,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    text = out.stdout.strip()
    if out.returncode != 0 or not text:
        return None
    try:
        return float(text.replace(",", "."))
    except ValueError:
        return None

def parse_pid(spec):
    name, sep, pid = spec.partition("=")
    if not sep or not pid.strip().isdigit():
        raise argparse.ArgumentTypeError(f"expected NAME=PID, got {spec!r}")
    return name.strip() or pid.strip(), int(pid)

def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pid", action="append", type=parse_pid, required=True,
                        help="NAME=PID of a process to sample (repeatable)")
    parser.add_argument("--seconds", type=float, default=15.0,
                        help="how long to sample (default 15)")
    parser.add_argument("--interval", type=float, default=1.0,
                        help="seconds between samples (default 1)")
    parser.add_argument("--json", action="store_true",
                        help="also print the result as one JSON line")
    args = parser.parse_args()

    samples = {name: [] for name, _ in args.pid}
    deadline = time.monotonic() + args.seconds
    while True:
        started = time.monotonic()
        for name, pid in args.pid:
            value = sample_cpu(pid)
            if value is not None:
                samples[name].append(value)
        if started + args.interval > deadline:
            break
        time.sleep(max(0.0, args.interval - (time.monotonic() - started)))

    result = {}
    for name, pid in args.pid:
        values = samples[name]
        if values:
            mean = sum(values) / len(values)
            peak = max(values)
            print(f"{name} (pid {pid}): mean {mean:.1f} %CPU, max {peak:.1f} %CPU "
                  f"over {len(values)} samples")
            result[name] = {"pid": pid, "mean": round(mean, 2), "max": round(peak, 2),
                            "samples": len(values)}
        else:
            print(f"{name} (pid {pid}): no samples (the process is gone)")
            result[name] = {"pid": pid, "mean": None, "max": None, "samples": 0}
    if args.json:
        print(json.dumps(result))
    return 0 if all(r["samples"] for r in result.values()) else 1

if __name__ == "__main__":
    sys.exit(main())
