#!/usr/bin/env python3
"""Tests of the pure parts of scripts/ci/ios_phone_gpu.py (no phone needed).   python3 -I scripts/ci/test_ios_phone_gpu.py"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ios_phone_gpu as p  # noqa: E402

PHONE_GPU = (
    '[   31022us][Info][Rendering] [iOS GPU] GL_VENDOR="Apple Inc." GL_RENDERER="Apple A17 Pro GPU" GL_VERSION="OpenGL ES 3.0 APPLE-24.5.5" GLSL="OpenGL ES GLSL ES 3.00"\n'
    "[  412044us][Info][Rendering] [iOS GPU] WebRender renderer created in 61 ms\n"
    "[  498100us][Info][Rendering] [iOS GPU] first frame 1179x2556: 41 draw calls, drawn in 14 ms, 470 ms after the GPU path began\n"
    "[  498300us][Info][Rendering] [render] backend=gpu requested=gpu device=Apple A17 Pro GPU\n"
    "[ 9812031us][Info][Rendering] [render] frames n=119 over_ms=1004 interval_ms median=8.3 p95=9.1 max=16.7 draw_ms median=2.1 p95=3.0\n"
    "[10820113us][Info][Rendering] [render] frames n=118 over_ms=1002 interval_ms median=8.4 p95=11.0 max=16.7 draw_ms median=2.2 p95=3.4\n")
CPU = (
    "[  300044us][Info][Rendering] [render] backend=cpu requested=cpu device=software\n"
    "[ 9812031us][Info][Rendering] [render] frames n=60 over_ms=1003 interval_ms median=16.7 p95=17.9 max=33.4\n")
FALLBACK = (
    "[  412031us][Warn][Rendering] [render] gpu unavailable at renderer: create_webrender_instance: Shader(...); drawing on the CPU\n"
    "[  412044us][Info][Rendering] [render] backend=cpu requested=gpu device=software\n")


class ReportTests(unittest.TestCase):
    def test_a_phone_gpu_run_is_read_completely(self):
        r = p.report("gpu", PHONE_GPU)
        self.assertEqual((r["ran"], r["requested"], r["device"]), ("gpu", "gpu", "Apple A17 Pro GPU"))
        self.assertEqual(r["first_frame"], {"w": 1179, "h": 2556, "draw_calls": 41, "draw_ms": 14, "since_ms": 470})
        self.assertEqual(r["renderer_ms"], 61)
        self.assertEqual(r["gl_strings"]["renderer"], "Apple A17 Pro GPU")
        self.assertEqual(r["gl_strings"]["version"], "OpenGL ES 3.0 APPLE-24.5.5")
        self.assertEqual((r["frames"]["median"], r["frames"]["p95"], r["frames"]["n"]), (8.4, 11.0, 237))
        self.assertEqual(r["fallbacks"], [])

    def test_a_cpu_run_has_no_draw_time_and_no_first_gpu_frame(self):
        r = p.report("cpu", CPU)
        self.assertEqual((r["ran"], r["device"]), ("cpu", "software"))
        self.assertIsNone(r["first_frame"])
        self.assertNotIn("draw_median", r["frames"])

    def test_a_fallback_names_its_stage(self):
        r = p.report("gpu", FALLBACK)
        self.assertEqual([stage for stage, _cause in r["fallbacks"]], ["renderer"])
        self.assertEqual(r["ran"], "cpu")

    def test_nothing_on_the_console_is_unknown(self):
        r = p.report("gpu", "")
        self.assertEqual(r["ran"], "unknown")
        self.assertIsNone(r["frames"])


class VerdictTests(unittest.TestCase):
    def verdict(self, **texts):
        return p.verdict([p.report(mode, text) for mode, text in texts.items()])

    def test_a_working_phone_passes(self):
        lines = self.verdict(cpu=CPU, gpu=PHONE_GPU, auto=PHONE_GPU.replace("requested=gpu", "requested=auto"))
        self.assertTrue(any(l.startswith("PASS: the GPU path ran on Apple A17 Pro GPU") for l in lines), lines)
        self.assertTrue(any(l.startswith("PASS: auto took the GPU") for l in lines), lines)
        self.assertFalse(any(l.startswith(("FAIL", "FINDING")) for l in lines), lines)

    def test_a_gpu_run_on_a_software_renderer_is_a_finding_not_a_pass(self):
        sim = PHONE_GPU.replace("Apple A17 Pro GPU", "Apple Software Renderer")
        lines = self.verdict(gpu=sim)
        self.assertTrue(any(l.startswith("FINDING") and "SOFTWARE" in l for l in lines), lines)

    def test_a_fallback_fails_and_says_where(self):
        lines = self.verdict(gpu=FALLBACK)
        self.assertTrue(any(l.startswith("FAIL: requested gpu, ran cpu (stage renderer") for l in lines), lines)

    def test_a_build_without_a_gpu_path_says_so(self):
        lines = self.verdict(gpu=CPU.replace("requested=cpu", "requested=gpu"))
        self.assertTrue(any("this build has no GPU path" in l for l in lines), lines)

    def test_auto_that_ran_the_cpu_on_a_phone_fails(self):
        lines = self.verdict(auto=CPU.replace("requested=cpu", "requested=auto"))
        self.assertTrue(any(l.startswith("FAIL: auto ran the CPU renderer on a phone") for l in lines), lines)

    def test_no_line_at_all_is_a_failure_with_its_reason(self):
        lines = self.verdict(gpu="")
        self.assertTrue(any(l.startswith("FAIL: no `[render] backend=` line") for l in lines), lines)


if __name__ == "__main__":
    unittest.main()
