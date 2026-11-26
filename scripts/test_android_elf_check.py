#!/usr/bin/env python3
"""Tests of scripts/android_elf_check.py (the 16 KiB page gate of the Android libraries), and of the one place that links them so.

    python3 scripts/test_android_elf_check.py            (Python 3.11+, standard library only)
"""
import contextlib
import io
import pathlib
import struct
import sys
import tempfile
import unittest
import zipfile

try:
    import tomllib
except ImportError:  # pragma: no cover
    tomllib = None

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
import android_elf_check  # noqa: E402

PT_LOAD, PT_DYNAMIC, PT_NOTE = 1, 2, 4


def elf(aligns, bits=64, machine=183, kinds=None):
    """A minimal ELF shared library: the header and one program header per entry of `aligns` (all PT_LOAD unless `kinds` says)."""
    kinds = kinds or [PT_LOAD] * len(aligns)
    if bits == 64:
        header = b"\x7fELF" + bytes([2, 1, 1, 0]) + bytes(8) + struct.pack("<HHIQQQIHHHHHH", 3, machine, 1, 0, 64, 0, 0, 64, 56, len(aligns), 64, 0, 0)
        phdrs = b"".join(struct.pack("<IIQQQQQQ", kind, 5, 0, 0, 0, 0, 0, align) for kind, align in zip(kinds, aligns))
    else:
        header = b"\x7fELF" + bytes([1, 1, 1, 0]) + bytes(8) + struct.pack("<HHIIIIIHHHHHH", 3, machine, 1, 0, 52, 0, 0, 52, 32, len(aligns), 40, 0, 0)
        phdrs = b"".join(struct.pack("<IIIIIIII", kind, 0, 0, 0, 0, 0, 5, align) for kind, align in zip(kinds, aligns))
    return header + phdrs


def apk(path, entries):
    """A zip at `path`: entries is [(name, bytes, stored, pad_to_offset)]."""
    with zipfile.ZipFile(path, "w") as z:
        for name, data, stored, align in entries:
            info = zipfile.ZipInfo(name)
            info.compress_type = zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED
            if stored and align:   # pad the local header's extra field so the data starts on the boundary
                here = z.fp.tell()
                pad = (-(here + 30 + len(name.encode()))) % align
                info.extra = bytes(pad)
            z.writestr(info, data)


class ElfTest(unittest.TestCase):
    def test_a_library_linked_with_the_ndk_default_fails_and_the_line_names_the_file(self):
        problems = android_elf_check.check_elf("libazul.so", elf([0x1000, 0x1000, 0x1000]))
        self.assertEqual(len(problems), 1)
        self.assertIn("libazul.so", problems[0])
        self.assertIn("0x1000", problems[0])
        self.assertIn("max-page-size=16384", problems[0])

    def test_sixteen_and_sixty_four_kib_alignment_pass(self):
        self.assertEqual(android_elf_check.check_elf("a.so", elf([0x4000, 0x4000, 0x4000])), [])
        self.assertEqual(android_elf_check.check_elf("a.so", elf([0x10000, 0x10000])), [])

    def test_one_bad_segment_among_good_ones_fails(self):
        problems = android_elf_check.check_elf("a.so", elf([0x4000, 0x1000, 0x4000]))
        self.assertEqual(len(problems), 1)
        self.assertIn("0x1000", problems[0])

    def test_only_load_segments_count(self):
        # the dynamic section and the notes are aligned to 8 or 4 bytes, and that is right
        self.assertEqual(android_elf_check.check_elf("a.so", elf([0x4000, 8, 4], kinds=[PT_LOAD, PT_DYNAMIC, PT_NOTE])), [])

    def test_a_32_bit_library_is_read_with_its_own_layout(self):
        self.assertEqual(android_elf_check.check_elf("a.so", elf([0x4000, 0x4000], bits=32, machine=40)), [])
        problems = android_elf_check.check_elf("a.so", elf([0x1000, 0x1000], bits=32, machine=40))
        self.assertEqual(len(problems), 1)
        self.assertIn("0x1000", problems[0])

    def test_a_file_that_is_no_elf_and_a_library_without_load_segments_fail(self):
        self.assertIn("not an ELF", android_elf_check.check_elf("a.so", b"PK\x03\x04 not a library")[0])
        self.assertIn("not an ELF", android_elf_check.check_elf("a.so", b"")[0])
        self.assertIn("no PT_LOAD", android_elf_check.check_elf("a.so", elf([8], kinds=[PT_DYNAMIC]))[0])

    def test_a_torn_off_library_fails_instead_of_raising(self):
        self.assertTrue(android_elf_check.check_elf("a.so", elf([0x4000, 0x4000])[:80]))


class ApkTest(unittest.TestCase):
    def check(self, entries):
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "app.apk"
            apk(path, entries)
            return android_elf_check.check_apk(str(path))

    def test_every_library_of_the_apk_is_read_and_a_bad_one_is_named_with_its_place_in_the_apk(self):
        problems = self.check([
            ("classes.dex", b"dex\n035", False, 0),
            ("lib/arm64-v8a/libazul.so", elf([0x4000] * 3), False, 0),
            ("lib/arm64-v8a/libazul_widgets.so", elf([0x1000] * 3), False, 0),
        ])
        self.assertEqual(len(problems), 1)
        self.assertIn("app.apk!lib/arm64-v8a/libazul_widgets.so", problems[0])

    def test_an_apk_without_a_native_library_is_a_failure(self):
        self.assertIn("no native library", self.check([("classes.dex", b"dex", False, 0)])[0])

    def test_a_compressed_library_needs_no_zip_alignment(self):
        self.assertEqual(self.check([("lib/x86_64/libazul.so", elf([0x4000]), False, 0)]), [])

    def test_a_stored_library_must_start_on_a_16_kib_boundary(self):
        problems = self.check([("lib/x86_64/libazul.so", elf([0x4000]), True, 0)])
        self.assertEqual(len(problems), 1)
        self.assertIn("zipalign", problems[0])
        self.assertEqual(self.check([("lib/x86_64/libazul.so", elf([0x4000]), True, 16384)]), [])


class CommandTest(unittest.TestCase):
    def run_main(self, argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = android_elf_check.main(argv)
        return code, out.getvalue() + err.getvalue()

    def test_exit_codes_and_the_failing_file_in_the_output(self):
        with tempfile.TemporaryDirectory() as d:
            good, bad = pathlib.Path(d) / "good.so", pathlib.Path(d) / "bad.so"
            good.write_bytes(elf([0x4000] * 3))
            bad.write_bytes(elf([0x1000] * 3))
            self.assertEqual(self.run_main([str(good)])[0], 0)
            code, text = self.run_main([str(good), str(bad)])
            self.assertEqual(code, 1)
            self.assertIn("bad.so", text)
            self.assertNotIn("good.so:", text)
            self.assertEqual(self.run_main([str(pathlib.Path(d) / "missing.so")])[0], 2)
            self.assertEqual(self.run_main([])[0], 2)

    def test_an_apk_is_taken_by_its_content_not_its_name(self):
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "AzWidgets-android-x86_64.apk"
            apk(path, [("lib/x86_64/libazul.so", elf([0x1000]), False, 0)])
            code, text = self.run_main([str(path)])
        self.assertEqual(code, 1)
        self.assertIn("lib/x86_64/libazul.so", text)


@unittest.skipIf(tomllib is None, "needs tomllib")
class LinkerFlagsTest(unittest.TestCase):
    """The place that makes every Android library of the workspace 16 KiB aligned is .cargo/config.toml: the library of the lib
    job and the app's own library are linked by cargo for the target, so the target's rustflags reach both."""

    def test_both_android_targets_link_with_16_kib_pages(self):
        config = tomllib.loads((ROOT / ".cargo" / "config.toml").read_text(encoding="utf-8"))
        for target in ("aarch64-linux-android", "x86_64-linux-android"):
            flags = config["target"][target]["rustflags"]
            self.assertIn("-Clink-arg=-Wl,-z,max-page-size=16384", flags, target)
            self.assertIn("-Clink-arg=-Wl,-z,common-page-size=16384", flags, target)

    def test_the_scripts_that_set_rustflags_for_android_would_drop_them(self):
        # RUSTFLAGS and CARGO_TARGET_<T>_RUSTFLAGS replace the config's rustflags: no script of the demos pipeline may set them
        import re
        for name in ("build-android.sh", "demo_lib.sh"):
            code = re.findall(r"(?m)^[^#\n]*RUSTFLAGS", (HERE / name).read_text(encoding="utf-8"))   # not the comments
            self.assertEqual(code, [], name)


if __name__ == "__main__":
    unittest.main()
