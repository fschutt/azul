#!/usr/bin/env python3
"""font_wait_series.py: cold starts of one app, build A against build B, with and without the font cache (the startup agent's E73 lever: the first layout no longer
waits for the scan of the system's fonts when the faces registered in memory draw every character of the DOM). Standard library only.

    font_wait_series.py android --out DIR --package P --activity A --starts 10 --plan "control:A=a1.apk;control:B=b1.apk;bundled:A=a2.apk;bundled:B=b2.apk"
    font_wait_series.py ios     --out DIR --udid U --bundle B        --starts 10 --plan "control:A=a1.app;control:B=b1.app;..."

The plan names `<app variant>:<build>=<apk or .app>`. For each app variant, in the order of its entries (A first, then B), one boot / one Simulator measures
  MISS  `--starts` starts, each from an EMPTY font cache (Android `pm clear` before the start, the Simulator: uninstall and install again before the start: no file is deleted by hand),
  HIT   one start to fill the cache, then `--starts` starts that find it (the process is stopped between starts, nothing else is touched),
so that the blocks read: control A miss, A hit, B miss, B hit; bundled A miss, A hit, B miss, B hit. A warm-up start that is not counted precedes every MISS block (the first start after an
install also pays for the page cache). Every start runs with the engine's startup timeline on (Android `setprop debug.az.startup 1`, the Simulator SIMCTL_CHILD_AZ_STARTUP=1) and is read
from the `[startup-json]` and `[startup-marks]` lines it printed at its first present. Per start: first_present_ms, the cache mark (`fonts:cache miss|hit ...`), the first layout's wait mark
(`fonts:first_layout_wait skipped: ...|waited N ms`; none on a build without the lever), the first layout's total ms, `fonts:cache_written`. Reported per block: n, median, min, max of
first_present_ms, how many starts skipped / waited, in font-wait.md and font-wait.json; the lines of every start are kept in starts/. A shared runner's numbers are indicative: the blocks of
one job share one machine, read the medians next to each other, not the milliseconds.
"""
import argparse
import json
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import startup_measure as sm  # noqa: E402

WAIT_MARK = "fonts:first_layout_wait"
CACHE_MARK = "fonts:cache"
WRITTEN_MARK = "fonts:cache_written"
KEPT = re.compile(r"\[startup[\]-]|fonts:|first_present|layout_")


def mark_detail(timeline, name):
    """The detail of the first mark called `name` (`fonts:cache` -> 'miss, 0 faces, check 0.4 ms, load 0.0 ms'), or None."""
    for m in (timeline or {}).get("marks") or []:
        if m.get("name") == name:
            return str(m.get("detail") or "")
    return None


def table_detail(text, name):
    """The same from the table lines of the text (`[startup]  11  542.135  0.896  main  fonts:cache miss, 0 faces ...`), for a log that lost the JSON marks."""
    m = re.search(r"\[startup\]\s+\d+\s+[-\d.]+\s+[-\d.]+\s+\S+\s+" + re.escape(name) + r"(?: ([^\n]*))?$", text or "", re.M)
    return (m.group(1) or "").strip() if m else None


def layout_total_ms(timeline, text):
    """The first layout's own total (`layout_phases [phases] total 337.0ms ...`), from the marks or from the table."""
    detail = mark_detail(timeline, "layout_phases") or table_detail(text, "layout_phases")
    m = re.search(r"total ([\d.]+)ms", detail or "")
    return float(m.group(1)) if m else None


def wait_kind(detail):
    """'skipped' | 'waited' | 'none' (no such mark: a build without the lever) | 'other'."""
    if detail is None:
        return "none"
    word = detail.strip().lower()
    if word.startswith("skipped"):
        return "skipped"
    if word.startswith("waited"):
        return "waited"
    return "other"


def parse_start(text):
    """One start's printed lines -> {first_present_ms, cache, cache_kind, wait, wait_kind, wait_ms, layout_ms, written} or None when the start printed no timeline."""
    timeline = sm.parse_timeline(text or "")
    if not timeline or not isinstance(timeline.get("first_present_ms"), (int, float)):
        return None
    cache = mark_detail(timeline, CACHE_MARK)
    if cache is None:
        cache = table_detail(text, CACHE_MARK)
    wait = mark_detail(timeline, WAIT_MARK)
    if wait is None:
        wait = table_detail(text, WAIT_MARK)
    written = mark_detail(timeline, WRITTEN_MARK)
    if written is None:
        written = table_detail(text, WRITTEN_MARK)
    waited_ms = re.search(r"waited ([\d.]+)\s*ms", wait or "")
    cache_word = (cache or "").strip().split(",")[0].split(" ")[0].lower()
    return {"first_present_ms": float(timeline["first_present_ms"]), "cache": cache, "cache_kind": cache_word or None, "wait": wait, "wait_kind": wait_kind(wait),
            "wait_ms": float(waited_ms.group(1)) if waited_ms else None, "layout_ms": layout_total_ms(timeline, text), "written": written}


def summarize(rows):
    """Per block: n, median / min / max of first_present_ms, the wait marks counted, the cache marks counted, the median first layout."""
    got = [r for r in rows if r]
    values = sorted(r["first_present_ms"] for r in got)
    layouts = sorted(r["layout_ms"] for r in got if r.get("layout_ms") is not None)

    def count(key):
        out = {}
        for r in got:
            out[r.get(key)] = out.get(r.get(key), 0) + 1
        return out

    return {"n": len(got), "asked": len(rows),
            "median": round(statistics.median(values), 1) if values else None, "min": round(values[0], 1) if values else None, "max": round(values[-1], 1) if values else None,
            "layout_median": round(statistics.median(layouts), 1) if layouts else None, "waits": count("wait_kind"), "caches": count("cache_kind")}


def parse_plan(text):
    """'control:A=a.apk;control:B=b.apk;bundled:A=c.apk' -> [(variant, build, path)] in the order given."""
    out = []
    for part in (text or "").split(";"):
        part = part.strip()
        if not part:
            continue
        left, _, path = part.partition("=")
        variant, _, build = left.partition(":")
        if not (variant and build and path):
            raise ValueError(f"plan entry '{part}': want <variant>:<build>=<path>")
        out.append((variant.strip(), build.strip(), path.strip()))
    return out


def blocks_of(plan):
    """[(variant, build, path, mode)]: per variant, per build in order: miss, then hit."""
    variants = list(dict.fromkeys(v for v, _b, _p in plan))
    out = []
    for variant in variants:
        for v, build, path in plan:
            if v == variant:
                out += [(variant, build, path, "miss"), (variant, build, path, "hit")]
    return out


def report_markdown(results, label=""):
    """results: [{variant, build, mode, summary, starts: [row]}] -> the markdown."""
    lines = [f"# Font wait, A against B{(' (' + label + ')') if label else ''}", "",
             "MISS = every start from an empty font cache; HIT = the cache filled by a start before. first_present_ms is the engine's own clock from the process start.", "",
             "| app | build | cache | n | median first_present ms | min | max | first layout median ms | first-layout wait (skipped / waited / no mark) |", "|---|---|---|---|---|---|---|---|---|"]
    for r in results:
        s = r["summary"]
        w = s["waits"]
        lines.append(f"| {r['variant']} | {r['build']} | {r['mode']} | {s['n']} of {s['asked']} | {s['median']} | {s['min']} | {s['max']} | {s['layout_median']} | "
                     f"{w.get('skipped', 0)} / {w.get('waited', 0)} / {w.get('none', 0)} |")
    lines += ["", "## Every start", ""]
    for r in results:
        lines += [f"### {r['variant']} {r['build']} {r['mode']}", "", "| # | first_present ms | cache mark | first-layout wait mark | first layout ms | cache written |", "|---|---|---|---|---|---|"]
        for i, row in enumerate(r["starts"], 1):
            if not row:
                lines.append(f"| {i} | no timeline | | | | |")
            else:
                lines.append(f"| {i} | {row['first_present_ms']:.1f} | {row['cache'] or ''} | {(row['wait'] or 'none')[:90]} | {row['layout_ms'] if row['layout_ms'] is not None else ''} | {(row['written'] or '')[:40]} |")
        lines.append("")
    return "\n".join(lines) + "\n"


def run_block(start, prepare, variant, build, path, mode, count, out, log=print):
    """One block: install, (warm-up), `count` starts. `start(clear)` -> the text the start printed; `prepare(path)` installs the build. -> the result dict."""
    log(f"== {variant} {build} {mode}: {path}")
    prepare(path)
    if mode == "hit":
        start(False)                   # fills the cache; not counted
    else:
        start(True)                    # the warm-up (page cache); not counted
    rows, texts = [], []
    for _ in range(count):
        text = start(mode == "miss")
        texts.append(text)
        rows.append(parse_start(text))
        log(f"   first_present {rows[-1]['first_present_ms'] if rows[-1] else 'none'} ms  {rows[-1]['cache'] if rows[-1] else ''}  wait: {rows[-1]['wait'] if rows[-1] else ''}")
    kept = Path(out) / "starts"
    kept.mkdir(parents=True, exist_ok=True)
    for i, text in enumerate(texts, 1):
        lines = [l for l in (text or "").splitlines() if KEPT.search(l)]
        (kept / f"{variant}-{build}-{mode}-{i:02d}.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return {"variant": variant, "build": build, "mode": mode, "path": str(path), "summary": summarize(rows), "starts": rows}


# ---- Android ------------------------------------------------------------------------------------------------------------------

def sh(argv, timeout=120):
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
        return p.returncode, (p.stdout + p.stderr)
    except (OSError, subprocess.TimeoutExpired) as e:
        return 127, f"{type(e).__name__}: {e}"


def android_runner(package, activity, run=sh, sleep=time.sleep, clock=time.monotonic, wait_s=60.0):
    """-> (prepare, start) for an Android emulator on adb."""
    def adb(*args, timeout=120):
        return run(["adb", *args], timeout)[1]

    def prepare(apk):
        adb("uninstall", package)       # two builds of one package from two runners are signed by two debug keys: install -r alone would refuse the second
        adb("install", "-r", "-t", str(apk), timeout=300)
        adb("shell", "setprop", "debug.az.startup", "1")

    def start(clear):
        adb("shell", "am", "force-stop", package)
        if clear:
            adb("shell", "pm", "clear", package)
        sleep(2.0)
        adb("logcat", "-c")
        adb("shell", "am", "start", "-n", f"{package}/{activity}")
        t0 = clock()
        text = ""
        while clock() - t0 < wait_s:
            sleep(1.0)
            text = adb("logcat", "-d", "-v", "threadtime", timeout=60)
            if "[startup-json]" in text:
                break
        sleep(1.0)
        text = adb("logcat", "-d", "-v", "threadtime", timeout=60)
        adb("shell", "am", "force-stop", package)
        return text
    return prepare, start


# ---- iOS Simulator ------------------------------------------------------------------------------------------------------------

def ios_runner(udid, bundle, work, run=sh, sleep=time.sleep, clock=time.monotonic, wait_s=60.0, popen=subprocess.Popen):
    """-> (prepare, start) for a booted Simulator. A MISS start uninstalls and installs again first (a fresh data container: the font cache is in it)."""
    state = {"path": None}

    def prepare(app):
        state["path"] = str(app)
        run(["xcrun", "simctl", "install", udid, state["path"]], 300)

    def start(clear):
        run(["xcrun", "simctl", "terminate", udid, bundle], 60)
        if clear:
            run(["xcrun", "simctl", "uninstall", udid, bundle], 120)
            run(["xcrun", "simctl", "install", udid, state["path"]], 300)
        sleep(2.0)
        out = Path(work) / "console.txt"
        with open(out, "w", encoding="utf-8") as fh:
            env = dict(os.environ, SIMCTL_CHILD_AZ_STARTUP="1", SIMCTL_CHILD_RUST_BACKTRACE="1")
            proc = popen(["xcrun", "simctl", "launch", "--console", udid, bundle], stdout=fh, stderr=subprocess.STDOUT, env=env)
            t0 = clock()
            text = ""
            while clock() - t0 < wait_s:
                sleep(1.0)
                text = out.read_text(encoding="utf-8", errors="replace")
                if "[startup-json]" in text:
                    break
            sleep(1.0)
            text = out.read_text(encoding="utf-8", errors="replace")
            run(["xcrun", "simctl", "terminate", udid, bundle], 60)
            try:
                proc.wait(timeout=10)
            except Exception:  # noqa: BLE001  (the console process is ours: stop it)
                proc.kill()
        return text
    return prepare, start


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="platform", required=True)
    for name in ("android", "ios"):
        p = sub.add_parser(name)
        p.add_argument("--out", required=True)
        p.add_argument("--plan", required=True)
        p.add_argument("--starts", type=int, default=10)
        p.add_argument("--label", default="")
        if name == "android":
            p.add_argument("--package", required=True)
            p.add_argument("--activity", required=True)
        else:
            p.add_argument("--udid", required=True)
            p.add_argument("--bundle", required=True)
    a = ap.parse_args(argv)
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    plan = parse_plan(a.plan)
    if a.platform == "android":
        prepare, start = android_runner(a.package, a.activity)
    else:
        prepare, start = ios_runner(a.udid, a.bundle, out)
    results = []
    for variant, build, path, mode in blocks_of(plan):
        results.append(run_block(start, prepare, variant, build, path, mode, a.starts, out))
        (out / "font-wait.json").write_text(json.dumps(results, indent=1), encoding="utf-8")
        (out / "font-wait.md").write_text(report_markdown(results, a.label), encoding="utf-8")
    print((out / "font-wait.md").read_text(encoding="utf-8"))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
