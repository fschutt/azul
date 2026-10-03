#!/usr/bin/env python3
"""SYSUI8: look at every app's UI text before and after a font change.

The system UI font changed (scripts/SYSUI8_2026_10_03.md): `system-ui` / the
widgets' `system:ui` are now San Francisco at the optical size of the text
plus its `trak` tracking, as CoreText and Chrome draw it - UI text at
11-14px is 10-14% WIDER than the opsz-28 "Display" instance drawn before.
This script records, per app, a screenshot and every text node's box against
its parent's box, and compares two such records:

    # one at a time, capped (the outer runner holds the machine-wide lock;
    # the apps inside use azul_debug's own watchdog):
    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 1200 --log /tmp/look.log -- \
        python3 scripts/sysui8_look.py record target/sysui8/look/before
    # ... build the new engine, then:
    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 1200 --log /tmp/look.log -- \
        python3 scripts/sysui8_look.py record target/sysui8/look/after
    python3 scripts/sysui8_look.py compare target/sysui8/look/before target/sysui8/look/after

`compare` lists, per app, the text that now OVERFLOWS its parent box (it did
not before) and the text that now WRAPS (its box grew taller), with the
classes of its parent - the widget CSS to look at - and the widths. Apps:
AZUL_APPS (comma list of names) or every prebuilt target/release/Az* app;
AZUL_APP_DIR overrides target/release. Window 1280 x 800, `wait_settled`
before every reading.
"""

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.environ.get("AZUL_ROOT") or os.path.dirname(HERE)
sys.path.insert(0, os.path.join(REPO, "scripts", "refci"))

APPS = [
    "AzBuilder", "AzCalculator", "AzCalendar", "AzContacts", "AzDashboard", "AzDrive",
    "AzMail", "AzMaps", "AzMeet", "AzNotes", "AzPaint", "AzPhoto", "AzReview", "AzSetup",
    "AzSheets", "AzShells", "AzShow", "AzTasks", "AzWriter",
    # with a <video>: kept short (house rules)
    "AzVideoCut", "AzWidgets",
]
WIDTH, HEIGHT = 1280, 800


def text_records(hierarchy):
    """Every text node: its box, its parent's box and a stable key."""
    nodes = {n["index"]: n for n in hierarchy.get("nodes", [])}

    def label(n):
        parts = [n.get("tag") or n.get("type") or "?"]
        if n.get("id"):
            parts.append("#" + n["id"])
        parts.extend("." + c for c in (n.get("classes") or []))
        return "".join(parts)

    def path(i):
        out = []
        while i in nodes and i >= 0:
            n = nodes[i]
            parent = n.get("parent", -1)
            siblings = nodes.get(parent, {}).get("children", []) if parent >= 0 else [i]
            out.append("%s[%d]" % (label(n), siblings.index(i) if i in siblings else 0))
            i = parent
        return "/".join(reversed(out))

    records = []
    for i, n in nodes.items():
        kind = (n.get("type") or n.get("tag") or "").lower()
        if kind not in ("text", "#text"):
            continue
        parent = nodes.get(n.get("parent", -1))
        if not parent or not n.get("rect") or not parent.get("rect"):
            continue
        r, p = n["rect"], parent["rect"]
        records.append({
            "key": path(i),
            "text": (n.get("text") or n.get("content") or "")[:60],
            "parent": label(parent),
            "x": r["x"], "y": r["y"], "w": r["width"], "h": r["height"],
            "pw": p["width"], "ph": p["height"],
            "overflow": round((r["x"] + r["width"]) - (p["x"] + p["width"]), 2),
        })
    return records


def record(out_dir):
    from azul_debug import AzulHeadless  # noqa: E402 - needs the path above

    app_dir = os.environ.get("AZUL_APP_DIR") or os.path.join(REPO, "target", "release")
    names = [a for a in os.environ.get("AZUL_APPS", "").split(",") if a] or APPS
    os.makedirs(out_dir, exist_ok=True)
    for name in names:
        app = os.path.join(app_dir, name)
        if not os.access(app, os.X_OK):
            print("skip %s (no binary)" % name)
            continue
        seconds = 45 if name in ("AzVideoCut", "AzWidgets") else 90
        try:
            with AzulHeadless(app=app, log_path=os.path.join(out_dir, name + ".log"),
                              seconds=seconds) as az:
                az.resize(WIDTH, HEIGHT)
                az.op("wait_settled")
                png = az.screenshot()
                with open(os.path.join(out_dir, name + ".png"), "wb") as f:
                    f.write(png)
                hierarchy = az.op("get_node_hierarchy") or {}
                records = text_records(hierarchy)
                with open(os.path.join(out_dir, name + ".json"), "w") as f:
                    json.dump(records, f, indent=0)
                over = sum(1 for r in records if r["overflow"] > 0.5)
                print("%-14s %4d text nodes, %3d overflow their parent" % (name, len(records), over))
        except Exception as e:  # noqa: BLE001 - one app failing must not stop the look
            print("%-14s FAILED: %s" % (name, e))


def compare(before_dir, after_dir):
    worse = 0
    for fname in sorted(os.listdir(after_dir)):
        if not fname.endswith(".json"):
            continue
        name = fname[:-5]
        try:
            before = {r["key"]: r for r in json.load(open(os.path.join(before_dir, fname)))}
        except FileNotFoundError:
            print("%s: no before record" % name)
            continue
        after = json.load(open(os.path.join(after_dir, fname)))
        rows = []
        for r in after:
            b = before.get(r["key"])
            if not b:
                continue
            if r["overflow"] > 0.5 and b["overflow"] <= 0.5:
                rows.append("  OVERFLOW %+6.1fpx  %r in %s (w %.1f -> %.1f, parent %.1f)"
                            % (r["overflow"], r["text"], r["parent"], b["w"], r["w"], r["pw"]))
            elif r["h"] > b["h"] * 1.4 and r["h"] - b["h"] > 4:
                rows.append("  WRAPS    h %.1f -> %.1f  %r in %s (w %.1f -> %.1f, parent %.1f)"
                            % (b["h"], r["h"], r["text"], r["parent"], b["w"], r["w"], r["pw"]))
        grown = [r["w"] / before[r["key"]]["w"] for r in after
                 if r["key"] in before and before[r["key"]]["w"] > 1]
        median = sorted(grown)[len(grown) // 2] if grown else 1.0
        print("%s: %d text nodes matched, median width x%.3f, %d newly broken"
              % (name, len(grown), median, len(rows)))
        for row in rows:
            print(row)
        worse += len(rows)
    return worse


if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "record":
        record(sys.argv[2])
    elif len(sys.argv) >= 4 and sys.argv[1] == "compare":
        sys.exit(1 if compare(sys.argv[2], sys.argv[3]) else 0)
    else:
        print(__doc__)
        sys.exit(2)
