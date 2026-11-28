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
from unittest import mock

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

_NO_SDK = mock.patch.dict(os.environ, {"ANDROID_HOME": "", "ANDROID_SDK_ROOT": ""})


def setUpModule():
    # verify_apk runs apksigner and aapt2 of the SDK's build-tools when the machine has them (ubuntu-latest has: its preflight job ran
    # them on the made-up APKs of these tests, which have no manifest and no signature, and failed twice). No test of this file is about
    # an SDK, so none sees one, wherever it is run.
    _NO_SDK.start()


def tearDownModule():
    _NO_SDK.stop()


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
        self.assertEqual(wanting, ["AzMeet", "AzCompanion"])

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
        # the desktop QA builds: the same rule, the platform suffix of the shipped file after it
        self.assertEqual(demo_apps.staged_name(w, "linux", e2e=True), "AzWidgets-e2e-linux")
        self.assertEqual(demo_apps.staged_name(w, "macos-arm64", e2e=True), "AzWidgets-e2e-macos")
        self.assertEqual(demo_apps.staged_name(w, "windows", e2e=True), "AzWidgets-e2e-windows.exe")
        self.assertEqual(demo_apps.staged_name(w, "linux"), "AzWidgets-linux")

    def test_without_the_input_a_run_plans_nothing_of_it(self):
        plan = demo_apps.plan(self.m)
        for key, entries in plan.items():
            if key.startswith(("libs_", "apps_")):
                for e in entries:
                    self.assertFalse(str(e.get("variant")).startswith("mobile-e2e"), key)
                    self.assertFalse(e["artifact"].startswith("demoe2e-"), key)

    def test_with_the_input_the_libs_of_the_listed_columns_and_the_apps_that_ask_are_planned(self):
        plan = demo_apps.plan(self.m, e2e=True)
        libs = {(e["leg"], e["variant"]): e for k in ("libs_android", "libs_ios") for e in plan[k] if e["variant"].startswith("mobile-e2e")}
        self.assertEqual(sorted(libs), [("android-arm64", "mobile-e2e"), ("android-arm64", "mobile-e2e-iroh"),
                                        ("android-x86_64", "mobile-e2e"), ("android-x86_64", "mobile-e2e-iroh"),
                                        ("ios-sim", "mobile-e2e"), ("ios-sim", "mobile-e2e-iroh")])
        for (leg, variant), e in libs.items():
            feats = e["features"].split(",")
            self.assertIn("debug-server", feats)
            self.assertIn("e2e-scripting", feats)
            self.assertIn("test-hooks", feats)   # the failure-injection hooks of the QA matrix (AZ_GPU_FAIL, ...), read only when asked
            self.assertEqual("iroh" in feats, variant == "mobile-e2e-iroh")
            self.assertEqual("android-activity" in feats, leg.startswith("android"))
            self.assertEqual(e["artifact"], f"demolib-{leg}-{variant}")
        entries = {(e["leg"], e["variant"]): e for k in ("apps_android", "apps_ios") for e in plan[k]
                   if e.get("variant", "").startswith("mobile-e2e")}
        self.assertEqual(sorted(entries), sorted(libs))
        for (leg, variant), e in entries.items():
            want = sorted(a["package"] for a in self.m["app"] if a.get("e2e") and a["e2e_lib"] == variant and leg in demo_apps.columns_of(a))
            self.assertEqual(sorted(e["apps"].split()), want)
            self.assertEqual(e["lib_artifacts"], f"demolib-{leg}-{variant}")
            self.assertTrue(e["artifact"].startswith("demoe2e-"), e["artifact"])   # a name the shipped-files patterns never match
            self.assertTrue(e["results_artifact"].startswith("demoresults-"))
        # the apps that use iroh build against the iroh twin, the others against the plain QA library
        self.assertEqual(sorted(entries[("android-x86_64", "mobile-e2e-iroh")]["apps"].split()), ["AzCompanion", "AzMeet"])
        self.assertNotIn("AzMeet", entries[("android-x86_64", "mobile-e2e")]["apps"].split())
        # two uploads of one artifact name fail a run: every artifact of every entry has a name of its own
        names = [e["artifact"] for e in entries.values()] + [e["results_artifact"] for e in entries.values()]
        self.assertEqual(len(names), len(set(names)))
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

    def test_the_qa_lib_must_carry_the_debug_server_and_the_columns_of_one_family(self):
        for variant in ("mobile-e2e", "desktop-e2e"):
            m = demo_apps.load()
            m["lib"][variant]["features"] = "std,logging,link-static"
            self.assertTrue(any(variant in e and "debug-server" in e for e in demo_apps.check(m)), variant)
            # phone and desktop columns in one library: its features belong to one kind of build only
            m = demo_apps.load()
            m["lib"][variant]["targets"] = ["linux", "android-x86_64"]
            self.assertTrue(any(variant in e and "targets" in e and "not a mix" in e for e in demo_apps.check(m)), variant)
            m = demo_apps.load()
            m["lib"][variant]["targets"] = []
            self.assertTrue(any(variant in e and "targets" in e for e in demo_apps.check(m)), variant)
        # a column that is none of the built ones is refused as well
        m = demo_apps.load()
        m["lib"]["desktop-e2e"]["targets"] = ["linux", "freebsd"]
        self.assertTrue(any("desktop-e2e" in e and "targets" in e for e in demo_apps.check(m)))

    def test_an_e2e_app_needs_a_column_the_qa_lib_builds(self):
        self.m["lib"]["mobile-e2e"]["targets"] = ["ios-sim"]
        self.m["lib"]["desktop-e2e"]["targets"] = ["macos-arm64"]
        self.app("AzWidgets")["platforms"] = ["linux"]
        self.assertTrue(any("AzWidgets" in e and "e2e" in e for e in demo_apps.check(self.m)), demo_apps.check(self.m))

    def test_a_qa_lib_of_the_other_family_is_no_qa_lib_of_the_app(self):
        # e2e_lib is the library of the PHONE columns of an app, e2e_desktop_lib the one of its desktop columns
        self.app("AzWidgets")["e2e_lib"] = "desktop-e2e"
        self.assertTrue(any("AzWidgets" in e and "e2e_lib" in e and "phone" in e for e in demo_apps.check(self.m)), demo_apps.check(self.m))
        m = demo_apps.load()
        next(a for a in m["app"] if a["package"] == "AzWidgets")["e2e_desktop_lib"] = "mobile-e2e"
        self.assertTrue(any("AzWidgets" in e and "e2e_desktop_lib" in e and "desktop" in e for e in demo_apps.check(m)), demo_apps.check(m))
        # and never a lib that ships: the debug server would go to the users
        m = demo_apps.load()
        next(a for a in m["app"] if a["package"] == "AzWidgets")["e2e_desktop_lib"] = "desktop"
        self.assertTrue(any("AzWidgets" in e and "e2e_desktop_lib" in e for e in demo_apps.check(m)), demo_apps.check(m))

    def test_the_real_manifest_passes_its_own_check_with_the_variant_in_it(self):
        self.assertTrue(self.m["lib"]["mobile-e2e"]["artifact_only"])
        # the phone apps whose QA variant the layout audit and the scripted checks of the phone QA drive
        self.assertEqual([a["package"] for a in self.m["app"] if a.get("e2e")],
                         ["AzWidgets", "AzMaps", "AzPaint", "AzMeet", "AzWriter", "AzCompanion"])
        self.assertTrue(self.m["lib"]["mobile-e2e-iroh"]["artifact_only"])
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


def tomllib_loads(text):
    return demo_apps.tomllib.loads(text)


class DesktopE2eTest(unittest.TestCase):
    """The QA builds of the DESKTOP apps for the E2E runs of the desktop column: the artifact-only `[lib.desktop-e2e]` (the shipped dist
    features plus the engine's debug server, E2E scripting and test hooks: an app cannot get them from its own dependency on azul-dll,
    `azul = link-dynamic` leaves the Az* symbols to the library) and the apps that say `e2e = true` linked against it as
    <Name>-e2e-<platform>, in artifacts named demoe2e-*. Like the phone QA variant: built only when a run asks, never published."""

    def setUp(self):
        self.m = demo_apps.load()

    def test_the_qa_lib_is_the_shipped_one_plus_the_three_qa_features_and_is_never_shipped(self):
        shipped, qa = self.m["lib"]["desktop"], self.m["lib"]["desktop-e2e"]
        self.assertTrue(qa["artifact_only"])
        self.assertFalse(shipped.get("artifact_only"))
        self.assertEqual(qa["profile"], shipped["profile"])
        self.assertEqual(sorted(set(qa["features"].split(",")) - set(shipped["features"].split(","))),
                         ["debug-server", "e2e-scripting", "test-hooks"])
        self.assertLessEqual(set(shipped["features"].split(",")), set(qa["features"].split(",")))
        self.assertEqual(demo_apps.lib_family(qa), "desktop")
        self.assertEqual(demo_apps.lib_family(self.m["lib"]["mobile-e2e"]), "phone")
        self.assertLessEqual(set(qa["targets"]), set(shipped["targets"]))
        self.assertEqual([e for e in demo_apps.check(self.m) if "e2e" in e], [])
        # the shipped library of the leg is the one without the features: no app's lib may be the QA one
        self.assertEqual(sorted(demo_apps.artifact_only_libs(self.m, "desktop")), ["desktop-e2e"])
        self.assertEqual(sorted(demo_apps.artifact_only_libs(self.m, "phone")), ["mobile-e2e", "mobile-e2e-iroh"])

    def test_without_the_input_a_run_plans_no_desktop_qa_build(self):
        plan = demo_apps.plan(self.m)
        for key in ("libs_linux", "libs_macos", "libs_windows", "apps_linux", "apps_macos", "apps_windows"):
            for e in plan[key]:
                self.assertNotEqual(e["variant"], "desktop-e2e", key)
                self.assertFalse(e["artifact"].startswith("demoe2e-"), key)
        # the shipped app entries say so: the workflow passes `--variant "$DEMO_VARIANT"` to every desktop app job
        for key in ("apps_linux", "apps_macos", "apps_windows"):
            self.assertTrue(plan[key])
            self.assertTrue(all(e["variant"] == "" for e in plan[key]), key)

    def test_with_the_input_the_lib_and_the_apps_of_each_qa_leg_are_planned(self):
        plan = demo_apps.plan(self.m, e2e=True)
        libs = {e["leg"]: e for k in ("libs_linux", "libs_macos", "libs_windows") for e in plan[k] if e["variant"] == "desktop-e2e"}
        self.assertEqual(sorted(libs), ["linux", "macos-arm64"])
        shipped = {e["leg"]: e for k in ("libs_linux", "libs_macos", "libs_windows") for e in plan[k] if e["variant"] == "desktop"}
        for leg, e in libs.items():
            self.assertEqual(e["features"], self.m["lib"]["desktop-e2e"]["features"])
            self.assertEqual((e["kind"], e["id"], e["artifact"]), ("desktop", f"lib-{leg}-desktop-e2e", f"demolib-{leg}-desktop-e2e"))
            # the runner, the target and the deployment target of the shipped library of the leg
            for key in ("os", "target", "codegen_os", "profile", "deployment_target"):
                self.assertEqual(e.get(key), shipped[leg].get(key), (leg, key))
        qa = {e["leg"]: e for k in ("apps_linux", "apps_macos", "apps_windows") for e in plan[k] if e["variant"] == "desktop-e2e"}
        self.assertEqual(sorted(qa), sorted(libs))
        for leg, e in qa.items():
            want = sorted(a["package"] for a in self.m["app"] if a.get("e2e") and leg in a["platforms"])
            self.assertTrue(want, leg)
            self.assertEqual(sorted(e["apps"].split()), want)
            self.assertEqual((e["lib_artifact"], e["artifact"], e["results_artifact"]),
                             (f"demolib-{leg}-desktop-e2e", f"demoe2e-{leg}", f"demoresults-{leg}-e2e"))
            self.assertEqual(e["lib_file"], "libazul.a")
        # two uploads of one artifact name fail a run: every artifact and every id of the plan is unique
        names = [e[k] for key, entries in plan.items() if key.startswith(("libs_", "apps_")) for e in entries
                 for k in ("artifact", "results_artifact") if k in e]
        self.assertEqual(sorted(n for n in set(names) if names.count(n) > 1), [])
        ids = [e["id"] for key, entries in plan.items() if key.startswith("libs_") for e in entries]
        self.assertEqual(sorted(i for i in set(ids) if ids.count(i) > 1), [])

    def test_the_shipped_builds_of_the_same_legs_are_unchanged_by_the_qa_ones(self):
        before = demo_apps.plan(self.m)
        after = demo_apps.plan(self.m, e2e=True)
        for key in ("libs_linux", "libs_macos", "libs_windows", "apps_linux", "apps_macos", "apps_windows"):
            self.assertEqual([e for e in after[key] if e["variant"] in ("", "desktop")], before[key], key)

    def test_the_features_command_prints_the_features_of_the_qa_lib_of_each_leg(self):
        for leg in ("linux", "macos-arm64"):
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                self.assertEqual(demo_apps.main(["features", leg, "desktop-e2e"]), 0)
            self.assertEqual(buf.getvalue().strip(), "dist,debug-server,e2e-scripting,test-hooks")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            self.assertEqual(demo_apps.main(["features", "linux", "desktop"]), 0)           # the shipped library: no QA feature in it
        self.assertEqual(buf.getvalue().strip(), self.m["lib"]["desktop"]["features"])
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(demo_apps.main(["features", "windows", "desktop-e2e"]), 1)    # no QA build of that leg

    def test_a_qa_file_of_a_desktop_leg_never_becomes_a_release_asset(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "demoe2e-linux").mkdir()
            (root / "demoe2e-linux" / "AzWidgets-e2e-linux").write_bytes(b"an app with the debug server")
            (root / "demoapps-linux-0").mkdir()
            (root / "demoapps-linux-0" / "AzWidgets-linux").write_bytes(b"the shipped app")
            listing = demo_publish.assemble(root, root / "release", "0.2.0", "abc")
        self.assertEqual([x["name"] for x in listing], ["AzWidgets-linux.tar.gz"])
        for app in self.m["app"]:
            for col in demo_apps.columns_of(app):
                self.assertNotIn("-e2e", demo_apps.release_name(app, col))

    def test_the_status_keeps_a_desktop_qa_build_apart_from_the_shipped_cell(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "AzWidgets.linux.result.json").write_text(json.dumps({
                "app": "AzWidgets", "column": "linux", "status": "built", "size": 4096, "shipped_size": 4096, "seconds": 90,
                "engine_crates": [], "artifact": "AzWidgets-linux"}))
            (root / "AzWidgets.linux-e2e.result.json").write_text(json.dumps({
                "app": "AzWidgets", "column": "linux", "variant": "desktop-e2e", "status": "built", "size": 5000, "shipped_size": 5000,
                "seconds": 80, "engine_crates": [], "artifact": "AzWidgets-e2e-linux"}))
            data, md = demo_apps.status([root], m=self.m)
        cell = next(r for r in data["apps"] if r["app"] == "AzWidgets")["cells"]["linux"]
        self.assertEqual(cell["size"], 4096)
        self.assertEqual([(q["app"], q["column"], q["artifact"]) for q in data["summary"]["qa_builds"]],
                         [("AzWidgets", "linux", "AzWidgets-e2e-linux")])
        self.assertIn("never published", md)

    def test_the_driver_stages_a_qa_build_under_its_own_name_and_result_file(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            lib = root / "libazul.a"
            lib.write_bytes(b"!<arch>\n")
            exe = root / "built-AzWidgets"
            exe.write_bytes(b"\x7fELF an app")
            args = argparse.Namespace(out=str(root / "out"), results=str(root / "res"), leg="linux", lib=str(lib), apps="AzWidgets",
                                      target="", deployment_target="", run_check=False, variant="")
            job = demo_build.Job(args)
            envs = []

            def fake_cargo(cmd, env, tag):
                envs.append(dict(env))
                jpath, epath = job.logs / f"{tag}.json", job.logs / f"{tag}.stderr.txt"
                jpath.write_text(cargo_json((APP, [], "bin", str(exe))))
                epath.write_text("")
                return 0, jpath, epath

            with mock.patch.object(job, "run_cargo", fake_cargo), mock.patch.object(demo_build, "strip_binary"):
                demo_build.desktop(args, job)                                  # the shipped build of the leg ...
                args.variant = "desktop-e2e"
                demo_build.desktop(args, job)                                  # ... and the QA build of it, in the same directories
            staged = sorted(p.name for p in (root / "out").iterdir() if p.is_file())
            names = sorted(p.name for p in (root / "res").iterdir())
            qa = json.loads((root / "res" / "AzWidgets.linux-e2e.result.json").read_text())
            shipped = json.loads((root / "res" / "AzWidgets.linux.result.json").read_text())
        self.assertEqual(staged, ["AzWidgets-e2e-linux", "AzWidgets-linux"])
        self.assertEqual(names, ["AzWidgets.linux-e2e.result.json", "AzWidgets.linux.result.json"])
        self.assertEqual((qa["variant"], qa["status"], qa["artifact"]), ("desktop-e2e", "built", "AzWidgets-e2e-linux"))
        self.assertEqual((shipped["status"], shipped["artifact"]), ("built", "AzWidgets-linux"))
        self.assertNotIn("variant", shipped)
        self.assertEqual([e["AZ_LINK_PATH"] for e in envs], [str(lib.resolve())] * 2)     # the library the app is linked against

    def test_the_driver_takes_a_desktop_qa_variant_only_for_an_app_that_asks_for_it(self):
        with tempfile.TemporaryDirectory() as d:
            argv = ["desktop", "--leg", "linux", "--lib", str(pathlib.Path(d) / "libazul.a"), "--out", str(pathlib.Path(d) / "o"),
                    "--results", str(pathlib.Path(d) / "r")]
            with self.assertRaises(SystemExit):
                demo_build.main(argv + ["--apps", "AzWidgets", "--variant", "desktop"])          # a shipped lib is no QA variant
            with self.assertRaises(SystemExit):
                demo_build.main(argv + ["--apps", "AzWidgets", "--variant", "mobile-e2e"])       # the phone QA lib is not the desktop one
            no_e2e = next(a["package"] for a in self.m["app"] if not a.get("e2e") and "linux" in a["platforms"])
            with self.assertRaises(SystemExit):
                demo_build.main(argv + ["--apps", no_e2e, "--variant", "desktop-e2e"])           # an app that did not ask for it

    def test_the_build_script_builds_the_lib_and_the_apps_the_plan_names_and_deletes_nothing_of_the_checkout(self):
        path = HERE / "ci" / "desktop_e2e_build.sh"
        text = path.read_text(encoding="utf-8")
        self.assertEqual(subprocess.run(["bash", "-n", str(path)]).returncode, 0)
        for needle in ("demo_apps.py plan --e2e", "demo_apps.py features", "demo_lib.sh desktop", "demo_build.py", "--variant",
                       "demo-out/apps", "demo-out/results"):
            self.assertIn(needle, text)
        # the lib and the apps come from the plan: no hand-written feature list and no hard-coded app list
        code = "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))
        self.assertNotIn("debug-server", code)
        self.assertNotRegex(code, r"Az[A-Z]\w+")
        # the only removal is the runner cleanup (GitHub Actions only), of directories nobody needs
        removals = [l.strip() for l in code.splitlines() if re.search(r"\brm\b", l)]
        self.assertEqual(len(removals), 1)
        self.assertIn("/usr/share/dotnet", removals[0])

    def test_the_workflow_hands_the_variant_to_every_desktop_app_job(self):
        text = (demo_apps.ROOT / ".github" / "workflows" / "demos.yml").read_text(encoding="utf-8")
        body = text[text.index("\njobs:\n"):]
        parts = re.split(r"(?m)^  ([A-Za-z_][\w-]*):[ \t]*$", body)
        jobs = {parts[i]: parts[i + 1] for i in range(1, len(parts) - 1, 2)}
        for job in ("apps_linux", "apps_macos", "apps_windows"):
            self.assertIn("DEMO_VARIANT: ${{ matrix.variant }}", jobs[job], job)
            self.assertIn('--variant "$DEMO_VARIANT"', jobs[job], job)


def qa_only_manifest(directory, package="AzPaint"):
    """The real manifest with the entry of `package` made what AzSelfTest is: an app that exists only as a QA build (phones only, linked
    against the artifact-only library, `qa_only = true`). The package stands in for it because the checks look at crates that exist."""
    text = demo_apps.MANIFEST.read_text(encoding="utf-8")
    head, found, rest = text.partition(f'package = "{package}"\n')
    assert found, package
    block, sep, tail = rest.partition("\n[[app]]")
    block = block.replace('platforms = ["linux", "macos-arm64", "macos-intel", "windows", "android", "ios"]', 'platforms = ["android", "ios"]')
    block = block.replace('mobile_lib = "mobile"\n', 'mobile_lib = "mobile-e2e"\nqa_only = true\n')
    assert "qa_only = true" in block and 'platforms = ["android", "ios"]' in block, block
    path = pathlib.Path(directory) / "demo_apps.toml"
    path.write_text(head + found + block + sep + tail, encoding="utf-8")
    return demo_apps.load(path)


class QaOnlyAppTest(unittest.TestCase):
    """An app that says `qa_only = true` (AzSelfTest: every device capability, one probe each) exists only as a QA build: built when a run asks
    for QA builds, for the columns of the artifact-only library it links, and nothing else. It is no row or link of the release page, no
    release asset, no cell of the status table and no entry of the shipped plan: it is not in the list of the apps of the release at all
    (`m["app"]`), so nothing that reads that list can ship it by forgetting to skip it."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.m = qa_only_manifest(self._tmp.name)

    def test_the_app_is_a_qa_app_and_not_one_of_the_release(self):
        self.assertNotIn("AzPaint", [a["package"] for a in self.m["app"]])
        self.assertEqual([a["package"] for a in self.m["qa_app"]], ["AzPaint"])
        app = self.m["qa_app"][0]
        self.assertTrue(app["qa_only"] and app["e2e"])
        self.assertEqual((app["mobile_lib"], app["e2e_lib"]), ("mobile-e2e", "mobile-e2e"))   # the QA lib it links is its QA lib
        self.assertTrue(all(a["qa_only"] is False for a in demo_apps.load()["app"]))

    def test_the_manifest_with_such_an_app_passes_its_own_check(self):
        self.assertEqual(demo_apps.check(self.m), [])

    def test_nothing_of_it_is_in_the_plan_of_a_run_that_asks_for_no_qa_builds(self):
        plan = demo_apps.plan(self.m)
        for key, entries in plan.items():
            if key.startswith(("libs_", "apps_")):
                for e in entries:
                    self.assertNotIn("AzPaint", e.get("apps", "").split(), key)
                    self.assertNotEqual(e.get("variant"), "mobile-e2e", key)

    def test_a_run_that_asks_for_qa_builds_builds_it_for_the_columns_of_its_qa_lib_only(self):
        plan = demo_apps.plan(self.m, e2e=True)
        built = {e["leg"] for k in ("apps_android", "apps_ios") for e in plan[k] if "AzPaint" in e["apps"].split()}
        self.assertEqual(built, {"android-arm64", "android-x86_64", "ios-sim"})      # no iOS device build: the QA lib has none
        for k in ("apps_android", "apps_ios"):
            for e in plan[k]:
                if "AzPaint" in e["apps"].split():
                    self.assertEqual(e["variant"], "mobile-e2e")                     # never a shipped entry (variant "")
                    self.assertTrue(e["artifact"].startswith("demoe2e-"), e["artifact"])
        libs = {e["leg"] for k in ("libs_android", "libs_ios") for e in plan[k] if e["variant"] == "mobile-e2e"}
        self.assertLessEqual(built, libs)

    def test_the_status_table_has_no_row_for_it_only_its_qa_builds(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "AzPaint.android-arm64-e2e.result.json").write_text(json.dumps({
                "app": "AzPaint", "column": "android-arm64", "variant": "mobile-e2e", "status": "built", "size": 9000,
                "shipped_size": 9000, "seconds": 20, "engine_crates": [], "artifact": "AzPaint-e2e-android-arm64.apk"}))
            data, md = demo_apps.status([root], m=self.m)
        self.assertNotIn("AzPaint", [r["app"] for r in data["apps"]])
        self.assertEqual([(q["app"], q["column"], q["artifact"]) for q in data["summary"]["qa_builds"]],
                         [("AzPaint", "android-arm64", "AzPaint-e2e-android-arm64.apk")])
        self.assertEqual(data["summary"]["assets"], 0)

    def test_the_asset_list_never_names_it_and_the_library_commands_answer_for_it(self):
        with mock.patch.object(demo_apps, "load", lambda path=demo_apps.MANIFEST: self.m):
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                self.assertEqual(demo_apps.main(["assets"]), 0)
            self.assertNotIn("AzPaint", buf.getvalue())
            self.assertTrue(buf.getvalue())
            for command in ("mobile-lib", "e2e-lib"):
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf):
                    self.assertEqual(demo_apps.main([command, "AzPaint"]), 0, command)
                self.assertEqual(buf.getvalue().strip(), "mobile-e2e", command)

    def test_the_check_names_every_way_such_an_app_could_ship(self):
        def errors(**changes):
            m = qa_only_manifest(self._tmp.name)
            m["qa_app"][0].update(changes)
            return demo_apps.check(m)

        def named(errs, *words):
            return any("AzPaint" in e and all(w in e for w in words) for e in errs)

        self.assertTrue(named(errors(mobile_lib="mobile", e2e_lib="mobile"), "qa_only", "mobile_lib"), errors(mobile_lib="mobile", e2e_lib="mobile"))
        self.assertTrue(named(errors(platforms=["linux", "android"]), "qa_only", "platforms"), errors(platforms=["linux", "android"]))
        self.assertTrue(named(errors(e2e_lib="mobile-e2e-iroh"), "qa_only", "e2e_lib"), errors(e2e_lib="mobile-e2e-iroh"))
        # a value that is no boolean is not `true`: the app would stay in the list of the release
        text = demo_apps.MANIFEST.read_text(encoding="utf-8").replace('package = "AzPaint"\n', 'package = "AzPaint"\nqa_only = "yes"\n', 1)
        path = pathlib.Path(self._tmp.name) / "string.toml"
        path.write_text(text, encoding="utf-8")
        self.assertTrue(named(demo_apps.check(demo_apps.load(path)), "qa_only", "true or false"))
        # a shipped app that links the QA library stays an error: the debug server would go to the users
        m = demo_apps.load()
        next(a for a in m["app"] if a["package"] == "AzPaint")["mobile_lib"] = "mobile-e2e"
        self.assertTrue(named(demo_apps.check(m), "artifact_only"))

    def test_the_build_driver_has_no_shipped_build_of_it(self):
        with tempfile.TemporaryDirectory() as d:
            argv = ["android", "--target", "x86_64-linux-android", "--libs", d, "--apps", "AzPaint",
                    "--out", str(pathlib.Path(d) / "o"), "--results", str(pathlib.Path(d) / "r")]
            with mock.patch.object(demo_apps, "load", lambda path=demo_apps.MANIFEST: self.m), \
                    mock.patch.object(demo_build, "phone") as phone:
                with self.assertRaises(SystemExit) as caught:
                    demo_build.main(argv)                                     # no --variant: a shipped build
                self.assertIn("qa_only", str(caught.exception))
                phone.assert_not_called()
                self.assertEqual(demo_build.main(argv + ["--variant", "mobile-e2e"]), 0)    # the QA build is the build of it
                phone.assert_called_once()


class WorkflowContractTest(unittest.TestCase):
    """.github/workflows/demos.yml reads the plan job's outputs and the fields of its matrix entries: a name that the plan does not
    write is an empty value at run time, found hours into a run. The workflow is read as text (the standard library has no YAML)."""

    def setUp(self):
        self.path = demo_apps.ROOT / ".github" / "workflows" / "demos.yml"
        self.text = self.path.read_text(encoding="utf-8")
        self.plan = demo_apps.plan(demo_apps.load(), e2e=True)

    def jobs(self):
        """{job name: body text} of the `jobs:` section."""
        body = self.text[self.text.index("\njobs:\n"):]
        parts = re.split(r"(?m)^  ([A-Za-z_][\w-]*):[ \t]*$", body)
        return {parts[i]: parts[i + 1] for i in range(1, len(parts) - 1, 2)}

    def test_the_plan_job_hands_on_every_key_the_plan_writes(self):
        outputs = set(re.findall(r"(?m)^      (\w+): \$\{\{ steps\.plan\.outputs\.(\w+) \}\}$", self.jobs()["plan"]))
        self.assertTrue(all(a == b for a, b in outputs), outputs)
        self.assertEqual({a for a, _ in outputs}, set(self.plan))

    def test_every_matrix_field_a_job_reads_is_in_every_entry_of_its_plan_list(self):
        seen = 0
        for name, body in self.jobs().items():
            m = re.search(r"matrix: \$\{\{ fromJSON\(needs\.plan\.outputs\.(\w+)\) \}\}", body)
            if not m:
                continue
            key = m.group(1)
            self.assertIn(key, self.plan, f"{name} reads a list the plan does not write")
            fields = set(re.findall(r"matrix\.(\w+)", body))
            for entry in self.plan[key]:
                missing = sorted(fields - set(entry))
                self.assertEqual(missing, [], f"{name}: matrix.{missing} is in no field of the plan's {key} entry {entry.get('id') or entry.get('leg')}")
            seen += 1
        self.assertGreaterEqual(seen, 10)   # five libs, five apps

    def test_at_most_three_macos_jobs_run_at_the_same_time(self):
        # The macOS runners are the scarce kind (5 at once on the account, shared with every other run): the workflow takes at most three.
        # Two jobs can run together unless one of them needs the other (transitively); a matrix job runs min(max-parallel, entries) of its
        # entries at once. The worst case is the heaviest set of pairwise independent macOS jobs.
        import itertools
        jobs = self.jobs()
        needs = {}
        for name, body in jobs.items():
            m = re.search(r"(?m)^    needs: \[([^\]]*)\]", body)
            needs[name] = [n.strip() for n in m.group(1).split(",")] if m else []

        def ancestors(name, seen=None):
            seen = set() if seen is None else seen
            for n in needs.get(name, []):
                if n not in seen:
                    seen.add(n)
                    ancestors(n, seen)
            return seen

        weight = {}
        for name, body in jobs.items():
            key = re.search(r"matrix: \$\{\{ fromJSON\(needs\.plan\.outputs\.(\w+)\) \}\}", body)
            if key:
                entries = self.plan[key.group(1)]
                if entries and all(str(e.get("os", "")).startswith("macos") for e in entries):
                    limit = re.search(r"(?m)^      max-parallel: (\d+)", body)
                    weight[name] = min(int(limit.group(1)) if limit else len(entries), len(entries))
            elif re.search(r"(?m)^    runs-on: macos", body):
                weight[name] = 1
        self.assertGreaterEqual(len(weight), 5, weight)     # codegen, two libraries, two sets of apps, the iOS boot test
        worst = 0
        for size in range(1, len(weight) + 1):
            for group in itertools.combinations(sorted(weight), size):
                if all(a not in ancestors(b) and b not in ancestors(a) for a, b in itertools.combinations(group, 2)):
                    worst = max(worst, sum(weight[g] for g in group))
        self.assertLessEqual(worst, 3, f"macOS jobs at the same time: {worst} ({weight})")

    def test_no_expression_of_the_matrix_or_the_plan_is_expanded_into_a_script(self):
        # zizmor's template-injection audit (the supply-chain workflow's zizmor job runs it on every workflow file): a `${{ }}` inside a
        # `run:` block is pasted INTO the script, an environment variable is data. The matrix values of the plan reach the shell steps
        # through `env:` (DEMO_KIND, DEMO_TARGET, DEMO_FEATURES, DEMO_LEG, DEMO_APPS, DEMO_LIB_FILE, DEMO_DEPLOYMENT_TARGET, DEMO_VARIANT).
        for name, body in self.jobs().items():
            for run in re.findall(r"(?ms)^        run: (.*?)(?=^        \S|^      - |\Z)", body):
                self.assertNotIn("${{ matrix.", run, f"{name}: a matrix value is expanded into a script")
                self.assertNotIn("${{ needs.plan.outputs.", run, f"{name}: a plan output is expanded into a script")

    def test_the_phone_app_jobs_pass_the_variant_and_the_publish_job_never_reads_the_qa_artifacts(self):
        jobs = self.jobs()
        for name in ("apps_android", "apps_ios"):
            self.assertIn('--variant "$DEMO_VARIANT"', jobs[name], name)
            self.assertIn("DEMO_VARIANT: ${{ matrix.variant }}", jobs[name], name)
        self.assertIn("pattern: demoapps-*", jobs["publish"])
        self.assertNotIn("demoe2e", jobs["publish"])
        for entry in self.plan["apps_android"] + self.plan["apps_ios"]:
            self.assertEqual(entry["artifact"].startswith("demoe2e-"), entry["variant"].startswith("mobile-e2e"))

    def test_the_boot_tests_read_only_the_shipped_apps(self):
        jobs = self.jobs()
        self.assertIn("name: demoapps-android-x86_64", jobs["smoke_android"])
        self.assertIn("name: demoapps-ios-sim", jobs["smoke_ios"])

    def test_the_transport_test_step_of_the_companion_names_a_feature_the_crate_has(self):
        # `cargo test -p azul-review-link --features <f>` fails with "the package does not have that feature" when it has none: the
        # step may only land together with (or after) the feature, which the companion agent's commit adds
        rust = (demo_apps.ROOT / ".github" / "workflows" / "rust.yml").read_text(encoding="utf-8")
        steps = re.findall(r"cargo test -p azul-review-link --lib --features (\S+)", rust)
        self.assertEqual(len(steps), 1, "the dll_tests job runs the transport tests of the companion once")
        crate = tomllib_loads((demo_apps.ROOT / "examples" / "azul-review-link" / "Cargo.toml").read_text(encoding="utf-8"))
        self.assertIn(steps[0], crate["features"])
        step = rust[rust.index("Run azul-review-link transport tests"):]
        step = step[:step.index("\n      - name:")]
        self.assertIn("AZ_REQUIRE_IROH_ENGINE: \"1\"", step)               # a libazul without the engine fails the tests, it does not skip them
        self.assertIn("AZ_LINK_PATH: ${{ runner.temp }}/azul-cabi", step)   # the libazul the job builds for the other link-dynamic tests
        self.assertIn("zero_test_guard.sh --strict", step)

    def test_rust_yml_hands_the_inputs_to_the_workflow_and_keeps_no_list_of_apps(self):
        rust = (demo_apps.ROOT / ".github" / "workflows" / "rust.yml").read_text(encoding="utf-8")
        self.assertIn("uses: ./.github/workflows/demos.yml", rust)
        self.assertIn("publish: ${{ inputs.publish_demos }}", rust)
        self.assertIn("e2e: ${{ inputs.demos_e2e }}", rust)
        self.assertEqual(demo_apps.check(), [])


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

    def first_error_of(self, text):
        with tempfile.TemporaryDirectory() as d:
            j = pathlib.Path(d) / "b.json"
            j.write_text("")
            e = pathlib.Path(d) / "b.err"
            e.write_text(text)
            return demo_build.first_error(j, e)

    def test_a_link_with_duplicate_c_abi_symbols_says_that_an_engine_crate_was_compiled_into_the_app(self):
        # AzMail on Linux, 2026-11-28: azul-mail-core, azul-appkit and azul-icons-haiku depend on azul-core, whose #[no_mangle] C ABI
        # functions (AzApp_*) are also in the prebuilt libazul.a: rust-lld stops at the first duplicate
        text = ("   Compiling AzMail v0.1.0\nerror: linking with `cc` failed: exit status: 1\n  = note: \"cc\" \"-m64\" ...\n"
                "  = note: rust-lld: error: duplicate symbol: AzApp_getInvocationCtx\n"
                "          >>> defined at host_invoker.rs:547 (core/src/host_invoker.rs:547)\n"
                "          rust-lld: error: duplicate symbol: AzApp_setGenericInvoker\n"
                "          rust-lld: error: duplicate symbol: AzApp_getInvocationCtx\n")
        got = self.first_error_of(text)
        self.assertTrue(got.startswith("engine crate compiled into the app: the link has 2 duplicate C ABI symbols (AzApp_getInvocationCtx, ...)"), got)
        self.assertIn("azul-core", got)
        self.assertLessEqual(len(got), 300)

    def test_a_link_with_undefined_symbols_names_the_first_and_counts_the_rest(self):
        text = ("error: linking with `link.exe` failed: exit code: 1181\n  = note: azul.lib(x.o) : error LNK2019: unresolved\n"
                "  = note: /usr/bin/ld: x.o: undefined reference to `XOpenDisplay'\n  = note: /usr/bin/ld: y.o: undefined reference to `glXSwapBuffers'\n")
        self.assertEqual(self.first_error_of(text), "link failed: undefined symbol XOpenDisplay and 1 more")
        self.assertEqual(self.first_error_of("error: linking with `cc` failed: exit status: 1\n  = note: something else\n"),
                         "error: linking with `cc` failed: exit status: 1")     # a link error the reader knows nothing about stays as it is
        self.assertEqual(self.first_error_of("error[E0063]: missing fields `concealed`\n"), "error[E0063]: missing fields `concealed`")

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


class IrohLibraryTest(unittest.TestCase):
    """A phone app that uses azul.iroh must link a libazul that has it: with the lean `mobile` library (no ring, no tokio) its endpoint cannot
    bind on a phone (bind_failure), which is how AzMeet shipped. `demo_apps.py check` reads the sources of every phone app and fails the preflight
    for an app that uses the engine's iroh API and names no iroh library, for its release build and for its QA build."""

    def setUp(self):
        self.m = demo_apps.load()

    def app(self, name):
        return next(a for a in self.m["app"] if a["package"] == name)

    def errors(self, *words):
        return [e for e in demo_apps.check(self.m) if all(w in e for w in words)]

    def test_the_real_manifest_names_an_iroh_library_for_every_app_that_uses_iroh_and_a_lean_one_for_the_rest(self):
        self.assertEqual([e for e in demo_apps.check(self.m) if "iroh" in e], [])
        for name in ("AzMeet", "AzCompanion"):
            self.assertEqual(self.app(name)["mobile_lib"], "mobile-iroh", name)
            self.assertEqual(self.app(name)["e2e_lib"], "mobile-e2e-iroh", name)
            self.assertTrue(self.app(name)["e2e"], name)
        for name in ("AzWidgets", "AzMaps", "AzPaint", "AzWriter"):
            self.assertEqual(self.app(name)["mobile_lib"], "mobile", name)     # no ring and no tokio on the phone for what does not use them
            self.assertEqual(self.app(name)["e2e_lib"], "mobile-e2e", name)

    def test_the_sources_of_azmeet_use_the_iroh_api_and_those_of_the_lean_apps_do_not(self):
        self.assertIn("examples/azul-meet/src/", demo_apps.iroh_use(self.app("AzMeet")))
        for name in ("AzWidgets", "AzMaps", "AzPaint", "AzWriter"):
            self.assertIsNone(demo_apps.iroh_use(self.app(name)), name)
        # AzCompanion reaches iroh through the link crate, not in its own sources: it says so
        self.assertIsNone(demo_apps.iroh_use_in("examples/azul-companion"))
        self.assertEqual(demo_apps.iroh_use(self.app("AzCompanion")), "scripts/demo_apps.toml (iroh = true)")

    def test_an_app_that_uses_iroh_and_links_the_lean_library_fails_the_check_and_names_the_file(self):
        self.app("AzMeet")["mobile_lib"] = "mobile"
        errors = self.errors("AzMeet", "iroh", "mobile_lib")
        self.assertEqual(len(errors), 1, errors)
        self.assertIn("examples/azul-meet/src/", errors[0])
        self.assertIn("bind_failure", errors[0])
        self.assertIn("mobile-iroh", errors[0])

    def test_the_qa_build_of_such_an_app_needs_the_iroh_twin_of_the_qa_library(self):
        self.app("AzMeet")["e2e_lib"] = "mobile-e2e"
        errors = self.errors("AzMeet", "QA lib")
        self.assertEqual(len(errors), 1, errors)
        self.assertIn("mobile-e2e-iroh", errors[0])
        # without e2e the QA library does not matter
        self.app("AzMeet")["e2e"] = False
        self.assertEqual(self.errors("AzMeet", "QA lib"), [])

    def test_an_app_that_declares_iroh_is_held_to_the_same_rule(self):
        self.app("AzWidgets")["iroh"] = True
        errors = self.errors("AzWidgets", "iroh")
        self.assertTrue(any("mobile_lib" in e for e in errors) and any("QA lib" in e for e in errors), errors)
        self.assertTrue(all("scripts/demo_apps.toml (iroh = true)" in e for e in errors if "mobile_lib" in e))

    def test_an_app_that_uses_iroh_on_ios_says_why_it_wants_the_local_network(self):
        del self.app("AzMeet")["ios"]["local_network"]
        errors = self.errors("AzMeet", "local_network")
        self.assertEqual(len(errors), 1, errors)
        self.assertIn("NSBonjourServices", errors[0])
        self.app("AzMeet")["platforms"] = ["linux", "android"]        # no iOS build: nothing to say to iOS
        self.assertEqual(self.errors("AzMeet", "local_network"), [])

    def test_an_app_that_only_a_desktop_builds_is_not_held_to_a_phone_library(self):
        self.app("AzMeet")["platforms"] = ["linux", "windows"]
        self.assertEqual(self.errors("AzMeet", "iroh API"), [])

    def test_the_comments_and_the_tests_of_an_app_do_not_count_as_a_use_of_iroh(self):
        with tempfile.TemporaryDirectory() as d:
            crate = pathlib.Path(d) / "examples" / "azul-x-comments"
            (crate / "src").mkdir(parents=True)
            (crate / "Cargo.toml").write_text('[package]\nname = "AzX"\n')
            (crate / "src" / "lib.rs").write_text("// IrohEndpoint::bind(config) is what the old version did\nfn f() { let x = 1; }\n")
            (crate / "src" / "a_test.rs").write_text("fn t() { IrohEndpoint::bind(c) }\n")
            (crate / "src" / "tests").mkdir()
            (crate / "src" / "tests" / "b.rs").write_text("fn t() { IrohConfig::create() }\n")
            with mock.patch.object(demo_apps, "ROOT", pathlib.Path(d)):
                self.assertIsNone(demo_apps.iroh_use_in("examples/azul-x-comments"))
                (crate / "src" / "lib.rs").write_text("fn f() { let e = IrohEndpoint::bind(config); }\n")
                demo_apps.iroh_use_in.cache_clear()
                self.assertEqual(demo_apps.iroh_use_in("examples/azul-x-comments"), "examples/azul-x-comments/src/lib.rs")
            demo_apps.iroh_use_in.cache_clear()

    def test_the_twins_are_their_plain_libraries_with_iroh_and_nothing_else(self):
        libs = self.m["lib"]
        self.assertEqual(set(libs["mobile-iroh"]["features"].split(",")), set(libs["mobile"]["features"].split(",")) | {"iroh"})
        self.assertEqual(set(libs["mobile-e2e-iroh"]["features"].split(",")), set(libs["mobile-e2e"]["features"].split(",")) | {"iroh"})
        self.assertTrue(libs["mobile-e2e-iroh"]["artifact_only"])
        self.assertEqual(libs["mobile-e2e-iroh"]["targets"], libs["mobile-e2e"]["targets"])

    def test_an_e2e_lib_that_is_no_qa_library_is_an_error(self):
        self.app("AzMeet")["e2e_lib"] = "mobile-iroh"
        self.assertTrue(self.errors("AzMeet", "e2e_lib", "artifact_only"))
        self.app("AzMeet")["e2e_lib"] = "no-such-lib"
        self.assertTrue(self.errors("AzMeet", "e2e_lib"))

    def test_a_qa_build_names_its_own_library_and_the_driver_refuses_another(self):
        with tempfile.TemporaryDirectory() as d:
            argv = ["android", "--target", "x86_64-linux-android", "--libs", d, "--out", str(pathlib.Path(d) / "o"),
                    "--results", str(pathlib.Path(d) / "r")]
            with self.assertRaises(SystemExit) as caught:
                demo_build.main(argv + ["--apps", "AzMeet", "--variant", "mobile-e2e"])
            self.assertIn("mobile-e2e-iroh", str(caught.exception))
            with self.assertRaises(SystemExit):
                demo_build.main(argv + ["--apps", "AzWidgets", "--variant", "mobile-e2e-iroh"])

    def test_the_multicast_permission_of_the_engines_lock_comes_with_every_app_and_the_two_iroh_apps_list_the_wifi_state(self):
        template = (demo_apps.ROOT / "scripts" / "android" / "AndroidManifest.xml").read_text(encoding="utf-8")
        self.assertIn("android.permission.CHANGE_WIFI_MULTICAST_STATE", template)   # the lock of dll/src/desktop/extra/iroh/multicast_lock.rs
        for name in ("AzMeet", "AzCompanion"):
            self.assertIn("android.permission.ACCESS_WIFI_STATE", self.app(name)["android"]["permissions"], name)

    def test_azmeet_asks_ios_for_the_local_network_and_the_bonjour_service_of_iroh_in_two_languages(self):
        ios = self.app("AzMeet")["ios"]
        self.assertIn("Wi-Fi", ios["local_network"])
        self.assertIn("NSLocalNetworkUsageDescription", ios["strings"]["de"])
        self.assertIn("WLAN", ios["strings"]["de"]["NSLocalNetworkUsageDescription"])


class LibraryDeltaTest(unittest.TestCase):
    """What the iroh libraries add to the plain ones (ring and tokio) is a table of the status: the size and the build seconds of the
    library job, against the closest smaller library of the same target."""

    ANDROID_PLAIN = {"kind": "android", "target": "x86_64-linux-android", "seconds": 384, "size": 51713400,
                     "features": "std,logging,link-static,a11y,map-tiles,pdf,android-activity"}
    ANDROID_IROH = dict(ANDROID_PLAIN, seconds=639, size=61162072, features=ANDROID_PLAIN["features"] + ",iroh")

    def test_the_measured_android_pair_gives_nine_mib_and_four_minutes_more(self):
        # the first emulator runs of 2026-11-26 (run 45773860433: the plain library, run 45773884468: the one with iroh)
        deltas = demo_apps.lib_deltas([self.ANDROID_PLAIN, self.ANDROID_IROH])
        self.assertEqual(len(deltas), 1)
        self.assertEqual((deltas[0]["added"], deltas[0]["size"], deltas[0]["seconds"]), (["iroh"], 9448672, 255))

    def test_a_library_with_no_smaller_sibling_has_no_delta_and_other_targets_do_not_mix(self):
        other = dict(self.ANDROID_IROH, target="aarch64-linux-android")
        self.assertEqual(demo_apps.lib_deltas([self.ANDROID_PLAIN, other]), [])
        self.assertEqual(demo_apps.lib_deltas([self.ANDROID_PLAIN]), [])
        self.assertEqual(demo_apps.lib_deltas([]), [])

    def test_the_closest_smaller_library_is_the_base(self):
        qa = dict(self.ANDROID_PLAIN, seconds=481, size=57808488, features=self.ANDROID_PLAIN["features"] + ",debug-server,e2e-scripting,test-hooks")
        qa_iroh = dict(qa, seconds=700, size=67000000, features=qa["features"] + ",iroh")
        deltas = demo_apps.lib_deltas([self.ANDROID_PLAIN, self.ANDROID_IROH, qa, qa_iroh])
        got = sorted((tuple(d["added"]), d["size"]) for d in deltas)
        # the plain library with iroh against the plain one; the QA library against the plain one; the QA library with iroh against the QA
        # library (it contains the features of the QA library, which are more than the plain one with iroh has): the same "iroh", two sizes
        self.assertEqual(got, [(("debug-server", "e2e-scripting", "test-hooks"), 57808488 - 51713400), (("iroh",), 67000000 - 57808488),
                               (("iroh",), 61162072 - 51713400)])

    def test_the_status_prints_the_delta_with_its_percent_and_keeps_it_in_the_summary(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            for i, lib in enumerate((self.ANDROID_PLAIN, self.ANDROID_IROH)):
                (root / f"l{i}").mkdir()
                (root / f"l{i}" / "libazul.info.json").write_text(json.dumps(lib))
            data, md = demo_apps.status([root], m=demo_apps.load())
        self.assertEqual(data["summary"]["lib_deltas"][0]["added"], ["iroh"])
        self.assertIn("What a library adds to the one without it", md)
        self.assertIn("| x86_64-linux-android (android) | iroh | +9.0 MB (+18 %) | +255 s (+66 %) |", md)

    def test_no_pair_no_table(self):
        data, md = demo_apps.status([], m=demo_apps.load())
        self.assertEqual(data["summary"]["lib_deltas"], [])
        self.assertNotIn("What a library adds", md)


class PublishListTest(unittest.TestCase):
    """`demo_publish.py list`: what a publication carries and which files of the manifest it does not (their links read "not available")."""

    def assembled(self, d, names):
        root = pathlib.Path(d)
        run = root / "run" / "demoapps-x"
        run.mkdir(parents=True)
        for n in names:
            (run / n).write_bytes(b"binary of " + n.encode())
        out = root / "release"
        demo_publish.assemble(root / "run", out, "0.2.0", "abc")
        return out

    def test_the_listing_names_the_assets_and_the_files_that_are_missing(self):
        with tempfile.TemporaryDirectory() as d:
            out = self.assembled(d, ["AzWidgets-linux", "AzWidgets-android-x86_64.apk"])
            listing = json.loads((out / "release-assets.json").read_text())
            text = demo_publish.render_listing(listing, demo_apps.load())
        self.assertIn("| AzWidgets-linux.tar.gz | AzWidgets | linux |", text)
        self.assertIn("| AzWidgets-android-x86_64.apk | AzWidgets | android-x86_64 |", text)
        self.assertIn("**2 assets,", text)
        self.assertIn("- AzWidgets: macos-arm64, macos-intel, windows, android-arm64, ios-device, ios-sim", text)
        self.assertIn("- AzMaps: linux, macos-arm64", text)        # every other app is missing everywhere
        self.assertIn('the page reads "not available" for each', text)

    def test_a_publication_of_everything_says_so(self):
        m = demo_apps.load()
        listing = [{"name": demo_apps.release_name(a, c), "app": a["package"], "column": c, "sha256": "0" * 64, "size": 1}
                   for a in m["app"] for c in demo_apps.columns_of(a)]
        text = demo_publish.render_listing(listing, m)
        self.assertIn("Every file of the manifest is in this publication.", text)
        self.assertEqual(demo_publish.unpublished(listing, m), [])

    def test_the_known_failing_reason_travels_with_the_missing_file(self):
        m = demo_apps.load()
        m["app"][0]["known_failing"] = {"ios": "the Simulator shows a black window"}
        missing = demo_publish.unpublished([], m)
        ios = [x for x in missing if x[0] == m["app"][0]["package"] and x[1].startswith("ios")]
        self.assertEqual({x[3] for x in ios}, {"the Simulator shows a black window"})

    def test_the_command_prints_the_listing_and_fails_on_an_empty_one(self):
        with tempfile.TemporaryDirectory() as d:
            out = self.assembled(d, ["AzWidgets-linux"])
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                self.assertEqual(demo_publish.main(["list", "--out", str(out)]), 0)
            self.assertIn("AzWidgets-linux.tar.gz", buf.getvalue())
            empty = pathlib.Path(d) / "empty"
            empty.mkdir()
            (empty / "release-assets.json").write_text("[]")
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(demo_publish.main(["list", "--out", str(empty)]), 1)
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(demo_publish.main(["list", "--out", str(pathlib.Path(d) / "none")]), 1)


class PublishWorkflowTest(unittest.TestCase):
    """.github/workflows/demos-publish.yml uploads the apps of a finished demos run to the release: read as text (no YAML module)."""

    def setUp(self):
        self.text = (demo_apps.ROOT / ".github" / "workflows" / "demos-publish.yml").read_text(encoding="utf-8")
        self.demos = (demo_apps.ROOT / ".github" / "workflows" / "demos.yml").read_text(encoding="utf-8")

    def test_it_is_dispatched_by_hand_and_a_dry_run_by_default(self):
        self.assertIn("workflow_dispatch:", self.text)
        for trigger in ("push:", "pull_request:", "schedule:", "workflow_call:", "workflow_run:"):
            self.assertNotIn("\n  " + trigger, self.text)
        self.assertRegex(self.text, r"(?s)run_id:\s+description: .*?required: true")
        self.assertRegex(self.text, r"(?s)dry_run:\s+description: .*?default: true")

    def test_it_can_write_to_the_release_only_inside_its_one_job(self):
        top = self.text.split("\njobs:\n")[0]
        self.assertIn("permissions:\n  contents: read", top)
        self.assertNotIn("contents: write", top)
        self.assertEqual(self.text.count("contents: write"), 1)
        self.assertIn("    permissions:\n      contents: write\n      actions: read", self.text)

    def test_the_upload_runs_only_when_it_was_asked_for_and_nothing_expands_an_input_into_a_script(self):
        self.assertIn("if: ${{ !inputs.dry_run }}", self.text)
        for run in re.findall(r"(?ms)^        run: (.*?)(?=^        \S|^      - |\Z)", self.text):
            self.assertNotIn("${{", run)

    def test_it_reads_only_the_shipped_artifacts_of_the_run_never_the_qa_builds(self):
        self.assertIn("pattern: demoapps-*", self.text)
        self.assertNotIn("demoe2e", "\n".join(l for l in self.text.splitlines() if not l.lstrip().startswith("#")))
        self.assertIn("run-id: ${{ inputs.run_id }}", self.text)
        self.assertIn("name: demo-status", self.text)

    def test_the_run_must_be_finished_and_a_run_of_rust_yml(self):
        self.assertIn('[ "$status" = completed ]', self.text)
        self.assertIn(".github/workflows/rust.yml*", self.text)
        self.assertIn("*[!0-9]*", self.text)      # a run id is a number

    def test_it_uses_the_same_pinned_actions_as_the_demos_workflow(self):
        for line in re.findall(r"(?m)^\s+- uses: (\S+@[0-9a-f]{40}) # (\S+)", self.text):
            self.assertIn(f"{line[0]} # {line[1]}", self.demos, line)
        self.assertEqual(len(re.findall(r"uses: \S+@", self.text)), len(re.findall(r"uses: \S+@[0-9a-f]{40} # v", self.text)))

    def test_it_assembles_with_the_script_the_demos_workflow_uses_and_lists_what_the_page_will_lack(self):
        self.assertIn("python3 scripts/demo_publish.py assemble --in demo-all --out release-assets", self.text)
        self.assertIn("python3 scripts/demo_publish.py list --out release-assets", self.text)
        self.assertIn("cp demo-status/demo-status.json release-assets/demo-status.json", self.text)


class PageAlignedStatusTest(unittest.TestCase):
    """The 16 KiB page check of the Android libraries is a line of the status table, not only a gate that fails a build: the result of an
    APK says how many of its libraries passed (page_aligned_libs), the info of a library says that it did (page_aligned)."""

    def test_no_test_of_this_file_sees_an_sdk_and_so_none_runs_the_apksigner_of_a_machine_that_has_one(self):
        # a machine with build-tools (ubuntu-latest) makes verify_apk run apksigner on the made-up APK of a test and fail it
        self.assertEqual(os.environ.get("ANDROID_HOME"), "")
        self.assertEqual(os.environ.get("ANDROID_SDK_ROOT"), "")
        with tempfile.TemporaryDirectory() as d:
            tools = pathlib.Path(d) / "build-tools" / "34.0.0"
            tools.mkdir(parents=True)
            (tools / "apksigner").write_text("#!/bin/sh\nexit 1\n")
            (tools / "apksigner").chmod(0o755)
            sys.path.insert(0, str(HERE))
            import test_android_elf_check as t
            apk = pathlib.Path(d) / "AzX.apk"
            t.apk(apk, [("classes.dex", b"dex\n035", False, 0), ("lib/x86_64/libazul.so", t.elf([0x4000] * 3), False, 0)])
            with mock.patch.dict(os.environ, {"ANDROID_HOME": d}):     # the machine HAS an SDK: the apksigner of it fails the APK
                with self.assertRaises(RuntimeError) as caught:
                    demo_build.verify_apk(apk, "x86_64")
            self.assertIn("not validly signed", str(caught.exception))
            self.assertEqual(demo_build.verify_apk(apk, "x86_64"), 1)   # without it, as every test of this file runs: the ELF check only

    def test_the_count_of_the_libraries_of_an_apk_is_returned_by_its_verification(self):
        import android_elf_check
        sys.path.insert(0, str(HERE))
        import test_android_elf_check as t
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "AzX.apk"
            t.apk(path, [("classes.dex", b"dex\n035", False, 0), ("lib/x86_64/libazul.so", t.elf([0x4000] * 3), False, 0),
                         ("lib/x86_64/libAzX.so", t.elf([0x4000] * 3), False, 0), ("res/mipmap/icon.png", b"png", False, 0)])
            self.assertEqual(android_elf_check.libraries(str(path)), ["lib/x86_64/libazul.so", "lib/x86_64/libAzX.so"])
            self.assertEqual(demo_build.verify_apk(path, "x86_64"), 2)
        self.assertEqual(android_elf_check.libraries("/no/such.apk"), [])

    def test_the_status_shows_the_count_in_the_files_table_and_the_alignment_of_a_library(self):
        m = demo_apps.load()
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "AzWidgets.android-x86_64.result.json").write_text(json.dumps({
                "app": "AzWidgets", "column": "android-x86_64", "status": "built", "size": 2048, "shipped_size": 2048, "seconds": 30,
                "engine_crates": [], "artifact": "AzWidgets-android-x86_64.apk", "page_aligned_libs": 2}))
            (root / "libs").mkdir()
            (root / "libs" / "libazul.info.json").write_text(json.dumps({
                "kind": "android", "target": "x86_64-linux-android", "features": "std", "seconds": 400, "size": 51713400, "page_aligned": True}))
            data, md = demo_apps.status([root], m=m)
        cell = next(r for r in data["apps"] if r["app"] == "AzWidgets")["cells"]["android-x86_64"]
        self.assertEqual(cell["page_aligned_libs"], 2)
        self.assertIn("16 KiB pages: 2 libraries aligned", md)
        self.assertIn("platform checks", md)
        self.assertIn("| 16 KiB pages aligned |", md)

    def test_the_library_script_writes_the_alignment_for_android_only(self):
        text = (HERE / "demo_lib.sh").read_text(encoding="utf-8")
        self.assertIn('info["page_aligned"] = True', text)
        self.assertIn('if kind == "android":', text)


class AbiGuardInitializerTest(unittest.TestCase):
    """`xcrun dyld_info -inits` of an Apple binary lists its initializers (the entries of __init_offsets on iOS, __mod_init_func on macOS).
    The generated ABI guard's load-time entry is ONE of them: dyld_info prints a Rust function pointer anonymously, so ONE entry is the
    expected evidence and none is the failure to look for (the iOS shell agent, from a miniature of the guard)."""

    ONE = "demo-out/apps/AzWidgets-macos [arm64]:\n    -inits:\n        0x00024068  <anon-00024068>\n"
    TWO = ONE + "        0x00031010  _some_c_constructor\n"
    NONE = "demo-out/apps/AzWidgets-macos [arm64]:\n    -inits:\n"

    def test_the_entries_of_the_listing_are_counted(self):
        self.assertEqual(demo_build.count_initializers(self.ONE), 1)
        self.assertEqual(demo_build.count_initializers(self.TWO), 2)
        self.assertEqual(demo_build.count_initializers(self.NONE), 0)
        self.assertEqual(demo_build.count_initializers(""), 0)

    def test_a_machine_without_the_tool_says_nothing(self):
        self.assertIsNone(demo_build.apple_initializers("/no/such/binary", which=lambda name: None))

    def test_the_count_travels_in_the_result_and_the_status_says_one_initializer(self):
        m = demo_apps.load()
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "AzWidgets.ios-sim.result.json").write_text(json.dumps({
                "app": "AzWidgets", "column": "ios-sim", "status": "built", "size": 2048, "shipped_size": 2048, "seconds": 60,
                "engine_crates": [], "artifact": "AzWidgets-ios-sim.app.zip", "initializers": 1}))
            (root / "AzMaps.ios-sim.result.json").write_text(json.dumps({
                "app": "AzMaps", "column": "ios-sim", "status": "built", "size": 2048, "shipped_size": 2048, "seconds": 60,
                "engine_crates": [], "artifact": "AzMaps-ios-sim.app.zip", "initializers": 0}))
            data, md = demo_apps.status([root], m=m)
        cell = next(r for r in data["apps"] if r["app"] == "AzWidgets")["cells"]["ios-sim"]
        self.assertEqual(cell["initializers"], 1)
        self.assertIn("1 initializer", md)
        self.assertNotIn("1 initializers", md)
        # none is the failure to look for
        self.assertEqual(data["summary"]["abi_guard_missing"], ["AzMaps/ios-sim"])
        self.assertIn("0 initializers", md)

    def test_the_build_driver_counts_them_for_the_apple_binaries_only(self):
        text = (HERE / "demo_build.py").read_text(encoding="utf-8")
        self.assertIn("apple_initializers(", text)
        self.assertIn('startswith("macos")', text)


class SymbolGateStatusTest(unittest.TestCase):
    """The numbers of scripts/ci/ios_symbol_gate.py (min_os, imports, unchecked, violations[...]) travel in the small results artifact as
    <App>.<column>.symbols.json (AZ_IOS_SYMBOL_JSON) and the status table shows them next to the iOS files, as information."""

    def test_the_counts_of_the_gate_are_in_the_cell_and_in_the_files_table(self):
        m = demo_apps.load()
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "AzWidgets.ios-sim.result.json").write_text(json.dumps({
                "app": "AzWidgets", "column": "ios-sim", "status": "built", "size": 2048, "shipped_size": 2048, "seconds": 60,
                "engine_crates": [], "artifact": "AzWidgets-ios-sim.app.zip"}))
            (root / "AzWidgets.ios-sim.symbols.json").write_text(json.dumps({
                "min_os": "16.0", "imports": 412, "unchecked": 37, "violations": []}))
            (root / "AzWidgets.ios-sim-e2e.symbols.json").write_text(json.dumps({"min_os": "16.0", "imports": 999, "unchecked": 1, "violations": []}))
            (root / "broken.symbols.json").write_text("{not json")
            data, md = demo_apps.status([root], m=m)
        cell = next(r for r in data["apps"] if r["app"] == "AzWidgets")["cells"]["ios-sim"]
        self.assertEqual(cell["symbols"], {"min_os": "16.0", "imports": 412, "unchecked": 37, "violations": 0})
        self.assertIn("412 imports, 37 unchecked, 0 violations", md)
        self.assertEqual(data["summary"]["symbol_violations"], [])

    def test_a_violation_is_listed_by_app_and_column(self):
        m = demo_apps.load()
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "AzMaps.ios-device.symbols.json").write_text(json.dumps({
                "min_os": "16.0", "imports": 10, "unchecked": 0,
                "violations": [{"name": "_AVCaptureDeviceTypeExternal", "lib": "AVFoundation", "weak": False, "why": "iOS 17.0"}]}))
            data, _ = demo_apps.status([root], m=m)
        self.assertEqual(data["summary"]["symbol_violations"], ["AzMaps/ios-device"])

    def test_an_app_without_the_numbers_shows_nothing(self):
        data, md = demo_apps.status([], m=demo_apps.load())
        cell = next(r for r in data["apps"] if r["app"] == "AzWidgets")["cells"]["ios-sim"]
        self.assertNotIn("symbols", cell)
        self.assertNotIn("unchecked", md)


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

    def test_the_libraries_are_those_of_the_app_it_is_given_and_not_of_the_word(self):
        # AzMeet and AzCompanion link the iroh libraries, the lean apps the plain ones: the script asks the manifest
        self.assertIn('python3 scripts/demo_apps.py mobile-lib "$app"', self.text)
        self.assertIn('python3 scripts/demo_apps.py e2e-lib "$app"', self.text)
        self.assertIn('demo-out/libs/demolib-$col-$lib', self.text)

    def test_the_script_deletes_nothing(self):
        code = "\n".join(l for l in self.text.splitlines() if not l.lstrip().startswith("#"))
        self.assertNotRegex(code, r"\brm\b")


class MacosBashTest(unittest.TestCase):
    """macOS runners (and Macs) have bash 3.2, where `"${array[@]}"` of an EMPTY array is an "unbound variable" error under `set -u`:
    the first macOS desktop library of the pipeline died at its first command with `target_flag[@]: unbound variable`
    (run 45779775681), because the host build has no --target. The scripts that run on a Mac expand an array only the empty-safe
    way, `${array[@]+"${array[@]}"}`."""

    SCRIPTS = ("demo_lib.sh", "build-ios.sh", "ci/ios_smoke.sh", "ci/ios_build_qa.sh")

    def test_no_script_that_runs_on_a_mac_expands_an_array_the_bash_3_2_cannot(self):
        for name in self.SCRIPTS:
            text = (HERE / name).read_text(encoding="utf-8")
            # an array expansion in double quotes that is not the inner half of `${array[@]+"${array[@]}"}` (that one follows a `+`)
            unsafe = [text.count("\n", 0, m.start()) + 1 for m in re.finditer(r'(?<!\+)"\$\{(\w+)\[@\]\}"', text)]
            self.assertEqual(unsafe, [], f"{name}: lines {unsafe} expand an array that may be empty")

    def test_the_host_build_of_the_desktop_library_survives_an_empty_target(self):
        text = (HERE / "demo_lib.sh").read_text(encoding="utf-8")
        self.assertIn('${target_flag[@]+"${target_flag[@]}"}', text)
        self.assertNotIn('"${target_flag[@]}" -p', text)


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

    def test_the_apps_that_use_iroh_name_the_iroh_libraries(self):
        self.assertEqual(self.mobile_lib("AzMeet"), (0, "mobile-iroh"))
        self.assertEqual(self.mobile_lib("AzCompanion"), (0, "mobile-iroh"))
        self.assertEqual(self.mobile_lib("AzWriter"), (0, "mobile"))

    def test_an_app_that_has_no_phone_build_or_is_unknown_is_an_error(self):
        self.assertEqual(self.mobile_lib("AzMail")[0], 1)
        self.assertEqual(self.mobile_lib("AzNoSuchApp")[0], 1)
        self.assertEqual(self.mobile_lib()[0], 2)

    def e2e_lib(self, *argv):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(io.StringIO()):
            code = demo_apps.main(["e2e-lib", *argv])
        return code, buf.getvalue().strip()

    def test_the_qa_library_of_an_app_is_its_e2e_lib(self):
        self.assertEqual(self.e2e_lib("AzWidgets"), (0, "mobile-e2e"))
        self.assertEqual(self.e2e_lib("AzMeet"), (0, "mobile-e2e-iroh"))
        self.assertEqual(self.e2e_lib("AzCompanion"), (0, "mobile-e2e-iroh"))
        self.assertEqual(self.e2e_lib("AzMail")[0], 1)           # no QA build
        self.assertEqual(self.e2e_lib()[0], 2)


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

    def test_the_script_parses_and_builds_the_libraries_of_the_plan_then_every_build_of_the_plan_then_checks_the_pages(self):
        self.assertEqual(subprocess.run(["bash", "-n", str(self.path)]).returncode, 0)
        order = [self.text.index(n) for n in ('[ "$kind" = lib ] || continue', 'bash scripts/demo_lib.sh android',
                                              '[ "$kind" = apps ] || continue', "python3 scripts/demo_build.py android --target",
                                              "android_elf_check.py demo-out/apps")]
        self.assertEqual(order, sorted(order))
        self.assertIn("demo_apps.py features", self.text)

    def test_what_it_builds_is_the_plan_of_its_column_the_shipped_apps_and_the_qa_builds_of_every_qa_library(self):
        # the script reads the plan (`plan --e2e`): the libraries and the builds with their variants, none is named in the script, so the
        # QA twin of the iroh library and the QA builds of the apps that use iroh come with it
        plan = demo_apps.plan(demo_apps.load(), e2e=True)
        libs = sorted(e["variant"] for e in plan["libs_android"] if e["leg"] == "android-x86_64")
        self.assertEqual(libs, ["mobile", "mobile-e2e", "mobile-e2e-iroh", "mobile-iroh"])
        builds = {e["variant"]: sorted(e["apps"].split()) for e in plan["apps_android"] if e["leg"] == "android-x86_64"}
        self.assertEqual(builds[""], ["AzCompanion", "AzMaps", "AzMeet", "AzPaint", "AzWidgets", "AzWriter"])
        self.assertEqual(builds["mobile-e2e"], ["AzMaps", "AzPaint", "AzWidgets", "AzWriter"])
        self.assertEqual(builds["mobile-e2e-iroh"], ["AzCompanion", "AzMeet"])
        self.assertIn("plan --e2e", self.text)
        for name in ("mobile-e2e", "mobile-iroh"):
            self.assertNotIn(name, "\n".join(l for l in self.text.splitlines() if not l.lstrip().startswith("#")))

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
        self.assertEqual(sorted(k for k in plist if k.endswith("UsageDescription")),
                         ["NSCameraUsageDescription", "NSLocalNetworkUsageDescription", "NSMicrophoneUsageDescription"])
        # AzMeet uses iroh: its direct path on the local network needs the prompt and the Bonjour service, as the companion's does
        self.assertEqual(plist["NSLocalNetworkUsageDescription"], app["ios"]["local_network"])
        self.assertEqual(plist["NSBonjourServices"], ["_irohv1._udp"])

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

    def test_the_probe_can_be_left_out_and_the_build_and_boot_script_can_run_both_ways(self):
        # run 45787048273: the key pressed after the toast of the probe did nothing, and the page scrolled; whether the toast layer
        # or the key is at fault is told by the same test with the key pressed FIRST, in a fresh process
        smoke = (HERE / "ci" / "android_smoke.sh").read_text(encoding="utf-8")
        self.assertIn('AZ_SMOKE_NO_PROBE', smoke)
        build = (HERE / "ci" / "android_build_smoke.sh").read_text(encoding="utf-8")
        self.assertIn("AZ_SMOKE_BOTH_WAYS", build)
        self.assertIn("AZ_SMOKE_NO_PROBE=1 bash scripts/ci/android_smoke.sh demo-out/smoke-noprobe", build)

    def test_the_android_boot_test_turns_the_apps_info_log_on_for_an_app_with_a_press(self):
        text = (HERE / "ci" / "android_smoke.sh").read_text(encoding="utf-8")
        # AZ_LOG=info: the app's own lines of what a press did ("tap <Action>"); AZ_SMOKE_LOG=debug adds the shell's `[Android] motion
        # <Down|Up> at (x,y) -> <result>` line per touch event
        self.assertIn('setprop debug.az.log "${AZ_SMOKE_LOG:-info}"', text)
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

    def setUp(self):
        # verify_apk runs apksigner and aapt2 of the SDK's build-tools when the machine has them (ubuntu-latest has: its preflight job
        # ran them on these made-up APKs, which have no manifest, and failed); these tests are about the ELF check, so they see no SDK
        patcher = mock.patch.dict(os.environ, {"ANDROID_HOME": "", "ANDROID_SDK_ROOT": ""})
        patcher.start()
        self.addCleanup(patcher.stop)

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
