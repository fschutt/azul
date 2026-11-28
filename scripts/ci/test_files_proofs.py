#!/usr/bin/env python3
"""Tests of the pure parts of scripts/ci/android_files_proof.py and scripts/ci/ios_files_proof.py (python3 -I scripts/ci/test_files_proofs.py):
the bytes the self-test's rows write are the SAME bytes the proofs compute (the constants are the ones the Rust test pins:
examples/azul-selftest/src/probes/files_test.rs), and the lines the apps log are read the way they are written."""
import json
import plistlib
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import android_files_proof as android  # noqa: E402
import ios_files_proof as ios  # noqa: E402


def fnv1a64(data):
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


class BytesTests(unittest.TestCase):
    def test_the_files_of_the_rows_are_the_bytes_the_rust_test_pins(self):
        one = android.share_one_bytes()
        self.assertEqual((len(one), fnv1a64(one)), (1047, 0x9CBE42B1DF16258D))
        a, b = android.share_many_bytes("a"), android.share_many_bytes("b")
        self.assertEqual((len(a), fnv1a64(a)), (68, 0x662833021BBB1B4D))
        self.assertEqual((len(b), fnv1a64(b)), (74, 0xD66126D07FC5DB05))

    def test_the_digest_is_fnv_1a(self):
        self.assertEqual(fnv1a64(b""), 0xCBF29CE484222325)
        self.assertEqual(fnv1a64(b"a"), 0xAF63DC4C8601EC8C)
        self.assertEqual(fnv1a64(b"foobar"), 0x85944171F73967E8)


class LogTests(unittest.TestCase):
    def test_the_receivers_lines_are_read_in_order_and_a_broken_one_is_kept_as_such(self):
        log = ("11-28 10:00:00.1  1  2 I AzShareLab: AZ_SHARELAB_RECEIVED {\"action\":\"SEND\",\"items\":[]}\n"
               "11-28 10:00:01.1  1  2 I Other: nothing\n"
               "11-28 10:00:02.1  1  2 I AzShareLab: AZ_SHARELAB_RECEIVED {broken\n")
        got = android.received_lines(log)
        self.assertEqual(got[0], {"action": "SEND", "items": []})
        self.assertIn("unparsable", got[1])
        self.assertEqual(android.received_lines(""), [])

    def test_the_open_handlers_lines_and_the_report_row_are_found(self):
        console = ('noise\nAZ_SELFTEST_OPEN {"action":"View","launched_app":true,"skipped":0,"items":[{"kind":"Url","text":"azselftest://x"}]}\n'
                   'AZ_SELFTEST_REPORT {"app":"AzSelfTest","rows":[{"id":"files.share_one","verdict":"PASS","detail":"the system\'s sheet for one file was shown"}]}\n'
                   'AZ_SELFTEST_REPORT_PART 1/2 {"not":"this"}\n')
        lines = ios.open_lines(console)
        self.assertEqual(len(lines), 1)
        self.assertTrue(lines[0]["launched_app"])
        row = ios.report_row(console, "files.share_one")
        self.assertEqual(row["verdict"], "PASS")
        self.assertIsNone(ios.report_row(console, "files.nothing"))
        self.assertIsNone(ios.report_row("", "files.share_one"))


class PlistTests(unittest.TestCase):
    def test_the_declarations_of_a_built_app_are_read_from_its_info_plist(self):
        with tempfile.TemporaryDirectory() as d:
            info = {
                "CFBundleDocumentTypes": [{"CFBundleTypeName": "Markdown document"}, {"CFBundleTypeName": "Any file"}],
                "CFBundleURLTypes": [{"CFBundleURLSchemes": ["azselftest"]}],
                "LSSupportsOpeningDocumentsInPlace": True,
            }
            (Path(d) / "Info.plist").write_bytes(plistlib.dumps(info))
            names, schemes, in_place = ios.declarations(d)
            self.assertEqual(names, {"Markdown document", "Any file"})
            self.assertEqual(schemes, {"azselftest"})
            self.assertTrue(in_place)
            self.assertEqual(ios.declarations(Path(d) / "missing"), (set(), set(), False))


if __name__ == "__main__":
    unittest.main()
