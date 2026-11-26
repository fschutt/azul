#!/usr/bin/env python3
"""Tests of the demo pipeline's scripts: the manifest reader and planner (demo_apps.py), the engine gate
(demo_gate.py) and the pure parts of the builder (demo_build.py).

    python3 scripts/test_demo_apps.py            (Python 3.11+, standard library only)
"""
import argparse
import contextlib
import io
import json
import os
import pathlib
import plistlib
import re
import subprocess
import sys
import tarfile
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import demo_apps  # noqa: E402
import demo_build  # noqa: E402
import demo_gate  # noqa: E402
import demo_icon  # noqa: E402
import demo_publish  # noqa: E402
sys.path.insert(0, str(HERE / "ci"))
import png_compare  # noqa: E402
import smoke_verdict  # noqa: E402
import ui_find  # noqa: E402


def cargo_json(*artifacts):
    """A cargo JSON stream: each artifact is (package_id, features, kind, executable)."""
    lines = []
    for pid, features, kind, exe in artifacts:
        lines.append(json.dumps({
            "reason": "compiler-artifact", "package_id": pid, "features": features,
            "target": {"kind": [kind], "name": "x"}, "executable": exe, "filenames": [exe] if exe else [],
        }))
    lines.append(json.dumps({"reason": "build-finished", "success": True}))
    return "\n".join(lines) + "\n"


# What `python3 scripts/demo_apps.py assets <package>` prints: app, column, staged name, release name, one line
# per built file. doc/src/dllgen/demo_apps_test.rs holds the SAME lines against the names the release page
# generator gives the files (GOLDEN_AZWIDGETS and GOLDEN_AZCOMPANION there), and a test of that file reads this
# one for them: the script and the page agree to the byte as long as both match these, so change them together.
GOLDEN_AZWIDGETS = (
    "AzWidgets\tlinux\tAzWidgets-linux\tAzWidgets-linux.tar.gz\n"
    "AzWidgets\tmacos-arm64\tAzWidgets-macos\tAzWidgets-macos.tar.gz\n"
    "AzWidgets\tmacos-intel\tAzWidgets-macos-intel\tAzWidgets-macos-intel.tar.gz\n"
    "AzWidgets\twindows\tAzWidgets-windows.exe\tAzWidgets-windows.exe.tar.gz\n"
    "AzWidgets\tandroid-arm64\tAzWidgets-android-arm64.apk\tAzWidgets-android-arm64.apk\n"
    "AzWidgets\tandroid-x86_64\tAzWidgets-android-x86_64.apk\tAzWidgets-android-x86_64.apk\n"
    "AzWidgets\tios-device\tAzWidgets-ios-device.app.zip\tAzWidgets-ios-device.app.zip\n"
    "AzWidgets\tios-sim\tAzWidgets-ios-sim.app.zip\tAzWidgets-ios-sim.app.zip\n"
)
GOLDEN_AZCOMPANION = (
    "AzCompanion\tandroid-arm64\tAzCompanion-android-arm64.apk\tAzCompanion-android-arm64.apk\n"
    "AzCompanion\tandroid-x86_64\tAzCompanion-android-x86_64.apk\tAzCompanion-android-x86_64.apk\n"
    "AzCompanion\tios-device\tAzCompanion-ios-device.app.zip\tAzCompanion-ios-device.app.zip\n"
    "AzCompanion\tios-sim\tAzCompanion-ios-sim.app.zip\tAzCompanion-ios-sim.app.zip\n"
)

FACADE = "path+file:///w/dll#azul-dll@0.2.0"
CORE = "path+file:///w/core#azul-core@0.0.21"
LAYOUT = "path+file:///w/layout#azul-layout@0.0.21"
APP = "path+file:///w/examples/azul-widgets#AzWidgets@0.1.0"


class GateTest(unittest.TestCase):
    def write(self, text):
        f = tempfile.NamedTemporaryFile("w", suffix=".json", delete=False)
        f.write(text)
        f.close()
        self.addCleanup(pathlib.Path(f.name).unlink)
        return f.name

    def test_every_spelling_of_a_package_id_names_its_package(self):
        self.assertEqual(demo_gate.package_name("azul-core 0.0.21 (path+file:///w/core)"), "azul-core")
        self.assertEqual(demo_gate.package_name(CORE), "azul-core")
        self.assertEqual(demo_gate.package_name("registry+https://github.com/rust-lang/crates.io-index#azul-css@0.0.21"), "azul-css")
        # cargo drops the name when the directory is named after the package
        self.assertEqual(demo_gate.package_name("path+file:///w/examples/azul-writer#0.1.0"), "azul-writer")

    def test_the_facade_alone_passes(self):
        log = self.write(cargo_json((FACADE, ["link-dynamic", "cabi_external", "rust_api"], "lib", None), (APP, [], "bin", "/t/AzWidgets")))
        r = demo_gate.check(log)
        self.assertEqual(r["engine_crates"], [])
        self.assertEqual(r["dll_engine_features"], [])
        self.assertEqual(demo_gate.main([log]), 0)

    def test_a_compiled_engine_crate_fails_the_app(self):
        log = self.write(cargo_json((CORE, [], "lib", None), (LAYOUT, [], "lib", None), (FACADE, ["link-dynamic"], "lib", None)))
        self.assertEqual(demo_gate.check(log)["engine_crates"], ["azul-core", "azul-layout"])
        self.assertEqual(demo_gate.main([log]), 1)

    def test_the_facade_built_with_an_engine_feature_fails_the_app(self):
        log = self.write(cargo_json((FACADE, ["link-static", "cabi_export", "std"], "lib", None)))
        r = demo_gate.check(log)
        self.assertEqual(r["dll_engine_features"], ["cabi_export", "link-static"])
        self.assertEqual(demo_gate.main([log]), 1)

    def test_an_allowed_engine_crate_passes(self):
        log = self.write(cargo_json((LAYOUT, [], "lib", None), (FACADE, ["link-dynamic"], "lib", None)))
        self.assertEqual(demo_gate.main([log, "--allow", "azul-layout"]), 0)

    def test_a_log_of_nothing_says_nothing(self):
        self.assertEqual(demo_gate.main([self.write("not json\n")]), 2)


class NamesTest(unittest.TestCase):
    def setUp(self):
        self.m = demo_apps.load()
        self.app = {a["package"]: a for a in self.m["app"]}

    def test_desktop_files_are_named_after_the_package_and_the_platform(self):
        a = self.app["AzWidgets"]
        self.assertEqual(demo_apps.staged_name(a, "linux"), "AzWidgets-linux")
        self.assertEqual(demo_apps.staged_name(a, "macos-arm64"), "AzWidgets-macos")
        self.assertEqual(demo_apps.staged_name(a, "macos-intel"), "AzWidgets-macos-intel")
        self.assertEqual(demo_apps.staged_name(a, "windows"), "AzWidgets-windows.exe")
        self.assertEqual(demo_apps.release_name(a, "windows"), "AzWidgets-windows.exe.tar.gz")

    def test_the_phone_files_have_the_names_azreview_downloads(self):
        a = self.app["AzCompanion"]
        self.assertEqual(demo_apps.staged_name(a, "android-arm64"), "AzCompanion-android-arm64.apk")
        self.assertEqual(demo_apps.staged_name(a, "android-x86_64"), "AzCompanion-android-x86_64.apk")
        self.assertEqual(demo_apps.staged_name(a, "ios-device"), "AzCompanion-ios-device.app.zip")
        self.assertEqual(demo_apps.staged_name(a, "ios-sim"), "AzCompanion-ios-sim.app.zip")
        self.assertEqual(demo_apps.release_name(a, "ios-device"), "AzCompanion-ios-device.app.zip")

    def test_the_assets_command_prints_the_names_the_release_page_generator_pins(self):
        for package, golden in (("AzWidgets", GOLDEN_AZWIDGETS), ("AzCompanion", GOLDEN_AZCOMPANION)):
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(demo_apps.main(["assets", package]), 0)
            self.assertEqual(out.getvalue(), golden, package)

    def test_no_two_assets_share_a_name(self):
        seen = {}
        for a in self.m["app"]:
            for c in demo_apps.columns_of(a):
                for n in {demo_apps.staged_name(a, c), demo_apps.release_name(a, c)}:
                    self.assertNotIn(n, seen, f"{n}: {a['package']} and {seen.get(n)}")
                    seen[n] = a["package"]

    def test_a_phone_app_builds_both_files_of_each_phone_platform(self):
        self.assertEqual(demo_apps.columns_of(self.app["AzCompanion"]),
                         ["android-arm64", "android-x86_64", "ios-device", "ios-sim"])
        self.assertEqual(demo_apps.columns_of(self.app["AzBuilder"]), ["linux", "macos-arm64", "macos-intel", "windows"])


class PlanTest(unittest.TestCase):
    def setUp(self):
        self.m = demo_apps.load()
        self.plan = demo_apps.plan(self.m)

    def test_every_app_is_built_once_per_platform_it_declares(self):
        for leg in demo_apps.DESKTOP:
            family = {"linux": "linux", "macos-arm64": "macos", "macos-intel": "macos", "windows": "windows"}[leg]
            built = []
            for e in self.plan["apps_" + family]:
                if e["leg"] == leg:
                    built += e["apps"].split()
            self.assertEqual(sorted(built), sorted(a["package"] for a in demo_apps.apps_of(self.m, leg)), leg)

    def test_a_group_never_repeats_an_app(self):
        for key, entries in self.plan.items():
            if not key.startswith("apps_"):
                continue
            for e in entries:
                names = e["apps"].split()
                self.assertEqual(len(names), len(set(names)), key)

    def test_every_phone_app_finds_its_library_among_the_libs_the_plan_builds(self):
        built = {e["artifact"] for k in ("libs_android", "libs_ios") for e in self.plan[k]}
        for kind in ("android", "ios"):
            for e in self.plan["apps_" + kind]:
                for a in e["apps"].split():
                    app = next(x for x in self.m["app"] if x["package"] == a)
                    self.assertIn(f"demolib-{e['leg']}-{app['mobile_lib']}", built, f"{a} on {e['leg']}")

    def test_only_the_apps_that_ask_for_iroh_get_its_library(self):
        for k in ("libs_android", "libs_ios"):
            for e in self.plan[k]:
                self.assertEqual("iroh" in e["features"].split(","), e["variant"] == "mobile-iroh", e["id"])
        wanting = [a["package"] for a in self.m["app"] if a.get("mobile_lib") == "mobile-iroh"]
        self.assertEqual(wanting, ["AzCompanion"])

    def test_one_library_per_desktop_leg_and_the_windows_one_is_azul_lib(self):
        legs = [e["leg"] for k in ("libs_linux", "libs_macos", "libs_windows") for e in self.plan[k]]
        self.assertEqual(sorted(legs), sorted(demo_apps.DESKTOP))
        files = {e["leg"]: e["lib_file"] for k in ("apps_linux", "apps_macos", "apps_windows") for e in self.plan[k]}
        self.assertEqual(files["windows"], "azul.lib")
        self.assertEqual(files["linux"], "libazul.a")

    def test_the_intel_leg_builds_for_x86_64_with_the_oldest_macos(self):
        intel = [e for e in self.plan["apps_macos"] if e["leg"] == "macos-intel"]
        self.assertTrue(intel)
        for e in intel:
            self.assertEqual((e["target"], e["deployment_target"]), ("x86_64-apple-darwin", "10.12"))

    def test_the_plan_names_the_release_the_workflow_names(self):
        self.assertRegex(self.plan["version"], r"^\d+\.\d+\.\d+$")


class PackTest(unittest.TestCase):
    def test_the_heavy_apps_are_spread_over_the_groups(self):
        apps = [{"package": f"Az{n}", "weight": w} for n, w in
                zip("ABCDEFGH", (3, 3, 3, 3, 1, 1, 1, 1))]
        groups = demo_apps.pack(apps, 4)
        self.assertEqual(len(groups), 4)
        for g in groups:
            self.assertEqual(sum(next(a["weight"] for a in apps if a["package"] == p) for p in g), 4)

    def test_there_are_never_more_groups_than_apps(self):
        self.assertEqual(demo_apps.pack([{"package": "AzA", "weight": 1}], 4), [["AzA"]])


class E2eVariantTest(unittest.TestCase):
    """The QA variant: the phone libazul with the engine's debug server (AZ_DEBUG=<port>) and E2E scripting, linked by the SAME app
    build as the shipped one, an artifact of the run and never an asset of the release or a link of the page."""

    def setUp(self):
        self.m = demo_apps.load()

    def app(self, name):
        return next(a for a in self.m["app"] if a["package"] == name)

    def test_the_names_are_the_shipped_names_with_e2e_after_the_app(self):
        w = self.app("AzWidgets")
        self.assertEqual(demo_apps.staged_name(w, "android-x86_64", e2e=True), "AzWidgets-e2e-android-x86_64.apk")
        self.assertEqual(demo_apps.staged_name(w, "ios-sim", e2e=True), "AzWidgets-e2e-ios-sim.app.zip")
        self.assertEqual(demo_apps.staged_name(w, "android-x86_64"), "AzWidgets-android-x86_64.apk")
        with self.assertRaises(ValueError):
            demo_apps.staged_name(w, "linux", e2e=True)

    def test_without_the_input_a_run_plans_nothing_of_it(self):
        plan = demo_apps.plan(self.m)
        for key, entries in plan.items():
            if key.startswith(("libs_", "apps_")):
                for e in entries:
                    self.assertNotEqual(e.get("variant"), "mobile-e2e", key)
                    self.assertFalse(e["artifact"].startswith("demoe2e-"), key)

    def test_with_the_input_the_lib_of_the_listed_columns_and_the_app_that_asks_are_planned(self):
        plan = demo_apps.plan(self.m, e2e=True)
        libs = {e["leg"]: e for k in ("libs_android", "libs_ios") for e in plan[k] if e["variant"] == "mobile-e2e"}
        self.assertEqual(sorted(libs), ["android-x86_64", "ios-sim"])
        for leg, e in libs.items():
            feats = e["features"].split(",")
            self.assertIn("debug-server", feats)
            self.assertIn("e2e-scripting", feats)
            self.assertIn("test-hooks", feats)   # the failure-injection hooks of the QA matrix (AZ_GPU_FAIL, ...), read only when asked
            self.assertEqual("android-activity" in feats, leg.startswith("android"))
            self.assertEqual(e["artifact"], f"demolib-{leg}-mobile-e2e")
        entries = {e["leg"]: e for k in ("apps_android", "apps_ios") for e in plan[k] if e.get("variant") == "mobile-e2e"}
        self.assertEqual(sorted(entries), ["android-x86_64", "ios-sim"])
        for leg, e in entries.items():
            self.assertEqual(e["apps"], "AzWidgets")
            self.assertEqual(e["lib_artifacts"], f"demolib-{leg}-mobile-e2e")
            self.assertTrue(e["artifact"].startswith("demoe2e-"), e["artifact"])   # a name the shipped-files patterns never match
            self.assertTrue(e["results_artifact"].startswith("demoresults-"))
        # the shipped builds of the same columns are still there, against the plain lib
        shipped = [e for k in ("apps_android", "apps_ios") for e in plan[k] if e.get("variant") == ""]
        self.assertTrue(shipped)
        for e in shipped:
            self.assertFalse(e["artifact"].startswith("demoe2e-"))

    def test_the_plan_command_takes_the_input_as_a_flag(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            self.assertEqual(demo_apps.main(["plan", "--e2e"]), 0)
        self.assertIn("demolib-android-x86_64-mobile-e2e", buf.getvalue())
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            self.assertEqual(demo_apps.main(["plan"]), 0)
        self.assertNotIn("mobile-e2e", buf.getvalue())

    def test_only_an_artifact_only_lib_may_carry_test_hooks(self):
        # a build with the cargo feature `test-hooks` reads stand-ins and failure injections from the environment: never what ships
        for variant in ("mobile", "desktop", "mobile-iroh"):
            m = demo_apps.load()
            m["lib"][variant]["features"] += ",test-hooks"
            errors = demo_apps.check(m)
            self.assertTrue(any(variant in e and "test-hooks" in e and "artifact_only" in e for e in errors), (variant, errors))

    def test_the_lib_script_checks_the_test_hooks_marker_both_ways(self):
        # scripts/check_no_test_hooks.py: a library built without the feature must not hold the marker, one built with it must (the
        # negative control: a check that cannot fail checks nothing)
        text = (HERE / "demo_lib.sh").read_text(encoding="utf-8")
        self.assertIn("check_no_test_hooks.py", text)
        self.assertIn("--expect-marker=test-hooks", text)
        self.assertRegex(text, r'case ",\$features," in\s+\*,test-hooks,\*\)')

    def test_an_app_never_ships_against_the_qa_lib(self):
        self.app("AzMaps")["mobile_lib"] = "mobile-e2e"
        errors = demo_apps.check(self.m)
        self.assertTrue(any("artifact_only" in e and "AzMaps" in e for e in errors), errors)

    def test_the_qa_lib_must_carry_the_debug_server_and_only_phone_columns(self):
        self.m["lib"]["mobile-e2e"]["features"] = "std,logging,link-static"
        self.assertTrue(any("debug-server" in e for e in demo_apps.check(self.m)))
        m = demo_apps.load()
        m["lib"]["mobile-e2e"]["targets"] = ["linux"]
        self.assertTrue(any("mobile-e2e" in e and "targets" in e for e in demo_apps.check(m)))

    def test_an_e2e_app_needs_a_column_the_qa_lib_builds(self):
        self.m["lib"]["mobile-e2e"]["targets"] = ["ios-sim"]
        self.app("AzWidgets")["platforms"] = ["linux"]
        self.assertTrue(any("AzWidgets" in e and "e2e" in e for e in demo_apps.check(self.m)), demo_apps.check(self.m))

    def test_the_real_manifest_passes_its_own_check_with_the_variant_in_it(self):
        self.assertTrue(self.m["lib"]["mobile-e2e"]["artifact_only"])
        self.assertEqual([a["package"] for a in self.m["app"] if a.get("e2e")], ["AzWidgets"])
        self.assertEqual([e for e in demo_apps.check(self.m) if "e2e" in e], [])

    def test_a_qa_file_never_becomes_a_release_asset_or_a_page_link(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "demoe2e-android-x86_64").mkdir()
            (root / "demoe2e-android-x86_64" / "AzWidgets-e2e-android-x86_64.apk").write_bytes(b"an apk with the debug server")
            (root / "demoapps-android-x86_64").mkdir()
            (root / "demoapps-android-x86_64" / "AzWidgets-android-x86_64.apk").write_bytes(b"the shipped apk")
            listing = demo_publish.assemble(root, root / "release", "0.2.0", "abc")
        self.assertEqual([x["name"] for x in listing], ["AzWidgets-android-x86_64.apk"])
        for app in self.m["app"]:
            for col in demo_apps.columns_of(app):
                self.assertNotIn("-e2e-", demo_apps.release_name(app, col))

    def test_the_status_keeps_the_qa_builds_apart_from_the_shipped_cells(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "r").mkdir()
            (root / "r" / "AzWidgets.android-x86_64.result.json").write_text(json.dumps({
                "app": "AzWidgets", "column": "android-x86_64", "status": "built", "size": 2048, "shipped_size": 2048,
                "seconds": 12, "engine_crates": [], "artifact": "AzWidgets-android-x86_64.apk"}))
            (root / "r" / "AzWidgets.android-x86_64-e2e.result.json").write_text(json.dumps({
                "app": "AzWidgets", "column": "android-x86_64", "variant": "mobile-e2e", "status": "built", "size": 9000,
                "shipped_size": 9000, "seconds": 20, "engine_crates": [], "artifact": "AzWidgets-e2e-android-x86_64.apk"}))
            data, md = demo_apps.status([root / "r"], m=self.m)
        cell = next(r for r in data["apps"] if r["app"] == "AzWidgets")["cells"]["android-x86_64"]
        self.assertEqual(cell["size"], 2048)                       # the shipped cell is the shipped build
        self.assertEqual(data["summary"]["assets"], 1)             # and the QA build is not an asset
        self.assertEqual([(q["app"], q["column"], q["artifact"]) for q in data["summary"]["qa_builds"]],
                         [("AzWidgets", "android-x86_64", "AzWidgets-e2e-android-x86_64.apk")])
        self.assertIn("never published", md)

    def test_a_qa_result_is_a_file_of_its_own(self):
        with tempfile.TemporaryDirectory() as d:
            args = argparse.Namespace(out=str(pathlib.Path(d) / "out"), results=str(pathlib.Path(d) / "res"))
            job = demo_build.Job(args)
            app = job.apps["AzWidgets"]
            job.result(app, "android-x86_64", "x86_64-linux-android", "failed", "x", variant="mobile-e2e")
            job.result(app, "android-x86_64", "x86_64-linux-android", "failed", "x")
            names = sorted(p.name for p in (pathlib.Path(d) / "res").iterdir())
            qa = json.loads((pathlib.Path(d) / "res" / "AzWidgets.android-x86_64-e2e.result.json").read_text())
        self.assertEqual(names, ["AzWidgets.android-x86_64-e2e.result.json", "AzWidgets.android-x86_64.result.json"])
        self.assertEqual(qa["variant"], "mobile-e2e")

    def test_the_build_driver_takes_only_an_artifact_only_variant(self):
        with tempfile.TemporaryDirectory() as d:
            argv = ["android", "--target", "x86_64-linux-android", "--libs", d, "--apps", "AzWidgets",
                    "--out", str(pathlib.Path(d) / "o"), "--results", str(pathlib.Path(d) / "r")]
            with self.assertRaises(SystemExit):
                demo_build.main(argv + ["--variant", "mobile"])
            with self.assertRaises(SystemExit):
                demo_build.main(argv + ["--variant", "no-such-variant"])


class BuildTest(unittest.TestCase):
    def test_the_german_strings_are_written_escaped(self):
        app = {"ios": {"strings": {"de": {"K": 'a "b" \\ c\nd'}}}}
        with tempfile.TemporaryDirectory() as d:
            demo_build.strings_dir(app, d)
            text = (pathlib.Path(d) / "de.lproj" / "InfoPlist.strings").read_text(encoding="utf-8")
        self.assertEqual(text, '"K" = "a \\"b\\" \\\\ c\\nd";\n')

    def test_an_app_without_strings_writes_nothing(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIsNone(demo_build.strings_dir({}, d))
            self.assertEqual(list(pathlib.Path(d).iterdir()), [])

    def test_the_first_error_is_the_first_compiler_error_with_its_place(self):
        err = {"reason": "compiler-message", "package_id": APP, "message": {
            "level": "error", "message": "cannot find value `x`", "spans": [{"is_primary": True, "file_name": "src/a.rs", "line_start": 7}]}}
        with tempfile.TemporaryDirectory() as d:
            j = pathlib.Path(d) / "b.json"
            j.write_text(json.dumps(err) + "\n")
            e = pathlib.Path(d) / "b.err"
            e.write_text("warning: x\n")
            self.assertEqual(demo_build.first_error(j, e), "AzWidgets: cannot find value `x` (src/a.rs:7)")

    def test_without_json_the_first_error_line_of_the_text_is_taken(self):
        with tempfile.TemporaryDirectory() as d:
            j = pathlib.Path(d) / "b.json"
            j.write_text("")
            e = pathlib.Path(d) / "b.err"
            e.write_text("   Compiling x\nerror: failed to run custom build command for `ring`\nmore\n")
            self.assertEqual(demo_build.first_error(j, e), "error: failed to run custom build command for `ring`")

    def test_the_executable_is_the_one_cargo_reports_for_the_package(self):
        with tempfile.TemporaryDirectory() as d:
            j = pathlib.Path(d) / "b.json"
            j.write_text(cargo_json((FACADE, [], "lib", None), (APP, [], "bin", "/t/release/AzWidgets")))
            self.assertEqual(demo_build.executable_of(j, "AzWidgets"), "/t/release/AzWidgets")
            self.assertIsNone(demo_build.executable_of(j, "AzMaps"))


class AndroidLevelsTest(unittest.TestCase):
    """The SDK levels of every APK are recorded in the manifest and are the ones the template of the Android manifest says."""

    def test_the_manifest_records_the_levels_and_the_template_agrees(self):
        m = demo_apps.load()
        self.assertEqual(m["android"], {"min_sdk": 24, "target_sdk": 34})
        self.assertEqual([e for e in demo_apps.check(m) if "SdkVersion" in e or "min_sdk" in e or "target_sdk" in e], [])

    def test_a_level_the_template_does_not_say_is_an_error(self):
        m = demo_apps.load()
        m["android"]["target_sdk"] = 35
        self.assertTrue(any("targetSdkVersion" in e and "35" in e for e in demo_apps.check(m)))
        m = demo_apps.load()
        m["android"]["min_sdk"] = 26
        self.assertTrue(any("minSdkVersion" in e and "26" in e for e in demo_apps.check(m)))

    def test_levels_that_are_no_numbers_or_a_target_below_the_minimum_are_errors(self):
        m = demo_apps.load()
        m["android"] = {"min_sdk": "24", "target_sdk": 34}
        self.assertTrue(any("min_sdk" in e for e in demo_apps.check(m)))
        m["android"] = {"min_sdk": 34, "target_sdk": 24}
        self.assertTrue(any("below" in e for e in demo_apps.check(m)))
        m["android"] = {}
        self.assertTrue(any("target_sdk" in e for e in demo_apps.check(m)))


class FloorApkTest(unittest.TestCase):
    """scripts/floor-apk: the APK of an empty Java Activity that the startup measurements compare the demo apps with: the same
    levels, key and launch path as the demo APKs, and nothing of azul in it."""

    def setUp(self):
        self.dir = demo_apps.ROOT / "scripts" / "floor-apk"

    def test_the_floor_has_the_sdk_levels_of_the_demo_apks(self):
        manifest = (self.dir / "AndroidManifest.xml").read_text(encoding="utf-8")
        android = demo_apps.load()["android"]
        self.assertIn(f'android:minSdkVersion="{android["min_sdk"]}"', manifest)
        self.assertIn(f'android:targetSdkVersion="{android["target_sdk"]}"', manifest)

    def test_the_floor_launches_like_a_demo_app_and_is_a_java_activity_without_a_native_library(self):
        manifest = (self.dir / "AndroidManifest.xml").read_text(encoding="utf-8")
        for needle in ("android.intent.action.MAIN", "android.intent.category.LAUNCHER", 'android:exported="true"',
                       'android:launchMode="singleTop"', 'android:hasCode="true"'):
            self.assertIn(needle, manifest)
        self.assertNotIn("lib_name", manifest)
        java = (self.dir / "FloorActivity.java").read_text(encoding="utf-8")
        self.assertIn("extends Activity", java)
        for forbidden in ("loadLibrary", "extends NativeActivity", "import com.azul"):
            self.assertNotIn(forbidden, java)

    def test_the_build_script_signs_with_the_key_of_the_demo_apks_and_uses_no_cargo(self):
        script = (self.dir / "build.sh").read_text(encoding="utf-8")
        self.assertIn("scripts/android/debug.keystore", script)
        for tool in ("aapt2", "d8", "zipalign", "apksigner", "javac"):
            self.assertIn(tool, script)
        code = "\n".join(l for l in script.splitlines() if not l.lstrip().startswith("#"))
        self.assertNotIn("cargo", code)
        self.assertNotRegex(code, r"\brm\b")   # it is run on developers' machines too: it never deletes
        self.assertEqual(subprocess.run(["bash", "-n", str(self.dir / "build.sh")]).returncode, 0)


class PurposeStringsTest(unittest.TestCase):
    """iOS refuses a camera, a microphone or a location without the Info.plist text that says why (and the engine now refuses to
    start the feature, naming the key): the texts come from the manifest through AZ_IOS_USAGE, none is a default of the scripts."""

    def setUp(self):
        self.m = demo_apps.load()

    def app(self, name):
        return next(a for a in self.m["app"] if a["package"] == name)

    def test_every_sensor_permission_an_app_asks_for_on_android_has_its_purpose_string_on_ios(self):
        self.assertEqual([e for e in demo_apps.check(self.m) if "UsageDescription" in e], [])

    def test_the_phone_apps_that_use_the_camera_the_microphone_or_the_location_say_why(self):
        meet = self.app("AzMeet")["ios"]["usage"]
        self.assertIn("NSCameraUsageDescription", meet)
        self.assertIn("NSMicrophoneUsageDescription", meet)
        self.assertIn("NSLocationWhenInUseUsageDescription", self.app("AzMaps")["ios"]["usage"])
        self.assertIn("NSCameraUsageDescription", self.app("AzCompanion")["ios"]["usage"])

    def test_a_missing_key_is_an_error_that_names_the_key_and_the_permission_it_stands_for(self):
        del self.app("AzMeet")["ios"]["usage"]["NSMicrophoneUsageDescription"]
        errors = demo_apps.check(self.m)
        self.assertTrue(any("AzMeet" in e and "NSMicrophoneUsageDescription" in e and "RECORD_AUDIO" in e for e in errors), errors)

    def test_a_key_that_is_no_usage_description_or_has_no_text_is_an_error(self):
        self.app("AzMeet")["ios"]["usage"]["NSCameraUsageDescription"] = "  "
        self.app("AzMaps")["ios"]["usage"]["CFBundleName"] = "x"
        errors = demo_apps.check(self.m)
        self.assertTrue(any("AzMeet" in e and "NSCameraUsageDescription" in e and "text" in e for e in errors), errors)
        self.assertTrue(any("AzMaps" in e and "CFBundleName" in e for e in errors), errors)

    def test_the_texts_say_something_to_a_person(self):
        for a in self.m["app"]:
            for key, text in ((a.get("ios") or {}).get("usage") or {}).items():
                self.assertGreaterEqual(len(text), 25, (a["package"], key))
                self.assertTrue(text.endswith("."), (a["package"], key))
                self.assertNotRegex(text, r"(?i)todo|lorem|xxx|placeholder")

    def test_the_ios_build_script_runs_the_symbol_gate_after_the_bundle_and_before_it_signs(self):
        text = (HERE / "build-ios.sh").read_text(encoding="utf-8")
        gate, sign = text.index('python3 "$ASSET_ROOT/scripts/ci/ios_symbol_gate.py"'), text.index("codesign --force")
        self.assertLess(gate, sign)
        self.assertGreater(gate, text.index("strip -S -x"))
        self.assertIn('[[ -f "$ASSET_ROOT/scripts/ci/ios_symbol_gate.py" ]]', text)   # a checkout without the gate builds as before
        for needle in ('--binary "$BUNDLE_DIR/$APP_NAME"', '--min-os "$MIN_OS"', '--sdk "$(xcrun --sdk "$SDK_SHORT" --show-sdk-path)"',
                       "AZ_IOS_SYMBOL_JSON"):
            self.assertIn(needle, text)

    def test_the_builder_names_the_file_the_symbol_gate_writes_its_numbers_to(self):
        self.assertIn("AZ_IOS_SYMBOL_JSON", (HERE / "demo_build.py").read_text(encoding="utf-8"))


class FeaturesCommandTest(unittest.TestCase):
    """`demo_apps.py features <leg> <variant>`: the cargo features of a library of the plan, for the scripts that build one by hand
    (scripts/ci/ios_build_qa.sh): the same string the workflow's lib job gets, android-activity included on Android."""

    def features(self, *argv):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            code = demo_apps.main(["features", *argv])
        return code, buf.getvalue().strip()

    def test_the_features_of_a_library_are_the_ones_of_the_plan(self):
        plan = demo_apps.plan(demo_apps.load(), e2e=True)
        for kind in ("android", "ios"):
            for e in plan["libs_" + kind]:
                self.assertEqual(self.features(e["leg"], e["variant"]), (0, e["features"]), e["id"])

    def test_the_qa_variant_and_the_plain_one_differ_by_the_debug_server_and_the_hooks(self):
        _, plain = self.features("ios-sim", "mobile")
        _, qa = self.features("ios-sim", "mobile-e2e")
        self.assertTrue(qa.startswith(plain))
        self.assertEqual(sorted(set(qa.split(",")) - set(plain.split(","))), ["debug-server", "e2e-scripting", "test-hooks"])

    def test_a_library_the_plan_does_not_build_is_an_error(self):
        self.assertEqual(self.features("ios-sim", "no-such-variant")[0], 1)
        self.assertEqual(self.features("linux", "mobile")[0], 1)
        self.assertEqual(self.features("ios-sim")[0], 2)


class IosBuildQaScriptTest(unittest.TestCase):
    """scripts/ci/ios_build_qa.sh: build the Simulator app of one app from the `mobile` and the `mobile-e2e` library and run the QA of
    scripts/ci/ios_qa.sh on each, in ONE macOS job."""

    def setUp(self):
        self.path = HERE / "ci" / "ios_build_qa.sh"
        self.text = self.path.read_text(encoding="utf-8")

    def test_the_script_parses_and_builds_each_variant_with_the_library_the_plan_names(self):
        self.assertEqual(subprocess.run(["bash", "-n", str(self.path)]).returncode, 0)
        for needle in ("demo_apps.py features", "demo_lib.sh ios", "demo_build.py ios", "--variant", "ios_qa.sh", "ios_smoke.sh"):
            self.assertIn(needle, self.text)

    def test_the_qa_variant_gets_the_e2e_session_and_the_plain_one_the_lifecycle_session(self):
        self.assertRegex(self.text, r'mobile-e2e\)[^\n]*session=both')
        self.assertIn("session=qa", self.text)

    def test_the_script_deletes_nothing(self):
        code = "\n".join(l for l in self.text.splitlines() if not l.lstrip().startswith("#"))
        self.assertNotRegex(code, r"\brm\b")


class MobileLibCommandTest(unittest.TestCase):
    """`demo_apps.py mobile-lib <package>`: which library variant a phone app links (the manifest's mobile_lib)."""

    def mobile_lib(self, *argv):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(io.StringIO()):
            code = demo_apps.main(["mobile-lib", *argv])
        return code, buf.getvalue().strip()

    def test_a_phone_app_names_its_library(self):
        self.assertEqual(self.mobile_lib("AzWidgets"), (0, "mobile"))
        self.assertEqual(self.mobile_lib("AzCompanion"), (0, "mobile-iroh"))

    def test_an_app_that_has_no_phone_build_or_is_unknown_is_an_error(self):
        self.assertEqual(self.mobile_lib("AzMail")[0], 1)
        self.assertEqual(self.mobile_lib("AzNoSuchApp")[0], 1)
        self.assertEqual(self.mobile_lib()[0], 2)


class AndroidBuildSmokeScriptTest(unittest.TestCase):
    """scripts/ci/android_build_smoke.sh: build the named phone apps for Android x86_64 and boot-test them on the emulator, in ONE job."""

    def setUp(self):
        self.path = HERE / "ci" / "android_build_smoke.sh"
        self.text = self.path.read_text(encoding="utf-8")

    def test_the_script_builds_the_libraries_the_apps_name_then_the_apps_then_runs_the_boot_test_with_the_manifests_expectations(self):
        self.assertEqual(subprocess.run(["bash", "-n", str(self.path)]).returncode, 0)
        order = [self.text.index(n) for n in ("python3 scripts/demo_apps.py mobile-lib", "bash scripts/demo_lib.sh android",
                                              "python3 scripts/demo_build.py android", "bash scripts/ci/android_smoke.sh")]
        self.assertEqual(order, sorted(order))
        for var in ("AZ_SMOKE_TAPS", "AZ_SMOKE_EXPECTS", "AZ_SMOKE_AFTER"):
            self.assertIn(var, self.text)

    def test_it_deletes_only_on_a_runner(self):
        lines = self.text.splitlines()
        for i, line in enumerate(lines):
            if re.search(r"\brm\b", line) and not line.lstrip().startswith("#"):
                self.assertTrue(any('GITHUB_ACTIONS' in l for l in lines[max(0, i - 2):i]), line)


class AndroidBuildAllScriptTest(unittest.TestCase):
    """scripts/ci/android_build_all.sh: every phone app and the QA variant of AzWidgets for Android x86_64 in ONE runner job."""

    def setUp(self):
        self.path = HERE / "ci" / "android_build_all.sh"
        self.text = self.path.read_text(encoding="utf-8")

    def test_the_script_parses_and_builds_the_three_libraries_then_every_app_then_the_qa_build(self):
        self.assertEqual(subprocess.run(["bash", "-n", str(self.path)]).returncode, 0)
        order = [self.text.index(n) for n in ("for variant in mobile mobile-iroh mobile-e2e", "demo_build.py android --target",
                                              "--variant mobile-e2e", "android_elf_check.py demo-out/apps")]
        self.assertEqual(order, sorted(order))
        self.assertIn("demo_apps.py features", self.text)

    def test_the_apps_it_builds_are_the_ones_of_the_plan_and_the_qa_one_is_the_e2e_app_of_the_manifest(self):
        m = demo_apps.load()
        plan = demo_apps.plan(m, e2e=True)
        shipped = next(e["apps"] for e in plan["apps_android"] if e["leg"] == "android-x86_64" and e["variant"] == "")
        self.assertEqual(sorted(shipped.split()), ["AzCompanion", "AzMaps", "AzMeet", "AzPaint", "AzWidgets", "AzWriter"])
        qa = next(e["apps"] for e in plan["apps_android"] if e["leg"] == "android-x86_64" and e["variant"] == "mobile-e2e")
        self.assertEqual(qa, "AzWidgets")
        self.assertIn("--apps AzWidgets", self.text.split("--variant mobile-e2e")[0].rsplit("demo_build.py", 1)[1])

    def test_it_deletes_only_on_a_runner(self):
        lines = self.text.splitlines()
        for i, line in enumerate(lines):
            if re.search(r"\brm\b", line) and not line.lstrip().startswith("#"):
                self.assertTrue(any('GITHUB_ACTIONS' in l for l in lines[max(0, i - 2):i]), line)


class IosPlistTest(unittest.TestCase):
    """The Info.plist of an iOS app is rendered by build-ios.sh from the texts of the manifest: this runs that section of the script
    for real (bash, sed, python3) with the environment demo_build.py gives it and reads the plist back."""

    def render(self, package):
        text = (HERE / "build-ios.sh").read_text(encoding="utf-8")
        start = text.index('PLIST_EXTRA_FILE="$(mktemp)"')
        end = text.index('rm -f "$PLIST_EXTRA_FILE"') + len('rm -f "$PLIST_EXTRA_FILE"')
        app = next(a for a in demo_apps.load()["app"] if a["package"] == package)
        with tempfile.TemporaryDirectory() as d:
            bundle = pathlib.Path(d) / f"{package}.app"
            bundle.mkdir()
            env = dict(os.environ, **demo_build.ios_plist_env(app), ASSET_ROOT=str(demo_apps.ROOT), APP_NAME=package,
                       BUNDLE_ID=app["bundle_id"], DISPLAY_NAME=app["name"], VERSION="0.2.0", BUILD="1", MIN_OS="16.0",
                       BUNDLE_DIR=str(bundle))
            subprocess.run(["bash", "-c", "set -euo pipefail\n" + text[start:end]], env=env, check=True)
            return plistlib.loads((bundle / "Info.plist").read_bytes()), app

    def test_the_companion_asks_for_the_local_network_and_names_the_bonjour_service_of_its_link(self):
        plist, app = self.render("AzCompanion")
        # without these two keys iOS refuses the lookup of the Mac (the denied prompt is what its "Local Network is off" screen shows)
        self.assertEqual(plist["NSLocalNetworkUsageDescription"], app["ios"]["local_network"])
        self.assertEqual(plist["NSBonjourServices"], ["_irohv1._udp"])
        self.assertEqual(plist["NSCameraUsageDescription"], app["ios"]["usage"]["NSCameraUsageDescription"])

    def test_an_app_that_does_not_use_the_local_network_does_not_ask_for_it(self):
        plist, _ = self.render("AzWidgets")
        self.assertNotIn("NSLocalNetworkUsageDescription", plist)
        self.assertNotIn("NSBonjourServices", plist)
        self.assertFalse([k for k in plist if k.endswith("UsageDescription")])

    def test_the_meeting_app_says_why_it_wants_the_camera_and_the_microphone(self):
        plist, app = self.render("AzMeet")
        for key, text in app["ios"]["usage"].items():
            self.assertEqual(plist[key], text)
        self.assertEqual(sorted(k for k in plist if k.endswith("UsageDescription")), ["NSCameraUsageDescription", "NSMicrophoneUsageDescription"])

    def test_every_phone_app_renders_a_plist_with_its_identity_and_the_oldest_ios(self):
        for app in demo_apps.load()["app"]:
            if "ios" not in app["platforms"]:
                continue
            plist, _ = self.render(app["package"])
            self.assertEqual(plist["CFBundleIdentifier"], app["bundle_id"], app["package"])
            self.assertEqual(plist["CFBundleExecutable"], app["package"])
            self.assertEqual(plist["MinimumOSVersion"], "16.0")

    def test_a_text_with_xml_in_it_cannot_break_the_plist(self):
        # the texts are escaped: a quote, an ampersand and a tag are text, not markup
        env = demo_build.ios_plist_env({"ios": {"usage": {"NSCameraUsageDescription": 'Tom & "Jerry" <b>look</b>.'}}})
        self.assertEqual(env["AZ_IOS_USAGE"], 'NSCameraUsageDescription=Tom & "Jerry" <b>look</b>.')


ANDROID_DUMP = """<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
  <node index="0" text="" class="android.widget.FrameLayout" bounds="[0,0][1080,2400]">
    <node index="0" text="Demo Mac" content-desc="" class="android.view.View" bounds="[40,100][600,180]" />
    <node index="1" text="" content-desc="Remote" class="android.view.View" bounds="[0,2200][540,2400]" />
    <node index="2" text="Tone" content-desc="Quick button: Tone" class="android.view.View" bounds="[0,1800][270,1900]" />
  </node>
</hierarchy>
"""
IDB_DUMP = json.dumps([{"AXLabel": "Demo Mac", "type": "StaticText", "frame": {"x": 10, "y": 50, "width": 200, "height": 30}},
                       {"AXLabel": None, "AXValue": "Remote", "title": None, "frame": {"x": 0, "y": 800, "width": 100, "height": 40}}])


class UiFindTest(unittest.TestCase):
    """scripts/ci/ui_find.py: is a text on the screen, by the accessibility dump of uiautomator (Android) or idb (iOS)?"""

    def find(self, text, *words):
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "dump"
            path.write_text(text, encoding="utf-8")
            return ui_find.find(str(path), list(words))

    def test_a_text_or_a_content_description_of_the_android_dump_is_found_whatever_its_case(self):
        got = self.find(ANDROID_DUMP, "Demo Mac", "remote", "Quick button", "Connect to AzReview")
        self.assertEqual(got, {"Demo Mac": True, "remote": True, "Quick button": True, "Connect to AzReview": False})

    def test_a_label_or_a_value_of_the_idb_dump_is_found(self):
        got = self.find(IDB_DUMP, "demo mac", "Remote", "Files")
        self.assertEqual(got, {"demo mac": True, "Remote": True, "Files": False})

    def test_a_dump_that_cannot_be_read_finds_nothing(self):
        self.assertEqual(self.find("<hierarchy><node text='Demo Mac'", "Demo Mac"), {"Demo Mac": False})
        self.assertEqual(self.find("", "Demo Mac"), {"Demo Mac": False})
        self.assertEqual(ui_find.find("/no/such/dump", ["Demo Mac"]), {"Demo Mac": False})

    def test_the_command_prints_the_answer_as_json(self):
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "dump.xml"
            path.write_text(ANDROID_DUMP, encoding="utf-8")
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                self.assertEqual(ui_find.main([str(path), "Demo Mac", "Settings"]), 0)
            self.assertEqual(json.loads(buf.getvalue()), {"Demo Mac": True, "Settings": False})
            self.assertEqual(ui_find.main([]), 2)


class AfterTapTest(unittest.TestCase):
    """What must be on the screen once the button the manifest names was pressed (AzCompanion: the remote with its pretend Mac, and
    the pairing screen gone): the texts come from [app.smoke] after_tap and after_tap_gone."""

    BASE = {"alive": True, "alive_after": True, "screen_nonblank": True, "home_similarity": 0.2, "differs_from_home": True}

    def tap(self, **kw):
        return {**self.BASE, "tap": {"label": "Try it without a Mac", "alive": True, "fatal": None, "changed_screen": True, **kw}}

    def test_the_remote_is_there_and_the_pairing_screen_is_gone(self):
        facts = self.tap(after_found={"Demo Mac": True}, after_present={"Connect to AzReview": False})
        self.assertEqual(smoke_verdict.judge(facts), (True, "alive twice, on the screen, drawn, no fatal line"))

    def test_a_text_that_does_not_come_up_fails_the_app_and_says_which(self):
        passed, reason = smoke_verdict.judge(self.tap(after_found={"Demo Mac": False}, after_present={"Connect to AzReview": False}))
        self.assertFalse(passed)
        self.assertIn('"Try it without a Mac"', reason)
        self.assertIn('"Demo Mac"', reason)
        self.assertIn("does not show", reason)

    def test_a_text_that_must_be_gone_but_is_still_there_fails_the_app(self):
        passed, reason = smoke_verdict.judge(self.tap(after_found={"Demo Mac": True}, after_present={"Connect to AzReview": True}))
        self.assertFalse(passed)
        self.assertIn('"Connect to AzReview"', reason)
        self.assertIn("still shows", reason)

    def test_a_screen_that_did_not_change_is_the_first_thing_said(self):
        passed, reason = smoke_verdict.judge(self.tap(changed_screen=False, screen_similarity=1.0, after_found={"Demo Mac": False}))
        self.assertFalse(passed)
        self.assertIn("changed nothing", reason)

    def test_the_result_file_carries_the_questions_and_the_answers(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "home.png").write_bytes(synthetic_png(96, 160, home_screen))
            (root / "before.png").write_bytes(synthetic_png(96, 160, app_screen))
            (root / "after.png").write_bytes(synthetic_png(96, 160, lambda x, y: (30, 90, 200, 255) if (x + y) % 5 else (250, 250, 250, 255)))
            (root / "tap.json").write_text(json.dumps({"label": "Try", "found": "by its name", "alive": True, "fatal": None,
                                                       "after_found": {"Demo Mac": True}, "after_present": {"Connect to AzReview": False}}))
            argv = ["--out", str(root / "r.json"), "--app", "AzX", "--id", "x", "--alive", "1", "--alive-after", "1",
                    "--baseline", str(root / "home.png"), "--after", str(root / "before.png"), "--tap", str(root / "tap.json"),
                    "--tap-before", str(root / "before.png"), "--tap-after", str(root / "after.png")]
            self.assertEqual(smoke_verdict.main(argv), 0)
            tap = json.loads((root / "r.json").read_text())["tap"]
            self.assertEqual((tap["after_found"], tap["after_present"]), ({"Demo Mac": True}, {"Connect to AzReview": False}))

    def test_the_manifest_names_them_for_the_companion_and_the_plan_hands_them_on(self):
        m = demo_apps.load()
        smoke = next(a for a in m["app"] if a["package"] == "AzCompanion")["smoke"]
        self.assertEqual(smoke["after_tap"], ["Demo Mac"])
        self.assertEqual(smoke["after_tap_gone"], ["Connect to AzReview"])
        self.assertEqual(json.loads(demo_apps.plan(m)["smoke_after"]),
                         {"AzCompanion": {"found": ["Demo Mac"], "gone": ["Connect to AzReview"], "probe_tap": "Scan QR code",
                                          "probe_found": ["Scanning the code comes with the next version"]}})

    def test_a_first_press_with_a_short_effect_is_looked_at_one_second_after_it(self):
        # "Scan QR code" only shows a toast of 2.5 s ("Scanning the code comes with the next version ..."): if it is on the screen 1 s after
        # the press, a tap reaches the app's callbacks; if not, none does (the input path)
        passed, reason = smoke_verdict.judge(self.tap(probe_label="Scan QR code", probe_found={"Scanning the code": False},
                                                      after_found={"Demo Mac": True}))
        self.assertFalse(passed)
        self.assertIn('1 s after pressing "Scan QR code"', reason)
        self.assertIn('"Scanning the code"', reason)
        self.assertTrue(smoke_verdict.judge(self.tap(probe_label="Scan QR code", probe_found={"Scanning the code": True},
                                                     after_found={"Demo Mac": True}))[0])

    def test_the_probe_answers_come_from_the_dump_of_that_moment(self):
        env = {"AZ_SMOKE_AFTER": json.dumps({"AzCompanion": {"found": ["Demo Mac"], "gone": [], "probe_tap": "Scan QR code",
                                                              "probe_found": ["scanning the code"]}})}
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "probe.xml"
            path.write_text('<hierarchy><node text="Scanning the code comes with the next version." content-desc="" /></hierarchy>')
            self.assertEqual(ui_find.probe(str(path), "AzCompanion", env), {"probe_label": "Scan QR code", "probe_found": {"scanning the code": True}})
            self.assertEqual(ui_find.probe(str(path), "AzWidgets", env), {})
            self.assertEqual(ui_find.probe_label("AzCompanion", env), "Scan QR code")
            self.assertEqual(ui_find.probe_label("AzWidgets", env), "")

    def test_a_probe_needs_a_tap_and_a_list_of_texts(self):
        m = demo_apps.load()
        next(a for a in m["app"] if a["package"] == "AzWidgets")["smoke"] = {"probe_tap": "X", "probe_after": ["y"]}
        self.assertTrue(any("AzWidgets" in e and "probe" in e and "tap" in e for e in demo_apps.check(m)))
        m = demo_apps.load()
        next(a for a in m["app"] if a["package"] == "AzCompanion")["smoke"]["probe_after"] = "text"
        self.assertTrue(any("AzCompanion" in e and "probe_after" in e and "list" in e for e in demo_apps.check(m)))
        m = demo_apps.load()
        next(a for a in m["app"] if a["package"] == "AzCompanion")["smoke"]["probe_tap"] = ""
        self.assertTrue(any("AzCompanion" in e and "probe_tap" in e for e in demo_apps.check(m)))

    def test_the_android_boot_test_turns_the_apps_info_log_on_for_an_app_with_a_press(self):
        text = (HERE / "ci" / "android_smoke.sh").read_text(encoding="utf-8")
        self.assertIn("setprop debug.az.log info", text)    # AZ_LOG=info: the app's own lines of what a press did ("tap <Action>")
        self.assertIn("screenshot-probe.png", text)
        self.assertIn("--probe-label", text)

    def test_a_text_after_a_tap_needs_a_tap_and_is_a_list_of_texts(self):
        m = demo_apps.load()
        app = next(a for a in m["app"] if a["package"] == "AzWidgets")
        app["smoke"] = {"after_tap": ["Demo Mac"]}
        self.assertTrue(any("AzWidgets" in e and "after_tap" in e and "tap" in e for e in demo_apps.check(m)))
        m = demo_apps.load()
        next(a for a in m["app"] if a["package"] == "AzCompanion")["smoke"]["after_tap"] = "Demo Mac"
        self.assertTrue(any("AzCompanion" in e and "after_tap" in e and "list" in e for e in demo_apps.check(m)))

    def test_both_boot_tests_look_at_the_screen_after_the_press(self):
        for name in ("android_smoke.sh", "ios_smoke.sh"):
            text = (HERE / "ci" / name).read_text(encoding="utf-8")
            self.assertIn("AZ_SMOKE_AFTER", text, name)
            self.assertIn("ui_find.py", text, name)
            self.assertIn("--probe-label", text, name)     # the first press with a short effect (a toast), looked at 1 s after it
            self.assertIn("screenshot-probe.png", text, name)


class ApkGateTest(unittest.TestCase):
    """verify_apk is the gate of a built APK: what the phone checks before it runs anything, and that every library of it can be
    loaded on a device with 16 KiB pages (scripts/android_elf_check.py)."""

    def make_apk(self, root, aligns, name="AzX.apk"):
        sys.path.insert(0, str(HERE))
        import test_android_elf_check as t
        path = pathlib.Path(root) / name
        t.apk(path, [("classes.dex", b"dex\n035", False, 0),
                     ("lib/arm64-v8a/libazul.so", t.elf(aligns), False, 0),
                     ("lib/arm64-v8a/libazul_x.so", t.elf(aligns), False, 0)])
        return path

    def test_an_apk_with_4_kib_libraries_is_a_failed_app_that_names_the_file(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(RuntimeError) as caught:
                demo_build.verify_apk(self.make_apk(d, [0x1000] * 3), "arm64-v8a")
        self.assertIn("AzX.apk!lib/arm64-v8a/libazul.so", str(caught.exception))
        self.assertIn("p_align 0x1000", str(caught.exception))

    def test_an_apk_with_16_kib_libraries_passes(self):
        with tempfile.TemporaryDirectory() as d:
            demo_build.verify_apk(self.make_apk(d, [0x4000] * 3), "arm64-v8a")


class StatusTest(unittest.TestCase):
    def test_a_missing_result_is_unexplained_and_a_known_failure_is_not(self):
        m = demo_apps.load()
        data, md = demo_apps.status([], m=m)
        self.assertIn("AzWidgets/linux", data["summary"]["unexplained_failures"])
        self.assertIn("MISSING", md)

    def test_every_cell_names_the_file_it_stands_for_even_when_the_build_failed_or_never_reported(self):
        # the page's link pruning needs the names of the failed cells too (their `artifact` is null): they are in the status
        m = demo_apps.load()
        data, _ = demo_apps.status([], m=m)
        for row in data["apps"]:
            app = next(a for a in m["app"] if a["package"] == row["app"])
            for col, cell in row["cells"].items():
                self.assertEqual(cell["staged"], demo_apps.staged_name(app, col), (row["app"], col))
                self.assertEqual(cell["release"], demo_apps.release_name(app, col), (row["app"], col))

    def test_results_and_boot_tests_fill_the_cells(self):
        m = demo_apps.load()
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "r").mkdir()
            (root / "r" / "AzWidgets.android-x86_64.result.json").write_text(json.dumps({
                "app": "AzWidgets", "column": "android-x86_64", "status": "built", "size": 2048, "shipped_size": 2048,
                "seconds": 12, "engine_crates": [], "artifact": "AzWidgets-android-x86_64.apk"}))
            (root / "s" / "AzWidgets-android-x86_64").mkdir(parents=True)
            (root / "s" / "AzWidgets-android-x86_64" / "result.json").write_text(json.dumps({
                "app": "AzWidgets-android-x86_64", "alive": True, "fatal": None, "screen_nonblank": True}))
            data, md = demo_apps.status([root / "r", root / "s"], m=m)
        cell = next(r for r in data["apps"] if r["app"] == "AzWidgets")["cells"]["android-x86_64"]
        self.assertEqual((cell["status"], cell["boot"]), ("built", "boots"))
        self.assertEqual(data["summary"]["assets"], 1)
        self.assertIn("built, boots", md)


class PublishTest(unittest.TestCase):
    def run_dir(self, root, names):
        out = root / "demoapps-x" / "demos-out"
        out.mkdir(parents=True)
        for n in names:
            (out / n).write_bytes(b"binary of " + n.encode())
        (out / "logs").mkdir()
        (out / "logs" / "AzWidgets-linux").write_bytes(b"a log that happens to share the name")
        return root

    def test_a_desktop_binary_is_shipped_as_a_tar_gz_and_a_macos_one_with_its_unquarantine_script(self):
        with tempfile.TemporaryDirectory() as d:
            root = self.run_dir(pathlib.Path(d), ["AzWidgets-linux", "AzWidgets-macos", "AzWidgets-windows.exe"])
            out = pathlib.Path(d) / "release"
            listing = demo_publish.assemble(root, out, "0.2.0", "abc")
            self.assertEqual(sorted(x["name"] for x in listing),
                             ["AzWidgets-linux.tar.gz", "AzWidgets-macos.tar.gz", "AzWidgets-windows.exe.tar.gz"])
            with tarfile.open(out / "AzWidgets-macos.tar.gz") as t:
                self.assertEqual(sorted(t.getnames()), ["AzWidgets-macos", "README-macOS.txt", "unquarantine.sh"])
                self.assertEqual(t.getmember("AzWidgets-macos").mode, 0o755)
                self.assertIn(b'exec "./AzWidgets-macos"', t.extractfile("unquarantine.sh").read())
            with tarfile.open(out / "AzWidgets-linux.tar.gz") as t:
                self.assertEqual(t.getnames(), ["AzWidgets-linux"])

    def test_a_phone_file_is_shipped_as_it_is_and_the_listing_names_every_asset(self):
        with tempfile.TemporaryDirectory() as d:
            root = self.run_dir(pathlib.Path(d), ["AzCompanion-android-arm64.apk", "AzCompanion-ios-device.app.zip"])
            out = pathlib.Path(d) / "release"
            demo_publish.assemble(root, out, "0.2.0", "abc")
            self.assertEqual((out / "AzCompanion-android-arm64.apk").read_bytes(), b"binary of AzCompanion-android-arm64.apk")
            listing = json.loads((out / "release-assets.json").read_text())
            self.assertEqual(sorted(x["name"] for x in listing),
                             ["AzCompanion-android-arm64.apk", "AzCompanion-ios-device.app.zip", "companion.json"])

    def test_companion_json_names_every_file_with_its_hash_and_says_it_is_unsigned(self):
        with tempfile.TemporaryDirectory() as d:
            root = self.run_dir(pathlib.Path(d), ["AzCompanion-android-arm64.apk", "AzCompanion-ios-sim.app.zip"])
            out = pathlib.Path(d) / "release"
            demo_publish.assemble(root, out, "0.2.0", "deadbeef")
            c = json.loads((out / "companion.json").read_text())
        self.assertEqual((c["schema"], c["app"], c["release"], c["git"]), (1, "AzCompanion", "0.2.0", "deadbeef"))
        self.assertEqual([f["name"] for f in c["files"]], ["AzCompanion-android-arm64.apk", "AzCompanion-ios-sim.app.zip"])
        self.assertEqual([f["platform"] for f in c["files"]], ["android-arm64", "ios-sim"])
        for f in c["files"]:
            self.assertRegex(f["sha256"], r"^[0-9a-f]{64}$")
            self.assertGreater(f["size"], 0)
        self.assertIsNone(c["signature"])
        self.assertIn("unsigned", c["signature_note"])
        self.assertIsInstance(c["protocol_version"], int)

    def test_a_run_without_the_companion_makes_no_companion_json(self):
        with tempfile.TemporaryDirectory() as d:
            root = self.run_dir(pathlib.Path(d), ["AzWidgets-linux"])
            out = pathlib.Path(d) / "release"
            demo_publish.assemble(root, out, "0.2.0", "abc")
            self.assertFalse((out / "companion.json").exists())


class IconTest(unittest.TestCase):
    def test_an_icon_is_a_valid_png_of_the_asked_size(self):
        rows = demo_icon.render("AzMaps", 48, True)
        data = demo_icon.png(rows, 48, rgba=True)
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
        self.assertEqual(int.from_bytes(data[16:20], "big"), 48)
        self.assertEqual(int.from_bytes(data[20:24], "big"), 48)
        self.assertEqual(len(rows), 48)
        self.assertTrue(all(len(r) == 48 * 4 for r in rows))

    def test_the_corners_of_an_android_icon_are_transparent_and_an_ios_icon_is_square(self):
        self.assertEqual(demo_icon.render("AzMaps", 48, True)[0][3], 0)
        self.assertEqual(demo_icon.render("AzMaps", 48, False)[0][3], 255)

    def test_apps_have_their_own_colour_and_the_companion_its_blue(self):
        self.assertNotEqual(demo_icon.colour("AzMaps"), demo_icon.colour("AzPaint"))
        r, g, b = demo_icon.colour("AzCompanion")
        self.assertGreater(b, r)
        # the same phone remote under its display name: the same blue, the same letters
        self.assertEqual(demo_icon.colour("AzReview Companion"), demo_icon.colour("AzCompanion"))
        self.assertEqual(demo_icon.letters("AzReview Companion"), demo_icon.letters("AzCompanion"))

    def test_the_icons_of_a_platform_land_where_its_packager_looks(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            demo_icon.main(["android", "AzMaps", str(root / "res")])
            demo_icon.main(["ios", "AzMaps", str(root / "app")])
            self.assertTrue((root / "res" / "mipmap-xxxhdpi" / "ic_launcher.png").is_file())
            self.assertTrue((root / "app" / "AppIcon60x60@3x.png").is_file())


class IconLettersTest(unittest.TestCase):
    """The launcher icon says which app it is: the first letters of its name on a rounded square (no art of anybody's)."""

    def test_the_letters_are_the_first_two_after_the_family_prefix_or_the_humps_of_a_long_name(self):
        expect = {"AzWidgets": "WI", "AzMaps": "MA", "AzBuilder": "BU", "AzPdf": "PD", "AzERP": "ER", "AzInstallManager": "IM",
                  "AzPasswordManager": "PM", "AzVideoCut": "VC", "AzCompanion": "RC", "AzReview Companion": "RC",
                  "Notes": "NO", "AzX": "X"}
        for name, letters in expect.items():
            self.assertEqual(demo_icon.letters(name), letters, name)

    def test_every_app_of_the_manifest_gets_two_letters_of_its_own_alphabet(self):
        for app in demo_apps.load()["app"]:
            letters = demo_icon.letters(app["package"])
            self.assertTrue(1 <= len(letters) <= 2, app["package"])
            self.assertTrue(all(c in demo_icon.FONT for c in letters), (app["package"], letters))

    def test_the_font_has_every_letter_and_digit_as_seven_rows_of_five_and_no_two_alike(self):
        self.assertEqual(sorted(demo_icon.FONT), sorted("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789?"))
        for ch, rows in demo_icon.FONT.items():
            self.assertEqual(len(rows), 7, ch)
            for row in rows:
                self.assertRegex(row, r"^[#.]{5}$", ch)
        self.assertEqual(len({tuple(r) for r in demo_icon.FONT.values()}), len(demo_icon.FONT))

    def test_the_cells_of_the_icon_are_the_cells_of_the_font(self):
        # the centre of every cell of the two letters is lit exactly where the font has a '#'
        for text in ("WI", "MA", "RC"):
            for letter_index, ch in enumerate(text):
                for row in range(7):
                    for col in range(5):
                        x, y = demo_icon.cell_centre(text, letter_index, col, row)
                        self.assertEqual(demo_icon.lit(text, x, y), demo_icon.FONT[ch][row][col] == "#", (text, ch, col, row))

    def test_the_letters_fit_inside_the_rounded_square(self):
        x0, y0, x1, y1 = demo_icon.text_box("WI")
        self.assertGreaterEqual(x0, 0.12)
        self.assertLessEqual(x1, 0.88)
        self.assertGreaterEqual(y0, 0.2)
        self.assertLessEqual(y1, 0.8)
        # the box is centred
        self.assertAlmostEqual((x0 + x1) / 2, 0.5, places=3)
        self.assertAlmostEqual((y0 + y1) / 2, 0.5, places=3)

    def test_the_picture_shows_the_letters_in_white_and_two_apps_with_other_letters_look_other(self):
        def white(name):
            rows = demo_icon.render(name, 96, False)
            return [[row[i * 4:i * 4 + 3] == b"\xff\xff\xff" for i in range(96)] for row in rows]
        wi, ma, ma2 = white("AzWidgets"), white("AzMaps"), white("AzMail")
        self.assertGreater(sum(map(sum, wi)), 300)       # there is something white
        self.assertNotEqual(wi, ma)                      # other letters, other picture
        self.assertEqual(ma, ma2)                        # the same letters in the same place (the colours of the two differ)
        self.assertNotEqual(demo_icon.colour("AzMaps"), demo_icon.colour("AzMail"))

    def test_the_old_family_letter_is_gone(self):
        self.assertFalse(hasattr(demo_icon, "STROKES"))


def synthetic_png(width, height, pixel):
    """An RGBA PNG (filter 0) of pixel(x, y) -> (r, g, b, a)."""
    import struct
    import zlib

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    raw = b"".join(b"\x00" + b"".join(bytes(pixel(x, y)) for x in range(width)) for y in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def home_screen(x, y):          # a wallpaper-like gradient with a row of "icons"
    if 40 <= y < 60 and (x // 16) % 2 == 0:
        return (230, 230, 240, 255)
    return ((x * 3) % 256, (y * 2) % 256, 160, 255)


def app_screen(x, y):           # a page: white, with lines of glyph-like marks in grey ramps (anti-aliased edges)
    if 24 <= y < 136 and y % 9 < 4 and (x * 7 + y * 3) % 23 < 15:
        v = 40 + (x * 5 + y) % 120
        return (v, v, v, 255)
    return (250, 250, 250, 255)


def black_window_under_bars(x, y):   # a window that never drew: black, but the system draws its bars over it
    if y < 8 or y >= 150:
        v = (x * 13 + y * 5) % 256   # the clock, the icons, the navigation shapes: anti-aliased, many values
        return (v, v, v, 255)
    return (0, 0, 0, 255)


def white_window_under_bars(x, y):
    if y < 8 or y >= 150:
        v = (x * 13 + y * 5) % 256
        return (v, v, v, 255)
    return (255, 255, 255, 255)


class VerdictTest(unittest.TestCase):
    def pictures(self, d, after):
        root = pathlib.Path(d)
        (root / "home.png").write_bytes(synthetic_png(96, 160, home_screen))
        (root / "after.png").write_bytes(synthetic_png(96, 160, after))
        return str(root / "home.png"), str(root / "after.png")

    def verdict(self, d, after, **kw):
        home, shot = self.pictures(d, after)
        argv = ["--out", str(pathlib.Path(d) / "result.json"), "--app", "AzX", "--id", "com.azul.AzX", "--alive", kw.get("alive", "1"),
                "--alive-after", kw.get("alive_after", "1"), "--fatal", kw.get("fatal", ""), "--baseline", home, "--after", shot]
        if "resumed" in kw:
            argv += ["--resumed", kw["resumed"]]
        if "band" in kw:
            argv += ["--band", kw["band"]]
        code = smoke_verdict.main(argv)
        return code, json.loads((pathlib.Path(d) / "result.json").read_text())

    def test_an_app_that_is_alive_twice_and_drawn_over_the_home_screen_boots(self):
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, app_screen)
        self.assertEqual(code, 0)
        self.assertTrue(r["pass"] and r["differs_from_home"] and r["screen_nonblank"])

    def test_the_home_screen_left_behind_is_a_failure_even_though_it_is_not_black(self):
        # the iOS run of 2026-11-26: the app had died, the screenshot was the home screen, and a "not black" test passed it
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, home_screen)
        self.assertEqual(code, 1)
        self.assertFalse(r["pass"])
        self.assertIn("home screen is still showing", r["reason"])
        self.assertTrue(r["screen_nonblank"])

    def test_the_clock_of_the_status_bar_does_not_make_a_home_screen_a_different_picture(self):
        def with_clock(x, y):
            return (255, 255, 255, 255) if y < 4 and x < 24 else home_screen(x, y)
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, with_clock)
        self.assertEqual(code, 1)
        self.assertGreater(r["home_similarity"], 0.985)

    def test_dying_at_the_second_look_is_a_failure_with_its_fatal_line(self):
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, app_screen, alive_after="0", fatal="SIGABRT")
        self.assertEqual(code, 1)
        self.assertIn("second look", r["reason"])
        self.assertIn("SIGABRT", r["reason"])

    def test_not_alive_at_all_comes_first(self):
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, app_screen, alive="0", alive_after="0")
        self.assertEqual(code, 1)
        self.assertIn("not alive", r["reason"])

    def test_a_flat_colour_is_a_window_that_never_drew(self):
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, lambda x, y: (0, 0, 0, 255))
        self.assertEqual(code, 1)
        self.assertIn("flat colour", r["reason"])

    def test_the_system_naming_another_app_as_resumed_fails_it(self):
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, app_screen, resumed="no")
        self.assertEqual(code, 1)
        self.assertIn("launcher is in front", r["reason"])

    def test_a_fatal_line_fails_an_app_that_looks_fine(self):
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, app_screen, fatal="Fatal signal 11")
        self.assertEqual(code, 1)
        self.assertIn("Fatal signal 11", r["reason"])

    def test_a_button_that_took_the_app_down_and_a_missing_text_fail_it(self):
        base = {"alive": True, "alive_after": True, "screen_nonblank": True, "home_similarity": 0.2, "differs_from_home": True}
        self.assertFalse(smoke_verdict.judge({**base, "tap": {"label": "Try", "alive": False, "fatal": None}})[0])
        self.assertFalse(smoke_verdict.judge({**base, "expect": {"text": "Connect", "found": False}})[0])
        self.assertTrue(smoke_verdict.judge({**base, "expect": {"text": "Connect", "found": True}, "tap": {"alive": True}})[0])

    def test_pictures_of_different_sizes_are_different_pictures(self):
        with tempfile.TemporaryDirectory() as d:
            (pathlib.Path(d) / "a.png").write_bytes(synthetic_png(8, 8, home_screen))
            (pathlib.Path(d) / "b.png").write_bytes(synthetic_png(16, 8, home_screen))
            self.assertEqual(png_compare.same_fraction(png_compare.decode(str(pathlib.Path(d) / "a.png")),
                                                       png_compare.decode(str(pathlib.Path(d) / "b.png"))), 0.0)

    def test_a_black_window_under_the_system_bars_is_a_window_that_never_drew(self):
        # the status bar and the navigation bar are in every screenshot: a picture counted whole is "drawn" because of them
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, black_window_under_bars)
        self.assertEqual(code, 1)
        self.assertTrue(r["differs_from_home"])
        self.assertFalse(r["screen_nonblank"])
        self.assertIn("flat colour", r["reason"])

    def test_a_white_window_under_the_system_bars_is_a_window_that_never_drew(self):
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, white_window_under_bars)
        self.assertEqual(code, 1)
        self.assertIn("flat colour", r["reason"])

    def test_a_little_content_in_the_window_is_enough(self):
        def one_button(x, y):   # black, with a single grey button: 3 percent of the window
            return (90, 90, 90, 255) if 60 <= y < 70 and 20 <= x < 76 else (0, 0, 0, 255)
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, one_button)
        self.assertEqual(code, 0)
        self.assertTrue(r["screen_nonblank"])

    def test_a_screenshot_that_was_never_taken_or_cannot_be_read_says_so(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "home.png").write_bytes(synthetic_png(96, 160, home_screen))
            base = ["--out", str(root / "r.json"), "--app", "AzX", "--alive", "1", "--alive-after", "1"]
            self.assertEqual(smoke_verdict.main(base + ["--baseline", str(root / "home.png"), "--after", str(root / "none.png")]), 1)
            self.assertIn("no screenshot", json.loads((root / "r.json").read_text())["reason"])
            (root / "torn.png").write_bytes(synthetic_png(96, 160, app_screen)[:200])   # cut off, as a hiccup of the emulator does
            self.assertEqual(smoke_verdict.main(base + ["--baseline", str(root / "home.png"), "--after", str(root / "torn.png")]), 1)
            self.assertIn("cannot be read", json.loads((root / "r.json").read_text())["reason"])

    def test_the_platforms_own_launch_numbers_are_read_from_the_logs_and_never_decide(self):
        # `am start -W` prints TotalTime, the activity manager logs "Displayed <pkg>/<activity>: +1s234ms": informational only
        start = "Starting: Intent { cmp=com.azul.AzWidgets/com.azul.app.AzulActivity }\nStatus: ok\nLaunchState: COLD\nTotalTime: 1311\nWaitTime: 1325\n"
        log = ("11-26 07:01:02.123  1234  1260 I ActivityTaskManager: Displayed com.android.launcher/.Launcher: +300ms\n"
               "11-26 07:01:04.456  1234  1260 I ActivityTaskManager: Displayed com.azul.AzWidgets/com.azul.app.AzulActivity: +1s234ms\n")
        self.assertEqual(smoke_verdict.parse_total_time(start), 1311)
        self.assertEqual(smoke_verdict.parse_displayed(log, "com.azul.AzWidgets"), 1234)
        self.assertEqual(smoke_verdict.parse_displayed(log.replace("+1s234ms", "+842ms"), "com.azul.AzWidgets"), 842)
        self.assertEqual(smoke_verdict.parse_displayed(log.replace("+1s234ms", "+2s0ms"), "com.azul.AzWidgets"), 2000)
        self.assertIsNone(smoke_verdict.parse_displayed(log, "com.azul.AzMaps"))
        self.assertIsNone(smoke_verdict.parse_total_time("Error: Activity class does not exist."))
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "start.log").write_text(start)
            (root / "logcat.txt").write_text(log)
            home = root / "home.png"
            home.write_bytes(synthetic_png(96, 160, home_screen))
            shot = root / "after.png"
            shot.write_bytes(synthetic_png(96, 160, app_screen))
            argv = ["--out", str(root / "r.json"), "--app", "AzX", "--id", "com.azul.AzWidgets", "--alive", "1", "--alive-after", "1",
                    "--baseline", str(home), "--after", str(shot)]
            self.assertEqual(smoke_verdict.main(argv + ["--start-log", str(root / "start.log"), "--logcat", str(root / "logcat.txt")]), 0)
            r = json.loads((root / "r.json").read_text())
            self.assertEqual((r["launch_ms"], r["displayed_ms"]), (1311, 1234))
            # a log without the numbers costs nothing: the verdict is the same
            self.assertEqual(smoke_verdict.main(argv + ["--start-log", str(root / "missing.log")]), 0)
            self.assertNotIn("launch_ms", json.loads((root / "r.json").read_text()))
        text = demo_apps.boot_text({"pass": True, "launch_ms": 1311, "displayed_ms": 1234})
        self.assertEqual(text, "boots, first window 1234 ms (the platform's own number)")
        self.assertEqual(demo_apps.boot_text({"pass": True, "launch_ms": 1311}), "boots, launched in 1311 ms (the platform's own number)")
        self.assertEqual(demo_apps.boot_text({"pass": False, "reason": "x", "displayed_ms": 1}), "BOOT FAILED: x")

    def test_an_app_that_is_a_toolbar_and_a_blank_canvas_has_drawn_on_android(self):
        # AzPaint on the emulator, 2026-11-26: a dark toolbar under the status bar (3.75 to 11.7 percent of the height) and an empty
        # canvas. The band of 12 to 88 percent saw a flat canvas and failed an app that had drawn: the band of an Android screenshot
        # starts under the status bar (4.5 percent) and ends above the navigation bar (94)
        def toolbar_and_canvas(x, y):
            if y < 6 or y >= 152:
                return ((x * 13 + y * 5) % 256,) * 3 + (255,)     # the system bars: many colour values
            return (40, 40, 44, 255) if y < 19 and (x // 5) % 2 else (45, 45, 48, 255) if y < 19 else (250, 250, 247, 255)
        with tempfile.TemporaryDirectory() as d:
            code, r = self.verdict(d, toolbar_and_canvas, band="0.045,0.94")
            self.assertEqual(code, 0, r["reason"])
            self.assertTrue(r["screen_nonblank"])
            self.assertEqual(r["band"], [0.045, 0.94])
            # the same picture under the default band (8 to 94 percent) still sees the toolbar's lower rows
            code, r = self.verdict(d, toolbar_and_canvas)
            self.assertEqual(code, 0, r["reason"])

    def test_each_platform_keeps_its_own_system_bars_out_of_the_band(self):
        def black_under(top, bottom):
            def pixel(x, y):
                if y < top or y >= bottom:
                    return ((x * 13 + y * 5) % 256,) * 3 + (255,)
                return (0, 0, 0, 255)
            return pixel
        with tempfile.TemporaryDirectory() as d:
            # Android: status bar 3.75 percent, navigation bar from 95 percent of the height
            code, r = self.verdict(d, black_under(6, 152), band="0.045,0.94")
            self.assertEqual(code, 1)
            self.assertIn("flat colour", r["reason"])
            # iOS: status bar and Dynamic Island 6.9 percent, home indicator from 97 percent
            code, r = self.verdict(d, black_under(11, 155), band="0.075,0.965")
            self.assertEqual(code, 1)
            self.assertIn("flat colour", r["reason"])
            # the default band is the conservative one and holds for both
            self.assertEqual(self.verdict(d, black_under(6, 152))[0], 1)
            self.assertEqual(self.verdict(d, black_under(11, 155))[0], 1)

    def test_a_button_that_changed_nothing_on_the_screen_fails_the_app_that_was_asked_to_press_it(self):
        # AzCompanion on the emulator, 2026-11-26: "Try it without a Mac" was found by its name and pressed, the app lived, and the
        # three screenshots before and after were the same picture: the remote did not come up, which the manifest says it must
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "home.png").write_bytes(synthetic_png(96, 160, home_screen))
            (root / "before.png").write_bytes(synthetic_png(96, 160, app_screen))
            (root / "same.png").write_bytes(synthetic_png(96, 160, app_screen))
            (root / "other.png").write_bytes(synthetic_png(96, 160, lambda x, y: (30, 90, 200, 255) if (x + y) % 5 else (250, 250, 250, 255)))
            (root / "tap.json").write_text(json.dumps({"label": "Try it", "found": "by its name", "alive": True, "fatal": None}))
            base = ["--out", str(root / "r.json"), "--app", "AzX", "--id", "com.azul.AzX", "--alive", "1", "--alive-after", "1",
                    "--baseline", str(root / "home.png"), "--after", str(root / "before.png"), "--tap", str(root / "tap.json")]
            self.assertEqual(smoke_verdict.main(base + ["--tap-before", str(root / "before.png"), "--tap-after", str(root / "same.png")]), 1)
            r = json.loads((root / "r.json").read_text())
            self.assertIn('"Try it"', r["reason"])
            self.assertIn("changed nothing", r["reason"])
            self.assertFalse(r["tap"]["changed_screen"])
            self.assertEqual(smoke_verdict.main(base + ["--tap-before", str(root / "before.png"), "--tap-after", str(root / "other.png")]), 0)
            self.assertTrue(json.loads((root / "r.json").read_text())["tap"]["changed_screen"])
            # without the two pictures the question is not asked
            self.assertEqual(smoke_verdict.main(base), 0)
            self.assertNotIn("changed_screen", json.loads((root / "r.json").read_text())["tap"])

    def test_the_engines_first_present_is_read_from_the_startup_line_and_never_decides(self):
        # AZ_STARTUP=1: the app writes `[startup] ...` lines and ONE `[startup-json] {...}` line to stderr (Android: logcat tag
        # RustStdoutStderr, iOS: the console of the process); first_present_ms is counted from the process creation
        json_line = ('[startup-json] {"first_present_ms": 412.345, "before_library_ms": 118.5, "layout_passes": 3, '
                     '"marks": [["window", 130.1]], "render_backend": "backend=gpu requested=auto device=SwiftShader"}')
        logcat = ("11-26 07:42:03.529  2387  2387 I RustStdoutStderr: [startup] window 130.1 ms\\n"
                  "11-26 07:42:03.530  2387  2387 I RustStdoutStderr: " + json_line + "\\n")
        got = smoke_verdict.parse_startup(logcat)
        self.assertEqual((got["first_present_ms"], got["before_library_ms"], got["layout_passes"]), (412.345, 118.5, 3))
        self.assertEqual(got["render_backend"], "backend=gpu requested=auto device=SwiftShader")
        self.assertEqual(smoke_verdict.parse_startup(json_line + "\\n")["first_present_ms"], 412.345)       # the iOS console: the line as it is
        self.assertEqual(smoke_verdict.parse_startup(logcat + "[startup-json] " + json_line[len("[startup-json] "):].replace("412.345", "500.5"))["first_present_ms"], 500.5)   # the last one
        # a line the log cut off (logcat splits what is longer than 4 KB) still gives the numbers that come first
        cut = '11-26 07:42:03.530  2387  2387 I RustStdoutStderr: [startup-json] {"first_present_ms": 99.001, "before_library_ms": 20.0, "layout_pa'
        self.assertEqual(smoke_verdict.parse_startup(cut)["first_present_ms"], 99.001)
        # a run without the switch prints no such line: nothing is said
        self.assertIsNone(smoke_verdict.parse_startup("11-26 07:42:03.529 I azul: NativeTextBridge: input view installed\\n"))
        self.assertIsNone(smoke_verdict.parse_startup(""))
        self.assertIsNone(smoke_verdict.parse_startup("[startup-json] {not json at all"))
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "home.png").write_bytes(synthetic_png(96, 160, home_screen))
            (root / "after.png").write_bytes(synthetic_png(96, 160, app_screen))
            (root / "logcat.txt").write_text(logcat)
            argv = ["--out", str(root / "r.json"), "--app", "AzX", "--id", "com.azul.AzX", "--alive", "1", "--alive-after", "1",
                    "--baseline", str(root / "home.png"), "--after", str(root / "after.png")]
            self.assertEqual(smoke_verdict.main(argv + ["--startup-log", str(root / "logcat.txt"), "--startup-log", str(root / "none.txt")]), 0)
            r = json.loads((root / "r.json").read_text())
            self.assertEqual((r["first_present_ms"], r["before_library_ms"], r["layout_passes"]), (412.345, 118.5, 3))
            self.assertEqual(smoke_verdict.main(argv), 0)
            self.assertNotIn("first_present_ms", json.loads((root / "r.json").read_text()))
        both = demo_apps.boot_text({"pass": True, "displayed_ms": 1233, "first_present_ms": 412.345, "render_backend": "backend=gpu requested=auto device=x"})
        self.assertEqual(both, "boots, first window 1233 ms (the platform's own number), first present 412 ms (the engine's mark), gpu renderer")
        self.assertEqual(demo_apps.boot_text({"pass": True, "first_present_ms": 99.4}), "boots, first present 99 ms (the engine's mark)")
        self.assertEqual(demo_apps.boot_text({"pass": False, "reason": "x", "first_present_ms": 5}), "BOOT FAILED: x")

    def test_the_boot_tests_switch_the_startup_table_on_before_they_launch(self):
        android = (HERE / "ci" / "android_smoke.sh").read_text(encoding="utf-8")
        self.assertLess(android.index("setprop debug.az.startup 1"), android.index("am start -W"))
        ios = (HERE / "ci" / "ios_smoke.sh").read_text(encoding="utf-8")
        self.assertIn("SIMCTL_CHILD_AZ_STARTUP=1", ios)
        self.assertIn("--startup-log", android)
        self.assertIn("--startup-log", ios)

    def test_a_band_that_is_no_two_fractions_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            for bad in ("0.9,0.1", "0.1", "a,b", "-0.1,0.9", "0.1,1.5"):
                with self.assertRaises(SystemExit):
                    self.verdict(d, app_screen, band=bad)

    def test_the_status_text_of_a_failed_boot_carries_the_reason(self):
        text = demo_apps.boot_text({"pass": False, "reason": "the home screen is still showing (99.8% of the pixels are the same)"})
        self.assertTrue(text.startswith("BOOT FAILED: the home screen"))
        self.assertEqual(demo_apps.boot_text({"pass": True}), "boots")


if __name__ == "__main__":
    unittest.main(verbosity=2)
