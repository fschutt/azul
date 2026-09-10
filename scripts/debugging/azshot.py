#!/usr/bin/env python3
"""Silent Wayland screenshot via the xdg-desktop-portal Screenshot API.

Works on KWin/Plasma (and any compositor with a portal backend) without
root, provided the permission-store entry screenshot/screenshot = yes for
the host app id "" (set once via busctl; see scripts/wayland-testing.md).

Usage: azshot.py [output.png]   (default /tmp/azshot.png)
Exit 0 on success, 1 on failure/timeout (e.g. a consent dialog appeared).
"""
import os
import sys
import shutil
import urllib.parse

import dbus
import dbus.mainloop.glib
from gi.repository import GLib

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/azshot.png"
TIMEOUT_S = 12

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
portal = bus.get_object("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop")
iface = dbus.Interface(portal, "org.freedesktop.portal.Screenshot")

loop = GLib.MainLoop()
result = {}


def on_response(code, results):
    result["code"] = int(code)
    result["uri"] = str(results.get("uri", ""))
    loop.quit()


# Subscribe broadly to Request.Response — only our request fires in this window.
bus.add_signal_receiver(
    on_response,
    signal_name="Response",
    dbus_interface="org.freedesktop.portal.Request",
)

token = "azshot%d" % os.getpid()
iface.Screenshot("", {"handle_token": token, "interactive": dbus.Boolean(False)})
GLib.timeout_add_seconds(TIMEOUT_S, loop.quit)
loop.run()

code = result.get("code", -1)
uri = result.get("uri", "")
if code == 0 and uri.startswith("file://"):
    src = urllib.parse.unquote(uri[7:])
    shutil.copy(src, OUT)
    os.unlink(src)  # don't litter ~/Pictures
    print("saved %s" % OUT)
    sys.exit(0)

print("FAILED code=%d uri=%r (timeout => consent dialog?)" % (code, uri), file=sys.stderr)
sys.exit(1)
