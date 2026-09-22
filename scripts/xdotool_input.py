#!/usr/bin/env python3
"""Replay real pointer/keyboard input on X11, in the same vocabulary as
`ydotool_input.py` so a self-test plan can run unchanged on either session.

    ./scripts/xdotool_input.py moveto 500 400 down left path 500 400 700 520 40 up left
    ./scripts/xdotool_input.py moveto 300 250 click right sleep 0.3 key Escape
    ./scripts/xdotool_input.py moveto 400 300 wheel -3

Commands: reset | moveto X Y | at X Y | move DX DY | down|up|click [left|
right|middle] | key NAME | type TEXT | sleep SECS | wheel N | path X1 Y1 X2 Y2
STEPS.

WHY THIS IS SIMPLER THAN THE WAYLAND TWIN: xdotool drives XTEST, which takes
ABSOLUTE coordinates, so there is no pointer-acceleration problem (the whole
reason `ydotool_start.sh` has to set a flat profile on the virtual device) and
no need to park the pointer in a corner first. XTEST also holds a button
across other requests, so a drag needs no raw `input_event` socket. Nothing
here needs root or a daemon.

Wheel: X11 has no scroll axis — buttons 4/5 are up/down and 6/7 are left/
right, one click per detent.
"""
import subprocess
import sys
import time

BTN = {"left": "1", "middle": "2", "right": "3"}


def xdo(*args):
    subprocess.run(["xdotool", *[str(a) for a in args]], check=True)


def main(argv):
    pos = [None, None]
    i = 0
    while i < len(argv):
        c = argv[i]
        i += 1
        if c == "reset":
            xdo("mousemove", 0, 0)
            pos = [0, 0]
        elif c in ("moveto", "at"):
            x, y = int(argv[i]), int(argv[i + 1])
            i += 2
            if c == "moveto":
                xdo("mousemove", x, y)
            pos = [x, y]
        elif c == "move":
            dx, dy = int(argv[i]), int(argv[i + 1])
            i += 2
            xdo("mousemove_relative", "--", dx, dy)
            if pos[0] is not None:
                pos = [pos[0] + dx, pos[1] + dy]
        elif c in ("down", "up", "click"):
            b = "left"
            if i < len(argv) and argv[i] in BTN:
                b = argv[i]
                i += 1
            xdo({"down": "mousedown", "up": "mouseup", "click": "click"}[c], BTN[b])
        elif c == "key":
            xdo("key", argv[i])
            i += 1
        elif c == "type":
            xdo("type", "--delay", "20", argv[i])
            i += 1
        elif c == "sleep":
            time.sleep(float(argv[i]))
            i += 1
        elif c == "wheel":
            n = int(argv[i])
            i += 1
            # Negative = down (button 5), positive = up (button 4), matching
            # the ydotool driver's REL_WHEEL sign.
            button = "4" if n > 0 else "5"
            for _ in range(abs(n)):
                xdo("click", button)
                time.sleep(0.03)
        elif c == "path":
            x1, y1, x2, y2, n = (int(v) for v in argv[i : i + 5])
            i += 5
            # One xdotool invocation for the whole glide: a subprocess per
            # step is ~5ms of its own, which turns a 40-step drag into a
            # different gesture than the one being tested.
            chain = ["mousemove", str(x1), str(y1)]
            for k in range(1, n + 1):
                chain += [
                    "sleep",
                    "0.004",
                    "mousemove",
                    str(x1 + (x2 - x1) * k // n),
                    str(y1 + (y2 - y1) * k // n),
                ]
            xdo(*chain)
            pos = [x2, y2]
        else:
            raise SystemExit(f"unknown command {c}")


if __name__ == "__main__":
    main(sys.argv[1:])
