#!/usr/bin/env python3
"""ios_startup.py: cold starts of an app on the iOS Simulator with the engine's startup timeline switched on (AZ_STARTUP=1, handed over as
SIMCTL_CHILD_AZ_STARTUP), and the lines of the console the startup work reads:

    ios_startup.py --out DIR --udid UDID --bundle com.azul.AzWidgets --exe AzWidgets --app App.app|App.app.zip-dir [--starts 2]

Per start N: DIR/start-N.console.txt (the whole console of the cold start) and, in DIR/startup.md and DIR/startup.json, the `[startup]` table, the
`first_present` row, the `layout_end #1` row (its passes), the `fonts:cache` row (hit, miss or rebuilt, faces, check and load ms) and the
`fonts:cache_written` row. The app must be installed by this script first: `simctl launch` of an app that is not there starts nothing.
Exit 0 when the starts ran (what the lines say is the result), 1 when the app did not install."""
import argparse
import json
import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mobile_qa as q  # noqa: E402

WANTED = (
    ("first_present", re.compile(r"\bfirst_present\b")),
    ("layout_end_1", re.compile(r"layout_end\s*#?1\b")),
    ("fonts_cache", re.compile(r"fonts:cache(?!_written)")),
    ("fonts_cache_written", re.compile(r"fonts:cache_written")),
)


def startup_lines(text):
    """{table: [every `[startup]` line], first_present, layout_end_1, fonts_cache, fonts_cache_written: [lines]} of a console. The table lines are
    kept whole; a wanted row is any console line that names it (a row of the table, or a line of its own)."""
    lines = [l.rstrip() for l in text.splitlines()]
    found = {"table": [l for l in lines if "[startup]" in l]}
    for key, pattern in WANTED:
        found[key] = [l for l in lines if pattern.search(l)]
    return found


def render(starts):
    out = ["# iOS Simulator: the startup timeline of cold starts (AZ_STARTUP=1)", ""]
    for n, found in enumerate(starts, 1):
        out += [f"## start {n}", ""]
        for key, title in (("first_present", "first present"), ("layout_end_1", "layout end #1 (passes)"), ("fonts_cache", "fonts:cache"),
                           ("fonts_cache_written", "fonts:cache_written")):
            rows = found.get(key) or []
            out.append(f"- {title}: " + (" | ".join(r.strip() for r in rows) if rows else "NO LINE (the build does not print it, or the app never started)"))
        out += ["", "```"] + (found.get("table") or ["(no [startup] line: the timeline did not print)"]) + ["```", ""]
    return "\n".join(out)


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--udid", required=True)
    ap.add_argument("--bundle", required=True)
    ap.add_argument("--exe", default="")
    ap.add_argument("--app", required=True)
    ap.add_argument("--starts", type=int, default=2)
    ap.add_argument("--wait", type=float, default=60.0, help="seconds to wait for the app's first picture")
    args = ap.parse_args(argv)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    run = q.Run(args.out, "ios", {"label": "startup timeline (iOS)"})
    exe = args.exe or args.bundle.rsplit(".", 1)[-1]
    dev = q.IOS(run, args.udid, args.bundle, exe, args.app)
    rc, text = dev.to_file("install.log", "xcrun", "simctl", "install", args.udid, str(args.app), timeout=600)
    if rc != 0:
        (out / "startup.md").write_text(f"# iOS startup timeline\n\nthe app did not install on the Simulator: {text.strip()[-300:]}\n", encoding="utf-8")
        print("the app did not install")
        return 1
    dev.simctl("ui", args.udid, "appearance", "light")
    dev.terminate()
    dev.settle(2.0)
    try:
        dev.home_sig = dev.capture("home", keep=False)[0]
    except RuntimeError:
        dev.home_sig = None
    starts = []
    for n in range(1, args.starts + 1):
        dev.terminate()
        dev.settle(2.0)
        dev.launch(extra_env={"AZ_STARTUP": "1"}, cold=True)
        shown = dev.wait_for_front(timeout=args.wait)
        dev.settle(6.0)
        console = dev.console_text()
        (out / f"start-{n}.console.txt").write_text(console, encoding="utf-8")
        found = startup_lines(console)
        found["picture_seconds"] = None if shown is None else round(shown, 1)
        starts.append(found)
        time.sleep(1.0)
    (out / "startup.json").write_text(json.dumps(starts, indent=1), encoding="utf-8")
    (out / "startup.md").write_text(render(starts), encoding="utf-8")
    print(render(starts))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
