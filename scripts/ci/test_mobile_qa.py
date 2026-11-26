#!/usr/bin/env python3
"""Tests of the pure parts of scripts/ci/mobile_qa.py (no device needed): the picture signatures, the BMP and PNG
code, and the readers of what dumpsys / uiautomator / logcat print.   python3 -I scripts/ci/test_mobile_qa.py"""
import json
import struct
import sys
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mobile_qa as q  # noqa: E402


def solid(w, h, rgb):
    return bytes(rgb + (255,)) * (w * h)


def halves(w, h, top, bottom):
    return bytes(top + (255,)) * (w * (h // 2)) + bytes(bottom + (255,)) * (w * (h - h // 2))


def android_raw(w, h, pixels, header=16):
    head = struct.pack("<III", w, h, 1) + (b"\0\0\0\0" if header == 16 else b"")
    return head + pixels


def bmp32(w, h, rgba_rows_top_down):
    """A bottom-up 32-bit BMP of the given pixels (RGBA, row 0 on top)."""
    rows = []
    for y in range(h):
        row = rgba_rows_top_down[y * w * 4:(y + 1) * w * 4]
        bgra = bytearray(len(row))
        bgra[0::4], bgra[1::4], bgra[2::4], bgra[3::4] = row[2::4], row[1::4], row[0::4], row[3::4]
        rows.append(bytes(bgra))
    body = b"".join(reversed(rows))
    header = b"BM" + struct.pack("<IHHI", 54 + len(body), 0, 0, 54)
    dib = struct.pack("<IiiHHIIiiII", 40, w, h, 1, 32, 0, len(body), 2835, 2835, 0, 0)
    return header + dib + body


class SignatureTests(unittest.TestCase):
    def test_a_flat_screen_is_flat_and_a_split_one_is_not(self):
        w, h = 100, 200
        sig, *_ = q.signature_android_raw(android_raw(w, h, solid(w, h, (10, 20, 30))))
        self.assertTrue(q.is_flat(sig))
        sig2, *_ = q.signature_android_raw(android_raw(w, h, halves(w, h, (255, 255, 255), (0, 0, 0))))
        self.assertFalse(q.is_flat(sig2))
        self.assertGreater(q.sig_diff(sig, sig2), 0.2)

    def test_the_same_picture_differs_by_nothing(self):
        w, h = 64, 128
        px = halves(w, h, (200, 10, 10), (10, 200, 10))
        a, *_ = q.signature_android_raw(android_raw(w, h, px))
        b, *_ = q.signature_android_raw(android_raw(w, h, px, header=12))
        self.assertEqual(q.sig_diff(a, b), 0.0)

    def test_a_small_change_is_below_the_threshold_and_a_theme_switch_is_above(self):
        w, h = 120, 240
        light = halves(w, h, (250, 250, 250), (240, 240, 240))
        dark = halves(w, h, (20, 20, 20), (30, 30, 30))
        a, *_ = q.signature_android_raw(android_raw(w, h, light))
        b, *_ = q.signature_android_raw(android_raw(w, h, dark))
        self.assertGreater(q.sig_diff(a, b), q.SAME_BELOW)
        near = bytearray(light)
        near[0:4] = b"\x00\x00\x00\xff"          # one pixel of 28800
        c, *_ = q.signature_android_raw(android_raw(w, h, bytes(near)))
        self.assertLess(q.sig_diff(a, c), q.SAME_BELOW)

    def test_a_bad_screencap_layout_is_refused(self):
        with self.assertRaises(ValueError):
            q.signature_android_raw(b"\0" * 40)
        with self.assertRaises(ValueError):
            q.signature_android_raw(struct.pack("<III", 10, 10, 1) + b"\0" * 7)

    def test_bmp_and_raw_give_the_same_signature(self):
        w, h = 40, 80
        px = bytearray(halves(w, h, (255, 0, 0), (0, 0, 255)))
        for x in range(w):                        # a diagonal so row order matters
            px[(x % h * w + x) * 4:(x % h * w + x) * 4 + 3] = b"\x00\xff\x00"
        raw_sig, *_ = q.signature_android_raw(android_raw(w, h, bytes(px)))
        bmp_sig, bw, bh, rgba = q.parse_bmp(bmp32(w, h, bytes(px)))
        self.assertEqual((bw, bh), (w, h))
        self.assertEqual(raw_sig, bmp_sig)
        self.assertEqual(rgba, bytes(px))

    def test_the_png_writer_makes_a_png_that_inflates_to_the_pixels(self):
        w, h = 5, 3
        px = bytes(range(w * h * 4))
        import tempfile
        path = Path(tempfile.mkdtemp(prefix="azqa-test-")) / "out.png"     # a temp folder of its own: nothing is deleted
        q.write_png(path, w, h, px)
        data = path.read_bytes()
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
        width, height = struct.unpack(">II", data[16:24])
        self.assertEqual((width, height), (w, h))
        idat = data[data.index(b"IDAT") + 4:data.index(b"IEND") - 8]
        raw = zlib.decompress(idat)
        self.assertEqual(len(raw), h * (1 + w * 4))
        self.assertEqual(raw[1:1 + w * 4], px[:w * 4])


class ReaderTests(unittest.TestCase):
    def test_focus(self):
        dump = ("  mCurrentFocus=Window{4c3a1f2 u0 com.azul.AzWidgets/com.azul.app.AzulActivity}\n"
                "  mFocusedApp=ActivityRecord{1 u0 com.azul.AzWidgets/com.azul.app.AzulActivity t8}\n")
        self.assertEqual(q.parse_focus(dump), "com.azul.AzWidgets/com.azul.app.AzulActivity")
        self.assertEqual(q.parse_focus("  mCurrentFocus=null\n"), "")
        self.assertIn("Launcher", q.parse_focus("mCurrentFocus=Window{ab12 u0 com.android.launcher3/com.android.launcher3.uioverrides.QuickstepLauncher}"))

    def test_ime(self):
        self.assertTrue(q.parse_ime_shown("  mInputShown=true\n"))
        self.assertTrue(q.parse_ime_shown("    isInputViewShown=true\n"))
        self.assertFalse(q.parse_ime_shown("  mInputShown=false\n    isInputViewShown=false\n"))

    def test_uiautomator_bounds(self):
        xml = ('<hierarchy><node index="0" text="" bounds="[0,0][1080,2400]" />'
               '<node index="1" text="Dark" content-desc="" bounds="[500,200][700,300]" />'
               '<node index="2" text="" content-desc="Option A" bounds="[100,1800][300,1900]" /></hierarchy>')
        self.assertEqual(q.parse_bounds(xml), [("Dark", 600, 250), ("Option A", 200, 1850)])

    def test_fatal_lines_belong_to_the_app(self):
        log = ("11-26 06:35:55.1  2424  2456 F libc    : Fatal signal 11 (SIGSEGV), code 1 in tid 2456 (azul), pid 2424 (.azul.AzWidgets)\n"
               "11-26 06:35:56.1  900  900 E AndroidRuntime: FATAL EXCEPTION: main (com.other.app)\n"
               "11-26 06:35:57.1  2424  2456 I RustStdoutStderr: thread 'main' panicked at dll/src/x.rs:1\n"
               "11-26 06:35:58.1  900  900 I Something: ordinary line\n")
        got = q.fatal_lines(log, "com.azul.AzWidgets")
        self.assertEqual(len(got), 2)
        self.assertTrue(any("SIGSEGV" in g for g in got))
        self.assertTrue(any("panicked at" in g for g in got))


class RunRecordTests(unittest.TestCase):
    def test_an_exception_in_a_step_is_that_steps_failure_and_the_run_goes_on(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            run = q.Run(d, "android", {"label": "t"})

            def boom(_s):
                raise RuntimeError("no device")

            def fine(s):
                s.check("alive", True)

            run.step("a", "explodes", boom)
            run.step("b", "passes", fine)
            self.assertFalse(run.steps[0].ok)
            self.assertTrue(run.steps[1].ok)
            self.assertIn("no device", run.steps[0].error)
            self.assertTrue((Path(d) / "qa.json").exists())
            self.assertIn("| a | FAIL |", (Path(d) / "qa.md").read_text())

    def test_an_informational_check_does_not_fail_the_step(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            run = q.Run(d, "android", {})
            run.step("a", "x", lambda s: (s.check("must", True), s.check("nice", False, required=False)))
            self.assertTrue(run.steps[0].ok)


def bmp_v5(w, h, rgba_rows_top_down, top_down=True):
    """The BMP `sips -s format bmp` writes: 32 bit, BITMAPV5HEADER, BI_BITFIELDS with B,G,R,A byte order, top-down."""
    body = bytearray()
    rows = [rgba_rows_top_down[y * w * 4:(y + 1) * w * 4] for y in range(h)]
    if not top_down:
        rows.reverse()
    for row in rows:
        bgra = bytearray(len(row))
        bgra[0::4], bgra[1::4], bgra[2::4], bgra[3::4] = row[2::4], row[1::4], row[0::4], row[3::4]
        body += bgra
    dib = struct.pack("<IiiHHIIiiII", 124, w, -h if top_down else h, 1, 32, 3, len(body), 2835, 2835, 0, 0)
    dib += struct.pack("<IIII", 0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000) + b"\0" * (124 - 40 - 16)
    return b"BM" + struct.pack("<IHHI", 14 + len(dib) + len(body), 0, 0, 14 + len(dib)) + dib + bytes(body)


def bmp24(w, h, rgb_rows_top_down):
    """A 24-bit bottom-up BMP (rows padded to 4 bytes)."""
    stride = (w * 3 + 3) // 4 * 4
    rows = []
    for y in range(h):
        row = bytearray()
        for x in range(w):
            r, g, b = rgb_rows_top_down[(y * w + x) * 3:(y * w + x) * 3 + 3]
            row += bytes((b, g, r))
        rows.append(bytes(row) + b"\0" * (stride - w * 3))
    body = b"".join(reversed(rows))
    dib = struct.pack("<IiiHHIIiiII", 40, w, h, 1, 24, 0, len(body), 2835, 2835, 0, 0)
    return b"BM" + struct.pack("<IHHI", 14 + len(dib) + len(body), 0, 0, 14 + len(dib)) + dib + body


class BmpVariantTests(unittest.TestCase):
    def test_the_sips_bmp_gives_the_same_signature_as_the_raw_frame(self):
        w, h = 40, 80
        px = bytearray(halves(w, h, (250, 10, 10), (10, 10, 250)))
        px[(5 * w + 7) * 4:(5 * w + 7) * 4 + 3] = b"\x00\xff\x00"
        raw_sig, *_ = q.signature_android_raw(android_raw(w, h, bytes(px)))
        for top_down in (True, False):
            sig, bw, bh, rgba = q.parse_bmp(bmp_v5(w, h, bytes(px), top_down))
            self.assertEqual((bw, bh), (w, h))
            self.assertEqual(sig, raw_sig, f"top_down={top_down}")
            self.assertEqual(rgba, bytes(px))

    def test_a_24_bit_bmp_with_padded_rows(self):
        w, h = 5, 4           # 15 bytes a row: padded to 16
        rgb = bytes((x * 40 + y * 3) % 256 for y in range(h) for x in range(w) for _c in range(3))
        sig, bw, bh, rgba = q.parse_bmp(bmp24(w, h, rgb))
        self.assertEqual((bw, bh), (w, h))
        self.assertEqual(rgba[0:3], rgb[0:3])
        self.assertEqual(rgba[(h - 1) * w * 4:(h - 1) * w * 4 + 3], rgb[(h - 1) * w * 3:(h - 1) * w * 3 + 3])
        self.assertEqual(len(sig), q.COLS * q.ROWS * 3)

    def test_png_size_reads_the_header(self):
        import tempfile
        path = Path(tempfile.mkdtemp(prefix="azqa-test-")) / "size.png"     # a temp folder of its own: nothing is deleted
        q.write_png(path, 7, 9, bytes(7 * 9 * 4))
        self.assertEqual(q.png_size(path), (7, 9))


class ModeComparisonTests(unittest.TestCase):
    def test_identical_pictures_do_not_differ_and_the_image_is_dimmed(self):
        w, h = 20, 30
        a = halves(w, h, (200, 100, 50), (10, 20, 30))
        d = q.diff_pixels(a, a, w, h)
        self.assertEqual(d["differing"], 0)
        self.assertIsNone(d["bbox"])
        self.assertEqual(d["image"][0], 200 // 3)

    def test_a_block_of_differing_pixels_is_found_with_its_box_and_painted_red(self):
        w, h = 20, 30
        a = bytearray(solid(w, h, (100, 100, 100)))
        b = bytearray(a)
        for y in range(10, 14):
            for x in range(5, 9):
                b[(y * w + x) * 4:(y * w + x) * 4 + 3] = bytes((100 + 60, 100, 100))
        d = q.diff_pixels(bytes(a), bytes(b), w, h)
        self.assertEqual(d["differing"], 16)
        self.assertEqual(d["bbox"], (5, 10, 8, 13))
        self.assertEqual(d["max_abs"], 60)
        i = (10 * w + 5) * 4
        self.assertEqual(tuple(d["image"][i:i + 3]), (255, 0, 0))
        self.assertAlmostEqual(d["share"], 16 / 600.0)

    def test_anti_aliasing_noise_under_the_tolerance_is_not_a_difference(self):
        w, h = 10, 10
        a = solid(w, h, (100, 100, 100))
        b = solid(w, h, (100 + q.PIXEL_TOLERANCE, 100, 100))
        self.assertEqual(q.diff_pixels(a, b, w, h)["differing"], 0)
        c = solid(w, h, (100 + q.PIXEL_TOLERANCE + 1, 100, 100))
        self.assertEqual(q.diff_pixels(a, c, w, h)["differing"], 100)

    def test_the_backend_line(self):
        log = ("11-26 06:57:07.1  2528  2528 I RustStdoutStderr: [render] backend=cpu requested=gpu device=software\n"
               "other line\n")
        self.assertEqual(q.backend_ran(log), ("cpu", "gpu", "software"))
        self.assertEqual(q.backend_ran("[render] backend=gpu requested=auto device=Adreno (TM) 640"), ("gpu", "auto", "Adreno (TM) 640"))
        self.assertEqual(q.backend_ran("nothing here"), ("unknown", None, None))
        # the last line wins (a window recreated with another renderer)
        two = "[render] backend=gpu requested=gpu\n[render] backend=cpu requested=gpu\n"
        self.assertEqual(q.backend_ran(two)[0], "cpu")

    def test_frame_time_statistics(self):
        self.assertEqual(q.percentile([1, 2, 3, 4, 5], 50), 3)
        self.assertEqual(q.percentile([], 50), None)
        ns = [1_000_000_000 + i * 16_666_667 for i in range(10)] + [0, 9223372036854775807]
        iv = q.frame_intervals_ms(ns)
        self.assertEqual(len(iv), 9)
        self.assertAlmostEqual(q.percentile(iv, 50), 16.666667, places=3)
        text = "16666666\n1000000000 1010000000 1012000000\n1016000000 1026000000 1028000000\n0 9223372036854775807 0\n"
        refresh, times = q.parse_sf_latency(text)
        self.assertAlmostEqual(refresh, 16.666666)
        self.assertEqual(times, [1010000000, 1026000000, 9223372036854775807])
        self.assertEqual(q.frame_intervals_ms(times), [16.0])

    def test_displayed_time(self):
        log = "11-26 06:35:55.238 521 543 I ActivityTaskManager: Displayed com.azul.AzWidgets/com.azul.app.AzulActivity for user 0: +2s383ms"
        # the real line has `for user 0:` between: the reader takes the +NsNms after the last colon-plus
        self.assertEqual(q.displayed_ms(log, "com.azul.AzWidgets"), 2383)
        self.assertEqual(q.displayed_ms("Displayed com.azul.AzWidgets/.A: +850ms", "com.azul.AzWidgets"), 850)
        self.assertIsNone(q.displayed_ms("nothing", "com.azul.AzWidgets"))

    def test_a_gpu_request_that_ran_the_cpu_is_a_finding_that_fails_once_a_gpu_path_is_expected(self):
        import tempfile
        w, h = 12, 12
        px = solid(w, h, (90, 90, 90))
        res = {m: {"ran": "cpu", "requested": None, "device": None, "rgba": px, "dims": (w, h), "first_frame_s": 1.0, "displayed_ms": 1200,
                   "frames": {"median": 16.7, "p95": 33.3, "n": 40}} for m in ("cpu", "gpu", "auto")}
        for expect, ok in ((False, True), (True, False)):
            with tempfile.TemporaryDirectory() as d:
                run = q.Run(d, "android", {})
                run.step("modes", "x", lambda s: q.report_modes(run, s, res, expect, "android"))
                self.assertEqual(run.steps[0].ok, ok, f"expect_gpu={expect}")
                finding = [c for c in run.steps[0].checks if "GL renderer ran" in c["what"]]
                self.assertTrue(finding and not finding[0]["ok"])      # the finding is there in both cases
                self.assertIn("render_modes", run.meta)

    def test_auto_on_a_software_gl_runs_the_cpu_renderer_by_design(self):
        """The rule agreed for the GPU work: gpu FORCES GL (software GL included), auto takes GL only on a hardware GPU."""
        import tempfile
        w, h = 12, 12
        px = solid(w, h, (90, 90, 90))

        def row(ran, device):
            return {"ran": ran, "requested": None, "device": device, "rgba": px, "dims": (w, h), "first_frame_s": 1.0,
                    "displayed_ms": 1200, "frames": None}

        def verdict(res, hardware=False, expect_gpu=False):
            with tempfile.TemporaryDirectory() as d:
                run = q.Run(d, "android", {})
                run.step("modes", "x", lambda s: q.report_modes(run, s, res, expect_gpu, "android", hardware=hardware))
                return run.steps[0]

        # CI emulator, software GL: cpu -> cpu, gpu -> gpu (forced), auto -> cpu (by design): all pass
        ok = verdict({"cpu": row("cpu", "cpu raster"), "gpu": row("gpu", "Android Emulator OpenGL ES (SwiftShader)"),
                      "auto": row("cpu", "cpu raster")}, expect_gpu=True)
        self.assertTrue(ok.ok, [c for c in ok.checks if not c["ok"]])
        # auto that took the GL path on a software renderer is a failure, whatever --expect-gpu says
        bad = verdict({"cpu": row("cpu", None), "gpu": row("gpu", "SwiftShader"), "auto": row("gpu", "SwiftShader")})
        self.assertFalse(bad.ok)
        self.assertTrue(any("by design" in c["what"] and not c["ok"] for c in bad.checks))
        # a real phone (--hardware): auto must run GL when GPU is expected
        phone = verdict({"cpu": row("cpu", None), "gpu": row("gpu", "Adreno (TM) 640"), "auto": row("cpu", "cpu raster")}, hardware=True, expect_gpu=True)
        self.assertFalse(phone.ok)
        self.assertTrue(any("hardware GPU" in c["what"] for c in phone.checks))
        self.assertTrue(q.device_is_software("ANGLE (Apple, Apple Software Renderer)") and q.device_is_software("llvmpipe (LLVM 15)"))
        self.assertFalse(q.device_is_software("Mali-G78") or q.device_is_software(None))

    def test_a_gpu_request_waits_long_for_its_first_frame(self):
        # the first GL frame on a software renderer compiles every shader: two minutes are normal there, not a missing picture
        self.assertEqual(q.first_frame_timeout_for("gpu", 45), 120)
        self.assertEqual(q.first_frame_timeout_for("gpu", 300), 300)       # a longer wish stays
        self.assertEqual(q.first_frame_timeout_for("cpu", 45), 45)
        self.assertEqual(q.first_frame_timeout_for("auto", 45), 45)


LOGCAT_START = """\
11-26 07:41:11.070   517   886 I ActivityTaskManager: START u0 {flg=0x10000000 cmp=com.azul.AzWidgets/com.azul.app.AzulActivity} with LAUNCH_SINGLE_TOP from uid 2000 (BAL_ALLOW_PERMISSION) result code=0
11-26 07:41:11.083   517   546 I ActivityManager: Start proc 2878:com.azul.AzWidgets/u0a141 for next-top-activity {com.azul.AzWidgets/com.azul.app.AzulActivity}
11-26 07:41:11.426  2878  2904 I RustStdoutStderr: [           0us][Info][EventLoop] [Android] android_main entered
11-26 07:41:11.933   517   534 I ActivityTaskManager: Displayed com.azul.AzWidgets/com.azul.app.AzulActivity for user 0: +872ms
11-26 07:41:12.224  2878  2904 I RustStdoutStderr: [      798868us][Info][Rendering] [Android] present full - 2400 of 2400 rows
11-26 07:41:12.342  2878  2904 I RustStdoutStderr: [      916085us][Info][Rendering] [Android] present full - 2400 of 2400 rows
11-26 07:41:20.043   517   576 I ActivityTaskManager: START u0 {flg=0x10000000 cmp=com.azul.AzWidgets/com.azul.app.AzulActivity} with LAUNCH_SINGLE_TOP from uid 2000 (BAL_ALLOW_PERMISSION) result code=0
11-26 07:41:20.055   517   546 I ActivityManager: Start proc 2984:com.azul.AzWidgets/u0a141 for next-top-activity {com.azul.AzWidgets/com.azul.app.AzulActivity}
11-26 07:41:20.378  2984  3010 I RustStdoutStderr: [           0us][Info][EventLoop] [Android] android_main entered
11-26 07:41:20.930   517   534 I ActivityTaskManager: Displayed com.azul.AzWidgets/com.azul.app.AzulActivity for user 0: +1s897ms
11-26 07:41:21.204  2984  3010 I RustStdoutStderr: [      826074us][Info][Rendering] [Android] present full - 2400 of 2400 rows
11-26 07:41:25.000   517   576 I ActivityTaskManager: START u0 {flg=0x10000000 cmp=com.azul.floor/.FloorActivity} with LAUNCH_SINGLE_TOP
11-26 07:41:25.030   517   546 I ActivityManager: Start proc 3999:com.azul.floor/u0a150 for next-top-activity {com.azul.floor/.FloorActivity}
11-26 07:41:25.400   517   534 I ActivityTaskManager: Displayed com.azul.floor/.FloorActivity for user 0: +402ms
"""


class StartTimelineTests(unittest.TestCase):
    def test_a_start_is_followed_from_the_system_to_the_first_picture(self):
        recs = q.start_timelines(LOGCAT_START, "com.azul.AzWidgets")
        self.assertEqual(len(recs), 2)                       # the floor app's start is another package
        self.assertEqual(recs[0], {"proc": 13, "main": 356, "displayed": 872, "present": 1154})
        # the second start: 1 s 897 ms is one second and 897 ms; its engine lines belong to its own process
        self.assertEqual(recs[1], {"proc": 12, "main": 335, "displayed": 1897, "present": 1161})

    def test_the_floor_app_has_no_engine_lines(self):
        recs = q.start_timelines(LOGCAT_START, "com.azul.floor")
        self.assertEqual(recs, [{"proc": 30, "displayed": 402}])

    def test_the_summary_is_a_median_per_mark_and_the_engine_share(self):
        out = q.summarize_timelines([{"proc": 10, "main": 300, "displayed": 800, "present": 1000},
                                     {"proc": 12, "main": 340, "displayed": 900, "present": 1100},
                                     {"proc": 14, "main": 320, "present": 1040}])
        self.assertEqual(out["main"], {"median": 320, "min": 300, "max": 340, "n": 3})
        self.assertEqual(out["displayed"]["n"], 2)
        self.assertEqual(out["main_to_present"]["median"], 720)      # 700, 760, 720
        self.assertEqual(q.summarize_timelines([]), {})


class ImeAndTableTests(unittest.TestCase):
    def test_the_ime_inset_says_whether_the_keyboard_is_up(self):
        shown = "      InsetsSource type=ime frame=[0,1364][1080,2400] visible=true flags= \n"
        hidden = "      InsetsSource type=ime frame=[0,2400][1080,2400] visible=false flags= \n"
        older = "    InsetsSource: mType=ITYPE_IME mFrame=[0,1400][1080,2400] mVisible=true\n"
        self.assertTrue(q.parse_ime_inset(shown))
        self.assertFalse(q.parse_ime_inset(hidden))
        self.assertTrue(q.parse_ime_inset(older))
        self.assertFalse(q.parse_ime_inset(hidden + "  InsetsSource type=statusBars frame=[0,0][1080,128] visible=true\n"))
        self.assertIsNone(q.parse_ime_inset("nothing about the keyboard here\n"))

    def test_the_standing_render_section_is_in_qa_md_and_qa_json(self):
        import tempfile
        w, h = 12, 12
        px = solid(w, h, (90, 90, 90))
        res = {m: {"ran": "cpu", "requested": None, "device": "software", "rgba": px, "dims": (w, h), "first_frame_s": 0.5,
                   "displayed_ms": 944, "frames": {"median": 25.2, "p95": 47.2, "n": 117}} for m in ("cpu", "gpu", "auto")}
        with tempfile.TemporaryDirectory() as d:
            run = q.Run(d, "android", {"label": "t"})
            run.step("modes", "x", lambda s: q.report_modes(run, s, res, False, "android"))
            md = (Path(d) / "qa.md").read_text()
            self.assertIn("## Render backends: requested against ran", md)
            self.assertIn("| gpu | cpu | software | 944 | 0.5 | 25.2 | 47.2 | 117 |", md)
            self.assertIn("cpu against gpu, pixel by pixel", md)
            doc = json.loads((Path(d) / "qa.json").read_text())
            self.assertEqual(len(doc["render"]["table"]), 3)
            self.assertTrue(doc["render"]["cpu_against_gpu"]["ok"])


class LayoutOverflowTests(unittest.TestCase):
    def test_nodes_crossing_the_right_edge_are_found_widest_first_without_their_children(self):
        import mobile_e2e as e
        nodes = [
            {"node_id": 0, "tag": "body", "id": None, "classes": [], "rect": {"x": 0.0, "y": 0.0, "width": 411.4, "height": 817.5}},
            {"node_id": 1, "tag": "div", "id": None, "classes": [], "rect": {"x": 42.0, "y": 1970.0, "width": 480.0, "height": 270.0}},
            {"node_id": 2, "tag": "button", "id": "video-overlay", "classes": ["b"], "rect": {"x": 42.0, "y": 2000.0, "width": 480.0, "height": 40.0}},
            {"node_id": 3, "tag": "p", "id": None, "classes": [], "rect": {"x": 85.0, "y": 2094.0, "width": 394.0, "height": 42.0}},
            {"node_id": 4, "tag": "span", "id": None, "classes": [], "rect": None},
            {"node_id": 5, "tag": "div", "id": None, "classes": [], "rect": {"x": 500.0, "y": 10.0, "width": 50.0, "height": 10.0}},   # entirely off to the right: not 'crossing'
        ]
        answer = {"status": "ok", "data": {"type": "all_nodes_layout", "value": {"dom_id": 0, "node_count": 6, "nodes": nodes}}}
        count, top = e.layout_overflow(answer, 411.4)
        self.assertEqual(count, 3)                       # the video box, the button inside it, the paragraph
        self.assertEqual(top[0][:2], ("div", None))      # the widest first
        self.assertTrue(all(t[1] != "video-overlay" for t in top))   # the button starts at the same x inside the box: not listed again
        self.assertEqual(e.layout_overflow({"data": {}}, 411.4), (0, []))


class ExitReasonTests(unittest.TestCase):
    EXIT = """ACTIVITY MANAGER PROCESS EXIT INFO (dumpsys activity exit-info)
  package: com.azul.AzWidgets
    Historical Process Exit for uid=10141
        ApplicationExitInfo #0:
          timestamp=2026-11-26 07:02:04.972 pid=3889 realUid=10141 packageUid=10141 definingUid=10141 user=0
          process=com.azul.AzWidgets reason=4 (APP CRASH(EXCEPTION)) subreason=0 (UNKNOWN) status=0
          importance=100 pss=0.00 rss=0.00 description=crash state=empty trace=null
        ApplicationExitInfo #1:
          timestamp=2026-11-26 07:01:58.140 pid=3791 realUid=10141 packageUid=10141 definingUid=10141 user=0
          process=com.azul.AzWidgets reason=10 (USER REQUESTED) subreason=24 (KILL BACKGROUND) status=0
        ApplicationExitInfo #2:
          timestamp=2026-11-26 07:01:50.767 pid=3095 realUid=10141 packageUid=10141 definingUid=10141 user=0
          process=com.azul.AzWidgets reason=2 (SIGNALED) subreason=0 (UNKNOWN) status=9
"""

    def test_the_exit_reasons_are_read_and_only_crashes_count(self):
        got = q.parse_exit_reasons(self.EXIT)
        self.assertEqual([g[2] for g in got], [4, 10, 2])
        self.assertEqual(got[0][3], "APP CRASH(EXCEPTION)")
        self.assertEqual([g for g in got if g[2] in q.CRASH_EXIT_REASONS], [got[0]])   # a self-kill (SIGNALED) is not a crash

    def test_has_died_is_not_a_fatal_line_but_a_crash_is(self):
        died = "11-26 06:57:07.897 516 1880 I ActivityManager: Process com.azul.AzWidgets (pid 2528) has died: fg  TOP"
        self.assertEqual(q.fatal_lines(died, "com.azul.AzWidgets"), [])
        crash = "11-26 06:57:07.858 2528 2528 E AndroidRuntime: FATAL EXCEPTION: main (com.azul.AzWidgets)"
        self.assertEqual(len(q.fatal_lines(crash, "com.azul.AzWidgets")), 1)

    def test_activity_creations_are_found_in_the_events_buffer(self):
        ev = ("11-26 06:57:55.100   516  1234 I wm_create_activity: [0,12345,8,com.azul.AzWidgets/com.azul.app.AzulActivity,android.intent.action.MAIN,NULL,NULL,268435456]\n"
              "11-26 06:57:56.100   516  1234 I wm_create_activity: [0,12346,9,com.other.app/.Main,android.intent.action.MAIN,NULL,NULL,0]\n"
              "11-26 06:57:57.100   516  1234 I am_proc_start: [0,1,2,3]\n")
        got = q.create_activity_lines(ev, "com.azul.AzWidgets")
        self.assertEqual(len(got), 1)
        self.assertTrue(got[0][:18] >= "11-26 06:57:55")


class GpuPathTests(unittest.TestCase):
    """The reads the GPU path's QA steps make: the fallback line, the stage a failure spec names, the memory dump, the env."""

    def test_the_fallback_line_names_stage_and_cause(self):
        log = ("11-26 09:01:07.1  2528  2528 W RustStdoutStderr: [render] gpu unavailable at surface: eglCreateWindowSurface failed "
               "(EGL_BAD_NATIVE_WINDOW 0x300b); drawing on the CPU\n"
               "11-26 09:01:07.2  2528  2528 I RustStdoutStderr: [render] backend=cpu requested=gpu device=software\n")
        self.assertEqual(q.gpu_fallbacks(log), [("surface", "eglCreateWindowSurface failed (EGL_BAD_NATIVE_WINDOW 0x300b)")])
        # the fallback line carries no backend=, so the contract line is the only one backend_ran reads
        self.assertEqual(q.backend_ran(log), ("cpu", "gpu", "software"))
        self.assertEqual(q.gpu_fallbacks("[render] backend=gpu requested=gpu device=Mali-G78\n"), [])
        two = ("[render] gpu unavailable at renderer: injected by AZ_GPU_FAIL; drawing on the CPU\n"
               "[render] gpu unavailable at lost: the context was lost twice without a frame in between; drawing on the CPU\n")
        self.assertEqual([stage for stage, _ in q.gpu_fallbacks(two)], ["renderer", "lost"])

    def test_a_failure_spec_names_the_stage_the_fallback_will_carry(self):
        for spec, stage in (("library", "library"), ("context", "context"), ("surface", "surface"), ("make_current", "make_current"),
                            ("functions", "functions"), ("probe", "probe"), ("renderer", "renderer"), ("document", "document"),
                            ("shaders", "shaders"), ("renderer:2", "renderer"), ("make-current", "make_current"),
                            # a frame that cannot go out is a lost context; twice in a row it ends the GPU path at `lost`
                            ("frame", "lost"), ("lost:2", "lost"), ("lost", "lost")):
            self.assertEqual(q.gpu_fail_stage(spec), stage, spec)

    def test_the_memory_dump_gives_pss_rss_and_graphics(self):
        dump = ("** MEMINFO in pid 2528 [com.azul.AzWidgets] **\n"
                "                   Pss  Private  Private  SwapPss      Rss     Heap     Heap     Heap\n"
                "                 Total    Dirty    Clean    Dirty    Total     Size    Alloc     Free\n"
                "  Native Heap   104512   104400        0        0   110000   150000   120000    30000\n"
                "     Graphics    38208    38208        0        0    38208\n"
                "        TOTAL PSS:   251332            TOTAL RSS:   310200       TOTAL SWAP PSS:        0\n")
        self.assertEqual(q.parse_meminfo(dump), {"pss_kb": 251332, "rss_kb": 310200, "graphics_kb": 38208})
        self.assertEqual(q.parse_meminfo("nothing"), {"pss_kb": None, "rss_kb": None, "graphics_kb": None})

    def test_the_engine_env_is_one_property_that_keeps_the_steps_own_keys(self):
        self.assertEqual(q.render_env(), "AZ_LOG=info;AZ_DUMP_PRESENT=1")
        self.assertEqual(q.render_env("AZ_BACKEND=gpu"), "AZ_LOG=info;AZ_DUMP_PRESENT=1;AZ_BACKEND=gpu")
        # setprop replaces the whole value: the log keys stay when a failure is added
        self.assertEqual(q.render_env("AZ_BACKEND=gpu;AZ_GPU_FAIL=renderer:2"),
                         "AZ_LOG=info;AZ_DUMP_PRESENT=1;AZ_BACKEND=gpu;AZ_GPU_FAIL=renderer:2")
        self.assertEqual(q.render_env(";"), "AZ_LOG=info;AZ_DUMP_PRESENT=1")

    def test_a_step_that_needs_the_app_is_followed_by_the_after_hook(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            run = q.Run(d, "android", {})
            seen = []
            run.after = lambda s: seen.append(s.name)
            run.step("a-needs", "x", lambda s: None, needs_app=True)
            run.step("b-plain", "x", lambda s: None)
            self.assertEqual(seen, ["a-needs"])


if __name__ == "__main__":
    unittest.main()
