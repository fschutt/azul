#!/usr/bin/env python3
"""smoke_verdict.py: THE pass line of the boot tests (scripts/ci/android_smoke.sh and ios_smoke.sh call it, nothing else decides).

An app BOOTS when all of these hold, and the verdict says which one failed first:
  1. it is alive when the wait is over (`--alive 1`),
  2. it is still alive at a second look a few seconds later (`--alive-after 1`),
  3. it is on the screen: the picture differs from the launcher / home screen taken BEFORE the launch (`--baseline`, `--after`;
     98.5 percent of the sampled pixels the same = the home screen is still showing: an app that crashed or never came to the
     front leaves it behind) and, where the platform can say so, the system names the app as the resumed one (`--resumed yes`),
  4. the window of the picture (the rows between the system's bars) is more than one flat colour: it drew something,
  5. no fatal line of the app in the log and no crash report (`--fatal`, empty when none),
  6. the button the manifest names (`--tap`: a JSON file {label, found, alive, fatal}) did not take the app down AND changed the
     screen (`--tap-before`, `--tap-after`: the same 98.5 percent rule; a button that does nothing is not a working app, and the
     manifest says what must come up), the texts the manifest says must be on the screen afterwards are there and the ones that
     must be gone are gone (tap.json's after_found and after_present, scripts/ci/ui_find.py), and the text it expects on the first
     screen (`--expect-found`) is there when it names one.
Informational only, never part of the verdict: the platform's own launch numbers of an Android app, `launch_ms` (the TotalTime
`am start -W` printed, --start-log) and `displayed_ms` (the "Displayed <package>/<activity>: +1s234ms" line of the activity manager
in the log, --logcat); the status table shows them next to the verdict. So are the engine's own startup numbers (AZ_STARTUP=1: the
`[startup-json] {...}` line an app writes at its first present, --startup-log): first_present_ms from the process creation,
before_library_ms, layout_passes and render_backend (cpu or gpu); a run without the switch has none, and the table shows nothing.
A screenshot that is "not black" is NOT in this list: the iOS home screen of 2026-11-26 passed that test while the app had died.

    smoke_verdict.py --out result.json --app <name> --id <package or bundle id> --alive 1 --alive-after 1 [--fatal "<line>"]
        [--rust-lines N] [--baseline b.png --after a.png] [--resumed yes|no] [--tap tap.json]
        [--expect-text "<text>" --expect-found yes|no] [--start-log start.log] [--logcat logcat.txt] [--extra '{"key": value}']

Writes result.json ({app, id, booted, alive, alive_after, differs_from_home, home_similarity, screen_nonblank, drawn_fraction,
resumed, fatal, tap, expect, pass, reason, ...}) and exits 0 when the app booted, 1 when not.
"""
import argparse
import json
import os
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import png_compare  # noqa: E402
import png_nonblank  # noqa: E402


def parse_total_time(text):
    """The TotalTime (ms) of `am start -W`, or None."""
    m = re.search(r"(?m)^TotalTime:\s*(\d+)", text or "")
    return int(m.group(1)) if m else None


def parse_displayed(text, package):
    """The ms of the last "Displayed <package>/<activity>: +1s234ms" line of the activity manager for `package`, or None."""
    found = None
    for m in re.finditer(r"Displayed\s+" + re.escape(package) + r"/\S+:\s+\+(?:(\d+)s)?(\d+)ms", text or ""):
        found = int(m.group(1) or 0) * 1000 + int(m.group(2))
    return found


STARTUP_MARK = "[startup-json] "
STARTUP_KEYS = ("first_present_ms", "before_library_ms", "layout_passes", "render_backend")


def parse_startup(text):
    """The engine's own startup numbers from the LAST `[startup-json] {...}` line of `text` (the line AZ_STARTUP=1 makes an app write
    to stderr at its first present: Android's logcat, the console of an iOS process), as a dict of first_present_ms (counted from the
    process creation the OS recorded), before_library_ms, layout_passes and render_backend; None for a run without the switch."""
    text = text or ""
    at = text.rfind(STARTUP_MARK)
    if at < 0:
        return None
    rest = text[at + len(STARTUP_MARK):].lstrip()
    try:
        obj, _ = json.JSONDecoder().raw_decode(rest)
    except ValueError:
        obj = None
    if isinstance(obj, dict):
        got = {k: obj[k] for k in STARTUP_KEYS if k in obj}
        return got or None
    # a line the log cut off (logcat splits what is longer than 4 KB): the numbers that come first are still whole
    got = {}
    for key in ("first_present_ms", "before_library_ms"):
        m = re.search(r'"%s":\s*(-?[0-9.]+)' % key, rest)
        if m:
            got[key] = float(m.group(1))
    m = re.search(r'"layout_passes":\s*(\d+)', rest)
    if m:
        got["layout_passes"] = int(m.group(1))
    m = re.search(r'"render_backend":\s*"([^"]*)"', rest)
    if m:
        got["render_backend"] = m.group(1)
    return got or None


def judge(facts):
    """facts -> (passed, reason). The order is the order of the list in the docstring."""
    if not facts.get("alive"):
        return False, "not alive when the wait was over" + (f" ({facts['fatal']})" if facts.get("fatal") else "")
    if not facts.get("alive_after"):
        return False, "alive when the wait was over, gone at the second look" + (f" ({facts['fatal']})" if facts.get("fatal") else "")
    if facts.get("screenshot_error"):
        return False, f"the screenshot {facts['screenshot_error']}"
    if facts.get("resumed") == "no":
        return False, "the system does not name the app as the resumed one: the launcher is in front"
    if facts.get("compare_error"):
        return False, f"the screenshot cannot be compared with the home screen: {facts['compare_error']}"
    if facts.get("home_similarity") is not None and not facts.get("differs_from_home"):
        return False, f"the home screen is still showing ({facts['home_similarity'] * 100:.1f}% of the pixels are the same as before the launch)"
    if not facts.get("screen_nonblank"):
        return False, "the window is one flat colour: it never drew"
    if facts.get("fatal"):
        return False, f"fatal line: {facts['fatal']}"
    tap = facts.get("tap")
    if tap and (not tap.get("alive") or tap.get("fatal")):
        return False, f"after pressing \"{tap.get('label')}\": {tap.get('fatal') or 'the app is gone'}"
    for text, found in ((tap or {}).get("probe_found") or {}).items():       # a first press with a short effect ([app.smoke] probe_tap)
        if not found:
            return False, f"1 s after pressing \"{tap.get('probe_label')}\" the screen does not show \"{text}\""
    if tap and tap.get("changed_screen") is False:
        return False, (f"pressing \"{tap.get('label')}\" changed nothing on the screen ({tap['screen_similarity'] * 100:.1f}% of the "
                       f"pixels are the same as before): the app ignored the tap or its action did nothing")
    for text, found in ((tap or {}).get("after_found") or {}).items():       # what the manifest says must come up ([app.smoke] after_tap)
        if not found:
            return False, f"after pressing \"{tap.get('label')}\" the screen does not show \"{text}\""
    for text, present in ((tap or {}).get("after_present") or {}).items():   # what must be gone ([app.smoke] after_tap_gone)
        if present:
            return False, f"after pressing \"{tap.get('label')}\" the screen still shows \"{text}\""
    expect = facts.get("expect")
    if expect and not expect.get("found"):
        return False, f"the first screen does not show \"{expect.get('text')}\""
    return True, "alive twice, on the screen, drawn, no fatal line"


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--app", required=True)
    ap.add_argument("--id", default="")
    ap.add_argument("--alive", default="0")
    ap.add_argument("--alive-after", default="0")
    ap.add_argument("--fatal", default="")
    ap.add_argument("--rust-lines", default="0")
    ap.add_argument("--baseline")
    ap.add_argument("--after")
    ap.add_argument("--resumed", default="")
    ap.add_argument("--tap")
    ap.add_argument("--tap-before", help="the screenshot before the button was pressed")
    ap.add_argument("--tap-after", help="the screenshot after it")
    ap.add_argument("--expect-text", default="")
    ap.add_argument("--expect-found", default="")
    ap.add_argument("--band", default="", help="TOP,BOTTOM: the rows of the screenshot between the system bars, as fractions of the "
                    "height (default 0.08,0.94; the Android emulator 0.045,0.94, an iPhone 0.075,0.965)")
    ap.add_argument("--start-log")
    ap.add_argument("--logcat")
    ap.add_argument("--startup-log", action="append", default=[], help="a log that may hold the [startup-json] line of AZ_STARTUP=1")
    ap.add_argument("--extra", default="{}")
    a = ap.parse_args(argv)
    band = png_nonblank.DEFAULT_BAND
    if a.band:
        try:
            top, bottom = (float(x) for x in a.band.split(","))
        except ValueError:
            ap.error("--band wants TOP,BOTTOM, two fractions of the height")
        if not 0.0 <= top < bottom <= 1.0:
            ap.error("--band wants 0 <= TOP < BOTTOM <= 1")
        band = (top, bottom)

    facts = {"app": a.app, "id": a.id, "booted": True, "alive": a.alive == "1", "alive_after": a.alive_after == "1",
             "fatal": a.fatal or None, "rust_log_lines": int(a.rust_lines or 0), "resumed": a.resumed or None}
    after = None
    if not a.after or not os.path.exists(a.after):
        facts["screenshot_error"] = "was never taken: no screenshot"
    else:
        try:
            after = png_compare.decode(a.after)
            if after is None:
                facts["screenshot_error"] = "cannot be read: not an 8-bit, non-interlaced PNG"
        except Exception as e:  # noqa: BLE001  (torn off by a hiccup of the emulator, or not a PNG)
            facts["screenshot_error"] = f"cannot be read: {str(e)[:100]}"
    if after is not None:
        facts["band"] = list(band)
        facts["drawn_fraction"] = round(png_nonblank.drawn_fraction(after, band), 5)
        facts["screen_nonblank"] = facts["drawn_fraction"] >= png_nonblank.DRAWN_FRACTION
        if a.baseline and os.path.exists(a.baseline):
            try:
                sim = png_compare.same_fraction(png_compare.decode(a.baseline), after)
                facts["home_similarity"] = round(sim, 4)
                facts["differs_from_home"] = sim < png_compare.SAME
            except Exception as e:  # noqa: BLE001  (a picture that cannot be read says nothing about the app)
                facts["compare_error"] = str(e)[:120]
    if a.tap and os.path.exists(a.tap):
        facts["tap"] = json.load(open(a.tap))
        if a.tap_before and a.tap_after and os.path.exists(a.tap_before) and os.path.exists(a.tap_after):
            try:
                sim = png_compare.same_fraction(png_compare.decode(a.tap_before), png_compare.decode(a.tap_after))
                facts["tap"]["screen_similarity"] = round(sim, 4)
                facts["tap"]["changed_screen"] = sim < png_compare.SAME
            except Exception as e:  # noqa: BLE001  (pictures that cannot be read say nothing about the button)
                facts["tap"]["compare_error"] = str(e)[:120]
    if a.expect_text:
        facts["expect"] = {"text": a.expect_text, "found": a.expect_found == "yes"}
    for key, path, parse in (("launch_ms", a.start_log, parse_total_time), ("displayed_ms", a.logcat, lambda t: parse_displayed(t, a.id))):
        if path and os.path.exists(path):
            value = parse(pathlib.Path(path).read_text(encoding="utf-8", errors="replace"))
            if value is not None:
                facts[key] = value
    for path in a.startup_log:
        if os.path.exists(path):
            got = parse_startup(pathlib.Path(path).read_text(encoding="utf-8", errors="replace"))
            if got:
                facts.update(got)
    facts.update(json.loads(a.extra))
    passed, reason = judge(facts)
    facts["pass"] = passed
    facts["reason"] = reason
    pathlib.Path(a.out).write_text(json.dumps(facts, indent=2, sort_keys=True), encoding="utf-8")
    print(f"[{'PASS' if passed else 'FAIL'}] {a.app}: {reason}")
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
