#!/usr/bin/env python3
"""Inject real pointer/keyboard input on a KWin Wayland session via the
xdg-desktop-portal RemoteDesktop API. Events go through the compositor and
arrive at apps as genuine wl_pointer/wl_keyboard input — the path we must
verify. No root, no uinput, no fake_input protocol (KWin 5.27 omits it).

Absolute positioning without a ScreenCast stream: warp to (0,0) with a huge
negative relative motion (compositor clamps), then track position internally
and emit relative deltas.

Commands (one invocation keeps the portal session alive for the whole
sequence; separate commands with a literal ","):
  move X Y | rel DX DY | btn left|right|middle down|up|click | click X Y
  drag X1 Y1 X2 Y2 [STEPS] [MS] | wheel DY | wheelat X Y DY
  key EVDEV down|up|click | sleep MS
Evdev keycodes: ESC=1 a=30 ... Enter=28 (same numbering as Linux input.h).

A restore token is cached in ~/.cache/azinput-portal.token and reused so the
KDE consent dialog only ever appears once. Exit 3 = Start() blocked/denied
(dialog not accepted) — caller should fall back to app-level injection.
"""
import os
import sys
import time
import dbus
import dbus.mainloop.glib
from gi.repository import GLib

BTN = {"left": 0x110, "right": 0x111, "middle": 0x112}
TOKEN_FILE = os.path.expanduser("~/.cache/azinput-portal.token")
WARP = -8000.0  # large enough to clamp to the top-left corner on any monitor

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
portal = bus.get_object("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop")
rd = dbus.Interface(portal, "org.freedesktop.portal.RemoteDesktop")
req_iface = "org.freedesktop.portal.Request"

_sender = bus.get_unique_name().replace(".", "_").lstrip(":")
_counter = [0]


def call_with_response(method, *args, build_options=None, timeout_s=20):
    """Invoke a portal method whose result arrives via the Request.Response
    signal. Returns (code, results) or (None, None) on timeout."""
    _counter[0] += 1
    token = "az%d_%d" % (os.getpid(), _counter[0])
    handle = "/org/freedesktop/portal/desktop/request/%s/%s" % (_sender, token)
    out = {}
    loop = GLib.MainLoop()

    def on_resp(code, results):
        out["code"] = int(code)
        out["results"] = results
        loop.quit()

    match = bus.add_signal_receiver(
        on_resp, signal_name="Response", dbus_interface=req_iface, path=handle
    )
    options = dict(build_options or {})
    options["handle_token"] = token
    method(*args, options)
    GLib.timeout_add_seconds(timeout_s, loop.quit)
    loop.run()
    match.remove()
    return out.get("code"), out.get("results")


def main():
    args = [a for a in sys.argv[1:]]
    if not args:
        sys.exit("usage: azinput-portal.py CMD ARGS [, CMD ARGS]...")

    # 1. CreateSession
    _counter[0] += 1
    stoken = "azsess%d" % os.getpid()
    code, res = call_with_response(
        rd.CreateSession,
        build_options={"session_handle_token": stoken},
    )
    if code != 0:
        sys.exit("CreateSession failed code=%s" % code)
    session = res["session_handle"]

    # 2. SelectDevices (pointer|keyboard = 3), persistent so the grant sticks
    restore = None
    if os.path.exists(TOKEN_FILE):
        restore = open(TOKEN_FILE).read().strip() or None
    sel_opts = {"types": dbus.UInt32(3), "persist_mode": dbus.UInt32(2)}
    if restore:
        sel_opts["restore_token"] = restore
    code, res = call_with_response(rd.SelectDevices, session, build_options=sel_opts)
    if code != 0:
        sys.exit("SelectDevices failed code=%s" % code)

    # 3. Start — may pop a one-time KDE consent dialog. Short timeout so a
    #    blocking dialog doesn't hang the run; exit 3 tells the caller to fall
    #    back to app-level injection.
    code, res = call_with_response(rd.Start, session, "", timeout_s=25)
    if code is None:
        sys.exit(3)  # timed out => dialog awaiting interaction
    if code != 0:
        sys.exit(3)  # denied
    if res and "restore_token" in res:
        os.makedirs(os.path.dirname(TOKEN_FILE), exist_ok=True)
        with open(TOKEN_FILE, "w") as f:
            f.write(str(res["restore_token"]))

    empty = dbus.Dictionary({}, signature="sv")

    def motion(dx, dy):
        rd.NotifyPointerMotion(session, empty, dbus.Double(dx), dbus.Double(dy))

    def button(code_, state):
        rd.NotifyPointerButton(session, empty, dbus.Int32(code_), dbus.UInt32(state))

    def axis(dx, dy):
        rd.NotifyPointerAxis(session, empty, dbus.Double(dx), dbus.Double(dy))

    def key(kc, state):
        rd.NotifyKeyboardKeycode(session, empty, dbus.Int32(kc), dbus.UInt32(state))

    def flush_wait(ms):
        # let the compositor process the queued events
        end = time.time() + ms / 1000.0
        while time.time() < end:
            GLib.MainContext.default().iteration(False)
            time.sleep(0.002)

    # warp to (0,0)
    motion(WARP, WARP)
    flush_wait(60)
    pos = [0.0, 0.0]

    def move_to(x, y):
        motion(x - pos[0], y - pos[1])
        pos[0], pos[1] = x, y

    for cmd in " ".join(args).split(","):
        c = cmd.split()
        if not c:
            continue
        op = c[0]
        if op == "move":
            move_to(float(c[1]), float(c[2])); flush_wait(20)
        elif op == "rel":
            motion(float(c[1]), float(c[2])); pos[0] += float(c[1]); pos[1] += float(c[2]); flush_wait(20)
        elif op == "btn":
            b = BTN[c[1]]
            if c[2] == "down": button(b, 1)
            elif c[2] == "up": button(b, 0)
            else:
                button(b, 1); flush_wait(40); button(b, 0)
            flush_wait(20)
        elif op == "click":
            move_to(float(c[1]), float(c[2])); flush_wait(40)
            button(BTN["left"], 1); flush_wait(40); button(BTN["left"], 0); flush_wait(20)
        elif op == "drag":
            x1, y1, x2, y2 = map(float, c[1:5])
            steps = int(c[5]) if len(c) > 5 else 16
            ms = int(c[6]) if len(c) > 6 else 12
            move_to(x1, y1); flush_wait(60)
            button(BTN["left"], 1); flush_wait(60)
            for i in range(1, steps + 1):
                t = i / steps
                move_to(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t); flush_wait(ms)
            flush_wait(60); button(BTN["left"], 0); flush_wait(20)
        elif op == "wheel":
            axis(0.0, float(c[1])); flush_wait(30)
        elif op == "wheelat":
            move_to(float(c[1]), float(c[2])); flush_wait(40)
            axis(0.0, float(c[3])); flush_wait(30)
        elif op == "key":
            kc = int(c[1])
            if c[2] == "down": key(kc, 1)
            elif c[2] == "up": key(kc, 0)
            else:
                key(kc, 1); flush_wait(40); key(kc, 0)
            flush_wait(20)
        elif op == "sleep":
            flush_wait(int(c[1]))
        else:
            sys.exit("unknown command %r" % op)

    flush_wait(80)
    print("ok")


if __name__ == "__main__":
    main()
