#!/usr/bin/env python3
"""Tests of scripts/ci/e2e_matrix.py: pure functions on the engine's real report format (python3 -I scripts/ci/test_e2e_matrix.py)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import e2e_matrix as m  # noqa: E402

# what the engine's `render_report` prints (layout/src/e2e/report.rs): colours around the verdict, the failures block, the summary
REPORT = (
    "\n"
    "test counter_increments ... \x1b[32mPASS\x1b[0m (12 ms)\n"
    "test the_hotkey_row ... \x1b[33mSKIP\x1b[0m (3 ms) (this test holds on linux, macos, windows only)\n"
    "test waits_for_a_part ... \x1b[33mPENDING\x1b[0m (0 ms) (waits for: a command the app has not got)\n"
    "test a_known_bug ... \x1b[33mXFAIL\x1b[0m (9 ms) (expected failure)\n"
    "test form_submits ... \x1b[31mFAIL\x1b[0m (40 ms)\n"
    "\n"
    "failures:\n\n"
    "---- form_submits (FAIL) ----\n"
    "  step 4: click → FAILED: no node with the text \"Submit\"\n"
    "  step 7: assert_text → FAILED: expected 5, got 6\n"
    "\n"
    "failures:\n"
    "    form_submits (FAIL)\n"
    "\n"
    "test result: \x1b[31mFAILED\x1b[0m. 1 passed; 1 failed; 1 xfailed; 0 xpassed; 1 skipped; 1 pending; 0 measured; 0 filtered out\n"
)


class ParseTests(unittest.TestCase):
    def test_every_verdict_and_its_reason_are_read(self):
        r = m.parse_report(REPORT)
        self.assertEqual(r["counter_increments"][0], "PASS")
        self.assertEqual(r["the_hotkey_row"], ("SKIP", "this test holds on linux, macos, windows only"))
        self.assertEqual(r["waits_for_a_part"], ("PENDING", "waits for: a command the app has not got"))
        self.assertEqual(r["a_known_bug"][0], "XFAIL")

    def test_a_fail_carries_its_first_failing_step(self):
        verdict, detail = m.parse_report(REPORT)["form_submits"]
        self.assertEqual(verdict, "FAIL")
        self.assertEqual(detail, 'step 4: click -> FAILED: no node with the text "Submit"')

    def test_logcat_prefixes_are_stripped(self):
        text = "11-27 23:19:37.900  4243  4267 I RustStdoutStderr: test a_test ... \x1b[32mPASS\x1b[0m (5 ms)\n"
        self.assertEqual(m.parse_report(text), {"a_test": ("PASS", "")})

    def test_the_summary_counts_are_read(self):
        self.assertEqual(m.summary_counts(REPORT), (1, 1))
        self.assertIsNone(m.summary_counts("nothing"))

    def test_text_without_a_report_gives_nothing(self):
        self.assertEqual(m.parse_report("[azul] logging enabled\nsome line"), {})
        self.assertEqual(m.parse_report(None), {})


class DiscoverTests(unittest.TestCase):
    def make_repo(self):
        root = Path(tempfile.mkdtemp(prefix="e2e-matrix-test-"))
        (root / "tests/e2e").mkdir(parents=True)
        (root / "examples/azul-widgets/e2e").mkdir(parents=True)
        (root / "tests/e2e/self.json").write_text(json.dumps([{"name": "mounts", "steps": [{"op": "mount", "html": "<p>x</p>"}, {"op": "wait_frame"}]}]))
        (root / "tests/e2e/hello_world_counter.json").write_text(json.dumps({"name": "counter", "steps": [{"op": "click", "text": "Increase counter"}]}))
        (root / "tests/e2e/azwriter_boot.json").write_text(json.dumps([{"name": "boot", "steps": []}]))
        (root / "examples/azul-widgets/e2e/tab.json").write_text(json.dumps([{"name": "tabs", "only_on": ["linux"], "steps": [{"op": "wait_frame"}]}]))
        (root / "tests/e2e/broken.json").write_text("{ not json")
        return root

    def test_files_tests_hosts_and_gates_are_found(self):
        found = {s["rel"]: s for s in m.discover(self.make_repo())}
        self.assertEqual(sorted(found), ["examples/azul-widgets/e2e/tab.json", "tests/e2e/azwriter_boot.json",
                                         "tests/e2e/hello_world_counter.json", "tests/e2e/self.json"])       # the broken one is skipped
        self.assertEqual(found["tests/e2e/self.json"]["host"], "AzWidgets")                 # it mounts its own DOM: any app hosts it
        self.assertTrue(found["tests/e2e/self.json"]["self_mounted"])
        self.assertEqual(found["tests/e2e/azwriter_boot.json"]["host"], "AzWriter")
        self.assertIsNone(found["tests/e2e/hello_world_counter.json"]["host"])              # asserts a desktop C example's DOM
        self.assertEqual(found["examples/azul-widgets/e2e/tab.json"]["tests"][0]["only_on"], ["linux"])

    def test_host_for_the_real_file_names(self):
        self.assertEqual(m.host_for("examples/azul-widgets/e2e/video_card.json", False), "AzWidgets")
        self.assertEqual(m.host_for("examples/azul-mail/e2e/text_size.json", False), "AzMail")
        self.assertEqual(m.host_for("tests/e2e/css23_proof.json", True), "AzWidgets")
        self.assertIsNone(m.host_for("tests/e2e/resume_http_get.json", False))


class TableTests(unittest.TestCase):
    SCN = [{"rel": "a.json", "host": "AzWidgets", "tests": [{"name": "t1"}, {"name": "t2"}]},
           {"rel": "b.json", "host": None, "tests": [{"name": "t3"}]}]

    def records(self):
        desktop = [{"file": "a.json", "test": "t1", "platform": "desktop", "verdict": "PASS", "detail": ""},
                   {"file": "a.json", "test": "t2", "platform": "desktop", "verdict": "PASS", "detail": ""}]
        android = [{"file": "a.json", "test": "t1", "platform": "android", "verdict": "PASS", "detail": ""},
                   {"file": "a.json", "test": "t2", "platform": "android", "verdict": "FAIL", "detail": "step 2: click -> FAILED: no node"}]
        phones_without_host = m.not_applicable(self.SCN[1], "android", m.NO_PHONE_HOST_REASON)
        return [desktop, android + phones_without_host]

    def test_a_row_per_test_and_a_cell_per_platform(self):
        table = m.merge(self.records(), self.SCN)
        text = m.render_markdown(table, self.SCN)
        self.assertIn("| a.json | t1 | PASS | not run | PASS | not run |", text)
        self.assertIn("| a.json | t2 | PASS | not run | FAIL: step 2: click -> FAILED: no node | not run |", text)
        self.assertIn("| b.json | t3 | not run | not run | N/A: asserts the DOM of a desktop C example", text)
        self.assertIn("desktop (in-process runner)", text)

    def test_a_test_whose_platforms_disagree_is_listed_with_each_verdict(self):
        text = m.render_markdown(m.merge(self.records(), self.SCN), self.SCN)
        self.assertIn("| a.json | t1 | PASS | not run | PASS | not run | same |", text)
        self.assertIn("| a.json | t2 | PASS | not run | FAIL: step 2: click -> FAILED: no node | not run | DIFFERENT |", text)
        self.assertIn("## Where the verdicts differ", text)
        self.assertIn("- a.json :: t2: desktop PASS, android FAIL", text)

    def test_the_totals_count_every_verdict_per_platform(self):
        text = m.render_markdown(m.merge(self.records(), self.SCN), self.SCN)
        self.assertIn("## Totals", text)
        self.assertIn("| android | 1 | 1 | 1 | 0 |", text)  # FAIL 1, N/A 1, PASS 1, not run 0 (sorted verdict names)

    def test_a_failure_only_a_phone_has_is_listed_for_a_verdict(self):
        table = m.merge(self.records(), self.SCN)
        self.assertEqual([(f, t, p, v) for f, t, p, v, _d in m.phone_only_failures(table)], [("a.json", "t2", "android", "FAIL")])

    def test_a_failure_the_desktop_has_too_is_not_phone_only(self):
        records = self.records()
        records[0][1] = {"file": "a.json", "test": "t2", "platform": "desktop", "verdict": "FAIL", "detail": "x"}
        self.assertEqual(m.phone_only_failures(m.merge(records, self.SCN)), [])


class RunnerColumnTests(unittest.TestCase):
    def test_the_in_process_run_is_its_own_column_next_to_the_apps_run(self):
        scns = [{"rel": "c.json", "file": "/nonexistent.json", "host": "AzWidgets", "self_mounted": True, "tests": [{"name": "t1"}]}]
        records = [{"file": "c.json", "test": "t1", "platform": "runner", "verdict": "FAIL", "detail": "step 6: assert_layout -> FAILED: height"},
                   {"file": "c.json", "test": "t1", "platform": "desktop", "verdict": "PASS", "detail": ""}]
        text = m.render_markdown(m.merge([records], scns), scns)
        self.assertIn("| c.json | t1 | PASS | FAIL: step 6: assert_layout -> FAILED: height | not run | not run | DIFFERENT |", text)
        self.assertIn("- c.json :: t1: desktop PASS, runner FAIL", text)


class RecordsTests(unittest.TestCase):
    def test_a_test_the_report_does_not_name_has_no_verdict_with_the_reason(self):
        scn = {"rel": "a.json", "tests": [{"name": "counter_increments"}, {"name": "never_ran"}]}
        records = m.records_for(scn, "android", REPORT, 0.0, "the app exited without a verdict")
        self.assertEqual([r["verdict"] for r in records], ["PASS", "NO VERDICT"])
        self.assertEqual(records[1]["detail"], "the app exited without a verdict")

    def test_not_applicable_records_say_why(self):
        recs = m.not_applicable({"rel": "b.json", "tests": [{"name": "x"}, {"name": "y"}]}, "ios", "no host")
        self.assertEqual([(r["verdict"], r["detail"], r["platform"]) for r in recs], [("N/A", "no host", "ios")] * 2)


class ShotFolderTests(unittest.TestCase):
    def test_the_shot_folder_of_every_file_fits_the_system_property_that_carries_it(self):
        # `setprop debug.az.env 'AZ_E2E_SHOT_DIR=<path>'`: a system property's value holds 91 characters; a longer one is refused and the
        # engine then writes its frames nowhere (nothing to pull)
        root = Path(__file__).resolve().parents[2]
        stems = [Path(s["rel"]).stem for s in m.discover(str(root))]
        self.assertTrue(stems, "no scenario file found")
        for package in ("com.azul.AzWidgets", "com.azul.AzCompanion"):
            for stem in stems:
                value = "AZ_E2E_SHOT_DIR=" + m.android_shot_dir(package, stem)
                self.assertLessEqual(len(value), 91, value)

    def test_the_frames_of_a_file_are_in_the_apps_own_cache_folder(self):
        self.assertEqual(m.android_shot_dir("com.azul.AzWidgets", "tab_touch_phone"), "/data/user/0/com.azul.AzWidgets/cache/tab_touch_phone")


if __name__ == "__main__":
    unittest.main(verbosity=1)
