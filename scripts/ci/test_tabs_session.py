#!/usr/bin/env python3
"""Tests of the tab strip sessions of both phones (scripts/ci/android_qa.sh `tabs`, scripts/ci/ios_qa.sh `tabs`): which scenario files they run by default
(python3 -I scripts/ci/test_tabs_session.py)."""
import re
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
E2E = ROOT / "examples" / "azul-widgets" / "e2e"
DEFAULT = re.compile(r"\$\{AZ_QA_TABS_FILES:-([^}]*)\}")


def default_parts(script):
    """The default AZ_QA_TABS_FILES of a session script, as one list per place that names it (the Android script names it in a comment too)."""
    text = (HERE / script).read_text(encoding="utf-8")
    return [m.group(1).split() for m in DEFAULT.finditer(text)]


def runs(parts, rel):
    """e2e_matrix.py --only: a file runs when its path contains the part."""
    return any(part in rel for part in parts)


class DefaultFilesTests(unittest.TestCase):
    def test_every_tab_scenario_of_azwidgets_runs_in_both_phone_sessions(self):
        files = sorted(p.name for p in E2E.glob("tab_*.json"))
        self.assertIn("tab_content_toggle.json", files)
        for script in ("android_qa.sh", "ios_qa.sh"):
            places = default_parts(script)
            self.assertTrue(places, script)
            for parts in places:
                for name in files:
                    self.assertTrue(runs(parts, f"examples/azul-widgets/e2e/{name}"), f"{script} does not run {name} by default")

    def test_a_part_names_exactly_one_file_so_no_file_runs_twice(self):
        files = sorted(p.name for p in E2E.glob("tab_*.json"))
        for script in ("android_qa.sh", "ios_qa.sh"):
            for parts in default_parts(script):
                for part in parts:
                    hits = [n for n in files if part in f"examples/azul-widgets/e2e/{n}"]
                    self.assertEqual(len(hits), 1, f"{script}: '{part}' names {hits}")

    def test_both_phones_run_the_same_files(self):
        android, ios = default_parts("android_qa.sh"), default_parts("ios_qa.sh")
        self.assertTrue(all(sorted(a) == sorted(ios[0]) for a in android), (android, ios))


if __name__ == "__main__":
    unittest.main(verbosity=2)
