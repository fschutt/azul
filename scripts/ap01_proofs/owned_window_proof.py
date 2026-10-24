#!/usr/bin/env python3
"""RUNTIME23 proof 8: AzMail's Options window is OWNED by the main window (real window, macOS):
it stays in front of the main window, and it closes when the main window closes.
Window order from Quartz (CGWindowListCopyWindowInfo, front to back). owned_window.py BIN PORT OUT"""
import json, os, sys, tempfile, time
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import azlin_e2e as e
import Quartz

bin_, port, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
os.makedirs(out, exist_ok=True)
e.BACKEND = "cpu"
tmp = tempfile.mkdtemp(prefix="owned-")
env = {"AZMAIL_DATA": os.path.join(tmp, "mail"), "AZLIN_DATA": os.path.join(tmp, "azlin"), "AZLIN_CONFIG": "off"}
app = e.App("azmail-owned", bin_, ["--language", "en"], port, tmp, 120, extra_env=env)

def our_windows():
    """(window number, bounds) of the app's on-screen windows, front to back."""
    info = Quartz.CGWindowListCopyWindowInfo(Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID)
    rows = []
    for w in info:
        if int(w.get("kCGWindowOwnerPID", 0)) == app.process.pid and int(w.get("kCGWindowLayer", 0)) == 0:
            b = w.get("kCGWindowBounds")
            rows.append((int(w["kCGWindowNumber"]), dict(b)))
    return rows

try:
    app.until("the debug server", lambda: app.op("wait_frame") is not None)
    app.frame(3)
    app.key(",", primary=True)
    app.until("the Options window", lambda: "azmail-options" in app.window_ids())
    app.frame(4)
    time.sleep(1.0)
    rows = app.until("two on-screen windows", lambda: our_windows() if len(our_windows()) >= 2 else None)
    print("front to back:", rows, flush=True)
    sizes = [r[1]["Width"] * r[1]["Height"] for r in rows]
    if sizes[0] >= sizes[1]:
        raise e.Failure("the Options window (the smaller) is not in front: %s" % rows)
    # Activate the main window (its focus op): the owned window stays in front of it.
    app.op("focus")
    app.frame(3)
    time.sleep(1.0)
    rows2 = our_windows()
    print("after focusing the main window:", rows2, flush=True)
    sizes2 = [r[1]["Width"] * r[1]["Height"] for r in rows2]
    if len(rows2) < 2 or sizes2[0] >= sizes2[1]:
        raise e.Failure("the main window came in front of its Options window: %s" % rows2)
    shot = app.value("take_native_screenshot", window_id="azmail-options")
    # Close the main window: the Options window goes with it, and the app ends.
    app.op("close")
    gone = False
    for _ in range(40):
        if app.process.poll() is not None or not our_windows():
            gone = True
            break
        time.sleep(0.25)
    print("after closing the main window:", our_windows() if app.process.poll() is None else "app ended", flush=True)
    if not gone:
        raise e.Failure("the Options window outlived the main window: %s" % our_windows())
    print("PASS owned window: in front of its owner, and closed with it", flush=True)
finally:
    app.stop()
