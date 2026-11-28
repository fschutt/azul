#!/usr/bin/env python3
"""Tests of scripts/ci/android_probe.py: the pure parts and a whole run on a fake device (python3 -I scripts/ci/test_android_probe.py)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import android_probe as p  # noqa: E402

LOG = ("11-28 02:10:01.100  100  120 I RustStdoutStderr: [  310000us][Info][EventLoop] [Android] android_main entered\n"
       "11-28 02:10:02.200  100  120 I RustStdoutStderr: AZ_SELFTEST_REPORT {\"probes\": 12, \"failed\": 0}\n"
       "11-28 02:10:02.300  100  120 I RustStdoutStderr: [startup] first_present 612 ms\n"
       "11-28 02:10:02.400  100  120 I RustStdoutStderr: [fonts] cache hit (check 3 ms, load 9 ms)\n")

DIALOG_XML = ('<hierarchy><node index="0" text="Allow Camera?" resource-id="com.android.permissioncontroller:id/permission_message" '
              'package="com.android.permissioncontroller" class="android.widget.TextView" bounds="[100,800][980,900]"/>'
              '<node index="1" text="While using the app" resource-id="com.android.permissioncontroller:id/permission_allow_foreground_only_button" '
              'package="com.android.permissioncontroller" class="android.widget.Button" clickable="true" bounds="[100,1000][980,1120]"/></hierarchy>')


class PureTests(unittest.TestCase):
    def test_the_marker_line_comes_without_the_logcat_prefix(self):
        self.assertEqual(p.marker_lines(LOG, "AZ_SELFTEST_REPORT"), ['AZ_SELFTEST_REPORT {"probes": 12, "failed": 0}'])
        self.assertEqual(p.marker_lines(LOG, "NOT_THERE"), [])
        self.assertEqual(p.marker_lines(LOG, ""), [])

    def test_the_timeline_keeps_the_startup_and_font_lines(self):
        lines = p.timeline_lines(LOG)
        self.assertEqual(lines, ["[startup] first_present 612 ms", "[fonts] cache hit (check 3 ms, load 9 ms)"])

    def test_a_permission_dialog_is_found_by_its_focus_or_its_dump(self):
        self.assertIsNotNone(p.permission_dialog_in("mCurrentFocus=Window{1 u0 com.android.permissioncontroller/com.android.permissioncontroller.permission.ui.GrantPermissionsActivity}", "", "com.azul.AzSelfTest"))
        self.assertIsNotNone(p.permission_dialog_in("mCurrentFocus=Window{1 u0 com.azul.AzSelfTest/com.azul.app.AzulActivity}", DIALOG_XML, "com.azul.AzSelfTest"))
        self.assertIsNone(p.permission_dialog_in("mCurrentFocus=Window{1 u0 com.azul.AzSelfTest/com.azul.app.AzulActivity}", "<hierarchy/>", "com.azul.AzSelfTest"))

    def test_a_button_is_found_by_text_or_resource_id(self):
        self.assertEqual(p.find_node(DIALOG_XML, resource_id="com.android.permissioncontroller:id/permission_allow_foreground_only_button"), (540, 1060))
        self.assertEqual(p.find_node(DIALOG_XML, text="While using the app"), (540, 1060))
        self.assertIsNone(p.find_node(DIALOG_XML, text="Deny"))

    def test_the_dialog_log_keeps_a_finding_once_and_a_new_one_next_to_it(self):
        log = p.DialogLog()
        for t, what in ((0.5, None), (1.0, "x"), (1.7, "x"), (2.4, None), (3.1, "x")):
            log.observe(t, what)
        self.assertEqual(log.seen, [(1.0, "x")], "the same finding is kept once, with the time it first showed")
        log.observe(4.0, "y")
        self.assertEqual(log.seen, [(1.0, "x"), (4.0, "y")])


class FakeDev:
    """a device whose log gets the marker at the third look and whose focus is the app's (or the permission controller's when asked)"""
    def __init__(self, dialog_at_poll=None):
        self.looks, self.polls, self.dialog_at_poll, self.calls = 0, 0, dialog_at_poll, []

    def adb(self, *a, timeout=90):
        self.calls.append(a)
        if a[:1] == ("install",):
            return "Success"
        if a[:2] == ("logcat", "-d") and "crash" not in a:
            self.looks += 1
            return LOG if self.looks >= 3 else "11-28 02:10:01.100  100  120 I RustStdoutStderr: nothing yet\n"
        return ""

    def sh(self, command, timeout=90):
        self.calls.append(("sh", command))
        if "getprop ro.build.version.sdk" in command:
            return "34\n"
        if "getprop ro.product.cpu.abi" in command:
            return "x86_64\n"
        if command.startswith("pidof"):
            return "1234\n"
        return ""

    def focus(self):
        self.polls += 1
        if self.dialog_at_poll and self.polls >= self.dialog_at_poll:
            return "mCurrentFocus=Window{1 u0 com.android.permissioncontroller/com.android.permissioncontroller.permission.ui.GrantPermissionsActivity}"
        return "mCurrentFocus=Window{1 u0 com.azul.AzSelfTest/com.azul.app.AzulActivity}"

    def dump_ui(self, path=""):
        return "<hierarchy/>"


class RunTests(unittest.TestCase):
    def run_probe(self, dev, **overrides):
        import time as _t
        out = tempfile.mkdtemp(prefix="android-probe-test-")
        args = type("A", (), dict(apk="", package="com.azul.AzSelfTest", activity="com.azul.app.AzulActivity", out=out, prop=["debug.az.selftest=1"], env="",
                                  marker="AZ_SELFTEST_REPORT", seconds=20.0, starts=1, clear_data=False, watch_permissions=True, startup=False, serial=""))()
        for k, v in overrides.items():
            setattr(args, k, v)
        real_dev, real_sleep, real_poll = p.Dev, _t.sleep, p.POLL_SECONDS
        p.Dev = lambda serial="": dev
        _t.sleep = lambda s: None
        real_settle = p.MARKER_SETTLE_SECONDS
        p.MARKER_SETTLE_SECONDS = 0.0
        try:
            rc = p.run_probe(args)
        finally:
            p.Dev, _t.sleep, p.POLL_SECONDS, p.MARKER_SETTLE_SECONDS = real_dev, real_sleep, real_poll, real_settle
        return rc, Path(out)

    def test_an_unattended_run_with_the_marker_and_no_dialog_passes(self):
        dev = FakeDev()
        rc, out = self.run_probe(dev)
        self.assertEqual(rc, 0)
        self.assertEqual((out / "marker.txt").read_text().strip(), 'AZ_SELFTEST_REPORT {"probes": 12, "failed": 0}')
        report = json.loads((out / "probe.json").read_text())
        self.assertTrue(report["marker_found"])
        self.assertEqual(report["permission_dialogs"], [])
        self.assertIn(("sh", "setprop debug.az.selftest '1'"), dev.calls)
        self.assertTrue((out / "start-1.log").exists())

    def test_a_dialog_that_showed_fails_the_run_whatever_the_marker_says(self):
        rc, out = self.run_probe(FakeDev(dialog_at_poll=2))
        self.assertEqual(rc, 4)
        report = json.loads((out / "probe.json").read_text())
        self.assertEqual(len(report["permission_dialogs"]), 1)
        self.assertIn("permission", report["permission_dialogs"][0]["what"].lower())

    def test_a_marker_that_never_comes_is_exit_3(self):
        rc, _out = self.run_probe(FakeDev(), marker="NEVER_LOGGED", seconds=0.2)
        self.assertEqual(rc, 3)

    def test_two_starts_keep_their_timelines_apart(self):
        rc, out = self.run_probe(FakeDev(), starts=2, startup=True, clear_data=True, marker="AZ_SELFTEST_REPORT")
        self.assertEqual(rc, 0)
        for n in (1, 2):
            self.assertTrue((out / f"start-{n}.timeline.txt").exists())
            self.assertTrue((out / f"start-{n}.cache.txt").exists())
        self.assertEqual(json.loads((out / "probe.json").read_text())["clear_data"], "")


if __name__ == "__main__":
    unittest.main(verbosity=1)
