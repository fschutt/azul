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


class DisplayFeaturesTests(unittest.TestCase):
    """The iOS session keeps what the app's window knows of the phone's display (the cutouts the strip builds its lanes around the island from)."""

    TEXT = (HERE / "ios_qa.sh").read_text(encoding="utf-8")

    def test_the_plain_start_and_every_extra_app_are_asked_for_their_display_features(self):
        self.assertEqual(self.TEXT.count('"op":"get_display_features"'), 2)
        self.assertIn("display-features-start.json", self.TEXT)
        self.assertIn("$xn-display-features.json", self.TEXT)
        self.assertEqual(self.TEXT.count("SIMCTL_CHILD_AZ_DEBUG=8797"), 2)

    def test_the_simulators_hardware_keyboard_is_disconnected_on_a_runner_only(self):
        line = next(l for l in self.TEXT.splitlines() if "ConnectHardwareKeyboard" in l and "defaults write" in l)
        self.assertIn('"${GITHUB_ACTIONS:-}" = "true"', line)       # on the user's Mac it is the user's preference
        self.assertLess(self.TEXT.index("ConnectHardwareKeyboard"), self.TEXT.index("simctl create azul-qa"))

    def test_the_script_is_valid_shell(self):
        import subprocess
        self.assertEqual(subprocess.run(["bash", "-n", str(HERE / "ios_qa.sh")]).returncode, 0)


class TextSizeTests(unittest.TestCase):
    """The tab sessions of both phones take a picture at the largest system text size, for the plain start and for every extra app (AzWriter), so that
    'the engine ignores the system text size' is measured on more than one app."""

    def test_android_takes_font_scale_1_3_and_2_0_for_the_plain_start_and_the_extra_apps(self):
        text = (HERE / "android_qa.sh").read_text(encoding="utf-8")
        self.assertIn('device-fontscale-$fs.png', text)
        self.assertIn('$xn-fontscale-$fs.png', text)
        self.assertEqual(text.count("for fs in 1.3 2.0; do"), 2)
        self.assertGreaterEqual(text.count("settings put system font_scale 1.0"), 2)        # put back after each of the two loops

    def test_ios_takes_the_largest_accessibility_size_and_puts_the_default_back(self):
        text = (HERE / "ios_qa.sh").read_text(encoding="utf-8")
        self.assertEqual(text.count("content_size accessibility-extra-extra-extra-large"), 2)
        self.assertGreaterEqual(text.count("content_size large"), 2)        # after the plain start, after each extra app (and after the matrix's sizes)
        self.assertIn("device-text-xxxl.png", text)
        self.assertIn("$xn-text-xxxl.png", text)

    def test_the_scripts_are_valid_shell(self):
        import subprocess
        for name in ("android_qa.sh", "ios_qa.sh"):
            self.assertEqual(subprocess.run(["bash", "-n", str(HERE / name)]).returncode, 0, name)


class MatrixTextSizeTests(unittest.TestCase):
    """The matrix sessions of both phones can run the same scenario files once per system text size (AzMail's text_size.json): AZ_QA_TEXT_SIZES."""

    def test_android_sets_the_font_scale_before_each_run_and_puts_1_0_back(self):
        text = (HERE / "android_qa.sh").read_text(encoding="utf-8")
        self.assertIn('for ts in $AZ_QA_TEXT_SIZES; do', text)
        self.assertIn('settings put system font_scale "$ts"', text)
        self.assertIn('--out "$d/matrix-text-$ts"', text)
        self.assertLess(text.index('settings put system font_scale "$ts"'), text.index('--out "$d/matrix-text-$ts"'))

    def test_ios_sets_the_content_size_before_each_run_and_puts_large_back(self):
        text = (HERE / "ios_qa.sh").read_text(encoding="utf-8")
        self.assertIn('for ts in $AZ_QA_TEXT_SIZES; do', text)
        self.assertIn('content_size "$ts"', text)
        self.assertIn('--out "$out/text-$ts"', text)
        self.assertIn("content_size large", text)

    def test_the_plain_matrix_is_what_runs_without_the_variable(self):
        for name in ("android_qa.sh", "ios_qa.sh"):
            text = (HERE / name).read_text(encoding="utf-8")
            self.assertIn("else", text[text.index("AZ_QA_TEXT_SIZES"):])


class TextSizeJobTests(unittest.TestCase):
    TEXT = (HERE / "text_size_job.sh").read_text(encoding="utf-8")

    def test_the_job_builds_the_e2e_variant_and_runs_the_matrix_once_per_size(self):
        self.assertIn("mobile-e2e", self.TEXT)
        self.assertIn('AZ_QA_TEXT_SIZES="$sizes"', self.TEXT)
        self.assertIn('AZ_QA_MATRIX_ONLY="$part"', self.TEXT)
        self.assertIn("1.0 1.3 2.0", self.TEXT)
        self.assertIn("large accessibility-extra-extra-extra-large", self.TEXT)

    def test_the_script_is_valid_shell(self):
        import subprocess
        self.assertEqual(subprocess.run(["bash", "-n", str(HERE / "text_size_job.sh")]).returncode, 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
