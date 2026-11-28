#!/usr/bin/env python3
"""Tests of scripts/ci/font_wait_series.py: what a start printed, the plan's block order, the commands of both runners (python3 -I scripts/ci/test_font_wait_series.py)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import font_wait_series as f  # noqa: E402

MARKS_HIT = [
    {"i": 11, "name": "fonts:cache", "at_ms": 599.4, "since_library_ms": 105.8, "thread": "main", "detail": "hit, 354 faces, check 27.5 ms, load 62.8 ms"},
    {"i": 25, "name": "fonts:cache_written", "at_ms": 935.9, "since_library_ms": 442.3, "thread": "azul-font-cache-persis", "detail": "Unchanged, 354 disk faces, 7.8 ms"},
    {"i": 26, "name": "layout_phases", "at_ms": 1105.0, "since_library_ms": 611.4, "thread": "main", "detail": "[phases] total 337.0ms | after_layout_and_dl 180.7ms"},
    {"i": 30, "name": "fonts:first_layout_wait", "at_ms": 770.0, "since_library_ms": 276.4, "thread": "main", "detail": "skipped: faces in memory draw every character"},
]
MARKS_MISS = [
    {"i": 11, "name": "fonts:cache", "detail": "miss, 0 faces, check 0.4 ms, load 0.0 ms"},
    {"i": 25, "name": "fonts:cache_written", "detail": "Written, 354 disk faces, 88.7 ms"},
    {"i": 26, "name": "layout_phases", "detail": "[phases] total 1012.9ms | after_shared_cache 488.8ms"},
    {"i": 30, "name": "fonts:first_layout_wait", "detail": "waited 489 ms"},
]


def console(first_present, marks, cut_json=False):
    head = {"enabled": True, "first_present_ms": first_present, "before_library_ms": 493.61, "layout_passes": 1, "render_backend": "cpu"}
    def detail(name):
        return next((m["detail"] for m in marks if m["name"] == name), None)
    lines = ["2026-11-28 AzWidgets[1:2] [render] backend=cpu", "[startup] timeline: process start from sysctl",
             "[startup]  11    599.413     0.025  main                   fonts:cache " + detail("fonts:cache"),
             "[startup]  25    935.896         -  azul-font-cache-persis fonts:cache_written " + detail("fonts:cache_written"),
             "[startup]  26   1105.062   337.088  main                   layout_phases " + detail("layout_phases")]
    if detail("fonts:first_layout_wait") is not None:
        lines.append("[startup]  30    770.000     1.000  main                   fonts:first_layout_wait " + detail("fonts:first_layout_wait"))
    lines += ["[startup]  29   1151.355     3.210  main                   first_present ios", "[startup-json] " + json.dumps(head)]
    if not cut_json:
        lines.append("[startup-marks] " + json.dumps(marks))
    return "\n".join(lines) + "\n"


class ParseTests(unittest.TestCase):
    def test_a_hit_start_with_the_lever(self):
        row = f.parse_start(console(1151.355, MARKS_HIT))
        self.assertEqual(row["first_present_ms"], 1151.355)
        self.assertEqual(row["cache_kind"], "hit")
        self.assertEqual((row["wait_kind"], row["wait_ms"]), ("skipped", None))
        self.assertEqual(row["layout_ms"], 337.0)
        self.assertTrue(row["written"].startswith("Unchanged"))

    def test_a_miss_start_that_waited(self):
        row = f.parse_start(console(3147.6, MARKS_MISS))
        self.assertEqual((row["cache_kind"], row["wait_kind"], row["wait_ms"]), ("miss", "waited", 489.0))
        self.assertEqual(row["layout_ms"], 1012.9)

    def test_the_table_lines_stand_in_when_the_marks_line_was_cut(self):
        row = f.parse_start(console(3147.6, MARKS_MISS, cut_json=True))
        self.assertEqual((row["cache_kind"], row["wait_kind"], row["wait_ms"], row["layout_ms"]), ("miss", "waited", 489.0, 1012.9))
        self.assertTrue(row["written"].startswith("Written"))

    def test_a_logcat_prefix_does_not_matter(self):
        text = "".join("11-28 09:10:07.400  3000  3020 I RustStdoutStderr: " + l + "\n" for l in console(900.0, MARKS_HIT, cut_json=True).splitlines())
        row = f.parse_start(text)
        self.assertEqual((row["first_present_ms"], row["cache_kind"], row["wait_kind"]), (900.0, "hit", "skipped"))

    def test_a_build_without_the_lever_has_no_wait_mark(self):
        marks = [m for m in MARKS_MISS if m["name"] != "fonts:first_layout_wait"]
        row = f.parse_start(console(3000.0, marks))
        self.assertEqual((row["wait_kind"], row["wait"]), ("none", None))

    def test_the_cache_row_is_not_confused_with_the_cache_written_row(self):
        self.assertEqual(f.table_detail("[startup]  25    935.896         -  azul-font-cache-persis fonts:cache_written Unchanged, 1 disk faces, 7.8 ms", "fonts:cache"), None)

    def test_a_start_without_a_timeline_is_none(self):
        self.assertIsNone(f.parse_start("nothing here\n"))
        self.assertIsNone(f.parse_start(""))


class SummaryTests(unittest.TestCase):
    def rows(self, values, wait="skipped"):
        return [{"first_present_ms": v, "layout_ms": v / 4, "wait_kind": wait, "cache_kind": "hit"} for v in values] + [None]

    def test_the_median_min_max_and_the_counts(self):
        s = f.summarize(self.rows([900, 1100, 1000]))
        self.assertEqual((s["n"], s["asked"], s["median"], s["min"], s["max"]), (3, 4, 1000, 900, 1100))
        self.assertEqual(s["waits"], {"skipped": 3})
        self.assertEqual(s["layout_median"], 250.0)

    def test_an_empty_block_has_no_numbers(self):
        s = f.summarize([None, None])
        self.assertEqual((s["n"], s["median"]), (0, None))


class PlanTests(unittest.TestCase):
    PLAN = "control:A=a1.apk;control:B=b1.apk;bundled:A=a2.apk;bundled:B=b2.apk"

    def test_the_blocks_are_miss_and_hit_of_a_then_of_b_per_app_variant(self):
        got = [(v, b, m) for v, b, _p, m in f.blocks_of(f.parse_plan(self.PLAN))]
        self.assertEqual(got, [("control", "A", "miss"), ("control", "A", "hit"), ("control", "B", "miss"), ("control", "B", "hit"),
                               ("bundled", "A", "miss"), ("bundled", "A", "hit"), ("bundled", "B", "miss"), ("bundled", "B", "hit")])

    def test_a_malformed_entry_is_refused(self):
        for bad in ("control=a.apk", "control:A", ":A=a.apk"):
            with self.assertRaises(ValueError):
                f.parse_plan(bad)
        self.assertEqual(f.parse_plan(""), [])


class BlockTests(unittest.TestCase):
    def test_a_miss_block_clears_before_the_warm_up_and_before_every_start_and_a_hit_block_never_does(self):
        calls = []
        for mode, expected in (("miss", [True, True, True]), ("hit", [False, False, False])):
            calls.clear()
            installed = []
            with tempfile.TemporaryDirectory() as d:
                r = f.run_block(lambda clear: (calls.append(clear), console(1000.0, MARKS_HIT))[1], installed.append, "control", "A", "a.apk", mode, 2, d, log=lambda *_: None)
                self.assertEqual(calls, expected)               # 1 warm-up or fill + 2 counted
                self.assertEqual(installed, ["a.apk"])
                self.assertEqual(r["summary"]["n"], 2)
                self.assertTrue((Path(d) / "starts" / f"control-A-{mode}-01.txt").exists())

    def test_the_kept_lines_of_a_start_are_the_timeline_lines(self):
        with tempfile.TemporaryDirectory() as d:
            f.run_block(lambda clear: "noise line\n" + console(1000.0, MARKS_HIT), lambda p: None, "control", "A", "a.apk", "hit", 1, d, log=lambda *_: None)
            kept = (Path(d) / "starts" / "control-A-hit-01.txt").read_text()
            self.assertIn("[startup-json]", kept)
            self.assertNotIn("noise line", kept)

    def test_the_report_has_the_summary_and_every_start(self):
        with tempfile.TemporaryDirectory() as d:
            res = [f.run_block(lambda clear: console(1000.0, MARKS_HIT), lambda p: None, "control", "A", "a.apk", "hit", 2, d, log=lambda *_: None)]
        text = f.report_markdown(res, "test")
        self.assertIn("| control | A | hit | 2 of 2 | 1000.0 |", text)
        self.assertIn("### control A hit", text)
        self.assertIn("skipped: faces in memory", text)
        self.assertIn("| hit 2 |", text)                       # the cache marks seen: a HIT block saw hits
        self.assertNotIn("NOT CLEAN", text)

    def test_a_miss_block_that_saw_a_hit_says_it_is_not_clean(self):
        with tempfile.TemporaryDirectory() as d:
            res = [f.run_block(lambda clear: console(1000.0, MARKS_HIT), lambda p: None, "control", "A", "a.apk", "miss", 2, d, log=lambda *_: None)]
        self.assertIn("NOT CLEAN", f.report_markdown(res))


class FakeClock:
    def __init__(self):
        self.now = 0.0

    def sleep(self, s):
        self.now += s

    def time(self):
        return self.now


class AndroidRunnerTests(unittest.TestCase):
    def run_start(self, clear):
        log = []
        clock = FakeClock()

        def run(argv, timeout=120):
            log.append(" ".join(argv))
            if "logcat" in argv and "-d" in argv:
                return 0, console(1000.0, MARKS_HIT)
            return 0, ""
        prepare, start = f.android_runner("com.azul.AzWidgets", "com.azul.app.AzulActivity", run=run, sleep=clock.sleep, clock=clock.time)
        prepare("a.apk")
        log.append("--- start")
        text = start(clear)
        return log, text

    def test_a_clearing_start_runs_pm_clear_between_the_stop_and_the_launch(self):
        log, text = self.run_start(True)
        after = log[log.index("--- start") + 1:]
        stop = next(i for i, l in enumerate(after) if "am force-stop" in l)
        clear = next(i for i, l in enumerate(after) if "pm clear com.azul.AzWidgets" in l)
        launch = next(i for i, l in enumerate(after) if "am start" in l)
        self.assertLess(stop, clear)
        self.assertLess(clear, launch)
        self.assertIn("[startup-json]", text)

    def test_a_plain_start_clears_nothing(self):
        log, _ = self.run_start(False)
        self.assertFalse(any("pm clear" in l for l in log))
        self.assertTrue(any("am start -n com.azul.AzWidgets/com.azul.app.AzulActivity" in l for l in log))

    def test_the_timeline_is_switched_on_once_per_install(self):
        log, _ = self.run_start(False)
        self.assertEqual(sum("setprop debug.az.startup 1" in l for l in log), 1)
        self.assertTrue(any("install -r -t a.apk" in l for l in log))

    def test_the_package_is_uninstalled_before_a_build_of_another_signature_goes_on(self):
        log, _ = self.run_start(False)
        before_start = log[:log.index("--- start")]
        self.assertLess(next(i for i, l in enumerate(before_start) if "uninstall com.azul.AzWidgets" in l), next(i for i, l in enumerate(before_start) if "install -r" in l))

    def test_the_logcat_is_cleared_before_the_launch(self):
        log, _ = self.run_start(False)
        after = log[log.index("--- start") + 1:]
        self.assertLess(next(i for i, l in enumerate(after) if "logcat -c" in l), next(i for i, l in enumerate(after) if "am start" in l))


class IosRunnerTests(unittest.TestCase):
    class Proc:
        def wait(self, timeout=None):
            return 0

        def kill(self):
            pass

    def run_start(self, clear):
        log = []
        clock = FakeClock()
        sample = console(1151.0, MARKS_HIT)

        def run(argv, timeout=120):
            log.append(" ".join(argv))
            return 0, ""

        def popen(argv, stdout=None, stderr=None, env=None):
            log.append(" ".join(argv))
            log.append("env " + str(env.get("SIMCTL_CHILD_AZ_STARTUP")))
            stdout.write(sample)
            stdout.flush()
            return self.Proc()
        with tempfile.TemporaryDirectory() as d:
            prepare, start = f.ios_runner("UDID", "com.azul.AzWidgets", d, run=run, sleep=clock.sleep, clock=clock.time, popen=popen)
            prepare("a.app")
            log.append("--- start")
            text = start(clear)
        return log[log.index("--- start") + 1:], text

    def test_a_clearing_start_uninstalls_and_installs_again_before_the_launch(self):
        log, text = self.run_start(True)
        joined = "\n".join(log)
        self.assertLess(joined.index("simctl uninstall UDID com.azul.AzWidgets"), joined.index("simctl install UDID a.app"))
        self.assertLess(joined.index("simctl install UDID a.app"), joined.index("simctl launch --console UDID com.azul.AzWidgets"))
        self.assertIn("env 1", joined)
        self.assertIn("[startup-json]", text)

    def test_a_plain_start_only_stops_the_app_first(self):
        log, _ = self.run_start(False)
        self.assertFalse(any("uninstall" in l for l in log))
        self.assertTrue(log[0].startswith("xcrun simctl terminate UDID"))


class JobScriptTests(unittest.TestCase):
    """The two runner jobs and the two sessions that carry the A/B (the build jobs per commit, one measure job per phone)."""

    HERE = Path(__file__).resolve().parent

    def text(self, name):
        return (self.HERE / name).read_text(encoding="utf-8")

    def test_the_scripts_are_valid_shell(self):
        import subprocess
        for name in ("fontwait_build.sh", "fontwait_measure.sh", "android_qa.sh", "ios_qa.sh"):
            self.assertEqual(subprocess.run(["bash", "-n", str(self.HERE / name)]).returncode, 0, name)

    def test_the_build_job_makes_a_control_and_a_variant_with_only_bundled_families(self):
        text = self.text("fontwait_build.sh")
        self.assertIn("build_apps control", text)
        self.assertIn("build_apps bundled", text)
        self.assertLess(text.index("build_apps control"), text.index("font-family: EB Garamond"))      # the control is built from the unedited source
        self.assertIn('replace("font-family: system:monospace", "font-family: EB Garamond")', text)
        self.assertIn('replace("font-family: system:ui", "font-family: EB Garamond")', text)

    def test_the_measure_job_reads_both_builds_and_passes_the_plan_with_the_control_first(self):
        text = self.text("fontwait_measure.sh")
        self.assertIn('plan="control:A=', text)
        self.assertIn(";bundled:A=", text)
        self.assertIn("fontwait", text)

    def test_both_sessions_hand_the_plan_to_the_series_script(self):
        for name, platform in (("android_qa.sh", "android"), ("ios_qa.sh", "ios")):
            text = self.text(name)
            self.assertIn(f'font_wait_series.py" {platform}', text)
            self.assertIn("AZ_QA_FONTWAIT_PLAN", text)
            self.assertIn("AZ_QA_FONTWAIT_STARTS", text)

    def test_the_lever_marks_are_the_ones_the_startup_agent_named(self):
        self.assertEqual(f.WAIT_MARK, "fonts:first_layout_wait")
        self.assertEqual(f.CACHE_MARK, "fonts:cache")
        self.assertEqual(f.WRITTEN_MARK, "fonts:cache_written")


if __name__ == "__main__":
    unittest.main(verbosity=2)
