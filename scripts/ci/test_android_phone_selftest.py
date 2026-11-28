#!/usr/bin/env python3
"""The pure parts of scripts/ci/android_phone_selftest.py: reading the report line out of a log, the table, who has to do what.

    python3 scripts/ci/test_android_phone_selftest.py
"""
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import android_phone_selftest as t  # noqa: E402

ROW = {"id": "sensors.accelerometer", "group": "sensors", "title": "Accelerometer", "verdict": "PASS", "detail": "x=0.1 y=9.8", "ms": 120}
REPORT = {"app": "AzSelfTest", "platform": "android", "arch": "aarch64", "total_ms": 5000, "pass": 1, "unavailable": 0, "denied": 0,
          "fail": 1, "needs_human": 1,  "pending": 0,
          "rows": [ROW,
                   {**ROW, "id": "security.keyring", "verdict": "FAIL", "detail": "store answered Error"},
                   {**ROW, "id": "system.haptics", "verdict": "NEEDS-HUMAN", "detail": "did the phone buzz?"}]}


class ReportLineTest(unittest.TestCase):
    def test_the_report_is_found_in_a_log_between_other_lines(self):
        log = "11-28 01:02:03.000 I RustStdoutStderr: something else\n" \
              f"11-28 01:02:04.000 I RustStdoutStderr: {t.REPORT_PREFIX} {json.dumps(REPORT)}\n" \
              "11-28 01:02:05.000 I RustStdoutStderr: after\n"
        found = t.reports_in(log)
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["rows"][1]["id"], "security.keyring")

    def test_every_report_of_the_log_comes_back_in_order_and_a_cut_line_is_skipped(self):
        first = {**REPORT, "total_ms": 1}
        second = {**REPORT, "total_ms": 2}
        log = f"{t.REPORT_PREFIX} {json.dumps(first)}\n{t.REPORT_PREFIX} {{\"rows\": [\n{t.REPORT_PREFIX} {json.dumps(second)}\n"
        self.assertEqual([r["total_ms"] for r in t.reports_in(log)], [1, 2])

    def parts_of(self, report, size):
        """`report` as the lines the app prints when it is longer than a logcat line: AZ_SELFTEST_REPORT_PART i/n <slice>."""
        text = json.dumps(report, separators=(",", ":"), ensure_ascii=False)
        slices = [text[at:at + size] for at in range(0, len(text), size)]
        return [f"11-28 01:02:04.000 100 200 I RustStdoutStderr: {t.PART_PREFIX} {i + 1}/{len(slices)} {s}" for i, s in enumerate(slices)]

    def test_a_report_in_numbered_parts_is_put_together(self):
        """Mutation: reading only the one-line form (the report of an emulator run was cut at ~4000 bytes and every later row was lost)."""
        report = {**REPORT, "rows": [{**ROW, "id": f"group.row_{n:02}", "detail": "Grüße 日本 " * 6} for n in range(40)]}
        lines = self.parts_of(report, 700)
        self.assertGreater(len(lines), 3)
        log = "noise\n" + "\n".join(lines[:2]) + "\nunrelated line in between\n" + "\n".join(lines[2:]) + "\nmore noise\n"
        found = t.reports_in(log)
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0], report)

    def test_a_blank_at_the_end_of_a_part_and_a_carriage_return_survive(self):
        report = {**REPORT, "rows": [{**ROW, "detail": "a " * 400}]}
        lines = self.parts_of(report, 100)
        found = t.reports_in("\r\n".join(lines) + "\r\n")
        self.assertEqual(found, [report])

    def test_parts_and_lines_come_back_in_the_order_of_the_log(self):
        first = {**REPORT, "total_ms": 1}
        second = {**REPORT, "total_ms": 2}
        third = {**REPORT, "total_ms": 3}
        log = f"{t.REPORT_PREFIX} {json.dumps(first)}\n" + "\n".join(self.parts_of(second, 200)) + f"\n{t.REPORT_PREFIX} {json.dumps(third)}\n"
        self.assertEqual([r["total_ms"] for r in t.reports_in(log)], [1, 2, 3])

    def test_a_set_of_parts_that_lost_one_is_dropped_not_guessed(self):
        report = {**REPORT, "total_ms": 7}
        lines = self.parts_of(report, 150)
        del lines[2]
        self.assertEqual(t.reports_in("\n".join(lines) + "\n"), [])
        # a later complete report is still found
        later = {**REPORT, "total_ms": 8}
        self.assertEqual([r["total_ms"] for r in t.reports_in("\n".join(lines) + "\n" + "\n".join(self.parts_of(later, 150)) + "\n")], [8])
        # a one-line report in the middle of a set breaks the set
        mixed = lines[:2] + [f"{t.REPORT_PREFIX} {json.dumps(later)}"] + lines[3:]
        self.assertEqual([r["total_ms"] for r in t.reports_in("\n".join(mixed) + "\n")], [8])


class TableTest(unittest.TestCase):
    def test_the_table_has_a_line_per_row_and_the_counts(self):
        lines = t.table(REPORT)
        self.assertEqual(len(lines), 4)
        self.assertTrue(lines[0].startswith("PASS"))
        self.assertIn("1 fail", lines[-1])
        self.assertIn("android aarch64", lines[-1])

    def test_only_a_fail_is_a_failure_and_a_person_is_asked_for_needs_human(self):
        self.assertEqual(t.failures(REPORT), ["security.keyring"])
        self.assertEqual(t.people_to_do(REPORT), [("system.haptics", "did the phone buzz?")])
        self.assertEqual(t.failures({"rows": [{**ROW, "verdict": "UNAVAILABLE"}, {**ROW, "verdict": "DENIED"}]}), [])


class PermissionDialogTest(unittest.TestCase):
    def test_the_permission_controller_in_front_is_seen_on_every_flavour_of_android(self):
        for line in (
            "  mCurrentFocus=Window{4f2d1a1 u0 com.google.android.permissioncontroller/com.android.permissioncontroller.permission.ui.GrantPermissionsActivity}",
            "  mCurrentFocus=Window{9a1 u0 com.android.permissioncontroller/com.android.permissioncontroller.permission.ui.GrantPermissionsActivity}",
            "  mFocusedWindow=Window{77 u0 com.samsung.android.permissioncontroller/x.GrantPermissionsActivity}",
        ):
            self.assertTrue(t.permission_dialog_focused(line), line)

    def test_the_app_the_launcher_or_nothing_in_front_is_no_dialog(self):
        for line in (
            "  mCurrentFocus=Window{4f2d1a1 u0 com.azul.AzSelfTest/com.azul.app.AzulActivity}",
            "  mCurrentFocus=Window{1 u0 com.google.android.apps.nexuslauncher/com.google.android.apps.nexuslauncher.NexusLauncherActivity}",
            "  mCurrentFocus=null",
            "",
        ):
            self.assertFalse(t.permission_dialog_focused(line), line)
        # a permission controller that is NOT the focus (a task list mentioning it) is not a dialog in front
        self.assertFalse(t.permission_dialog_focused("  mFocusedApp=ActivityRecord{1 u0 com.google.android.permissioncontroller/x t5}"))


if __name__ == "__main__":
    unittest.main()
