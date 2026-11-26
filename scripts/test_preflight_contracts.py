#!/usr/bin/env python3
"""Tests of the demo-naming contract of scripts/preflight_contracts.py: it reads scripts/demo_apps.toml (the
one list of the demo apps), holds every app's package, [[bin]] and asset stem to one AzXxx string, and keeps
no list of apps and no assertion about the text of rust.yml.

    python3 scripts/test_preflight_contracts.py        (standard library only; the comparison with tomllib
                                                         runs where Python is 3.11 or later)
"""
import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import preflight_contracts as pf  # noqa: E402

REAL_ROOT = pf.ROOT

SAMPLE = '''
schema = 1
[lib.desktop]
features = "dist"
package = "NotAnApp"           # a key of a table that is no app

# the apps
[[app]]
package = "AzOne"              # first
dir = "examples/azul-one"
description = "dir = \\"not this\\""
platforms = ["linux"]
[app.android]
permissions = [
    "android.permission.CAMERA",
]
[app.ios.strings.de]
package = "NotThisEither"

[[app]]
dir = "examples/azul-two"
package = "AzTwo"
known_failing = { android = "why" }

[[app]]
package = "AzThree"
dir = "examples/azul-three"
[app.known_failing]
ios = "no bin"

[plan]
dir = "elsewhere"
'''


def cargo(package, bins=()):
    text = f'[package]\nname = "{package}"\nversion = "0.1.0"\n\n[dependencies]\n'
    for b in bins:
        text += f'\n[[bin]]\nname = "{b}"\npath = "src/main.rs"\n'
    return text


class ReaderTest(unittest.TestCase):
    def test_the_package_and_the_dir_of_each_app_are_read_and_the_pending_ones_are_marked(self):
        self.assertEqual(pf.parse_demo_manifest(SAMPLE), [
            {"package": "AzOne", "dir": "examples/azul-one", "pending": False},
            {"package": "AzTwo", "dir": "examples/azul-two", "pending": True},
            {"package": "AzThree", "dir": "examples/azul-three", "pending": True},
        ])

    def test_the_names_of_a_cargo_manifest_are_the_package_and_every_bin(self):
        text = cargo("AzMail", ["AzMail", "azmail-send"]) + '\n[lib]\nname = "azul_mail"\n'
        self.assertEqual(pf.cargo_names(text), ("AzMail", ["AzMail", "azmail-send"]))
        self.assertEqual(pf.cargo_names(cargo("AzX")), ("AzX", []))
        self.assertEqual(pf.cargo_names("[lib]\nname = \"x\"\n"), (None, []))

    @unittest.skipUnless(sys.version_info >= (3, 11), "tomllib is Python 3.11+")
    def test_the_line_reader_agrees_with_tomllib_on_the_real_manifest(self):
        import tomllib

        text = (REAL_ROOT / "scripts" / "demo_apps.toml").read_text(encoding="utf-8")
        real = tomllib.loads(text)["app"]
        got = pf.parse_demo_manifest(text)
        self.assertEqual(
            got,
            [{"package": a["package"], "dir": a["dir"], "pending": bool(a.get("known_failing"))} for a in real],
        )
        self.assertGreater(len(got), 30)

    @unittest.skipUnless(sys.version_info >= (3, 11), "tomllib is Python 3.11+")
    def test_the_cargo_reader_agrees_with_tomllib_on_every_app(self):
        import tomllib

        for app in pf.parse_demo_manifest((REAL_ROOT / "scripts" / "demo_apps.toml").read_text(encoding="utf-8")):
            text = (REAL_ROOT / app["dir"] / "Cargo.toml").read_text(encoding="utf-8")
            real = tomllib.loads(text)
            self.assertEqual(
                pf.cargo_names(text),
                (real["package"]["name"], [b["name"] for b in real.get("bin", [])]),
                app["package"],
            )


class CheckTest(unittest.TestCase):
    """check_demo_naming() on trees made for the case (it reads pf.ROOT)."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = pathlib.Path(self.tmp.name)
        self.saved = (pf.ROOT, list(pf.FAILURES))
        pf.ROOT = self.root
        pf.FAILURES.clear()
        self.addCleanup(self.restore)

    def restore(self):
        pf.ROOT = self.saved[0]
        pf.FAILURES[:] = self.saved[1]

    def tree(self, apps, extra=""):
        """apps: [(package, dir, cargo text or None, pending)]"""
        manifest = "schema = 1\n"
        for package, d, text, pending in apps:
            manifest += f'\n[[app]]\npackage = "{package}"\ndir = "{d}"\ndescription = "x"\nplatforms = ["linux"]\n'
            if pending:
                manifest += '[app.known_failing]\nandroid = "no cdylib yet"\n'
            if text is not None:
                (self.root / d).mkdir(parents=True, exist_ok=True)
                (self.root / d / "Cargo.toml").write_text(text, encoding="utf-8")
        (self.root / "scripts").mkdir(exist_ok=True)
        (self.root / "scripts" / "demo_apps.toml").write_text(manifest + extra, encoding="utf-8")

    def failures(self):
        pf.FAILURES.clear()
        pf.check_demo_naming()
        return list(pf.FAILURES)

    def test_a_tree_that_agrees_passes_without_any_workflow_file(self):
        # (the check used to read rust.yml for a list of five apps; there is no .github here at all)
        self.tree([("AzOne", "examples/azul-one", cargo("AzOne", ["AzOne"]), False),
                   ("AzTwo", "examples/azul-two", cargo("AzTwo"), False)])
        self.assertFalse((self.root / ".github").exists())
        self.assertEqual(self.failures(), [])

    def test_the_real_tree_passes_and_every_app_of_the_manifest_is_checked(self):
        pf.ROOT = REAL_ROOT
        self.assertEqual(self.failures(), [])

    def test_a_binary_not_named_after_the_package_fails(self):
        self.tree([("AzWriter", "examples/azul-writer", cargo("AzWriter", ["azwriter"]), False)])
        fails = self.failures()
        self.assertEqual(len(fails), 1, fails)
        self.assertIn("AzWriter builds no binary named 'AzWriter'", fails[0])
        self.assertIn("['azwriter']", fails[0])

    def test_one_bin_named_after_the_package_is_enough_for_an_app_with_helpers(self):
        self.tree([("AzMail", "examples/azul-mail", cargo("AzMail", ["AzMail", "azmail-sanitize", "azmail-send"]), False)])
        self.assertEqual(self.failures(), [])

    def test_an_app_whose_build_is_a_known_failure_is_not_held_to_a_binary_it_lacks(self):
        self.tree([("AzCompanion", "examples/azul-companion", cargo("AzCompanion", ["companion_sim"]), True)])
        self.assertEqual(self.failures(), [])
        self.tree([("AzCompanion", "examples/azul-companion", cargo("AzCompanion", ["companion_sim"]), False)])
        self.assertEqual(len(self.failures()), 1)

    def test_a_package_that_differs_from_the_manifest_fails(self):
        self.tree([("AzOne", "examples/azul-one", cargo("AzOnee"), False)])
        fails = self.failures()
        self.assertEqual(len(fails), 1, fails)
        self.assertIn("declares package 'AzOnee'", fails[0])

    def test_a_missing_crate_a_missing_manifest_and_a_name_off_the_convention_fail(self):
        self.tree([("AzGone", "examples/azul-gone", None, False)])
        self.assertIn("examples/azul-gone/Cargo.toml is missing", self.failures()[0])
        self.tree([("azbad", "examples/azul-bad", cargo("azbad"), False)])
        self.assertTrue(any("breaks the AzXxx convention" in f for f in self.failures()))
        (self.root / "scripts" / "demo_apps.toml").unlink()
        self.assertIn("is the one list of the demo apps", self.failures()[0])

    def test_an_app_listed_twice_and_an_empty_manifest_fail(self):
        self.tree([("AzOne", "examples/azul-one", cargo("AzOne"), False),
                   ("AzOne", "examples/azul-one", None, False)])
        self.assertTrue(any("listed twice" in f for f in self.failures()))
        self.tree([])
        self.assertIn("read 0 apps for 0 [[app]] tables", self.failures()[0])

    def test_the_check_keeps_no_list_of_apps_of_its_own(self):
        source = (HERE / "preflight_contracts.py").read_text(encoding="utf-8")
        self.assertNotIn("DEMO_DIRS", source)
        self.assertNotIn("rust.yml", source[source.index("def check_demo_naming"):source.index("def check_widget_wiring")])
        for old in ("AzWidgets AzMaps", '"AzMaps": "azul-maps"'):
            self.assertNotIn(old, source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
