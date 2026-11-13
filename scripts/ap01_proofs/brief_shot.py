#!/usr/bin/env python3
"""A brief real-window shot: start an app in a real window (the CPU or GPU backend), wait for
its first frame, optionally scroll a text into view and move the pointer, take the screenshot
and close the app - seconds, not minutes (a real window takes the screen and the focus).

    brief_shot.py --bin BIN --out PNG [--backend cpu|gpu] [--port N] [--size WxH]
                  [--scroll TEXT] [--theme flat|flora] [--mode light|dark] [--pointer X,Y]
                  [-- app args...]

The screenshot is the OS's picture of the window (`take_native_screenshot`, or `screencapture`
while the display sleeps: the display is never woken)."""
import argparse
import os
import sys
import tempfile
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import azlin_e2e as e  # noqa: E402

LIMIT_SECONDS = 30


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--bin", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--backend", default="cpu", choices=("cpu", "gpu"))
    p.add_argument("--port", type=int, default=8838)
    p.add_argument("--size", default="1280x820")
    p.add_argument("--scroll")
    p.add_argument("--theme")
    p.add_argument("--mode")
    p.add_argument("--pointer")
    p.add_argument("rest", nargs="*")
    a = p.parse_args()
    e.BACKEND = a.backend
    logs = tempfile.mkdtemp(prefix="brief-shot-")
    app = e.App("brief", a.bin, a.rest, a.port, logs, LIMIT_SECONDS)
    started = time.time()
    try:
        app.until("the debug server", lambda: app.op("wait_frame") is not None)
        w, h = (float(v) for v in a.size.split("x"))
        app.must("resize", width=w, height=h)
        if a.theme:
            app.must("set_theme", theme=a.theme)
        if a.mode:
            app.must("set_mode", mode=a.mode)
        app.frame(3)
        if a.scroll:
            app.must("scroll_into_view", text=a.scroll, block="start", behavior="instant")
            app.frame(2)
        if a.pointer:
            x, y = (float(v) for v in a.pointer.split(","))
            app.must("mouse_move", x=x, y=y)
            app.frame(3)
        os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
        app.screenshot(a.out)
    finally:
        app.stop()
    print("brief shot %s in %.1f s" % (a.out, time.time() - started))


if __name__ == "__main__":
    main()
