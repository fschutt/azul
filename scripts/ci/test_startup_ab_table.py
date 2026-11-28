#!/usr/bin/env python3
"""Tests of startup_ab_table.py: blocks of two builds, the warm-up of a hit file left out, the words of the marks."""
import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import startup_ab_table as ab  # noqa: E402


def run(first_present, cache=None, wait=None):
    marks = []
    if cache is not None:
        marks.append({"name": "fonts:cache", "detail": cache})
    if wait is not None:
        marks.append({"name": "fonts:first_layout_wait", "detail": wait})
    return {"i": 1, "exit": 0, "timeline": {"first_present_ms": first_present, "marks": marks}}


def write(directory, name, runs):
    with open(os.path.join(directory, name), "w") as handle:
        json.dump({"summary": {}, "runs": runs}, handle)


class AbTable(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix="ab-table-")

    def test_a_hit_file_drops_its_warm_up_run_and_a_miss_file_keeps_its_run(self):
        write(self.dir, "ab-A-hit-1.json", [run(900, "miss, 0 faces"), run(500, "hit, 8 faces"), run(520, "hit, 8 faces")])
        write(self.dir, "ab-A-miss-1.json", [run(1500, "miss, 0 faces", "waited 400.0 ms")])
        found = ab.collect(self.dir)
        self.assertEqual(len(found[("A", "hit")]), 2)
        self.assertEqual(len(found[("A", "miss")]), 1)

    def test_the_medians_and_the_words_of_the_marks(self):
        write(self.dir, "ab-B-miss-1.json", [run(1000, "miss, 0 faces", "skipped: the DOM's text is drawn by faces in memory")])
        write(self.dir, "ab-B-miss-2.json", [run(1200, "rebuilt (no key file), 0 faces", "skipped: x")])
        write(self.dir, "ab-B-miss-3.json", [run(1400, "miss, 0 faces", "waited 12.0 ms")])
        summary = {k: ab.summarize(v) for k, v in ab.collect(self.dir).items()}
        block = summary[("B", "miss")]
        self.assertEqual(block["median_ms"], 1200)
        self.assertEqual((block["min_ms"], block["max_ms"]), (1000, 1400))
        self.assertEqual(block["font_cache"], {"miss": 2, "rebuilt": 1})
        self.assertEqual(block["first_layout_wait"], {"skipped": 2, "waited": 1})

    def test_a_run_without_a_timeline_counts_as_a_start_but_not_as_a_time(self):
        write(self.dir, "ab-A-miss-1.json", [{"i": 1, "exit": 1, "timeline": None}])
        block = ab.summarize(ab.collect(self.dir)[("A", "miss")])
        self.assertEqual((block["starts"], block["with_a_timeline"], block["median_ms"]), (1, 0, None))

    def test_files_of_other_names_are_left_alone_and_the_table_names_both_builds(self):
        write(self.dir, "ab-A-miss-1.json", [run(1000, "miss, 0 faces", "waited 1.0 ms")])
        write(self.dir, "ab-B-miss-1.json", [run(800, "miss, 0 faces", "skipped")])
        write(self.dir, "other.json", [run(1, "hit")])
        text = ab.table({k: ab.summarize(v) for k, v in ab.collect(self.dir).items()})
        self.assertIn("| A | miss |", text)
        self.assertIn("| B | miss |", text)
        self.assertNotIn("other", text)


if __name__ == "__main__":
    unittest.main()
