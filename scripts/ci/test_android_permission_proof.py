#!/usr/bin/env python3
"""The pure parts of scripts/ci/android_permission_proof.py (the rest needs an emulator).

    python3 scripts/ci/test_android_permission_proof.py
"""
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import android_font_cache_proof as fonts  # noqa: E402
import android_permission_proof as proof  # noqa: E402


def report_line(rows):
    return "11-28 02:10:11.000  123  456 I RustStdoutStderr: AZ_SELFTEST_REPORT " + json.dumps({"app": "AzSelfTest", "rows": rows})


class ReportRowsTest(unittest.TestCase):
    def test_the_rows_of_the_newest_report_are_the_answer(self):
        first = report_line([{"id": "sensors.geolocation", "verdict": "NEEDS-HUMAN", "detail": "answer the dialog"}])
        second = report_line([{"id": "sensors.geolocation", "verdict": "PASS", "detail": "lat=48.52 lon=8.67"}])
        rows = proof.report_rows("noise\n" + first + "\nmore noise\n" + second + "\n")
        self.assertEqual(rows["sensors.geolocation"], ("PASS", "lat=48.52 lon=8.67"))

    def test_a_report_in_numbered_parts_is_read_like_the_one_line(self):
        """The unattended run prints a long report as AZ_SELFTEST_REPORT_PART i/n lines: logcat keeps ~4000 bytes of a line."""
        text = json.dumps({"app": "AzSelfTest", "rows": [{"id": "sensors.geolocation", "verdict": "PASS", "detail": "lat=48.52 lon=8.67"}]})
        slices = [text[at:at + 40] for at in range(0, len(text), 40)]
        lines = [f"11-28 02:10:11.000  123  456 I RustStdoutStderr: AZ_SELFTEST_REPORT_PART {i + 1}/{len(slices)} {s}" for i, s in enumerate(slices)]
        self.assertGreater(len(lines), 2)
        rows = proof.report_rows("noise\n" + "\n".join(lines) + "\nmore noise\n")
        self.assertEqual(rows["sensors.geolocation"], ("PASS", "lat=48.52 lon=8.67"))

    def test_no_report_or_a_cut_line_is_no_rows(self):
        self.assertEqual(proof.report_rows("nothing here"), {})
        self.assertEqual(proof.report_rows("AZ_SELFTEST_REPORT {\"rows\": ["), {})


class FakeDev:
    """Taps recorded, nodes of a uiautomator dump by resource id."""

    def __init__(self, ids):
        self.ids = ids
        self.taps = []

    def tap_node(self, xml, text=None, resource_id=None):
        if resource_id in self.ids:
            self.taps.append(resource_id)
            return True
        return False


class TapAnyTest(unittest.TestCase):
    def test_the_first_button_the_dialog_has_is_tapped_and_only_one(self):
        dev = FakeDev({"com.android.permissioncontroller:id/permission_allow_foreground_only_button"})
        self.assertTrue(proof.tap_any(dev, "<xml/>", proof.ALLOW_IDS))
        self.assertEqual(dev.taps, ["com.android.permissioncontroller:id/permission_allow_foreground_only_button"])

    def test_a_dialog_without_the_button_is_not_answered(self):
        dev = FakeDev({"com.android.permissioncontroller:id/permission_deny_button"})
        self.assertFalse(proof.tap_any(dev, "<xml/>", proof.ALLOW_IDS))
        self.assertEqual(dev.taps, [])


LOG = """11-28 02:10:11.001  100  100 I RustStdoutStderr: [startup] +180.2 ms fonts:cache miss, 0 faces, check 1.4 ms, load 0.0 ms
11-28 02:10:11.402  100  100 I RustStdoutStderr: [startup] +582.9 ms fonts:cache_written Written, 231 faces, 38.5 ms
11-28 02:10:11.500  100  100 I RustStdoutStderr: something else
"""


class FontLinesTest(unittest.TestCase):
    def test_the_font_lines_of_a_start_and_what_they_say(self):
        lines = fonts.font_lines(LOG)
        self.assertEqual(len(lines), 2)
        verdict, line = fonts.verdict_of(lines)
        self.assertEqual(verdict, "miss")
        self.assertEqual(fonts.numbers_of(line), {"faces": 0, "check_ms": 1.4, "load_ms": 0.0})
        self.assertEqual(fonts.written_of(lines), "Written")

    def test_a_hit_and_a_rebuild(self):
        hit = ["fonts:cache hit, 231 faces, check 2.1 ms, load 9.8 ms", "fonts:cache_written Unchanged"]
        self.assertEqual(fonts.verdict_of(hit)[0], "hit")
        self.assertEqual(fonts.numbers_of(hit[0]), {"faces": 231, "check_ms": 2.1, "load_ms": 9.8})
        self.assertEqual(fonts.written_of(hit), "Unchanged")
        self.assertEqual(fonts.verdict_of(["fonts:cache rebuilt (manifest is not the one the key file describes)"])[0], "rebuilt")

    def test_a_start_without_a_cache_line_has_no_verdict(self):
        self.assertEqual(fonts.verdict_of(fonts.font_lines("nothing about fonts")), (None, ""))
        self.assertIsNone(fonts.written_of([]))


if __name__ == "__main__":
    unittest.main()
