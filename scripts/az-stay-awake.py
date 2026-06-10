#!/usr/bin/env python3
"""Hold screensaver + power-management inhibitions while alive.

Belt-and-suspenders for unattended GUI test runs: prevents screen dim,
DPMS off, locking and suspend for as long as this process runs. Kill it
to release everything. (The persistent config on this box already has
autolock and AC-suspend disabled; this guards against the 5-min dim and
anything else.)
"""
import dbus
from gi.repository import GLib

bus = dbus.SessionBus()

ss = dbus.Interface(
    bus.get_object("org.freedesktop.ScreenSaver", "/ScreenSaver"),
    "org.freedesktop.ScreenSaver",
)
ss_cookie = ss.Inhibit("aztest", "autonomous azul GUI verification")

try:
    pm = dbus.Interface(
        bus.get_object("org.freedesktop.PowerManagement.Inhibit",
                       "/org/freedesktop/PowerManagement/Inhibit"),
        "org.freedesktop.PowerManagement.Inhibit",
    )
    pm_cookie = pm.Inhibit("aztest", "autonomous azul GUI verification")
except dbus.DBusException:
    pm_cookie = None  # service absent — ScreenSaver inhibit covers KDE anyway

print(f"inhibitions held: screensaver={ss_cookie} powermgmt={pm_cookie}", flush=True)
GLib.MainLoop().run()
