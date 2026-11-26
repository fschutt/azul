#!/usr/bin/env python3
"""Tests of scripts/ci/startup_measure.py (std only): python3 scripts/ci/test_startup_measure.py"""
import json
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import startup_measure as sm  # noqa: E402

HEADLINE = {"enabled": True, "process_start_known": True, "before_library_ms": 12.5, "layout_passes": 2, "first_present_ms": 301.25}
MARKS = [
    {"i": 1, "name": "library_entered", "at_ms": 12.5, "since_library_ms": 0.0, "thread": "main", "detail": ""},
    {"i": 2, "name": "font_scan_complete", "at_ms": 100.0, "since_library_ms": 87.5, "thread": "azul-startup-fonts", "detail": "fonts=768"},
    {"i": 3, "name": "layout_end", "at_ms": 150.0, "since_library_ms": 137.5, "thread": "main", "detail": "#1"},
    {"i": 4, "name": "first_present", "at_ms": 301.25, "since_library_ms": 288.75, "thread": "main", "detail": ""},
]


def lines(*parts):
    return "\n".join(parts)


class ParseTimeline(unittest.TestCase):
    def test_the_headline_and_the_marks_of_two_lines_are_one_timeline(self):
        text = lines(
            "[startup] timeline: ...",
            "[startup-json] " + json.dumps(HEADLINE),
            "[startup-marks] " + json.dumps(MARKS),
        )
        timeline = sm.parse_timeline(text)
        self.assertEqual(timeline["first_present_ms"], 301.25)
        self.assertEqual([m["name"] for m in timeline["marks"]][-1], "first_present")

    def test_a_logcat_prefix_before_the_marker_is_ignored(self):
        text = "11-26 06:59:33.142  2535  2561 I RustStdoutStderr: [startup-json] " + json.dumps(HEADLINE)
        self.assertEqual(sm.parse_timeline(text)["layout_passes"], 2)

    def test_the_old_single_line_with_the_marks_inside_still_reads(self):
        old = dict(HEADLINE, marks=MARKS)
        self.assertEqual(len(sm.parse_timeline("[startup-json] " + json.dumps(old))["marks"]), 4)

    def test_no_line_and_a_broken_line_are_no_timeline(self):
        self.assertIsNone(sm.parse_timeline("nothing here"))
        self.assertIsNone(sm.parse_timeline("[startup-json] {\"first_present_ms\": 3"))

    def test_a_headline_without_marks_has_an_empty_list(self):
        self.assertEqual(sm.parse_timeline("[startup-json] " + json.dumps(HEADLINE))["marks"], [])


class ParseTimelines(unittest.TestCase):
    def test_a_log_of_three_starts_gives_three_timelines_in_order(self):
        starts = []
        for i, first in enumerate((310.5, 305.25, 330.0)):
            head = dict(HEADLINE, first_present_ms=first)
            marks = [dict(m, at_ms=m["at_ms"] + i) for m in MARKS]
            starts.append("11-26 06:59:3%d.1  2535  2561 I RustStdoutStderr: [startup-json] %s" % (i, json.dumps(head)))
            starts.append("11-26 06:59:3%d.1  2535  2561 I RustStdoutStderr: [startup-marks] %s" % (i, json.dumps(marks)))
            starts.append("11-26 06:59:3%d.2  2535  2561 I other: noise" % i)
        timelines = sm.parse_timelines("\n".join(starts))
        self.assertEqual([t["first_present_ms"] for t in timelines], [310.5, 305.25, 330.0])
        self.assertTrue(all(len(t["marks"]) == 4 for t in timelines))
        self.assertEqual(timelines[2]["marks"][0]["at_ms"], 14.5)

    def test_a_cut_marks_line_leaves_the_timeline_without_marks(self):
        text = "[startup-json] " + json.dumps(HEADLINE) + "\n[startup-marks] [{\"i\": 1, \"name\": \"libr"
        timelines = sm.parse_timelines(text)
        self.assertEqual(len(timelines), 1)
        self.assertEqual(timelines[0]["marks"], [])

    def test_the_old_single_line_with_the_marks_inside_is_one_timeline(self):
        text = "[startup-json] " + json.dumps(dict(HEADLINE, marks=MARKS))
        timelines = sm.parse_timelines(text)
        self.assertEqual(len(timelines[0]["marks"]), 4)

    def test_a_text_without_a_timeline_has_none(self):
        self.assertEqual(sm.parse_timelines("nothing\nhere"), [])


class Steps(unittest.TestCase):
    def test_a_step_counts_from_the_previous_mark_of_the_same_thread(self):
        steps = {name: step for name, _thread, _at, step, _detail in sm.with_steps(MARKS)}
        self.assertIsNone(steps["library_entered"])
        self.assertIsNone(steps["font_scan_complete"], "the first mark of a thread has no step")
        self.assertEqual(steps["layout_end"], 137.5)
        self.assertEqual(steps["first_present"], 151.25)

    def test_median_ignores_missing_values(self):
        self.assertEqual(sm.median([None, 3.0, 1.0, 2.0]), 2.0)
        self.assertIsNone(sm.median([None]))


FAKE_APP = """
import json, sys, time
mode = sys.argv[1]
print("noise on stdout")
if mode != "silent":
    print("[startup-json] " + json.dumps({HEADLINE!r}), file=sys.stderr, flush=True)
    print("[startup-marks] " + json.dumps({MARKS!r}), file=sys.stderr, flush=True)
if mode == "exits":
    sys.exit(0)
time.sleep(60)
"""


class RunOnce(unittest.TestCase):
    """An app that exits after its first frame is waited for; one that does not is ended after its timeline."""

    def run_fake(self, mode, timeout=20.0):
        source = FAKE_APP.replace("{HEADLINE!r}", repr(HEADLINE)).replace("{MARKS!r}", repr(MARKS))
        return sm.run_once([sys.executable, "-c", source, mode], dict(os.environ), None, timeout, grace=0.3)

    def test_an_app_that_exits_by_itself_keeps_its_exit_code(self):
        code, wall_ms, text = self.run_fake("exits")
        self.assertEqual(code, 0)
        self.assertEqual(sm.parse_timeline(text)["first_present_ms"], 301.25)
        self.assertLess(wall_ms, 10_000)

    def test_an_app_that_never_exits_is_ended_a_moment_after_its_timeline(self):
        code, wall_ms, text = self.run_fake("stays")
        self.assertEqual(code, "ended-after-timeline")
        self.assertEqual(sm.parse_timeline(text)["layout_passes"], 2)
        self.assertLess(wall_ms, 15_000, "it did not wait for the sixty seconds of the app")

    def test_an_app_that_prints_no_timeline_runs_into_the_timeout(self):
        code, _wall_ms, text = self.run_fake("silent", timeout=1.0)
        self.assertEqual(code, "timeout")
        self.assertIsNone(sm.parse_timeline(text))


if __name__ == "__main__":
    unittest.main()
