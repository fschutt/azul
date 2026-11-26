#!/usr/bin/env python3
"""Runs an azul app N times with the startup timeline on and prints where the first milliseconds go.

    startup_measure.py --exe ./AzWidgets --runs 9 [--env KEY=VALUE ...] [--wrap xvfb-run -a] [--json out.json]

Each run sets AZ_STARTUP=1 and AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1 (the app prints its timeline at the first
present and exits), parses the `[startup-json]` line of its stderr (dll/src/desktop/startup_timeline.rs) and
measures the wall time from the spawn to the exit as seen from outside. The report says the machine's load and
the number of runs, gives the FIRST run (cold: the first start after a pause or an install) apart from the
median of the others (warm), and a table of the phases: for every mark the median time since the process start
and the median step since the previous mark of the same thread, the five costliest steps on top.

Std only (the runners have nothing else). Exit status 0 when at least one run printed a timeline."""
import argparse
import json
import os
import statistics
import subprocess
import sys
import time


def parse_timeline(text):
    """The JSON object of the `[startup-json]` line, or None."""
    for line in text.splitlines():
        marker = "[startup-json] "
        at = line.find(marker)
        if at >= 0:
            try:
                return json.loads(line[at + len(marker):])
            except json.JSONDecodeError:
                return None
    return None


def with_steps(marks):
    """[(name, thread, at_ms, step_ms)] in order; the step counts from the previous mark of the same thread."""
    last = {}
    out = []
    for m in marks:
        prev = last.get(m["thread"])
        step = None if prev is None else round(m["at_ms"] - prev, 3)
        last[m["thread"]] = m["at_ms"]
        out.append((m["name"], m["thread"], m["at_ms"], step, m.get("detail", "")))
    return out


def median(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def run_once(cmd, env, cwd, timeout):
    started = time.monotonic()
    try:
        proc = subprocess.run(cmd, env=env, cwd=cwd, capture_output=True, text=True, timeout=timeout)
        code = proc.returncode
        text = proc.stdout + "\n" + proc.stderr
    except subprocess.TimeoutExpired as err:
        code = "timeout"
        text = (err.stdout or b"").decode("utf-8", "replace") if isinstance(err.stdout, bytes) else (err.stdout or "")
        text += "\n" + ((err.stderr or b"").decode("utf-8", "replace") if isinstance(err.stderr, bytes) else (err.stderr or ""))
    wall_ms = (time.monotonic() - started) * 1000.0
    return code, wall_ms, text


def fmt(value, width=10, digits=3):
    return f"{'-':>{width}}" if value is None else f"{value:>{width}.{digits}f}"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", required=True, help="the app binary")
    ap.add_argument("--runs", type=int, default=9)
    ap.add_argument("--env", action="append", default=[], help="KEY=VALUE added to the environment (repeatable)")
    ap.add_argument("--cwd", default=None)
    ap.add_argument("--timeout", type=float, default=60.0)
    ap.add_argument("--label", default="")
    ap.add_argument("--json", default=None, help="write the per-run timelines and the summary here")
    ap.add_argument("--wrap", nargs=argparse.REMAINDER, default=[], help="a command that wraps the app (xvfb-run -a ...); must be the last option")
    ap.add_argument("--keep-going", action="store_true", help="do not stop at the first run without a timeline")
    args = ap.parse_args()

    env = dict(os.environ)
    env.update({"AZ_STARTUP": "1", "AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER": "1"})
    for pair in args.env:
        key, _, value = pair.partition("=")
        env[key] = value
    cmd = list(args.wrap) + [args.exe]

    load_before = os.getloadavg()
    runs = []
    for i in range(args.runs):
        code, wall_ms, text = run_once(cmd, env, args.cwd, args.timeout)
        timeline = parse_timeline(text)
        runs.append({"i": i + 1, "exit": code, "wall_ms": round(wall_ms, 1), "timeline": timeline})
        shown = "-" if not timeline else timeline.get("first_present_ms", "-")
        print(f"run {i + 1}/{args.runs}: exit {code}, wall {wall_ms:.0f} ms, first_present {shown} ms"
              + ("" if timeline else "  (NO timeline in the output)"), flush=True)
        if timeline is None and not args.keep_going and i == 0:
            print("--- the output of the first run, last 40 lines:")
            print("\n".join(text.splitlines()[-40:]))
            break
    load_after = os.getloadavg()

    good = [r for r in runs if r["timeline"] and "first_present_ms" in r["timeline"]]
    if not good:
        print("NO run printed a timeline with a first present")
        return 1

    firsts = [r["timeline"]["first_present_ms"] for r in good]
    warm = firsts[1:] if len(firsts) > 1 else []
    walls = [r["wall_ms"] for r in good]
    summary = {
        "label": args.label,
        "exe": args.exe,
        "runs": len(runs),
        "runs_with_timeline": len(good),
        "load_before": load_before[0],
        "load_after": load_after[0],
        "first_run_first_present_ms": firsts[0],
        "median_first_present_ms_all": median(firsts),
        "median_first_present_ms_warm": median(warm),
        "min_first_present_ms": min(firsts),
        "max_first_present_ms": max(firsts),
        "median_wall_ms": median(walls),
        "process_start_source": good[0]["timeline"].get("process_start_source"),
        "before_library_ms_median": median([r["timeline"].get("before_library_ms") for r in good]),
        "layout_passes_median": median([r["timeline"].get("layout_passes") for r in good]),
    }

    # The phases: the median over the runs of each mark (by name and occurrence).
    by_key = {}
    order = []
    for r in good:
        seen = {}
        for name, thread, at_ms, step, detail in with_steps(r["timeline"]["marks"]):
            n = seen.get(name, 0)
            seen[name] = n + 1
            key = (name, n)
            if key not in by_key:
                by_key[key] = {"thread": thread, "at": [], "step": [], "detail": detail}
                order.append(key)
            by_key[key]["at"].append(at_ms)
            by_key[key]["step"].append(step)
    rows = []
    for key in order:
        entry = by_key[key]
        rows.append({
            "mark": key[0] if key[1] == 0 else f"{key[0]} #{key[1] + 1}",
            "thread": entry["thread"],
            "at_ms": median(entry["at"]),
            "step_ms": median(entry["step"]),
            "runs": len(entry["at"]),
            "detail": entry["detail"],
        })

    print()
    print(f"# {args.label or os.path.basename(args.exe)}: {len(good)} of {len(runs)} runs with a timeline; load {load_before[0]:.2f} before, {load_after[0]:.2f} after")
    print(f"first run (cold)        first_present {summary['first_run_first_present_ms']:.1f} ms")
    if warm:
        print(f"median of the others    first_present {summary['median_first_present_ms_warm']:.1f} ms (min {min(warm):.1f}, max {max(warm):.1f}, n={len(warm)})")
    print(f"median wall (spawn..exit)               {summary['median_wall_ms']:.1f} ms")
    if summary["before_library_ms_median"] is not None:
        print(f"before the library (OS floor + main)    {summary['before_library_ms_median']:.1f} ms  [{summary['process_start_source']}]")
    print(f"layout passes begun (median)            {summary['layout_passes_median']}")

    costly = sorted((r for r in rows if r["step_ms"] is not None and r["thread"] in ("main", rows[0]["thread"])), key=lambda r: -r["step_ms"])[:5]
    print("\ntop five steps (main thread, median):")
    for r in costly:
        print(f"  {r['step_ms']:>9.3f} ms  {r['mark']}  {r['detail']}")

    print(f"\n{'mark':<46} {'thread':<20} {'at_ms':>10} {'step_ms':>10}  runs")
    for r in rows:
        print(f"{r['mark']:<46} {r['thread'][:20]:<20} {fmt(r['at_ms'])} {fmt(r['step_ms'])}  {r['runs']}  {r['detail'][:60]}")

    if args.json:
        with open(args.json, "w") as f:
            json.dump({"summary": summary, "rows": rows, "runs": runs}, f, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
