#!/usr/bin/env python3
"""Replay real pointer/keyboard input on macOS, in the same vocabulary as
`ydotool_input.py` / `xdotool_input.py`, so a self-test plan can run unchanged
on any desktop session.

    ./scripts/cgevent_input.py activate 4242 moveto 500 400 down path 500 400 700 520 40 up
    ./scripts/cgevent_input.py moveto 300 250 click right sleep 0.3 key Escape
    ./scripts/cgevent_input.py moveto 400 300 wheel -3 key cmd+a key cmd+c

Commands: reset | moveto X Y | at X Y | move DX DY | down|up|click|dblclick
[left|right|middle] | key NAME | type TEXT | sleep SECS | wheel N | pixwheel
DY [DX] | path X1 Y1 X2 Y2 STEPS | activate PID | windows PID | shot FILE PID.

Positions are GLOBAL display points (top-left origin, what CGWindowList
reports), not backing pixels. `windows PID` prints the app's windows as
`id x y w h layer name` so a plan can find its content origin.

WHY THIS IS SIMPLER THAN THE WAYLAND TWIN: CGEventPost takes absolute
positions and holds a button across events, like XTEST. It needs the calling
terminal to be trusted for "post events" (System Settings -> Privacy &
Security -> Accessibility); `shot` needs Screen Recording.

What the real HID path does that this must copy: while a button is down,
motion arrives as *Dragged events, never MouseMoved (AppKit sends mouseDragged:
only for those); the second press of a double click carries click state 2;
every event carries the modifier flags that are held, which is what AppKit
reads - a separate modifier key-down alone is not enough.

Wheel: `wheel N` is N line-unit detents (positive = up/away, negative = down,
matching the other drivers). `pixwheel` posts one pixel-unit (precise,
trackpad-like) event.
"""
import subprocess
import sys
import time

import Quartz

BTN = {"left": Quartz.kCGMouseButtonLeft, "right": Quartz.kCGMouseButtonRight, "middle": Quartz.kCGMouseButtonCenter}
DOWN = {"left": Quartz.kCGEventLeftMouseDown, "right": Quartz.kCGEventRightMouseDown, "middle": Quartz.kCGEventOtherMouseDown}
UP = {"left": Quartz.kCGEventLeftMouseUp, "right": Quartz.kCGEventRightMouseUp, "middle": Quartz.kCGEventOtherMouseUp}
DRAG = {"left": Quartz.kCGEventLeftMouseDragged, "right": Quartz.kCGEventRightMouseDragged, "middle": Quartz.kCGEventOtherMouseDragged}

# ANSI virtual key codes (Carbon kVK_*): the physical key, layout-independent.
KEYS = {
    "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11, "q": 12,
    "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23,
    "equal": 24, "9": 25, "7": 26, "minus": 27, "8": 28, "0": 29, "o": 31, "u": 32, "i": 34, "p": 35,
    "l": 37, "j": 38, "k": 40, "semicolon": 41, "comma": 43, "slash": 44, "n": 45, "m": 46, "dot": 47,
    "enter": 36, "return": 36, "tab": 48, "space": 49, "backspace": 51, "esc": 53, "escape": 53,
    "delete": 117, "home": 115, "end": 119, "pageup": 116, "pagedown": 121,
    "left": 123, "right": 124, "down": 125, "up": 126, "f1": 122, "f2": 120,
}
MODS = {
    "cmd": (55, Quartz.kCGEventFlagMaskCommand), "meta": (55, Quartz.kCGEventFlagMaskCommand),
    "super": (55, Quartz.kCGEventFlagMaskCommand), "shift": (56, Quartz.kCGEventFlagMaskShift),
    "alt": (58, Quartz.kCGEventFlagMaskAlternate), "option": (58, Quartz.kCGEventFlagMaskAlternate),
    "ctrl": (59, Quartz.kCGEventFlagMaskControl),
}
CHARS = {" ": "space", ".": "dot", ",": "comma", "-": "minus", "=": "equal", "/": "slash", ";": "semicolon", "\n": "enter"}


def post(ev):
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, ev)


def here():
    p = Quartz.CGEventGetLocation(Quartz.CGEventCreate(None))
    return [p.x, p.y]


class Pointer:
    def __init__(self):
        self.held = None
        self.last_click = (0.0, None, 0)

    def move(self, x, y):
        kind = DRAG[self.held] if self.held else Quartz.kCGEventMouseMoved
        ev = Quartz.CGEventCreateMouseEvent(None, kind, (x, y), BTN[self.held or "left"])
        post(ev)

    def button(self, name, down, clicks=None):
        pos = here()
        if down:
            now = time.monotonic()
            t, where, n = self.last_click
            n = n + 1 if (now - t < 0.4 and where == (round(pos[0]), round(pos[1]))) else 1
            self.last_click = (now, (round(pos[0]), round(pos[1])), n)
            clicks = clicks or n
        else:
            clicks = clicks or self.last_click[2] or 1
        ev = Quartz.CGEventCreateMouseEvent(None, (DOWN if down else UP)[name], pos, BTN[name])
        Quartz.CGEventSetIntegerValueField(ev, Quartz.kCGMouseEventClickState, clicks)
        post(ev)
        self.held = name if down else None
        time.sleep(0.02)


def key(spec):
    parts = spec.lower().split("+")
    mods, k = parts[:-1], parts[-1]
    flags = 0
    for m in mods:
        code, mask = MODS[m]
        flags |= mask
        ev = Quartz.CGEventCreateKeyboardEvent(None, code, True)
        Quartz.CGEventSetFlags(ev, flags)
        post(ev)
    code = KEYS[k]
    for down in (True, False):
        ev = Quartz.CGEventCreateKeyboardEvent(None, code, down)
        Quartz.CGEventSetFlags(ev, flags)
        post(ev)
        time.sleep(0.01)
    for m in reversed(mods):
        code, mask = MODS[m]
        flags &= ~mask
        ev = Quartz.CGEventCreateKeyboardEvent(None, code, False)
        Quartz.CGEventSetFlags(ev, flags)
        post(ev)
    time.sleep(0.02)


def type_text(text):
    for ch in text:
        name = CHARS.get(ch, ch.lower())
        code = KEYS.get(name, 0)
        for down in (True, False):
            ev = Quartz.CGEventCreateKeyboardEvent(None, code, down)
            Quartz.CGEventKeyboardSetUnicodeString(ev, len(ch.encode("utf-16-le")) // 2, ch)
            if ch.isupper():
                Quartz.CGEventSetFlags(ev, Quartz.kCGEventFlagMaskShift)
            post(ev)
            time.sleep(0.008)
        time.sleep(0.02)


def windows(pid):
    out = []
    infos = Quartz.CGWindowListCopyWindowInfo(Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID)
    for w in infos or []:
        if int(w.get("kCGWindowOwnerPID", -1)) != pid:
            continue
        b = w["kCGWindowBounds"]
        out.append((int(w["kCGWindowNumber"]), int(b["X"]), int(b["Y"]), int(b["Width"]), int(b["Height"]),
                    int(w.get("kCGWindowLayer", 0)), w.get("kCGWindowName") or ""))
    return out


def activate(pid):
    import AppKit
    app = AppKit.NSRunningApplication.runningApplicationWithProcessIdentifier_(pid)
    if app is None:
        raise SystemExit(f"no running application with pid {pid}")
    app.activateWithOptions_(AppKit.NSApplicationActivateIgnoringOtherApps)
    time.sleep(0.3)


def main(argv):
    p = Pointer()
    i = 0
    while i < len(argv):
        c = argv[i]
        i += 1
        if c == "reset":
            p.move(0, 0)
        elif c in ("moveto", "at"):
            x, y = float(argv[i]), float(argv[i + 1])
            i += 2
            if c == "moveto":
                p.move(x, y)
                time.sleep(0.01)
        elif c == "move":
            dx, dy = float(argv[i]), float(argv[i + 1])
            i += 2
            x, y = here()
            p.move(x + dx, y + dy)
            time.sleep(0.01)
        elif c in ("down", "up", "click", "dblclick"):
            b = "left"
            if i < len(argv) and argv[i] in BTN:
                b = argv[i]
                i += 1
            if c == "down":
                p.button(b, True)
            elif c == "up":
                p.button(b, False)
            elif c == "click":
                p.button(b, True)
                p.button(b, False)
            else:
                p.button(b, True, 1)
                p.button(b, False, 1)
                p.button(b, True, 2)
                p.button(b, False, 2)
        elif c == "key":
            key(argv[i])
            i += 1
        elif c == "type":
            type_text(argv[i])
            i += 1
        elif c == "sleep":
            time.sleep(float(argv[i]))
            i += 1
        elif c == "wheel":
            n = int(argv[i])
            i += 1
            for _ in range(abs(n)):
                ev = Quartz.CGEventCreateScrollWheelEvent(None, Quartz.kCGScrollEventUnitLine, 1, 1 if n > 0 else -1)
                post(ev)
                time.sleep(0.03)
        elif c == "pixwheel":
            dy = int(argv[i])
            i += 1
            dx = 0
            if i < len(argv) and argv[i].lstrip("-").isdigit():
                dx = int(argv[i])
                i += 1
            ev = Quartz.CGEventCreateScrollWheelEvent(None, Quartz.kCGScrollEventUnitPixel, 2, dy, dx)
            post(ev)
            time.sleep(0.02)
        elif c == "path":
            x1, y1, x2, y2 = (float(v) for v in argv[i : i + 4])
            n = int(argv[i + 4])
            i += 5
            for k in range(0, n + 1):
                p.move(x1 + (x2 - x1) * k / n, y1 + (y2 - y1) * k / n)
                time.sleep(0.004)
        elif c == "activate":
            activate(int(argv[i]))
            i += 1
        elif c == "windows":
            for w in windows(int(argv[i])):
                print(*w)
            i += 1
        elif c == "shot":
            path, pid = argv[i], int(argv[i + 1])
            i += 2
            ws = [w for w in windows(pid) if w[5] == 0] or windows(pid)
            if not ws:
                raise SystemExit(f"pid {pid} has no window on screen")
            subprocess.run(["screencapture", "-x", "-o", "-l", str(ws[0][0]), path], check=True)
        else:
            raise SystemExit(f"unknown command {c}")


if __name__ == "__main__":
    main(sys.argv[1:])
