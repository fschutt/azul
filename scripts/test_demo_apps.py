#!/usr/bin/env python3
"""Tests of the demo pipeline's scripts: the manifest reader and planner (demo_apps.py), the engine gate
(demo_gate.py) and the pure parts of the builder (demo_build.py).

    python3 scripts/test_demo_apps.py            (Python 3.11+, standard library only)
"""
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

    def test_the_icons_of_a_platform_land_where_its_packager_looks(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            demo_icon.main(["android", "AzMaps", str(root / "res")])
            demo_icon.main(["ios", "AzMaps", str(root / "app")])
            self.assertTrue((root / "res" / "mipmap-xxxhdpi" / "ic_launcher.png").is_file())
            self.assertTrue((root / "app" / "AppIcon60x60@3x.png").is_file())


if __name__ == "__main__":
    unittest.main(verbosity=2)
