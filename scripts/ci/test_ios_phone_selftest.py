#!/usr/bin/env python3
"""Tests of scripts/ci/ios_phone_selftest.py: the report reader and the verdict (standard library only; no phone needed).

    python3 scripts/ci/test_ios_phone_selftest.py
"""
import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import ios_phone_selftest as st  # noqa: E402


def row(rid, verdict, detail="", title=None):
    return {"id": rid, "group": rid.split(".")[0], "title": title or rid, "verdict": verdict, "detail": detail, "ms": 5}


def report(rows):
    counts = {v: sum(1 for r in rows if r["verdict"] == v) for v in ("PASS", "UNAVAILABLE", "DENIED", "FAIL", "NEEDS-HUMAN", "PENDING")}
    return {"app": "AzSelfTest", "platform": "ios", "unattended": True, "pass": counts["PASS"], "unavailable": counts["UNAVAILABLE"],
            "denied": counts["DENIED"], "fail": counts["FAIL"], "needs_human": counts["NEEDS-HUMAN"], "pending": counts["PENDING"], "rows": rows}


GOOD = [row(rid, "PASS", "ok") for rid in st.PHONE_MUST_PASS]


class Parse(unittest.TestCase):
    def test_the_report_line_is_found_among_other_console_lines(self):
        line = "AZ_SELFTEST_REPORT " + json.dumps(report(GOOD))
        text = "2026-11-28 02:00:00 AzSelfTest[123] some log\n" + line + "\nmore\n"
        found = st.parse_reports(text)
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["app"], "AzSelfTest")

    def test_a_broken_line_is_skipped_and_the_last_report_is_the_newest(self):
        text = "AZ_SELFTEST_REPORT {not json\n" + "AZ_SELFTEST_REPORT " + json.dumps(report([row("a.b", "PASS")])) + "\n" \
            + "AZ_SELFTEST_REPORT " + json.dumps(report([row("c.d", "PASS")])) + "\n"
        found = st.parse_reports(text)
        self.assertEqual([r["rows"][0]["id"] for r in found], ["a.b", "c.d"])

    def test_no_report_is_an_empty_list(self):
        self.assertEqual(st.parse_reports("nothing here\n"), [])


class Verdict(unittest.TestCase):
    def test_a_phone_that_passes_what_it_must_is_clean(self):
        code, lines = st.verdict(report(GOOD))
        self.assertEqual(code, 0)
        self.assertIn("counts:", "\n".join(lines))

    def test_a_fail_is_a_bug_and_exits_one(self):
        code, lines = st.verdict(report(GOOD + [row("security.keyring", "FAIL", "unlock answered Error")]))
        self.assertEqual(code, 1)
        self.assertTrue(any(l.startswith("FAIL security.keyring") for l in lines))

    def test_a_motion_sensor_that_says_unavailable_on_a_real_phone_is_a_finding(self):
        rows = [row(rid, "PASS") for rid in st.PHONE_MUST_PASS if rid != "sensors.gyroscope"] + [row("sensors.gyroscope", "UNAVAILABLE", "no gyroscope")]
        code, lines = st.verdict(report(rows))
        self.assertEqual(code, 1, "the Simulator may say this; a phone may not")
        self.assertTrue(any("FINDING sensors.gyroscope" in l for l in lines))

    def test_a_row_the_app_does_not_have_is_missing_not_a_pass(self):
        rows = [row(rid, "PASS") for rid in st.PHONE_MUST_PASS if rid != "system.haptics"]
        code, lines = st.verdict(report(rows))
        self.assertEqual(code, 1)
        self.assertTrue(any(l.startswith("MISSING system.haptics") for l in lines))

    def test_a_refused_permission_says_where_to_allow_it(self):
        rows = [row(rid, "PASS") for rid in st.PHONE_MUST_PASS if rid != "media.camera"] + [row("media.camera", "DENIED", "camera refused")]
        code, lines = st.verdict(report(rows))
        self.assertEqual(code, 1)
        self.assertTrue(any("Settings" in l for l in lines))

    def test_the_instructions_of_the_human_rows_are_listed_in_plain_words(self):
        rows = GOOD + [row("sensors.proximity", "NEEDS-HUMAN", "cover the top of the phone with your hand", title="Proximity")]
        code, lines = st.verdict(report(rows))
        text = "\n".join(lines)
        self.assertEqual(code, 0, "a row that needs a person is neither a pass nor a failure")
        self.assertIn("run again with --attended", text)
        self.assertIn("cover the top of the phone with your hand", text)


class ReportIn(unittest.TestCase):
    def test_a_console_with_the_report_line_is_done(self):
        self.assertTrue(st.report_in('noise\nAZ_SELFTEST_REPORT {"rows": []}\n'))

    def test_a_console_without_it_or_with_half_of_it_is_not(self):
        self.assertFalse(st.report_in("noise only"))
        self.assertFalse(st.report_in('AZ_SELFTEST_REPORT {"rows": ['))


class Bundle(unittest.TestCase):
    def test_an_explicit_bundle_wins(self):
        self.assertEqual(st.bundle_of("whatever.app", "com.example.x"), "com.example.x")

    def test_the_app_names_its_own_bundle(self):
        import plistlib
        import tempfile

        with tempfile.TemporaryDirectory() as d:
            (pathlib.Path(d) / "Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier": "com.azul.azul_selftest"}))
            self.assertEqual(st.bundle_of(d, None), "com.azul.azul_selftest")

    def test_without_an_app_or_a_plist_it_is_the_default(self):
        self.assertEqual(st.bundle_of(None, None), "com.azul.AzSelfTest")
        self.assertEqual(st.bundle_of("/nonexistent.app", None), "com.azul.AzSelfTest")


if __name__ == "__main__":
    unittest.main()
