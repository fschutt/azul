#!/usr/bin/env python3
"""Tests of scripts/ci/ios_startup.py: the lines it picks out of a console (python3 -I scripts/ci/test_ios_startup.py)."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ios_startup as s  # noqa: E402

# what the engine prints at the first present (dll/src/desktop/startup_timeline.rs): the table, every line begins `[startup] `, then one JSON line
CONSOLE = """\
2026-11-28 04:50:01 azul: starting
[startup] timeline: process age from kern.proc.pid
[startup] #   at_ms     +ms thread        mark
[startup] 0    0.000   0.000 -             process_start
[startup] 1   38.412  38.412 main          library_entered
[startup] 7  210.100   3.000 main          fonts:cache hit, 791 faces, check 1.9 ms, load 12.4 ms
[startup] 8  220.000   9.900 main          layout_end #1 passes=1
[startup] 9  790.250  50.000 main          first_present ios
[startup] 10 805.000   0.500 worker        fonts:cache_written Written, 797 faces
[startup-json] {"marks": []}
some other line about first_present_ms that is not a row
"""


class LinesTests(unittest.TestCase):
    def test_the_table_is_every_startup_line_and_not_the_json_line(self):
        found = s.startup_lines(CONSOLE)
        self.assertEqual(len(found["table"]), 8)
        self.assertTrue(all(l.startswith("[startup] ") for l in found["table"]))

    def test_the_rows_the_startup_work_reads_are_picked_out(self):
        found = s.startup_lines(CONSOLE)
        self.assertEqual(len(found["layout_end_1"]), 1)
        self.assertIn("passes=1", found["layout_end_1"][0])
        self.assertEqual(len(found["first_present"]), 1, "a line that only names first_present_ms is no row")
        self.assertIn("first_present ios", found["first_present"][0])
        self.assertEqual(len(found["fonts_cache"]), 1, "fonts:cache_written is its own row, not the cache row")
        self.assertIn("hit, 791 faces", found["fonts_cache"][0])
        self.assertEqual(len(found["fonts_cache_written"]), 1)
        self.assertIn("Written, 797 faces", found["fonts_cache_written"][0])

    def test_a_console_without_the_timeline_has_no_rows(self):
        found = s.startup_lines("nothing here\n")
        self.assertEqual(found["table"], [])
        self.assertEqual(found["first_present"], [])

    def test_the_report_says_when_a_row_is_missing(self):
        text = s.render([s.startup_lines("nothing here\n")])
        self.assertIn("NO LINE", text)
        self.assertIn("no [startup] line", text)

    def test_the_report_has_one_section_per_start_with_its_rows(self):
        text = s.render([s.startup_lines(CONSOLE), s.startup_lines(CONSOLE)])
        self.assertIn("## start 1", text)
        self.assertIn("## start 2", text)
        self.assertIn("fonts:cache: ", text)
        self.assertIn("layout end #1 (passes): ", text)


class SourceTests(unittest.TestCase):
    def test_the_app_is_installed_before_its_first_launch(self):
        # the iOS flicker audit recorded the home screen for 80 s because nothing installed the app
        text = (Path(__file__).resolve().parent / "ios_startup.py").read_text(encoding="utf-8")
        body = text[text.index("def main("):]
        self.assertLess(body.index('"simctl", "install"'), body.index("dev.launch("))

    def test_the_timeline_is_switched_on_for_the_launch(self):
        text = (Path(__file__).resolve().parent / "ios_startup.py").read_text(encoding="utf-8")
        self.assertIn('extra_env={"AZ_STARTUP": "1"}', text)


if __name__ == "__main__":
    unittest.main(verbosity=1)
