#!/usr/bin/env python3
"""Tests of scripts/ci/ios_sim_feed.py: the commands that put a Simulator into a state (standard library only).

    python3 scripts/ci/test_ios_sim_feed.py
"""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import ios_sim_feed as feed  # noqa: E402

U = "BOOTED-UDID"


class Biometric(unittest.TestCase):
    def test_enrolling_sets_the_state_and_then_posts_it_in_two_calls(self):
        a, b = feed.enrollment_commands(U, True)
        self.assertEqual(a, ["xcrun", "simctl", "spawn", U, "notifyutil", "-s", "com.apple.BiometricKit.enrollmentChanged", "1"])
        self.assertEqual(b, ["xcrun", "simctl", "spawn", U, "notifyutil", "-p", "com.apple.BiometricKit.enrollmentChanged"])

    def test_unenrolling_is_state_zero(self):
        self.assertEqual(feed.enrollment_commands(U, False)[0][-1], "0")

    def test_face_id_match_and_no_match(self):
        self.assertEqual(feed.match_commands(U, True), [["xcrun", "simctl", "spawn", U, "notifyutil", "-p", "com.apple.BiometricKit_Sim.pearl.match"]])
        self.assertEqual(feed.match_commands(U, False)[0][-1], "com.apple.BiometricKit_Sim.pearl.nomatch")

    def test_touch_id_uses_the_finger_touch_words(self):
        self.assertEqual(feed.match_commands(U, True, touch=True)[0][-1], "com.apple.BiometricKit_Sim.fingerTouch.match")
        self.assertEqual(feed.match_commands(U, False, touch=True)[0][-1], "com.apple.BiometricKit_Sim.fingerTouch.nomatch")


class Other(unittest.TestCase):
    def test_a_location_is_lat_comma_lon(self):
        self.assertEqual(feed.location_commands(U, "48.52", "9.05"), [["xcrun", "simctl", "location", U, "set", "48.52,9.05"]])

    def test_a_coordinate_that_is_none_is_refused(self):
        with self.assertRaises(ValueError):
            feed.location_commands(U, "91", "0")
        with self.assertRaises(ValueError):
            feed.location_commands(U, "0", "181")
        with self.assertRaises(ValueError):
            feed.location_commands(U, "north", "0")

    def test_privacy_names_the_service_and_the_app(self):
        self.assertEqual(feed.privacy_commands(U, "revoke", "location", "com.azul.AzSelfTest"),
                         [["xcrun", "simctl", "privacy", U, "revoke", "location", "com.azul.AzSelfTest"]])

    def test_privacy_refuses_what_simctl_does_not_know(self):
        with self.assertRaises(ValueError):
            feed.privacy_commands(U, "deny", "location", "x")
        with self.assertRaises(ValueError):
            feed.privacy_commands(U, "grant", "camera", "x")  # the Simulator has no camera service to grant

    def test_appearance(self):
        self.assertEqual(feed.appearance_commands(U, "dark"), [["xcrun", "simctl", "ui", U, "appearance", "dark"]])
        with self.assertRaises(ValueError):
            feed.appearance_commands(U, "sepia")


class Runner(unittest.TestCase):
    def test_a_missing_tool_is_a_failure_not_an_exception(self):
        ok, log = feed.run([["definitely-not-a-tool-xyz"]])
        self.assertFalse(ok)
        self.assertEqual(log[0][1], 127)

    def test_it_stops_at_the_first_failure(self):
        ok, log = feed.run([["false"], ["true"]])
        self.assertFalse(ok)
        self.assertEqual(len(log), 1)


if __name__ == "__main__":
    unittest.main()
