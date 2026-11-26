#!/usr/bin/env python3
"""Tests of scripts/prune_dead_release_links.py: which release links are dead, what replaces them, and
(--demo-status) why a demo's file is not there. Standard library only; the test that holds the file names
of the script to scripts/demo_apps.py runs where that script can be imported (Python 3.11+).

    python3 scripts/test_prune_dead_release_links.py
"""
import contextlib
import io
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import prune_dead_release_links as prune  # noqa: E402

SCRIPT = HERE / "prune_dead_release_links.py"
VERSION = "0.2.0"
REPO = "owner/azul"
RELEASE = f"https://github.com/{REPO}/releases/download/{VERSION}"
PAGES = f"https://azul.rs/ui/release/{VERSION}"


def cell(status, first_error=None, artifact=None, reason=""):
    return {"status": status, "reason": reason, "first_error": first_error, "artifact": artifact}


def status_file(apps):
    """{"apps": [{"app": .., "cells": {..}}, ..]}: what `scripts/demo_apps.py status` writes."""
    return {"schema": 1, "apps": [{"app": name, "description": "", "cells": cells} for name, cells in apps]}


def link(name, label=".tar.gz"):
    return f"<a href='{RELEASE}/{name}' title='{name}' aria-label='x'>{label}</a>"


class PruneTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = pathlib.Path(self.tmp.name)
        self.release_dir = self.dir / "release"
        self.release_dir.mkdir()

    def run_prune(self, html, assets=(), status=None, status_text=None):
        """Run the script on one page -> (the page afterwards, stdout)."""
        page = self.dir / "page.html"
        page.write_text(html, encoding="utf-8")
        names = self.dir / "assets.txt"
        names.write_text("".join(a + "\n" for a in assets), encoding="utf-8")
        cmd = [sys.executable, str(SCRIPT), "--version", VERSION, "--repo", REPO,
               "--release-dir", str(self.release_dir), "--release-assets", str(names)]
        if status is not None or status_text is not None:
            path = self.dir / "demo-status.json"
            path.write_text(status_text if status_text is not None else json.dumps(status), encoding="utf-8")
            cmd += ["--demo-status", str(path)]
        done = subprocess.run(cmd + [str(page)], capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        return page.read_text(encoding="utf-8"), done.stdout

    # ---- what the script did before the status file: these must not change ---------------------
    def test_a_published_release_link_is_kept(self):
        html = f"<p>{link('AzWidgets-linux.tar.gz')}</p>"
        out, log = self.run_prune(html, assets=["AzWidgets-linux.tar.gz"])
        self.assertEqual(out, html)
        self.assertIn("artifact links kept:    1", log)

    def test_a_dead_release_link_becomes_a_span_with_the_label_and_the_bare_text(self):
        out, log = self.run_prune(f"<td>{link('AzWidgets-linux.tar.gz')}</td>", assets=["other.zip"])
        self.assertEqual(out, "<td><span class='release-link-missing is-missing'>.tar.gz &mdash; not available</span></td>")
        self.assertNotIn("href", out)
        self.assertIn("artifact links pruned:  1", log)

    def test_a_pages_link_is_dead_when_its_file_is_not_in_the_release_dir(self):
        (self.release_dir / "here.txt").write_text("x")
        html = (f"<a href='{PAGES}/here.txt'>here</a> <a href='{PAGES}/gone.txt'>gone</a> "
                f"<a href='https://example.org/x'>elsewhere</a>")
        out, _ = self.run_prune(html, assets=["x"])
        self.assertIn(f"<a href='{PAGES}/here.txt'>here</a>", out)
        self.assertIn("<span class='release-link-missing is-missing'>gone &mdash; not available</span>", out)
        self.assertIn("<a href='https://example.org/x'>elsewhere</a>", out)

    def test_a_dead_card_keeps_its_tile_and_loses_the_file_line(self):
        html = (f"<a class='docs-card' href='{RELEASE}/guide.pdf'><h4>Guide</h4>"
                f"<p class='docs-card-file'>guide.pdf</p></a>")
        out, _ = self.run_prune(html, assets=["x"])
        self.assertEqual(out, "<div class='docs-card is-missing'><h4>Guide</h4>"
                              "<p class='docs-card-file'>not available</p></div>")

    # ---- --demo-status --------------------------------------------------------------------------
    def test_a_dead_link_of_a_failed_desktop_build_says_why(self):
        status = status_file([("AzFoo", {"linux": cell("failed", "AzFoo: cannot find value `x` (src/a.rs:7)")})])
        out, log = self.run_prune(f"<td>{link('AzFoo-linux.tar.gz')}</td>", assets=["x"], status=status)
        self.assertEqual(
            out,
            "<td><span class='release-link-missing is-missing'>.tar.gz &mdash; "
            "not available: build failed: AzFoo: cannot find value `x` (src/a.rs:7)</span></td>")
        self.assertIn("build failed: AzFoo: cannot find value", log)
        self.assertIn("1 file names of failed demo builds", log)

    def test_every_column_is_named_by_the_file_its_build_stages(self):
        cases = {
            "linux": "AzFoo-linux.tar.gz",
            "macos-arm64": "AzFoo-macos.tar.gz",
            "macos-intel": "AzFoo-macos-intel.tar.gz",
            "windows": "AzFoo-windows.exe.tar.gz",
            "android-arm64": "AzFoo-android-arm64.apk",
            "android-x86_64": "AzFoo-android-x86_64.apk",
            "ios-device": "AzFoo-ios-device.app.zip",
            "ios-sim": "AzFoo-ios-sim.app.zip",
        }
        for column, asset in cases.items():
            with self.subTest(column=column):
                status = status_file([("AzFoo", {column: cell("failed", f"broke on {column}")})])
                out, _ = self.run_prune(link(asset), assets=["x"], status=status)
                self.assertIn(f"not available: build failed: broke on {column}</span>", out)
                # the other columns of the app are not blamed
                other = next(a for c, a in cases.items() if c != column)
                out, _ = self.run_prune(link(other), assets=["x"], status=status)
                self.assertTrue(out.endswith("&mdash; not available</span>"), out)

    def test_the_artifact_of_a_cell_names_the_file_too(self):
        status = status_file([("AzFoo", {"linux": cell("failed", "boom", artifact="AzFoo-linux")})])
        out, _ = self.run_prune(link("AzFoo-linux.tar.gz"), assets=["x"], status=status)
        self.assertIn("build failed: boom", out)
        # a name no column of the app stages, only the artifact: still found
        status = status_file([("AzFoo", {"linux": cell("failed", "boom", artifact="Renamed-binary")})])
        out, _ = self.run_prune(link("Renamed-binary.tar.gz"), assets=["x"], status=status)
        self.assertIn("build failed: boom", out)

    def test_a_dead_link_of_a_cell_that_did_not_fail_keeps_the_bare_text(self):
        for st in ("built", "reused", "skipped", "missing"):
            with self.subTest(status=st):
                status = status_file([("AzFoo", {"linux": cell(st, "ignored")})])
                out, _ = self.run_prune(link("AzFoo-linux.tar.gz"), assets=["x"], status=status)
                self.assertTrue(out.endswith("&mdash; not available</span>"), out)
                self.assertNotIn("build failed", out)

    def test_a_live_link_of_a_failed_cell_is_kept_as_it_is(self):
        status = status_file([("AzFoo", {"linux": cell("failed", "boom")})])
        html = link("AzFoo-linux.tar.gz")
        out, _ = self.run_prune(html, assets=["AzFoo-linux.tar.gz"], status=status)
        self.assertEqual(out, html)

    def test_a_dead_card_of_a_failed_build_says_why_in_its_file_line(self):
        status = status_file([("AzFoo", {"linux": cell("failed", "boom")})])
        html = (f"<a class='docs-card' href='{RELEASE}/AzFoo-linux.tar.gz'><h4>Foo</h4>"
                f"<p class='docs-card-file'>AzFoo-linux.tar.gz</p></a>")
        out, _ = self.run_prune(html, assets=["x"], status=status)
        self.assertIn("<p class='docs-card-file'>not available: build failed: boom</p>", out)

    def test_the_reason_is_one_escaped_line_of_at_most_200_characters(self):
        err = "AzFoo: expected `<T>` & found 'x'\n   --> src/lib.rs:3\n" + "y" * 400
        status = status_file([("AzFoo", {"linux": cell("failed", err)})])
        out, _ = self.run_prune(link("AzFoo-linux.tar.gz"), assets=["x"], status=status)
        self.assertIn("expected `&lt;T&gt;` &amp; found &#x27;x&#x27; --&gt; src/lib.rs:3 ", out)
        self.assertNotIn("\n", out)
        self.assertNotIn("<T>", out)
        reason = out.split("build failed: ")[1].split("</span>")[0]
        self.assertTrue(reason.endswith("..."), reason)
        self.assertLessEqual(len(prune.reason_text(cell("failed", err))), prune.MAX_REASON)

    def test_a_failed_cell_without_an_error_gives_its_reason_and_then_a_plain_statement(self):
        self.assertEqual(prune.reason_text(cell("failed", reason="the job did not run")), "the job did not run")
        self.assertEqual(prune.reason_text(cell("failed")), "no reason recorded")
        self.assertEqual(prune.reason_text(cell("failed", first_error="", reason="r")), "r")

    def test_an_unreadable_status_file_costs_the_reasons_and_not_the_prune(self):
        for text in ("not json", "[]", '{"apps": 3}', '{"apps": [{"app": "AzFoo", "cells": []}]}', "null"):
            with self.subTest(text=text):
                out, log = self.run_prune(link("AzFoo-linux.tar.gz"), assets=["x"], status_text=text)
                self.assertTrue(out.endswith("&mdash; not available</span>"), out)
        out, log = self.run_prune(link("AzFoo-linux.tar.gz"), assets=["x"], status_text="not json")
        self.assertIn("::warning::cannot read the demo status", log)

    def test_a_status_file_that_is_not_there_is_a_warning_too(self):
        page = self.dir / "p.html"
        page.write_text(link("AzFoo-linux.tar.gz"), encoding="utf-8")
        names = self.dir / "a.txt"
        names.write_text("x\n")
        done = subprocess.run(
            [sys.executable, str(SCRIPT), "--version", VERSION, "--repo", REPO, "--release-dir", str(self.release_dir),
             "--release-assets", str(names), "--demo-status", str(self.dir / "nope.json"), str(page)],
            capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("::warning::cannot read the demo status", done.stdout)
        self.assertIn("&mdash; not available</span>", page.read_text(encoding="utf-8"))

    def test_without_the_flag_nothing_changes_for_a_link_of_a_demo(self):
        out, log = self.run_prune(link("AzFoo-linux.tar.gz"), assets=["x"])
        self.assertTrue(out.endswith("&mdash; not available</span>"))
        self.assertNotIn("failed demo builds", log)

    def test_a_table_of_links_is_pruned_cell_by_cell(self):
        """The shape the release page's Demos table has: every cell its own link."""
        status = status_file([
            ("AzFoo", {"linux": cell("failed", "link error"), "windows": cell("built", artifact="AzFoo-windows.exe")}),
            ("AzBar", {"android-arm64": cell("failed", "no cdylib")}),
        ])
        row = "".join(f"<td>{link(n, k)}</td>" for n, k in (
            ("AzFoo-linux.tar.gz", ".tar.gz"), ("AzFoo-windows.exe.tar.gz", ".tar.gz"),
            ("AzBar-android-arm64.apk", ".apk"), ("AzBar-linux.tar.gz", ".tar.gz")))
        published = ["AzFoo-windows.exe.tar.gz"]
        out, log = self.run_prune(f"<tr>{row}</tr>", assets=published, status=status)
        self.assertEqual(out.count("<a "), 1, out)
        self.assertIn(f"href='{RELEASE}/AzFoo-windows.exe.tar.gz'", out)
        self.assertEqual(out.count("build failed: link error"), 1)
        self.assertEqual(out.count("build failed: no cdylib"), 1)
        self.assertEqual(out.count("&mdash; not available</span>"), 1)  # AzBar-linux: dead, not a failed cell
        self.assertIn("artifact links pruned:  3", log)

    # ---- the names -------------------------------------------------------------------------------
    @unittest.skipUnless(sys.version_info >= (3, 11), "scripts/demo_apps.py needs tomllib (Python 3.11+)")
    def test_the_names_are_the_ones_scripts_demo_apps_py_stages(self):
        import demo_apps

        m = demo_apps.load()
        checked = 0
        for app in m["app"]:
            for column in demo_apps.columns_of(app):
                self.assertEqual(prune.staged_name(app["package"], column), demo_apps.staged_name(app, column),
                                 f"{app['package']} {column}")
                checked += 1
        self.assertGreater(checked, 100)
        # a column the script does not know names no file
        self.assertIsNone(prune.staged_name("AzFoo", "amiga"))
        self.assertEqual(set(prune.DESKTOP_SUFFIX) | set(prune.PHONE_SUFFIX), set(demo_apps.ARTIFACT_COLUMNS))

    def test_the_release_name_of_a_failed_file_is_found_with_and_without_its_tarball_suffix(self):
        failed = {"AzFoo-linux": "why"}
        self.assertEqual(prune.why_failed("AzFoo-linux.tar.gz", failed), "why")
        self.assertEqual(prune.why_failed("demos/AzFoo-linux", failed), "why")
        self.assertEqual(prune.why_failed("AzFoo-linux.tar.gz?x=1#y", failed), "why")
        self.assertIsNone(prune.why_failed("AzFoo-linux-other.tar.gz", failed))
        self.assertIsNone(prune.why_failed("AzFoo-macos.tar.gz", failed))
        self.assertEqual(prune.failed_builds(None), {})
        self.assertEqual(prune.failed_builds({}), {})


if __name__ == "__main__":
    unittest.main(verbosity=2)
