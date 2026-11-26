#!/usr/bin/env python3
"""Tests of the pure parts of scripts/ci/mobile_qa.py (no device needed): the picture signatures, the BMP and PNG
code, and the readers of what dumpsys / uiautomator / logcat print.   python3 -I scripts/ci/test_mobile_qa.py"""
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
        path = Path(__file__).resolve().parent / "_test_out.png"
        try:
            q.write_png(path, w, h, px)
            data = path.read_bytes()
        finally:
            if path.exists():
                path.unlink()
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


if __name__ == "__main__":
    unittest.main()
