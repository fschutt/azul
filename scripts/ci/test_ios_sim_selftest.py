#!/usr/bin/env python3
"""Tests of scripts/ci/ios_sim_selftest.py: the pure parts (device choice, the environment of the launch, the await protocol,
the expectations of a Simulator run); the Simulator itself is the job's (standard library only).

    python3 scripts/ci/test_ios_sim_selftest.py
"""
import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import ios_sim_selftest as sim  # noqa: E402

LISTING = json.dumps({
    "devices": {
        "com.apple.CoreSimulator.SimRuntime.iOS-17-5": [
            {"name": "iPhone 16 Pro", "udid": "OLD-PRO", "state": "Shutdown", "isAvailable": True},
        ],
        "com.apple.CoreSimulator.SimRuntime.iOS-18-2": [
            {"name": "iPhone 16 Pro", "udid": "NEW-PRO", "state": "Shutdown", "isAvailable": True},
            {"name": "iPhone SE (3rd generation)", "udid": "SE", "state": "Booted", "isAvailable": True},
            {"name": "iPhone 15", "udid": "GONE", "state": "Shutdown", "isAvailable": False},
        ],
        "com.apple.CoreSimulator.SimRuntime.watchOS-11-0": [
            {"name": "Apple Watch", "udid": "WATCH", "state": "Shutdown", "isAvailable": True},
        ],
    }
})


class Devices(unittest.TestCase):
    def test_only_available_ios_devices_are_listed_newest_runtime_first(self):
        devices = sim.parse_devices(LISTING)
        self.assertEqual([d["udid"] for d in devices], ["NEW-PRO", "SE", "OLD-PRO"])

    def test_a_name_picks_the_newest_runtime(self):
        self.assertEqual(sim.pick_device(sim.parse_devices(LISTING), "iPhone 16 Pro")["udid"], "NEW-PRO")

    def test_a_udid_picks_exactly_that_device(self):
        self.assertEqual(sim.pick_device(sim.parse_devices(LISTING), "OLD-PRO")["runtime"].split(".")[-1], "iOS-17-5")

    def test_an_unknown_device_is_none(self):
        self.assertIsNone(sim.pick_device(sim.parse_devices(LISTING), "iPhone 99"))


TYPES = json.dumps({"devicetypes": [
    {"name": "iPhone 16 Pro", "identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro"},
    {"name": "iPhone SE (3rd generation)", "identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation"},
]})
RUNTIMES = json.dumps({"runtimes": [
    {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-17-5", "name": "iOS 17.5", "version": "17.5", "isAvailable": True},
    {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-26-2", "name": "iOS 26.2", "version": "26.2", "isAvailable": True,
     "supportedDeviceTypes": [{"identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation"},
                              {"identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro"}]},
    {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-18-0", "name": "iOS 18.0", "version": "18.0", "isAvailable": False},
    {"identifier": "com.apple.CoreSimulator.SimRuntime.watchOS-11-0", "name": "watchOS 11.0", "version": "11.0", "isAvailable": True},
]})


class Resolve(unittest.TestCase):
    def test_runtimes_are_the_available_ios_ones_newest_first(self):
        self.assertEqual([r["version"] for r in sim.parse_runtimes(RUNTIMES)], ["26.2", "17.5"])

    def test_an_existing_device_is_used_as_it_is(self):
        action, device, note = sim.resolve_device("iPhone 16 Pro", sim.parse_devices(LISTING), sim.parse_device_types(TYPES), sim.parse_runtimes(RUNTIMES))
        self.assertEqual((action, device["udid"], note), ("use", "NEW-PRO", ""))

    def test_a_missing_device_whose_type_exists_is_made_on_the_newest_runtime(self):
        devices = [d for d in sim.parse_devices(LISTING) if d["name"] != "iPhone SE (3rd generation)"]
        action, value, note = sim.resolve_device("iPhone SE (3rd generation)", devices, sim.parse_device_types(TYPES), sim.parse_runtimes(RUNTIMES))
        self.assertEqual(action, "create")
        self.assertEqual(value, ("AzSelfTest iPhone SE (3rd generation)", "com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation",
                                 "com.apple.CoreSimulator.SimRuntime.iOS-26-2"))
        self.assertIn("device type", note)

    def test_a_type_the_newest_runtime_cannot_run_goes_to_an_older_runtime_that_can(self):
        types = [{"name": "iPhone 8", "identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-8"}]
        runtimes = [{"identifier": "new", "version": "26.2", "supported": {"x"}}, {"identifier": "old", "version": "17.5", "supported": {"com.apple.CoreSimulator.SimDeviceType.iPhone-8"}}]
        action, value, _ = sim.resolve_device("iPhone 8", [], types, runtimes)
        self.assertEqual((action, value[2]), ("create", "old"))
        # and when no runtime can run it, the device is not made
        self.assertEqual(sim.resolve_device("iPhone 8", [], types, [runtimes[0]])[0], "none")

    def test_a_model_this_machine_does_not_have_falls_back_to_the_newest_plain_iphone_and_says_so(self):
        devices = sim.parse_devices(json.dumps({"devices": {"com.apple.CoreSimulator.SimRuntime.iOS-26-2": [
            {"name": "iPhone 16", "udid": "A", "state": "Shutdown", "isAvailable": True},
            {"name": "iPhone 17", "udid": "B", "state": "Shutdown", "isAvailable": True},
            {"name": "iPhone 17 Pro", "udid": "C", "state": "Shutdown", "isAvailable": True},
        ]}}))
        action, device, note = sim.resolve_device("iPhone 5c", devices, [], sim.parse_runtimes(RUNTIMES))
        self.assertEqual((action, device["udid"]), ("use", "B"))
        self.assertIn("iPhone 17", note)
        self.assertIn("iPhone 5c", note)

    def test_no_iphone_at_all_is_none(self):
        self.assertEqual(sim.resolve_device("iPhone 99", [], [], [])[0], "none")


class Display(unittest.TestCase):
    PRO = "cutouts=1; shapes=[Pill@138.5,14.0 125.0x37.0 approximate]; corner_top_left=62; status_bar=402x54@0,0 visible=true; fingerprint=None"
    SE = "cutouts=0; shapes=[]; corner_top_left=0; status_bar=375x20@0,0 visible=true; navigation_bar=none; fingerprint=Front"

    def rep(self, detail, verdict="PASS"):
        return {"rows": [{"id": "system.display_features", "verdict": verdict, "detail": detail}]}

    def test_each_device_is_held_to_what_it_has(self):
        self.assertEqual(sim.check_display(self.rep(self.PRO), "iPhone 16 Pro"), [])
        self.assertEqual(sim.check_display(self.rep(self.SE), "iPhone SE (3rd generation)"), [])

    def test_a_simulator_made_by_the_script_is_still_the_model_it_was_made_from(self):
        self.assertTrue(sim.check_display(self.rep(self.SE), "AzSelfTest iPhone 16 Pro"))
        self.assertEqual(sim.check_display(self.rep(self.PRO), "AzSelfTest iPhone 16 Pro"), [])

    def test_a_pill_on_an_iphone_se_is_a_finding(self):
        self.assertTrue(sim.check_display(self.rep(self.PRO), "iPhone SE (3rd generation)"))
        self.assertTrue(sim.check_display(self.rep(self.SE), "iPhone 16 Pro"))

    def test_a_row_that_says_nothing_is_a_finding_only_on_a_known_device(self):
        self.assertTrue(sim.check_display(self.rep("", verdict="UNAVAILABLE"), "iPhone 16 Pro"))
        self.assertEqual(sim.check_display(self.rep("", verdict="UNAVAILABLE"), "iPhone 17"), [], "a stand-in device has no expectation")
        self.assertEqual(sim.check_display({"rows": []}, "iPhone 16 Pro"), [], "a build without the row is not blamed")


class Fonts(unittest.TestCase):
    MISS = "fonts:cache miss, 0 faces, check 2.1 ms, load 0.0 ms"
    WRITTEN = "fonts:cache_written Written, 1204 faces, 41.0 ms"
    HIT = "fonts:cache hit, 1204 faces, check 3.0 ms, load 11.0 ms"

    def test_the_lines_of_the_timeline_are_the_ones_that_name_fonts(self):
        text = "noise\n[startup] fonts:cache miss, 0 faces\nother line\nfonts:cache_written Written, 3 faces, 1 ms"
        self.assertEqual(font_lines_of(text), ["[startup] fonts:cache miss, 0 faces", "fonts:cache_written Written, 3 faces, 1 ms"])

    def test_a_miss_then_a_hit_is_a_working_cache(self):
        self.assertEqual(sim.font_cache_findings([self.MISS, self.WRITTEN], [self.HIT, "fonts:cache_written Unchanged"]), [])

    def test_a_second_launch_that_rebuilds_names_the_path(self):
        found = sim.font_cache_findings([self.MISS], ["fonts:cache rebuilt (2 font files changed, 0 gone, first: /System/Library/Fonts/SFNS.ttf)"])
        self.assertEqual(len(found), 2, found)
        self.assertTrue(any("SFNS.ttf" in f for f in found))

    def test_a_second_launch_without_lines_is_a_finding(self):
        self.assertTrue(sim.font_cache_findings([self.MISS], []))
        self.assertTrue(sim.font_cache_findings([], [self.HIT]))


def font_lines_of(text):
    return sim.font_lines(text)


class Ledger(unittest.TestCase):
    def test_one_line_per_row_with_the_verdict_of_each_run_and_the_real_device_column(self):
        fresh = {"rows": [{"id": "device.network", "verdict": "PASS"}, {"id": "sensors.gyroscope", "verdict": "UNAVAILABLE"}]}
        granted = {"rows": [{"id": "device.network", "verdict": "PASS"}, {"id": "sensors.gyroscope", "verdict": "UNAVAILABLE"},
                            {"id": "sensors.geolocation", "verdict": "PASS"}]}
        md = sim.ledger_table({"fresh": fresh, "granted": granted}, "iPhone 16 Pro")
        self.assertIn("| row | fresh | granted | a real iPhone is needed |", md)
        self.assertIn("| device.network | PASS | PASS | no |", md)
        self.assertIn("| sensors.gyroscope | UNAVAILABLE | UNAVAILABLE | yes |", md)
        self.assertIn("| sensors.geolocation | - | PASS | no |", md)


class Oslog(unittest.TestCase):
    def test_the_command_asks_the_simulator_for_the_apps_own_lines(self):
        command = sim.oslog_command("U", "AzSelfTest")
        self.assertEqual(command[:5], ["xcrun", "simctl", "spawn", "U", "log"])
        self.assertIn('process == "AzSelfTest"', command)
        self.assertIn("30m", command)


class Crashes(unittest.TestCase):
    def test_only_the_reports_of_this_app_after_the_start_are_found(self):
        import os
        import tempfile
        import time

        with tempfile.TemporaryDirectory() as d:
            d = pathlib.Path(d)
            (d / "AzSelfTest-2026-11-28-040000.ips").write_text("crash")
            (d / "AzSelfTest-2026-11-28-040001.txt").write_text("not a crash report")
            (d / "Other-2026-11-28-040000.ips").write_text("another app")
            old = d / "AzSelfTest-old.ips"
            old.write_text("a crash of an earlier job")
            os.utime(old, (time.time() - 3600, time.time() - 3600))
            found = sim.crash_reports_since(d, "AzSelfTest", time.time() - 60)
            self.assertEqual([p.name for p in found], ["AzSelfTest-2026-11-28-040000.ips"])

    def test_a_missing_directory_is_no_crash(self):
        self.assertEqual(sim.crash_reports_since("/nonexistent/dir", "AzSelfTest", 0), [])


class Launch(unittest.TestCase):
    def test_the_app_environment_goes_through_the_simctl_child_prefix(self):
        env = sim.launch_env()
        self.assertEqual(env["SIMCTL_CHILD_AZ_SELFTEST"], "1")
        self.assertEqual(env["SIMCTL_CHILD_AZ_SELFTEST_FED"], "1")
        self.assertTrue(all(k.startswith("SIMCTL_CHILD_") for k in env))

    def test_an_unfed_launch_does_not_promise_a_harness(self):
        self.assertNotIn("SIMCTL_CHILD_AZ_SELFTEST_FED", sim.launch_env(fed=False))


class Await(unittest.TestCase):
    def test_every_await_line_is_answered_once(self):
        seen = {}
        console = "noise\nAZ_SELFTEST_AWAIT biometric.match\nmore\n"
        self.assertEqual(sim.new_awaits(console, seen), ["biometric.match"])
        self.assertEqual(sim.new_awaits(console, seen), [])
        console += "AZ_SELFTEST_AWAIT biometric.match\nAZ_SELFTEST_AWAIT biometric.nomatch\n"
        self.assertEqual(sim.new_awaits(console, seen), ["biometric.match", "biometric.nomatch"])

    def test_a_face_that_matches_and_one_that_does_not(self):
        self.assertEqual(sim.feed_for("biometric.match", "U")[0][-1], "com.apple.BiometricKit_Sim.pearl.match")
        self.assertEqual(sim.feed_for("biometric.nomatch", "U")[0][-1], "com.apple.BiometricKit_Sim.pearl.nomatch")
        self.assertEqual(sim.feed_for("biometric.match", "U", touch=True)[0][-1], "com.apple.BiometricKit_Sim.fingerTouch.match")

    def test_a_location_key_carries_its_coordinates(self):
        self.assertEqual(sim.feed_for("location.48.5,9.0", "U"), [["xcrun", "simctl", "location", "U", "set", "48.5,9.0"]])

    def test_the_background_key_sends_the_app_away_and_brings_it_back(self):
        commands = sim.feed_for("lifecycle.background", "U", bundle="com.azul.AzSelfTest")
        self.assertEqual(commands[0], ["xcrun", "simctl", "launch", "U", "com.apple.Preferences"])
        self.assertEqual(commands[-1], ["xcrun", "simctl", "launch", "U", "com.azul.AzSelfTest"])
        self.assertIsNone(sim.feed_for("lifecycle.background", "U"), "without the app's bundle there is nothing to bring back")

    def test_an_unknown_or_malformed_key_has_no_feed(self):
        self.assertIsNone(sim.feed_for("shake", "U"))
        self.assertIsNone(sim.feed_for("location.north", "U"))


class Asked(unittest.TestCase):
    def test_the_lines_that_say_the_app_asked_for_a_permission(self):
        console = "[avf_auth] requesting video access (async, OS prompt)\nplain line\nrequestWhenInUseAuthorization sent"
        self.assertEqual(len(sim.asked_lines(console)), 2)

    def test_a_quiet_console_asked_for_nothing(self):
        self.assertEqual(sim.asked_lines("[avf_auth] video access already granted\nframe 3"), [])


def report(**verdicts):
    return {"rows": [{"id": rid.replace("__", "."), "verdict": v, "detail": "d"} for rid, v in verdicts.items()]}


class Expectations(unittest.TestCase):
    def full(self, **over):
        base = {
            "device__capabilities": "PASS", "device__network": "PASS", "security__keyring": "PASS",
            "security__biometric_kind": "PASS", "security__biometric_prompt": "PASS", "input__clipboard": "PASS",
            "system__permission_state": "PASS", "system__settings": "PASS", "sensors__accelerometer": "UNAVAILABLE",
            "sensors__gyroscope": "UNAVAILABLE", "sensors__magnetometer": "UNAVAILABLE", "sensors__barometer": "UNAVAILABLE",
            "sensors__geolocation": "NEEDS-HUMAN",
        }
        base.update(over)
        return report(**base)

    def test_a_good_fresh_run_has_no_findings(self):
        self.assertEqual(sim.check_rows(self.full(), "fresh"), [])

    def test_a_fail_anywhere_is_a_finding(self):
        found = sim.check_rows(self.full(system__haptics="FAIL"), "fresh")
        self.assertTrue(any("FAIL system.haptics" in f for f in found))

    def test_the_geolocation_row_follows_the_privacy_decision(self):
        self.assertEqual(sim.check_rows(self.full(sensors__geolocation="PASS"), "granted"), [])
        self.assertEqual(sim.check_rows(self.full(sensors__geolocation="DENIED"), "revoked"), [])
        self.assertTrue(sim.check_rows(self.full(sensors__geolocation="PASS"), "revoked"))
        self.assertTrue(sim.check_rows(self.full(sensors__geolocation="NEEDS-HUMAN"), "granted"))

    def test_a_missing_row_is_named(self):
        rows = self.full()
        rows["rows"] = [r for r in rows["rows"] if r["id"] != "input.clipboard"]
        self.assertTrue(any("MISSING input.clipboard" in f for f in sim.check_rows(rows, "fresh")))

    def test_the_biometric_prompt_must_pass_with_a_face_fed(self):
        self.assertTrue(sim.check_rows(self.full(security__biometric_prompt="NEEDS-HUMAN"), "fresh"))

    def test_a_sensor_the_simulator_lacks_may_not_fail_or_need_a_person(self):
        self.assertTrue(sim.check_rows(self.full(sensors__gyroscope="NEEDS-HUMAN"), "fresh"))


if __name__ == "__main__":
    unittest.main()
