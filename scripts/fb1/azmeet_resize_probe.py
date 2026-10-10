#!/usr/bin/env python3
"""AzMeet resize probe (FB1): what does a window resize paint, headless?

The user saw two AzMeet bugs on the Mac, both on RESIZE: "the <input> field
sometimes 'forgets' to stretch" (the lobby card) and "the statistics sometimes
break lines if I resize the window" (the call view's device columns). The
desktop shells resize through the fast path (`IncrementalRelayout::Resize`,
`resize_only_hint`); until 1023db90c the headless backend did not, so no
headless run could show them.

This script starts a headless AzMeet with its debug server (never a real
window), drives it through a sequence of `resize` ops and fingerprints every
frame from `get_display_list`: the rects, borders, text runs (glyph counts per
run, i.e. where lines break) and hit-test areas (one per text line), rounded to
0.1 px. With --record it writes the fingerprints; with --compare it replays the
sequence and reports every step whose picture differs from the recording.

The committed references (reference_lobby.json, reference_call.json next to
this file) were recorded on the build of 66db869f1 (restyle relayout on
resize), on the Mac the bugs were reported on (fonts decide glyph widths:
re-record on another machine with a pre-1023db90c build). A difference after
1023db90c is a step where the fast path paints what a full relayout does not.

    python3 scripts/fb1/azmeet_resize_probe.py --view lobby --compare scripts/fb1/reference_lobby.json
    python3 scripts/fb1/azmeet_resize_probe.py --view call  --compare scripts/fb1/reference_call.json

The lobby needs nothing; its meeting server is pinned to an address that does
not answer, so the status line is fixed. The call view needs a meet Worker (the
local dev server, default http://127.0.0.1:8787) for --autocreate, and
fingerprints only the devices panel (the room code in the header differs per
run). Exit 0 = every step matches (or recorded), 1 = a difference, 2 = setup.
"""

import argparse
import json
import os
import signal
import subprocess
import sys
import time
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))

SEQUENCES = {
    # The card is 576 px wide: above that the inputs keep 520 / 461.4, below it
    # they follow the window, and back above it they must stretch again.
    "lobby": [
        (1100, 720), (900, 700), (700, 600), (576, 600), (560, 600), (500, 600),
        (420, 600), (350, 600), (420, 640), (560, 700), (700, 720), (900, 720),
        (1100, 720), (300, 720), (1100, 720), (1300, 800), (1100, 640), (1100, 720),
    ],
    # The devices panel: five columns that shrink, clamp at their min-content,
    # re-wrap, and un-wrap on the way back; the window height changes too.
    "call": [
        (1100, 720), (1000, 720), (900, 700), (800, 700), (760, 680), (700, 650),
        (640, 650), (600, 640), (560, 620), (520, 620), (560, 620), (600, 640),
        (700, 700), (900, 720), (1100, 720), (620, 720), (1100, 720), (1100, 600),
        (1100, 720),
    ],
}


def post(port, **body):
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}/",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read())


def wait_up(port, proc, timeout=40.0):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if proc.poll() is not None:
            raise RuntimeError(f"AzMeet exited early (code {proc.returncode})")
        try:
            if post(port, op="get_state").get("status") == "ok":
                return
        except Exception:
            pass
        time.sleep(0.2)
    raise RuntimeError("the debug server did not come up")


def frames(port, n=3):
    for _ in range(n):
        post(port, op="wait_frame")


def html(port):
    return post(port, op="get_html_string")["data"]["value"]["html"]


def text_node_id(port, text):
    import re

    for nid, s in re.findall(r'data-az-node-id="(\d+)"[^>]*>([^<]*)</text>', html(port)):
        if s.strip() == text:
            return int(nid)
    return None


def node_rect(port, node_id):
    v = post(port, op="get_node_layout", node_id=node_id)["data"]["value"]
    r = v.get("rect") or {}
    return r.get("x"), r.get("y"), r.get("width"), r.get("height")


def fingerprint(port, min_y=None):
    """The frame's paint, as comparable rows: [type, x, y, w, h, glyphs, color/tag]."""
    items = post(port, op="get_display_list")["data"]["value"]["items"]
    rows = []
    for it in items:
        kind = it.get("type")
        if kind not in ("rect", "border", "text", "hit_test_area"):
            continue
        y = it.get("y", 0.0)
        if min_y is not None and y < min_y:
            continue
        rows.append([
            kind,
            round(it.get("x", 0.0), 1),
            round(y, 1),
            round(it.get("width", 0.0), 1),
            round(it.get("height", 0.0), 1),
            it.get("glyph_count"),
            it.get("color") or it.get("debug_info"),
        ])
    return rows


def run(args, sequence):
    env = dict(os.environ)
    env.update({
        "DYLD_LIBRARY_PATH": args.lib,
        "LD_LIBRARY_PATH": args.lib,
        "AZ_BACKEND": "headless",
        "AZ_DEBUG": str(args.port),
    })
    # AzMeet's settings are switches (its --help); the AZMEET_* variables of this shell are
    # blanked (AzMeet reads a blank one as unset), so none changes the probe behind its back.
    for name in [n for n in env if n.startswith("AZMEET_")]:
        env[name] = ""
    if args.view == "lobby":
        # An address that never answers: the status line is fixed.
        flags = ["--worker", "http://127.0.0.1:9"]
    else:
        flags = ["--worker", args.worker, "--autocreate"]
    proc = subprocess.Popen(
        [args.binary] + flags, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    try:
        wait_up(args.port, proc)
        frames(args.port, 4)
        min_y = None
        if args.view == "call":
            # Wait for the call view, then fingerprint the devices panel only.
            t0 = time.time()
            while text_node_id(args.port, "Microphones") is None:
                if time.time() - t0 > 20:
                    raise RuntimeError("no call view (is the meet Worker at --worker running?)")
                time.sleep(0.3)
                frames(args.port, 1)
        steps = []
        for (w, h) in sequence:
            post(args.port, op="resize", width=float(w), height=float(h))
            frames(args.port, 3)
            if args.view == "call":
                nid = text_node_id(args.port, "Microphones")
                _, y, _, _ = node_rect(args.port, nid - 1)
                min_y = (y or 0.0) - 12.0
            steps.append({"size": [w, h], "items": fingerprint(args.port, min_y)})
        return steps
    finally:
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


def compare(recorded, now):
    bad = 0
    for a, b in zip(recorded, now):
        if a["items"] == b["items"]:
            print(f"ok   {a['size'][0]}x{a['size'][1]}")
            continue
        bad += 1
        first = next(
            (i for i, (x, y) in enumerate(zip(a["items"], b["items"])) if x != y),
            min(len(a["items"]), len(b["items"])),
        )
        print(f"DIFF {a['size'][0]}x{a['size'][1]}: {len(a['items'])} -> {len(b['items'])} rows, "
              f"first difference at row {first}")
        print(f"     recorded: {a['items'][first] if first < len(a['items']) else None}")
        print(f"     now:      {b['items'][first] if first < len(b['items']) else None}")
    return bad


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--view", choices=["lobby", "call"], default="lobby")
    p.add_argument("--binary", default=os.path.join(ROOT, "target", "release", "AzMeet"))
    p.add_argument("--lib", default=os.path.join(ROOT, "target", "azul-lib"))
    p.add_argument("--port", type=int, default=19480)
    p.add_argument("--worker", default="http://127.0.0.1:8787")
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--record", metavar="JSON")
    mode.add_argument("--compare", metavar="JSON")
    args = p.parse_args()

    try:
        steps = run(args, SEQUENCES[args.view])
    except Exception as e:  # setup, not a verdict
        print(f"setup failed: {e}", file=sys.stderr)
        return 2
    if args.record:
        with open(args.record, "w") as f:
            json.dump({"view": args.view, "steps": steps}, f, separators=(",", ":"))
            f.write("\n")
        print(f"recorded {len(steps)} steps to {args.record}")
        return 0
    with open(args.compare) as f:
        recorded = json.load(f)["steps"]
    bad = compare(recorded, steps)
    print(f"\n{len(steps) - bad} steps match, {bad} differ")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
