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
import threading
import time


def parse_timeline(text):
    """The headline object of the `[startup-json]` line with the array of the `[startup-marks]` line as its
    `marks`, or None."""
    headline = None
    marks = None
    for line in text.splitlines():
        for marker in ("[startup-json] ", "[startup-marks] "):
            at = line.find(marker)
            if at < 0:
                continue
            try:
                value = json.loads(line[at + len(marker):])
            except json.JSONDecodeError:
                continue
            if marker.startswith("[startup-json"):
                headline = value
            else:
                marks = value
    if headline is None:
        return None
    if marks is not None:
        headline["marks"] = marks
    headline.setdefault("marks", [])
    return headline


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


def timeline_complete(line):
    """True for the line that ends the app's timeline output: the `[startup-marks]` line, or (the older single line)
    a `[startup-json]` line that carries its marks."""
    return "[startup-marks] " in line or ("[startup-json] " in line and '"marks"' in line)


def run_once(cmd, env, cwd, timeout, grace=0.5):
    """Starts the app and reads its output (stdout and stderr in the order written). An app that exits by itself after
    its first frame (AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER) is waited for; one that does not (a shell that has no such
    exit) is ended `grace` seconds after its timeline was complete. Returns (exit code or "ended-after-timeline" or
    "timeout", wall ms from the spawn to the exit, or to the timeline's last line for an ended app, the output)."""
    started = time.monotonic()
    proc = subprocess.Popen(cmd, env=env, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, errors="replace")
    lines = []
    seen = {"complete_at": None, "exit_at": None, "code": None}

    def read():
        for line in proc.stdout:
            lines.append(line)
            if seen["complete_at"] is None and timeline_complete(line):
                seen["complete_at"] = time.monotonic()

    def wait():
        seen["code"] = proc.wait()
        seen["exit_at"] = time.monotonic()

    readers = [threading.Thread(target=read, daemon=True), threading.Thread(target=wait, daemon=True)]
    for t in readers:
        t.start()
    code = None
    while seen["exit_at"] is None:
        now = time.monotonic()
        if seen["complete_at"] is not None and now - seen["complete_at"] > grace:
            code = "ended-after-timeline"
            break
        if now - started > timeout:
            code = "timeout"
            break
        time.sleep(0.01)
    if code is not None:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    else:
        code = seen["code"]
    for t in readers:
        t.join(timeout=3)
    try:
        proc.stdout.close()
    except OSError:
        pass
    if seen["exit_at"] is not None and code not in ("ended-after-timeline", "timeout"):
        wall_ms = (seen["exit_at"] - started) * 1000.0
    elif seen["complete_at"] is not None:
        wall_ms = (seen["complete_at"] - started) * 1000.0
    else:
        wall_ms = (time.monotonic() - started) * 1000.0
    return code, wall_ms, "".join(lines)


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
    ap.add_argument("--expect-layouts", type=int, default=None,
                    help="fail (exit 2) unless every run began exactly this many layout passes before its first present")
    ap.add_argument("--max-first-present-ms", type=float, default=None,
                    help="fail (exit 2) when the median first present of the warm runs is above this")
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

    failed = False
    if args.expect_layouts is not None:
        counts = [r["timeline"].get("layout_passes") for r in good]
        wrong = [c for c in counts if c != args.expect_layouts]
        if wrong:
            print(f"GATE FAILED: layout passes before the first present: {counts}, expected {args.expect_layouts} in every run")
            failed = True
        else:
            print(f"gate ok: {args.expect_layouts} layout pass(es) before the first present in every run")
    if args.max_first_present_ms is not None:
        got = summary["median_first_present_ms_warm"] if warm else summary["median_first_present_ms_all"]
        if got is None or got > args.max_first_present_ms:
            print(f"GATE FAILED: median first present {got} ms is above {args.max_first_present_ms} ms")
            failed = True
        else:
            print(f"gate ok: median first present {got:.1f} ms <= {args.max_first_present_ms} ms")
    return 2 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
