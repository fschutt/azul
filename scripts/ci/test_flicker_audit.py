#!/usr/bin/env python3
"""Tests of scripts/ci/flicker_audit.py: the pure analysis (python3 -I scripts/ci/test_flicker_audit.py)."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import flicker_audit as f  # noqa: E402
import mobile_qa as q  # noqa: E402


def sig(colour, bar=None):
    """A 40 x 80 signature of one colour; `bar` = (r, g, b) paints the middle third of the rows."""
    rows = []
    for y in range(q.ROWS):
        c = bar if bar and q.ROWS // 3 <= y < 2 * q.ROWS // 3 else colour
        rows.append(bytes(c) * q.COLS)
    return b"".join(rows)


PAGE = sig((240, 240, 240), (30, 80, 200))
DARK = sig((20, 20, 20), (30, 80, 200))
BLACK = sig((0, 0, 0))


class PresentTests(unittest.TestCase):
    LOG = ("11-27 23:00:01.000  1  2 I RustStdoutStderr: [  900000us][Info][Rendering] [Android] present full - rows 0..2400 of 2400\n"
           "11-27 23:00:02.000  1  2 I RustStdoutStderr: [ 1400000us][Info][Rendering] [Android] present partial - rows 100..160 of 2400\n"
           "11-27 23:00:02.500  1  2 I RustStdoutStderr: [ 1900000us][Info][Rendering] [Android] present partial - rows 100..160 of 2400\n")

    def test_present_lines_carry_kind_rows_and_the_engine_clock(self):
        ev = f.present_events(self.LOG)
        self.assertEqual([(e["us"], e["kind"], e["rows"]) for e in ev], [(900000, "full", 2400), (1400000, "partial", 60), (1900000, "partial", 60)])
        self.assertEqual(f.intervals_ms(ev), [500.0, 500.0])
        self.assertEqual(f.present_events(self.LOG, window=(1000000, 2000000))[0]["us"], 1400000)

    def test_frame_time_percentiles(self):
        ev = [{"us": t * 1000} for t in (0, 16, 33, 50, 66, 150)]
        st = f.frame_time_stats(ev)
        self.assertEqual((st["n"], st["max"]), (5, 84.0))
        self.assertEqual(st["p50"], 17.0)
        self.assertIsNone(f.frame_time_stats(ev[:1]))


class BurstTests(unittest.TestCase):
    """the engine-perf agent's finding: the 489 ms 'max' of build 4's pan was the gap BETWEEN two swipes (0.4 s of sleep and adb's latency), not a frame"""

    def swipes(self):
        one = [t * 1000 for t in range(0, 700, 50)]                       # a swipe: 14 presents 50 ms apart
        two = [t * 1000 for t in range(1190, 1890, 50)]                    # 490 ms after the end of the first one
        return [{"us": us} for us in one + two]

    def test_the_gap_between_two_swipes_is_not_a_frame_interval(self):
        st = f.frame_time_stats(self.swipes())
        self.assertEqual((st["max"], st["n"]), (50.0, 26), "only the intervals inside a swipe are frames")
        self.assertEqual(st["p50"], 50.0)

    def test_the_bursts_and_the_gaps_between_them_are_told(self):
        bursts, gaps = f.split_bursts(self.swipes())
        self.assertEqual([len(b) for b in bursts], [14, 14])
        self.assertEqual(gaps, [540.0])

    def test_one_burst_has_no_gap(self):
        bursts, gaps = f.split_bursts([{"us": t * 1000} for t in (0, 16, 33)])
        self.assertEqual((len(bursts), gaps), (1, []))
        self.assertEqual(f.split_bursts([]), ([], []))


class IdleTests(unittest.TestCase):
    def events(self, specs):
        return [{"us": i * 500000, "kind": k, "rows": r, "share": r / 2400} for i, (k, r) in enumerate(specs)]

    def test_nothing_presented_and_equal_screenshots_is_idle(self):
        self.assertTrue(f.idle_verdict([], [0.0, 0.0, 0.0, 0.0])[0])

    def test_a_blinking_caret_is_allowed(self):
        ok, text = f.idle_verdict(self.events([("partial", 40)] * 6), [0.0, 0.001, 0.0, 0.0])
        self.assertTrue(ok, text)

    def test_a_full_present_while_idle_is_a_redraw(self):
        ok, text = f.idle_verdict(self.events([("full", 2400)] * 3), [0.0] * 4)
        self.assertFalse(ok)
        self.assertIn("a redraw, not a caret", text)

    def test_screenshots_that_differ_are_not_idle(self):
        self.assertFalse(f.idle_verdict([], [0.0, 0.2, 0.0])[0])

    def test_too_many_small_presents_are_not_a_caret(self):
        self.assertFalse(f.idle_verdict(self.events([("partial", 40)] * 20), [0.0] * 4)[0])


class FlagTests(unittest.TestCase):
    def test_a_clean_recording_flags_nothing(self):
        self.assertEqual(f.flag_frames([PAGE] * 6 + [DARK] * 6), [])

    def test_a_black_frame_between_two_normal_ones_is_flat(self):
        flagged = f.flag_frames([PAGE, PAGE, BLACK, PAGE, PAGE])
        self.assertEqual([(x["index"], x["kind"]) for x in flagged], [(2, "flat")])

    def test_a_one_frame_flash_of_another_picture_is_flagged(self):
        flagged = f.flag_frames([PAGE, PAGE, DARK, PAGE, PAGE])
        self.assertEqual([(x["index"], x["kind"]) for x in flagged], [(2, "flash")])

    def test_a_fade_through_two_states_is_not_a_flash(self):
        self.assertEqual(f.flag_frames([PAGE, PAGE, DARK, DARK, DARK]), [])

    def test_a_state_shown_twice_out_of_order_is_flagged(self):
        flagged = f.flag_frames([PAGE, PAGE, DARK, DARK, PAGE, PAGE, DARK, DARK])
        self.assertIn("out-of-order", [x["kind"] for x in flagged])

    def test_a_black_start_is_not_between_two_normal_frames(self):
        self.assertEqual(f.flag_frames([BLACK, BLACK, PAGE, PAGE, PAGE]), [])

    def test_a_long_black_period_is_one_flat_run_with_its_length(self):
        """the cold start of build 4: 31 frames (1.03 s) of black between the system's starting window and the first picture were not
        flagged, because only a flat frame whose two neighbours were normal counted"""
        flagged = f.flag_frames([PAGE, PAGE] + [BLACK] * 31 + [PAGE, PAGE])
        flat = [x for x in flagged if x["kind"] == "flat"]
        self.assertEqual([(x["index"], x["frames"]) for x in flat], [(2, 31)])
        self.assertIn("1.03 s", flat[0]["detail"])

    def test_a_nearly_flat_fade_counts_as_part_of_the_run(self):
        """the frames of a fade into black are 99.5-99.8 % one colour: still one flat period, not several single frames"""
        almost = sig((0, 0, 0), (8, 8, 8))[:-q.COLS * 3] + bytes([40, 40, 40]) * q.COLS      # one bottom row of another colour
        flagged = f.flag_frames([PAGE, almost, BLACK, BLACK, almost, PAGE])
        self.assertEqual([(x["index"], x["frames"]) for x in flagged if x["kind"] == "flat"], [(1, 4)])

    def test_a_black_run_over_the_limit_fails_and_a_shorter_one_is_only_told(self):
        """the user's rule: no all-black frame between two normal frames longer than N ms (100 ms by default)"""
        long_black = f.flag_frames([PAGE, PAGE] + [BLACK] * 4 + [PAGE, PAGE])            # 4 frames at 30 fps = 133 ms
        self.assertEqual([(x["kind"], x["black"], x["fail"], x["ms"]) for x in long_black], [("flat", True, True, 133)])
        short_black = f.flag_frames([PAGE, PAGE] + [BLACK] * 3 + [PAGE, PAGE])           # 100 ms: at the limit
        self.assertEqual([(x["black"], x["fail"], x["ms"]) for x in short_black], [(True, False, 100)])
        self.assertTrue(f.flag_frames([PAGE, PAGE, BLACK, PAGE, PAGE], max_black_ms=20)[0]["fail"], "the limit is a parameter")

    def test_a_flat_run_of_another_colour_fails_whatever_its_length(self):
        # the SECOND flat run of a recording (the first is the cold start's starting window): a flash of another colour fails
        white = sig((255, 255, 255))
        flagged = f.flag_frames([PAGE, PAGE, BLACK, PAGE, PAGE, white, PAGE, PAGE], max_black_ms=1000)
        self.assertEqual([(x["kind"], x["black"], x["fail"]) for x in flagged], [("flat", True, False), ("flat", False, True)])

    def test_the_starting_window_of_a_cold_start_is_told_not_failed(self):
        # E46, the Android shell agent's rule: the FIRST flat run, when it is not black, is the splash held on purpose; a black run after it still fails
        white = sig((255, 255, 255))
        flagged = f.flag_frames([PAGE] + [white] * 13 + [BLACK] * 13 + [PAGE, PAGE])
        flats = [x for x in flagged if x["kind"] == "flat"]
        self.assertEqual([(x["frames"], x["black"], x["fail"], x["splash"]) for x in flats], [(13, False, False, True), (13, True, True, False)])
        self.assertIn("told, not failed", flats[0]["detail"])

    def test_a_first_flat_run_that_is_black_gets_no_splash_allowance(self):
        white = sig((255, 255, 255))
        flagged = f.flag_frames([PAGE, BLACK, BLACK, PAGE, white, white, PAGE, PAGE], max_black_ms=1000)
        flats = [x for x in flagged if x["kind"] == "flat"]
        self.assertEqual([(x["black"], x["fail"], x["splash"]) for x in flats], [(True, False, False), (False, True, False)])

    def test_a_flat_run_that_never_ends_is_not_between_two_normal_frames(self):
        self.assertEqual([x for x in f.flag_frames([PAGE, PAGE, BLACK, BLACK, BLACK]) if x["kind"] == "flat"], [])


class VideoFrameTests(unittest.TestCase):
    def test_a_raw_stream_is_cut_into_frames_and_signed(self):
        w, h = 20, 40
        frame_a, frame_b = bytes([10, 20, 30]) * (w * h), bytes([200, 100, 50]) * (w * h)
        frames = f.split_frames(frame_a + frame_b + frame_a[:50], w * h * 3)          # a cut-off last frame is dropped
        self.assertEqual(len(frames), 2)
        sig_a = f.signature_of(frames[0], w, h)
        self.assertEqual(len(sig_a), q.COLS * q.ROWS * 3)
        self.assertEqual(sig_a[:3], bytes([10, 20, 30]))
        self.assertGreater(q.sig_diff(sig_a, f.signature_of(frames[1], w, h)), 0.2)


class ScrollSampleTests(unittest.TestCase):
    class FakeServer:
        """a page scroller whose offset follows the clock: `rate` px per second from the first question"""
        def __init__(self, rate):
            self.rate, self.t0 = rate, None

        def op(self, op, timeout=3, **params):
            import time as _t
            self.t0 = self.t0 or _t.time()
            return {"data": {"value": {"scroll_states": [{"node_id": 7, "scroll_y": (_t.time() - self.t0) * self.rate, "max_scroll_y": 2000}, {"node_id": 9, "scroll_y": 5}]}}}

    def test_the_scroller_is_sampled_while_the_swipe_runs(self):
        import time as _t
        samples = f.scroll_samples_during(self.FakeServer(500), lambda: _t.sleep(0.3), 7, interval=0.02)
        self.assertGreaterEqual(len(samples), 5)
        self.assertTrue(all(b[1] >= a[1] for a, b in zip(samples, samples[1:])))
        ok, text = f.scroll_verdict(samples, +1)
        self.assertTrue(ok, text)

    def test_a_snap_back_or_no_move_fails(self):
        snap = [(0.0, 0.0), (0.1, 100.0), (0.2, 40.0), (0.3, 160.0)]
        ok, text = f.scroll_verdict(snap, +1)
        self.assertFalse(ok)
        self.assertIn("worst step back 60", text)
        still = [(0.0, 10.0), (0.1, 10.0), (0.2, 10.0)]
        self.assertFalse(f.scroll_verdict(still, +1)[0])
        self.assertFalse(f.scroll_verdict([(0.0, 1.0)], +1)[0])

    def test_the_way_back_decreases(self):
        back = [(0.0, 300.0), (0.1, 220.0), (0.2, 90.0)]
        self.assertTrue(f.scroll_verdict(back, -1)[0])
        self.assertFalse(f.scroll_verdict(back, +1)[0])


class ScrollCheckTests(unittest.TestCase):
    """scroll_check end to end on a fake device and a fake debug server: the page scrolls with the finger, then back"""

    class Dev:
        def __init__(self):
            self.calls = []
            self.offset = 0.0

        def adb(self, *a, **k):
            self.calls.append(("adb",) + a)
            return ""

        def sh(self, cmd, **k):
            self.calls.append(("sh", cmd))
            return ""

        def launch(self, cold=True):
            self.calls.append(("launch", cold))

        def wait_for_front(self, timeout=0):
            return 1.0

        def settle(self, s):
            pass

        def swipe(self, x1, y1, x2, y2, ms):
            import time as _t
            direction = 1 if y2 < y1 else -1                   # the finger moves up: the page scrolls down
            for _ in range(10):
                self.offset = max(0.0, self.offset + direction * 60.0)
                _t.sleep(0.03)

    def test_a_page_that_follows_the_finger_passes_and_the_calls_are_made(self):
        import mobile_e2e
        dev = self.Dev()

        class Server:
            def __init__(self, port):
                pass

            def ready(self, seconds):
                return 0.1, {}

            def op(self, op, timeout=3, **params):
                return {"data": {"value": {"scroll_states": [{"node_id": 4, "scroll_y": dev.offset, "max_scroll_y": 1500}]}}}

        real = mobile_e2e.Server
        mobile_e2e.Server = Server
        try:
            lines, entry = [], {}
            f.scroll_check(dev, (540, 1800, 540, 600, 700), lines, entry, port=8796)
        finally:
            mobile_e2e.Server = real
        self.assertTrue(entry["scroll"]["ok"], entry["scroll"])
        self.assertIn(("adb", "forward", "tcp:8796", "tcp:8796"), dev.calls)
        self.assertTrue(any(c[0] == "sh" and "AZ_DEBUG=8796" in c[1] for c in dev.calls))
        self.assertEqual(len(lines), 2)
        self.assertIn("PASS", lines[0])

    def test_a_server_that_does_not_answer_is_not_verified_not_failed(self):
        import mobile_e2e
        dev = self.Dev()

        class Server:
            def __init__(self, port):
                pass

            def ready(self, seconds):
                return None, "connection refused"

        real = mobile_e2e.Server
        mobile_e2e.Server = Server
        try:
            lines, entry = [], {}
            f.scroll_check(dev, (540, 1800, 540, 600, 700), lines, entry)
        finally:
            mobile_e2e.Server = real
        self.assertIsNone(entry["scroll"]["ok"])
        self.assertIn("NOT VERIFIED", lines[0])


class IosAuditTests(unittest.TestCase):
    """ios_audit on a fake simulator and no ffmpeg: the idle verdict is made from pictures of the APP (not of the springboard), the flash audit says why it
    did not run, the pan is not verified"""

    HOME = sig((60, 120, 200), (200, 200, 60))         # the springboard

    class FakeIOS:
        shows_app = True
        install_rc = 0
        installed_first = []          # the order of what the audit did to the simulator, shared by the instances of one test

        def __init__(self, run, udid, bundle, exe, app_dir=None):
            self.base_env, self.calls, self.launched, self.home_sig = {}, [], False, None

        def simctl(self, *a, **k):
            self.calls.append(a)
            return 0, "", ""

        def to_file(self, log_name, *cmd, timeout=600):
            self.installed_first.append(("to_file", cmd))
            return self.install_rc, "No such file: the app bundle" if self.install_rc else ""

        def terminate(self):
            self.launched = False

        def launch(self, extra_env=None, cold=True):
            self.installed_first.append(("launch", ()))
            self.launched = True
            return {}

        def wait_for_front(self, timeout=45):
            return 1.0 if self.shows_app else None

        def settle(self, s):
            pass

        def capture(self, name, keep=True):
            picture = PAGE if (self.launched and self.shows_app) else IosAuditTests.HOME
            return picture, 1206, 2622, ""

    def run_audit(self, shows_app=True, install_rc=0):
        import tempfile
        import time as _t
        out = tempfile.mkdtemp(prefix="flicker-ios-test-")
        real_ios, real_ffmpeg, real_sleep = f.q.IOS, f.ensure_ffmpeg, _t.sleep
        self.order = []
        f.q.IOS = type("IOS", (self.FakeIOS,), {"shows_app": shows_app, "install_rc": install_rc, "installed_first": self.order})
        f.ensure_ffmpeg = lambda: None
        _t.sleep = lambda s: None
        try:
            args = type("A", (), {"out": out, "udid": "U", "bundle": "com.x.App", "exe": "", "app": "App.app", "backend": "", "first_frame_timeout": 60.0})()
            self.assertEqual(f.ios_audit(args), 0 if install_rc == 0 else 1)
        finally:
            f.q.IOS, f.ensure_ffmpeg, _t.sleep = real_ios, real_ffmpeg, real_sleep
        return Path(out)

    def test_the_app_is_installed_on_the_simulator_before_it_is_launched(self):
        """both first runs recorded the home screen for 80 s: nothing installed the app"""
        self.run_audit()
        self.assertEqual(self.order[0][0], "to_file")
        self.assertIn("install", self.order[0][1])
        self.assertIn("App.app", self.order[0][1])
        self.assertLess([k for k, _ in self.order].index("to_file"), [k for k, _ in self.order].index("launch"))

    def test_an_app_that_does_not_install_fails_the_audit_and_is_never_launched(self):
        out = self.run_audit(install_rc=1)
        import json
        report = json.loads((out / "flicker.json").read_text())
        self.assertFalse(report["idle"]["ok"])
        self.assertIn("the app did not install on the Simulator", report["idle"]["detail"])
        self.assertIsNone(report["flash"]["ok"])
        self.assertNotIn("launch", [k for k, _ in self.order])

    def test_an_idle_app_passes_and_the_rest_says_what_was_not_run(self):
        out = self.run_audit()
        text = (out / "flicker.md").read_text()
        self.assertIn("PASS", text)
        self.assertIn("ffmpeg is not installed", text)
        self.assertIn("NOT VERIFIED on iOS", text)
        import json
        report = json.loads((out / "flicker.json").read_text())
        self.assertTrue(report["idle"]["ok"])
        self.assertIsNone(report["flash"]["ok"])

    def test_the_springboard_is_not_taken_for_the_app(self):
        """the first run of the iOS audit judged six pictures of the home screen idle: without the home signature `wait_for_front` returns at once"""
        out = self.run_audit(shows_app=False)
        import json
        report = json.loads((out / "flicker.json").read_text())
        self.assertFalse(report["idle"]["ok"])
        self.assertIn("showed no picture of its own", report["idle"]["detail"])


class CommandLineTests(unittest.TestCase):
    def parse(self, argv):
        real = f.android_audit
        f.android_audit = lambda args: args
        try:
            return f.main(argv)
        finally:
            f.android_audit = real

    def test_flash_only_is_an_option_of_the_android_audit(self):
        args = self.parse(["android", "--out", "o", "--apk-plain", "a.apk", "--flash-only"])
        self.assertTrue(args.flash_only)
        self.assertFalse(self.parse(["android", "--out", "o", "--apk-plain", "a.apk"]).flash_only)


class MonotonicTests(unittest.TestCase):
    def test_an_offset_that_advances_is_fine(self):
        self.assertEqual(f.monotonic([0, 30, 90, 200, 360, 420, 430]), (True, 0.0))

    def test_a_snap_back_is_found(self):
        ok, worst = f.monotonic([0, 40, 120, 60, 200])
        self.assertFalse(ok)
        self.assertEqual(worst, 60.0)

    def test_the_other_direction(self):
        self.assertTrue(f.monotonic([500, 400, 300, 120], direction=-1)[0])


class SplashThenBlackTests(unittest.TestCase):
    """the held white starting window and the black frames after it are two runs, each with its own length (E46: a 0.87 s run called black)"""

    def test_a_white_splash_and_black_frames_after_it_are_two_runs(self):
        normal = sig((90, 140, 200), (250, 250, 60))
        sigs = [normal] + [sig((255, 255, 255))] * 13 + [sig((0, 0, 0))] * 13 + [normal]
        self.assertEqual(f.flat_runs(sigs), [(1, 14), (14, 27)])
        flagged = f.flag_frames(sigs)
        flats = [x for x in flagged if x["kind"] == "flat"]
        self.assertEqual([(x["frames"], x["black"]) for x in flats], [(13, False), (13, True)])
        black = [x for x in flats if x["black"]][0]
        self.assertEqual(black["ms"], 433)
        self.assertTrue(black["fail"], "13 black frames at 30 fps are 433 ms: over the 100 ms limit")

    def test_one_black_frame_after_a_splash_is_told_not_failed(self):
        normal = sig((90, 140, 200), (250, 250, 60))
        sigs = [normal] + [sig((255, 255, 255))] * 3 + [sig((0, 0, 0))] + [normal]
        flats = [x for x in f.flag_frames(sigs) if x["kind"] == "flat"]
        black = [x for x in flats if x["black"]][0]
        self.assertEqual(black["frames"], 1)
        self.assertFalse(black["fail"])

    def test_a_run_of_one_colour_stays_one_run(self):
        normal = sig((90, 140, 200), (250, 250, 60))
        sigs = [normal] + [sig((0, 0, 0))] * 5 + [normal]
        self.assertEqual(f.flat_runs(sigs), [(1, 6)])


class LeadingRunTests(unittest.TestCase):
    """a recording that starts with a long flat run: the iOS GPU path was black from the first frame for 17.4 s and the audit said PASS"""

    def test_a_long_black_run_at_the_start_of_the_recording_is_flagged(self):
        normal = sig((90, 140, 200), (250, 250, 60))
        sigs = [sig((0, 0, 0))] * 30 + [normal, normal]
        flats = [x for x in f.flag_frames(sigs) if x["kind"] == "flat"]
        self.assertEqual([(x["index"], x["frames"], x["black"], x["fail"]) for x in flats], [(0, 30, True, True)])

    def test_a_few_flat_frames_at_the_start_are_not_a_flag(self):
        normal = sig((90, 140, 200), (250, 250, 60))
        self.assertEqual(f.flat_runs([sig((0, 0, 0))] * 3 + [normal, normal]), [])

    def test_a_leading_light_run_is_the_starting_window_and_told(self):
        normal = sig((90, 140, 200), (250, 250, 60))
        sigs = [sig((255, 255, 255))] * 20 + [normal, normal]
        flats = [x for x in f.flag_frames(sigs) if x["kind"] == "flat"]
        self.assertEqual([(x["splash"], x["fail"]) for x in flats], [(True, False)])


class ColourTests(unittest.TestCase):
    """what colour a flat run is, and when it starts: the raw numbers of E46 for whoever re-measures it"""

    def test_a_flat_run_carries_its_start_time_and_its_colour(self):
        normal = sig((90, 140, 200), (250, 250, 60))
        sigs = [normal] * 3 + [sig((255, 255, 255))] * 13 + [sig((0, 0, 0))] * 33 + [normal]
        flats = [x for x in f.flag_frames(sigs) if x["kind"] == "flat"]
        self.assertEqual([(x["frames"], x["start_s"], x["rgb"]) for x in flats], [(13, 0.1, [255, 255, 255]), (33, 0.53, [0, 0, 0])])

    def test_the_mean_colour_leaves_the_bars_out(self):
        self.assertEqual(f.mean_rgb(sig((10, 20, 30))), (10, 20, 30))


class IosAuditSourceTests(unittest.TestCase):
    """The iOS audit is driven by `simctl`, which this machine has not: what it must do first is read from its source."""

    def ios_audit_source(self):
        text = (Path(__file__).resolve().parent / "flicker_audit.py").read_text(encoding="utf-8")
        start = text.index("def ios_audit(")
        return text[start:text.index("\ndef ", start + 1)]

    def test_the_app_is_installed_before_the_first_launch(self):
        # the two first runs of the audit recorded the home screen for 80 s: nothing installed the app and `simctl launch` of an app that is
        # not there starts nothing
        src = self.ios_audit_source()
        install = src.index('"simctl", "install"')
        self.assertLess(install, src.index("dev.launch("), "the app must be installed before it is launched")

    def test_an_app_that_does_not_install_is_a_failure_that_says_so(self):
        src = self.ios_audit_source()
        self.assertIn("the app did not install on the Simulator", src)
        self.assertIn("return 1", src)


if __name__ == "__main__":
    unittest.main(verbosity=1)
