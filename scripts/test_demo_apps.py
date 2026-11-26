#!/usr/bin/env python3
"""Tests of the demo pipeline's scripts: the manifest reader and planner (demo_apps.py), the engine gate
(demo_gate.py) and the pure parts of the builder (demo_build.py).

    python3 scripts/test_demo_apps.py            (Python 3.11+, standard library only)
"""
import argparse
import contextlib
import io
import json
import pathlib
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


class StatusTest(unittest.TestCase):
    def test_a_missing_result_is_unexplained_and_a_known_failure_is_not(self):
        m = demo_apps.load()
        data, md = demo_apps.status([], m=m)
        self.assertIn("AzWidgets/linux", data["summary"]["unexplained_failures"])
        self.assertIn("MISSING", md)

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

    def test_the_status_text_of_a_failed_boot_carries_the_reason(self):
        text = demo_apps.boot_text({"pass": False, "reason": "the home screen is still showing (99.8% of the pixels are the same)"})
        self.assertTrue(text.startswith("BOOT FAILED: the home screen"))
        self.assertEqual(demo_apps.boot_text({"pass": True}), "boots")


if __name__ == "__main__":
    unittest.main(verbosity=2)
