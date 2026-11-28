#!/usr/bin/env python3
"""Tests of scripts/ci/startup_lab.py: the pure analysis of the cold-start lab (python3 -I scripts/ci/test_startup_lab.py)."""
import sys
import unittest
import unittest.mock
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mobile_qa as q  # noqa: E402
import startup_lab as lab  # noqa: E402
import startup_strip_pair as pair  # noqa: E402


def sig(colour, bar=None):
    """A 40 x 80 signature of one colour; `bar` = (r, g, b) paints the middle third of the rows."""
    rows = []
    for y in range(q.ROWS):
        c = bar if bar and q.ROWS // 3 <= y < 2 * q.ROWS // 3 else colour
        rows.append(bytes(c) * q.COLS)
    return b"".join(rows)


LAUNCHER = sig((90, 120, 160), (200, 60, 60))
SPLASH = sig((255, 255, 255))
BLACK = sig((0, 0, 0))
FIRST = sig((238, 233, 220), (31, 90, 107))
SETTLED = sig((238, 233, 220), (200, 60, 120))


class LabelTests(unittest.TestCase):
    def test_a_cold_start_is_launcher_then_starting_window_then_black_then_the_pictures(self):
        sigs = [LAUNCHER] * 3 + [SPLASH] * 4 + [BLACK] * 5 + [FIRST] * 2 + [SETTLED] * 6
        labels = lab.label_frames(sigs)
        self.assertEqual("".join(labels), "LLL" + "SSSS" + "BBBBB" + "11" + "222222")
        self.assertEqual(lab.runs_text(labels), "L3 S4 B5 1x2 2x6")

    def test_the_summary_counts_the_black_after_the_starting_window_and_whether_the_page_moved(self):
        labels = list("LLSSSBBBB11222")
        s = lab.summarize_start(labels)
        self.assertEqual((s["splash"], s["black"], s["first"], s["settled"], s["jump"]), (3, 4, 2, 3, True))

    def test_a_start_with_no_black_and_one_picture_did_not_flicker(self):
        labels = lab.label_frames([LAUNCHER] * 2 + [SPLASH] * 6 + [SETTLED] * 8)
        s = lab.summarize_start(labels)
        self.assertEqual((s["black"], s["jump"], s["splash"]), (0, False, 6))

    def test_a_recording_with_no_flat_frame_is_all_before_the_app(self):
        self.assertEqual(set(lab.label_frames([LAUNCHER, FIRST, SETTLED])), {"L"})

    def test_black_before_the_starting_window_is_not_counted_after_it(self):
        self.assertEqual(lab.count_after_first(list("BBSSB"), "B"), 1)
        self.assertEqual(lab.count_after_first(list("LLL"), "B"), 0)

    def test_a_flat_first_frame_after_the_starting_window_is_the_same_letter_as_the_window(self):
        labels = lab.label_frames([LAUNCHER] + [SPLASH] * 3 + [sig((250, 250, 250))] * 3 + [SETTLED] * 3)
        self.assertEqual(lab.runs_text(labels), "L1 S6 2x3")


DARK_SPLASH = sig((0, 0, 0))
# a black window with a small icon on it: the dark starting window (a few percent of the samples, not one colour)
ICON = bytearray(sig((0, 0, 0)))
for row in range(36, 42):
    for col in range(18, 22):
        at = (row * q.COLS + col) * 3
        ICON[at:at + 3] = bytes((31, 90, 107))
DARK_SPLASH_WITH_ICON = bytes(ICON)
DARK_PAGE = sig((18, 20, 22), (36, 40, 44))
LIGHT_PAGE = sig((238, 233, 220), (255, 255, 255))


class MovedTests(unittest.TestCase):
    def test_a_first_picture_that_stood_before_the_page_moved_is_a_jump(self):
        sigs = [SPLASH] * 4 + [FIRST] * 6 + [SETTLED] * 6
        labels = lab.label_frames(sigs)
        s = lab.summarize_start(labels, sigs)
        self.assertEqual(s["moved"], 6)
        self.assertTrue(s["jump"])

    def test_the_frames_of_a_fade_are_no_picture_that_stood(self):
        fade = [sig((int(255 - 17 * k), int(255 - 12 * k), int(255 - 5 * k)), (31, 90, 107)) for k in range(6)]
        sigs = [SPLASH] * 4 + fade + [SETTLED] * 8
        labels = lab.label_frames(sigs)
        s = lab.summarize_start(labels, sigs)
        self.assertLess(s["moved"], lab.MOVED_FRAMES)
        self.assertFalse(s["jump"], "a cross-fade into the settled page is not a page that moved")


def mix(a, b, share):
    """`share` of signature `a` over `b` (what a step of a cross-fade looks like)."""
    return bytes(int(round(share * x + (1 - share) * y)) for x, y in zip(a, b))


class BlendTests(unittest.TestCase):
    def test_a_step_of_the_fade_the_recording_shows_for_many_frames_is_no_page_that_moved(self):
        # a slow compositor: ONE step of the system's cross-fade (the starting window half over the page) is in six recorded frames
        step = mix(SPLASH, SETTLED, 0.54)
        sigs = [SPLASH] * 4 + [step] * 6 + [SETTLED] * 8
        labels = lab.label_frames(sigs)
        self.assertEqual(labels.count("1"), 6, "the frames are pictures that are not the settled one")
        s = lab.summarize_start(labels, sigs)
        self.assertFalse(s["jump"], "a mix of the starting window and the page is the fade, however long it stood")
        self.assertEqual(s["moved"], 0)

    def test_a_picture_with_a_layout_of_its_own_that_stood_is_still_a_page_that_moved(self):
        sigs = [SPLASH] * 4 + [FIRST] * 6 + [SETTLED] * 8
        s = lab.summarize_start(lab.label_frames(sigs), sigs)
        self.assertTrue(s["jump"])
        self.assertGreater(lab.blend_residual(FIRST, SPLASH, SETTLED), lab.BLEND_RESIDUAL)

    def test_the_systems_opening_animation_before_the_starting_window_is_no_picture_that_stood(self):
        # the launcher turning into the starting window: pictures that are not the settled one, still, between two frames of the window
        sigs = [LAUNCHER] * 3 + [SPLASH] * 2 + [FIRST] * 4 + [SPLASH] * 10 + [SETTLED] * 8
        labels = lab.label_frames(sigs)
        s = lab.summarize_start(labels, sigs)
        self.assertEqual(s["moved"], 0)
        self.assertFalse(s["jump"])

    def test_the_residual_of_a_mix_is_nothing_and_of_the_two_ends_too(self):
        self.assertLess(lab.blend_residual(mix(SPLASH, SETTLED, 0.3), SPLASH, SETTLED), 0.005)
        self.assertLess(lab.blend_residual(SPLASH, SPLASH, SETTLED), 0.005)
        self.assertLess(lab.blend_residual(SETTLED, SPLASH, SETTLED), 0.005)

    def test_signatures_of_another_size_are_never_a_mix(self):
        self.assertEqual(lab.blend_residual(b"\x00\x01", SPLASH, SETTLED), 1.0)


MOSTLY_DARK_PAGE = bytearray(sig((12, 12, 14)))
for row in range(30, 33):
    for col in range(10, 30):
        at = (row * q.COLS + col) * 3
        MOSTLY_DARK_PAGE[at:at + 3] = bytes((90, 90, 96))     # a line of text: a few percent of the samples, as bright as an icon
MOSTLY_DARK_PAGE = bytes(MOSTLY_DARK_PAGE)


class SettledFirstTests(unittest.TestCase):
    def test_a_mostly_dark_page_the_recording_ends_with_is_the_settled_picture_not_the_starting_window(self):
        sigs = [LAUNCHER] * 2 + [DARK_SPLASH_WITH_ICON] * 5 + [DARK_SPLASH] * 3 + [MOSTLY_DARK_PAGE] * 6
        labels = lab.label_frames(sigs, night=True)
        self.assertEqual(lab.runs_text(labels), "L2 S5 B3 2x6")

    def test_a_recording_that_ends_on_the_starting_window_has_no_app_phase(self):
        self.assertEqual(set(lab.label_frames([LAUNCHER, SPLASH, SPLASH, SPLASH])), {"L"})


class NightTests(unittest.TestCase):
    def test_a_dark_window_with_an_icon_is_the_starting_window_and_one_without_is_the_gap(self):
        self.assertEqual(lab.dark_kind(DARK_SPLASH_WITH_ICON), "S")
        self.assertEqual(lab.dark_kind(DARK_SPLASH), "B")
        self.assertIsNone(lab.dark_kind(DARK_PAGE), "a page of cards is a picture")
        self.assertIsNone(lab.dark_kind(LIGHT_PAGE))

    def test_a_night_start_with_the_window_held_has_no_black(self):
        labels = lab.label_frames([LAUNCHER] * 2 + [DARK_SPLASH_WITH_ICON] * 8 + [DARK_PAGE] * 4, night=True)
        s = lab.summarize_start(labels)
        self.assertEqual((s["splash"], s["black"], s["wrong"]), (8, 0, 0))

    def test_a_night_start_that_shows_black_after_the_icon_and_then_a_light_picture_is_flagged_twice(self):
        labels = lab.label_frames([LAUNCHER] * 2 + [DARK_SPLASH_WITH_ICON] * 4 + [DARK_SPLASH] * 9 + [LIGHT_PAGE] * 3 + [DARK_PAGE] * 6, night=True)
        s = lab.summarize_start(labels)
        self.assertEqual((s["splash"], s["black"], s["wrong"]), (4, 9, 3))
        self.assertIn("W3", lab.runs_text(labels))


class LogTests(unittest.TestCase):
    LOG = "\n".join([
        "11-28 03:12:26.410   525  2222 I ActivityTaskManager: START u0 {flg=0x10000000 cmp=com.azul.startctl.before/com.azul.app.AzulActivity} with LAUNCH_SINGLE_TOP",
        "11-28 03:12:26.424   832   882 D SplashScreenView: Build android.window.SplashScreenView{477ef55}",
        "11-28 03:12:26.780  2691  2716 I azctl   : [azctl] InitWindow 1080x2400 format 4 gen 1 at 1234.5 ms",
        "11-28 03:12:26.830   525   545 I ActivityTaskManager: Displayed com.azul.startctl.before/com.azul.app.AzulActivity for user 0: +415ms",
        "11-28 03:12:26.850   525  2222 D CoreBackPreview: Window{fab7be5 u0 Splash Screen com.azul.startctl.before EXITING}: Setting back callback null",
        "11-28 03:12:27.257  2691  2716 I azctl   : [azctl] POSTED page1 at 1900.0 ms (frame 1080x2400)",
        "11-28 03:12:27.700  2691  2716 I azctl   : [azctl] POSTED page2 at 2300.0 ms (frame 1080x2400)",
    ])

    def test_the_events_are_told_relative_to_the_systems_start(self):
        ev = {label: ms for label, ms, _ in lab.log_events(self.LOG)}
        self.assertEqual(ev["START"], 0)
        self.assertEqual(ev["starting window"], 14)
        self.assertEqual(ev["surface"], 370)
        self.assertEqual(ev["Displayed"], 420)
        self.assertEqual(ev["splash exiting"], 440)
        self.assertEqual(ev["page1 posted"], 847)
        self.assertEqual(ev["page2 posted"], 1290)

    def test_the_events_come_in_the_order_they_happened(self):
        labels = [label for label, _ms, _l in lab.log_events(self.LOG)]
        self.assertEqual(labels, ["START", "starting window", "surface", "Displayed", "splash exiting", "page1 posted", "page2 posted"])

    def test_displayed_is_read_from_the_system_line(self):
        self.assertEqual(lab.displayed_ms(self.LOG), 415)
        self.assertIsNone(lab.displayed_ms("nothing"))

    def test_displayed_of_a_second_or_more_is_read_too(self):
        self.assertEqual(lab.displayed_ms("ActivityTaskManager: Displayed com.azul.AzWidgets/com.azul.app.AzulActivity for user 0: +1s259ms"), 1259)
        self.assertEqual(lab.displayed_ms("ActivityTaskManager: Displayed com.azul.floor/.FloorActivity for user 0: +2s136ms"), 2136)

    def test_a_line_that_is_no_threadtime_line_has_no_time(self):
        self.assertIsNone(lab.logcat_ms("--------- beginning of main"))


class PairTests(unittest.TestCase):
    def test_the_picture_of_a_cold_start_runs_from_just_before_the_app_to_just_after_the_settled_picture(self):
        labels = list("L" * 10 + "S" * 8 + "B" * 6 + "1" * 4 + "2" * 20)
        picks = pair.pick_frames(labels)
        self.assertEqual(picks[0], 8, "two frames before the first frame of the app phase")
        self.assertEqual(picks[-1], 34, "six frames after the first settled picture (frame 28), every second frame")
        self.assertTrue(all(b - a == 2 for a, b in zip(picks, picks[1:])))

    def test_a_recording_with_no_settled_picture_runs_to_its_end_and_a_long_one_is_cut_short(self):
        self.assertEqual(pair.pick_frames(list("LLSSBB"))[-1], 4)
        self.assertEqual(len(pair.pick_frames(list("L" + "S" * 500))), 84)

    def test_the_rows_are_fourteen_wide_and_the_second_recording_follows_the_first_in_the_frame_list(self):
        rows = pair.rows_of(list(range(0, 36, 2)), offset=100)
        self.assertEqual([len(r) for r in rows], [14, 4])
        self.assertEqual(rows[0][0], 100)


class RealRowsTests(unittest.TestCase):
    def test_every_real_app_is_started_in_the_light_mode_and_in_the_night_mode(self):
        rows = lab.real_rows(["AzWidgets", "AzWriter"])
        self.assertEqual([r["name"] for r in rows], ["AzWidgets-light", "AzWidgets-dark", "AzWriter-light", "AzWriter-dark"])
        self.assertEqual([r["night"] for r in rows], [False, True, False, True])
        self.assertTrue(all(r["startup"] and r["env"] == "AZ_LOG=info" for r in rows), "the engine's timeline and its info log are on")

    def test_home_and_back_is_recorded_once_per_app_in_the_light_mode(self):
        rows = lab.real_rows(["AzWidgets"])
        self.assertEqual([r["resume"] for r in rows], [True, False])


class FakeDevice:
    """The commands of a start, in order, with the pid answers `pidof` gives."""

    def __init__(self, pids, fail_on=None):
        self.calls, self.pids, self.fail_on = [], list(pids), fail_on

    def sh(self, command, timeout=90):
        self.calls.append(command)
        return (self.pids.pop(0) + "\n") if command.startswith("pidof") else ""

    def adb(self, *args, timeout=90, binary=False):
        self.calls.append("adb " + " ".join(args))
        return ""

    def record_start(self, remote="/sdcard/lab.mp4"):
        self.calls.append("record_start")
        return None, remote

    def record_stop(self, proc, remote, local):
        self.calls.append("record_stop")

    def wait_front(self, package, timeout=25.0):
        self.calls.append("wait_front")
        if self.fail_on == "wait_front":
            raise RuntimeError("the device went away")
        return True


class WarmStartTests(unittest.TestCase):
    def setUp(self):
        self.no_sleep = unittest.mock.patch.object(lab.time, "sleep", lambda s: None)
        self.no_frames = unittest.mock.patch.object(lab.fa, "cut_frames", lambda v: None)
        self.no_sleep.start()
        self.no_frames.start()
        self.addCleanup(self.no_sleep.stop)
        self.addCleanup(self.no_frames.stop)

    def run_warm(self, dev, tmp="/tmp/never-written"):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            return lab.warm_start(dev, "com.azul.AzWidgets", "com.azul.app.AzulActivity", {"settle": 1.0}, Path(d))

    def test_the_activity_is_destroyed_by_leaving_it_and_the_process_stays_and_the_next_start_is_recorded(self):
        dev = FakeDevice(["4242", "4242"])
        record = self.run_warm(dev)
        calls = dev.calls
        order = [calls.index(c) for c in ("settings put global always_finish_activities 1", "input keyevent KEYCODE_HOME", "pidof com.azul.AzWidgets",
                                          "record_start", "am start -n com.azul.AzWidgets/com.azul.app.AzulActivity", "record_stop")]
        self.assertEqual(order, sorted(order), "destroy-on-leave, home, the pid, the recording, the start, the end of the recording")
        self.assertEqual(calls[-1], "settings put global always_finish_activities 0", "the developer setting is put back")
        self.assertTrue(record["same_process"])

    def test_a_process_the_system_killed_is_not_a_warm_start(self):
        self.assertFalse(self.run_warm(FakeDevice(["4242", "5151"]))["same_process"], "another pid: a cold start")
        self.assertFalse(self.run_warm(FakeDevice(["", "5151"]))["same_process"], "no process before the start: a cold start")

    def test_the_developer_setting_is_put_back_when_the_device_goes_away(self):
        dev = FakeDevice(["4242", "4242"], fail_on="wait_front")
        with self.assertRaises(RuntimeError):
            self.run_warm(dev)
        self.assertEqual(dev.calls[-1], "settings put global always_finish_activities 0")

    def test_the_report_says_whether_it_was_the_same_process(self):
        text = lab.warm_text({"same_process": False, "runs": "L5 S3 2x9", "summary": {"black": 0, "splash": 3, "wrong": 0}})
        self.assertIn("NOT the same process", text)
        self.assertIn("L5 S3 2x9", text)

    def test_every_real_row_also_asks_for_a_warm_start(self):
        self.assertTrue(all(r["warm"] for r in lab.real_rows(["AzWidgets", "AzWriter"])))


class EngineEventsTests(unittest.TestCase):
    LOG = "\n".join([
        "11-28 03:12:26.410   525  2222 I ActivityTaskManager: START u0 {flg=0x10000000 cmp=com.azul.AzWidgets/com.azul.app.AzulActivity}",
        "11-28 03:12:26.800  2691  2716 I RustStdoutStderr: [startup]  24    67.251     0.625  ThreadId(7)            layout_begin #1 reason=Initial",
        "11-28 03:12:26.801  2691  2716 I RustStdoutStderr: [Android] the window's first insets are known at the surface: top=128 bottom=126 left=0 right=0",
        "11-28 03:12:27.257  2691  2716 I RustStdoutStderr: [startup]  28    529.225    51.772  ThreadId(7)            first_present android cpu full",
        "11-28 03:12:27.264  2691  2716 I RustStdoutStderr: [Android] insets top=128 bottom=126 left=0 right=0 ime=0 (physical px)",
        "11-28 03:12:27.265  2691  2716 I RustStdoutStderr: [startup] late: layout_begin at 532.119 ms (+0.006 ms, ThreadId(7)) #2 reason=Resize",
    ])

    def test_the_engines_own_marks_are_events_of_the_start(self):
        ev = {label: ms for label, ms, _ in lab.log_events(self.LOG)}
        self.assertEqual(ev["layout 1 begins"], 390)
        self.assertEqual(ev["insets known at the surface"], 391)
        self.assertEqual(ev["page1 posted"], 847)
        self.assertEqual(ev["insets applied"], 854)
        self.assertEqual(ev["layout 2 begins"], 855)

    def test_the_activitys_release_of_the_held_first_draw_is_an_event(self):
        log = "\n".join([
            "11-28 03:12:26.410   525  2222 I ActivityTaskManager: START u0 {flg=0x10000000 cmp=com.azul.AzWidgets/com.azul.app.AzulActivity}",
            "11-28 03:12:27.300  2691  2691 I azul    : AzulActivity: the window's first draw goes ahead after the engine's first picture, 612 ms after onCreate",
        ])
        ev = {label: ms for label, ms, _ in lab.log_events(log)}
        self.assertEqual(ev["first draw released"], 890)

    def test_a_second_layout_after_the_first_picture_is_the_jump(self):
        order = [label for label, _ms, _l in lab.log_events(self.LOG)]
        self.assertLess(order.index("page1 posted"), order.index("layout 2 begins"))


class VerdictTests(unittest.TestCase):
    def test_the_row_verdict_takes_medians_and_the_worst_black(self):
        starts = [{"summary": {"splash": 14, "black": b, "first": 3, "settled": 20, "jump": True}} for b in (10, 12, 30)]
        v = lab.row_verdict(starts)
        self.assertEqual((v["starts"], v["black_median"], v["black_max"], v["splash_median"]), (3, 12, 30, 14))
        self.assertTrue(v["jump"])

    def test_a_row_with_no_recording_has_no_numbers(self):
        v = lab.row_verdict([{}])
        self.assertEqual((v["starts"], v["black_median"], v["black_max"]), (0, None, None))

    def test_the_report_has_a_table_row_per_lab_row(self):
        results = [{"name": "before-450", "apk": "before", "props": {"p1": 450}, "night": False,
                    "starts": [{"runs": "L3 S4 B5 2x6", "displayed": 415, "events": [("START", 0, "x")]}],
                    "verdict": {"starts": 1, "splash_median": 4, "black_median": 5, "black_max": 5, "first_median": 0, "jump": False}}]
        text = lab.report_text(results)
        self.assertIn("| before-450 | before | p1=450 | 1 | 4 | 5 / 5 | 0 | no |", text)
        self.assertIn("L3 S4 B5 2x6", text)


if __name__ == "__main__":
    unittest.main()
