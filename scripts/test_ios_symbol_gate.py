#!/usr/bin/env python3
"""Tests of scripts/ci/ios_symbol_gate.py: the `nm -m -u` parser, the availability reader and the verdict.

    python3 scripts/test_ios_symbol_gate.py            (Python 3.11+, standard library only)

The header lines below are the real ones of the SDK (AVCaptureDevice.h): `AVCaptureDeviceTypeExternal` is iOS 17.0, the
app that imported it with `MinimumOSVersion 16.0` could not be launched on an iOS 16 device.
"""
import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "ci"))
import ios_symbol_gate as gate  # noqa: E402

NM = """\
                 (undefined) external _AVCaptureDeviceTypeBuiltInWideAngleCamera (from AVFoundation)
                 (undefined) external _AVCaptureDeviceTypeContinuityCamera (from AVFoundation)
                 (undefined) external _AVCaptureDeviceTypeExternal (from AVFoundation)
                 (undefined) external _AVCaptureDeviceTypeExternalUnknown (from AVFoundation)
                 (undefined) weak external _AVCaptureDeviceTypeAnythingNew (from AVFoundation)
                 (undefined) external _UIApplicationMain (from UIKit)
                 (undefined) external _objc_getProtocol (from libobjc)
                 (undefined) external _OBJC_CLASS_$_AVCaptureDevice (from AVFoundation)
0000000100000df8 (__TEXT,__text) external _main
"""

AVF_HEADER = """
/*!
 @constant AVCaptureDeviceTypeExternal
    An external device type. On iPad, external devices are those that conform to the UVC specification.
 */
AVF_EXPORT AVCaptureDeviceType const AVCaptureDeviceTypeExternal API_AVAILABLE(macos(14.0), ios(17.0), macCatalyst(17.0), tvos(17.0), visionos(2.1)) API_UNAVAILABLE(watchos);

AVF_EXPORT AVCaptureDeviceType const AVCaptureDeviceTypeContinuityCamera API_AVAILABLE(macos(14.0), ios(17.0), macCatalyst(17.0), tvos(17.0)) API_UNAVAILABLE(visionos) API_UNAVAILABLE(watchos);

AVF_EXPORT AVCaptureDeviceType const AVCaptureDeviceTypeExternalUnknown API_DEPRECATED_WITH_REPLACEMENT("AVCaptureDeviceTypeExternal", macos(10.15, 14.0)) API_UNAVAILABLE(ios, macCatalyst, tvos, visionos) API_UNAVAILABLE(watchos);

AVF_EXPORT AVCaptureDeviceType const AVCaptureDeviceTypeBuiltInWideAngleCamera API_AVAILABLE(macos(10.15), ios(10.0), macCatalyst(14.0), tvos(17.0)) API_UNAVAILABLE(watchos);

NS_CLASS_AVAILABLE(10_7, 4_0)
API_AVAILABLE(macos(10.7), ios(4.0))
@interface AVCaptureDevice : NSObject
@end

API_AVAILABLE(ios(15.0))
@interface AVNewThing : NSObject
@end
"""


class NmParser(unittest.TestCase):
    def test_the_undefined_imports_are_read_and_the_defined_ones_are_not(self):
        imports = gate.parse_nm_undefined(NM)
        names = [i["name"] for i in imports]
        self.assertIn("_AVCaptureDeviceTypeExternal", names)
        self.assertIn("_UIApplicationMain", names)
        self.assertNotIn("_main", names, "a defined symbol is not an import")
        external = next(i for i in imports if i["name"] == "_AVCaptureDeviceTypeExternal")
        self.assertEqual(external["lib"], "AVFoundation")
        self.assertFalse(external["weak"])
        weak = next(i for i in imports if i["name"] == "_AVCaptureDeviceTypeAnythingNew")
        self.assertTrue(weak["weak"], "a weak import may be missing at run time")


class Availability(unittest.TestCase):
    def setUp(self):
        self.text = gate.strip_comments(AVF_HEADER)

    def test_a_constant_introduced_in_ios_17_is_read(self):
        self.assertEqual(gate.availability_of_symbol(self.text, "_AVCaptureDeviceTypeExternal"), ("since", (17, 0)))
        self.assertEqual(gate.availability_of_symbol(self.text, "_AVCaptureDeviceTypeContinuityCamera"), ("since", (17, 0)))
        self.assertEqual(gate.availability_of_symbol(self.text, "_AVCaptureDeviceTypeBuiltInWideAngleCamera"), ("since", (10, 0)))

    def test_a_symbol_the_ios_sdk_marks_unavailable_is_unavailable(self):
        self.assertEqual(
            gate.availability_of_symbol(self.text, "_AVCaptureDeviceTypeExternalUnknown"), ("unavailable", None),
            "AVCaptureDeviceTypeExternalUnknown is macOS-only: the symbol is in the .tbd but an iOS device may lack it",
        )

    def test_a_class_is_read_from_the_macro_in_front_of_its_interface(self):
        self.assertEqual(gate.availability_of_symbol(self.text, "_OBJC_CLASS_$_AVNewThing"), ("since", (15, 0)))
        self.assertEqual(gate.availability_of_symbol(self.text, "_OBJC_CLASS_$_AVCaptureDevice"), ("since", (4, 0)))

    def test_what_the_headers_do_not_mention_is_not_judged(self):
        self.assertIsNone(gate.availability_of_symbol(self.text, "_SomethingElse"))
        self.assertIsNone(gate.availability_of_symbol(self.text, "_OBJC_CLASS_$_Nowhere"))

    def test_a_name_is_matched_as_a_whole_word(self):
        # `AVCaptureDeviceTypeExternal` is answered with its own line, not with the line of the longer
        # `AVCaptureDeviceTypeExternalUnknown` that follows it in the header (which is unavailable on iOS)
        self.assertEqual(gate.availability_of_symbol(self.text, "_AVCaptureDeviceTypeExternal"), ("since", (17, 0)))
        self.assertEqual(gate.availability_of_symbol(self.text, "_AVCaptureDeviceTypeExternalUn"), None)


class Verdict(unittest.TestCase):
    def verdict(self, min_os):
        text = gate.strip_comments(AVF_HEADER)
        lookup = lambda name, lib: gate.availability_of_symbol(text, name) if lib == "AVFoundation" else None  # noqa: E731
        return gate.violations(gate.parse_nm_undefined(NM), lookup, min_os)

    def test_an_app_for_ios_16_that_imports_ios_17_symbols_fails(self):
        found, unchecked = self.verdict("16.0")
        names = sorted(f["name"] for f in found)
        self.assertEqual(
            names,
            ["_AVCaptureDeviceTypeContinuityCamera", "_AVCaptureDeviceTypeExternal", "_AVCaptureDeviceTypeExternalUnknown"],
            "the two iOS 17 constants and the one the iOS SDK marks unavailable; not the old one, not the weak one",
        )
        self.assertEqual(unchecked, 2, "UIKit and libobjc have no headers here: unchecked, not guilty")
        why = {f["name"]: f["why"] for f in found}
        self.assertIn("introduced in iOS 17.0", why["_AVCaptureDeviceTypeExternal"])
        self.assertIn("not available on iOS", why["_AVCaptureDeviceTypeExternalUnknown"])

    def test_the_same_binary_for_ios_17_only_has_the_unavailable_symbol_left(self):
        found, _ = self.verdict("17.0")
        self.assertEqual([f["name"] for f in found], ["_AVCaptureDeviceTypeExternalUnknown"])

    def test_a_binary_that_looks_the_constants_up_by_name_passes(self):
        clean = NM.replace("_AVCaptureDeviceTypeContinuityCamera", "_x1").replace(
            "_AVCaptureDeviceTypeExternalUnknown", "_x2").replace("_AVCaptureDeviceTypeExternal ", "_x3 ")
        text = gate.strip_comments(AVF_HEADER)
        lookup = lambda name, lib: gate.availability_of_symbol(text, name) if lib == "AVFoundation" else None  # noqa: E731
        found, _ = gate.violations(gate.parse_nm_undefined(clean), lookup, "16.0")
        self.assertEqual(found, [])


class RealSdk(unittest.TestCase):
    """Against the headers of a real SDK when this machine has one (the macOS SDK carries the iOS availability too)."""

    SDKS = ["/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk"]

    def test_the_real_headers_say_what_the_tests_above_assume(self):
        sdk = next((s for s in self.SDKS if pathlib.Path(s).is_dir()), None)
        if sdk is None:
            self.skipTest("no SDK on this machine")
        headers = gate.Headers(sdk)
        self.assertEqual(headers.availability("_AVCaptureDeviceTypeExternal", "AVFoundation"), ("since", (17, 0)))
        self.assertEqual(headers.availability("_AVCaptureDeviceTypeExternalUnknown", "AVFoundation"), ("unavailable", None))


if __name__ == "__main__":
    unittest.main()
