#!/usr/bin/env python3
"""ios_symbol_gate.py: does an iOS binary import a system symbol that is NEWER than its deployment target?

A Rust `extern static` or `extern fn` (and every objc2 `extern` constant) is a STRONG import: the Mach-O records the
symbol as required and, on a device whose iOS does not have it, dyld refuses to LAUNCH the whole app ("Symbol not
found") before a single line of the app runs. A null check on the value comes too late. `AVCaptureDeviceTypeExternal`
(iOS 17.0) in an app that says `MinimumOSVersion 16.0` was exactly that; the symbols are looked up by name at run time
(`dlsym`) when they are newer than the target.

  ios_symbol_gate.py --binary AzWidgets.app/AzWidgets --min-os 16.0 [--sdk <iPhoneOS|iPhoneSimulator SDK path>]
                     [--nm <file with `nm -m -u` output>] [--json out.json]

For every UNDEFINED, non-weak symbol the binary imports it reads the availability the SDK's headers declare
(`API_AVAILABLE(... ios(17.0) ...)`, `API_DEPRECATED(... ios(15.0, 17.0))`, `API_UNAVAILABLE(ios)`) and reports the
ones introduced after --min-os (or not available on iOS at all). A symbol whose framework headers are not in the SDK
(or whose declaration carries no iOS availability) is counted as "unchecked", never as a violation.

Exit 0: no violation. 1: at least one violation. 2: the tool could not run (no nm, no binary).
The parsing and the decision are pure functions (scripts/test_ios_symbol_gate.py).
"""
import argparse
import json
import pathlib
import re
import subprocess
import sys

# `xcrun nm -m -u`: "                 (undefined) external _AVCaptureDeviceTypeExternal (from AVFoundation)"
NM_LINE = re.compile(
    r"^\s*\(undefined\)\s+(?P<weak>weak\s+)?(?:private\s+)?external\s+(?P<name>\S+)(?:\s+\(from (?P<lib>[^)]+)\))?\s*$"
)
IOS_VERSION = re.compile(r"\bios\(\s*(?P<v>[0-9]+(?:\.[0-9]+)*)")
NS_AVAILABLE_IOS = re.compile(r"\bNS_AVAILABLE_IOS\(\s*(?P<v>[0-9]+(?:_[0-9]+)*)\s*\)")
IOS_UNAVAILABLE = re.compile(r"API_UNAVAILABLE\s*\([^)]*\bios\b[^)]*\)")
OBJC_CLASS = re.compile(r"^_OBJC_(?:CLASS|METACLASS)_\$_(?P<cls>\w+)$")


def parse_nm_undefined(text):
    """The imports of an `nm -m -u` listing: [{"name", "lib", "weak"}], in order."""
    out = []
    for line in text.splitlines():
        m = NM_LINE.match(line)
        if m:
            out.append({"name": m.group("name"), "lib": m.group("lib") or "", "weak": bool(m.group("weak"))})
    return out


def version_tuple(text):
    return tuple(int(part) for part in text.replace("_", ".").split("."))


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    return re.sub(r"//[^\n]*", " ", text)


def ios_availability(statement):
    """What one declaration says about iOS: ("since", (17, 0)), ("unavailable", None) or None (no statement made)."""
    available = IOS_VERSION.search(statement)
    if available and ("API_AVAILABLE" in statement or "API_DEPRECATED" in statement):
        return ("since", version_tuple(available.group("v")))
    old = NS_AVAILABLE_IOS.search(statement)
    if old:
        return ("since", version_tuple(old.group("v")))
    if IOS_UNAVAILABLE.search(statement):
        return ("unavailable", None)
    return None


def availability_of_symbol(headers_text, symbol):
    """The iOS availability of `symbol` (a C name without its leading underscore, or an `_OBJC_CLASS_$_X`
    symbol) in the (comment-stripped) text of a framework's headers; None when the headers say nothing."""
    m = OBJC_CLASS.match(symbol)
    if m:
        for found in re.finditer(r"@interface\s+%s\b" % re.escape(m.group("cls")), headers_text):
            before = headers_text[max(0, found.start() - 400): found.start()]
            # the macro sits just before the `@interface`, after the previous declaration's end
            before = before.rsplit(";", 1)[-1].rsplit("}", 1)[-1].rsplit("@end", 1)[-1]
            verdict = ios_availability(before)
            if verdict:
                return verdict
        return None
    name = symbol[1:] if symbol.startswith("_") else symbol
    pattern = re.compile(r"\b%s\b" % re.escape(name))
    for statement in headers_text.split(";"):
        if pattern.search(statement) and ("extern" in statement or "_EXPORT" in statement or "API_" in statement):
            verdict = ios_availability(statement)
            if verdict:
                return verdict
    return None


def violations(imports, availability, min_os):
    """The strong imports newer than `min_os`: [{"name", "lib", "why"}], plus the number left unchecked.

    `availability(name, lib)` answers like `availability_of_symbol` (or None when it cannot tell)."""
    floor = version_tuple(min_os)
    found, unchecked = [], 0
    for item in imports:
        if item["weak"]:
            continue  # a weak import is allowed to be missing
        verdict = availability(item["name"], item["lib"])
        if verdict is None:
            unchecked += 1
            continue
        kind, since = verdict
        if kind == "unavailable":
            found.append({**item, "why": "not available on iOS at all"})
        elif since > floor:
            found.append({**item, "why": "introduced in iOS %s, deployment target %s" % (".".join(map(str, since)), min_os)})
    return found, unchecked


class Headers:
    """The comment-stripped text of a framework's headers in an SDK, read once per framework."""

    def __init__(self, sdk):
        self.sdk = pathlib.Path(sdk) if sdk else None
        self.cache = {}

    def text(self, lib):
        if lib in self.cache:
            return self.cache[lib]
        text = None
        if self.sdk and lib and "/" not in lib:
            folder = self.sdk / "System" / "Library" / "Frameworks" / ("%s.framework" % lib) / "Headers"
            if folder.is_dir():
                text = "\n".join(strip_comments(p.read_text(errors="replace")) for p in sorted(folder.glob("*.h")))
        self.cache[lib] = text
        return text

    def availability(self, name, lib):
        text = self.text(lib)
        return availability_of_symbol(text, name) if text else None


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", help="the Mach-O to inspect (xcrun nm -m -u)")
    ap.add_argument("--nm", help="a file with `nm -m -u` output instead of --binary")
    ap.add_argument("--min-os", default="16.0")
    ap.add_argument("--sdk", help="SDK path (default: xcrun --sdk iphoneos --show-sdk-path)")
    ap.add_argument("--json", help="write the verdict here")
    args = ap.parse_args(argv)
    try:
        if args.nm:
            nm_text = pathlib.Path(args.nm).read_text()
        elif args.binary:
            nm_text = subprocess.check_output(["xcrun", "nm", "-m", "-u", args.binary], text=True)
        else:
            ap.error("--binary or --nm")
        sdk = args.sdk or subprocess.check_output(["xcrun", "--sdk", "iphoneos", "--show-sdk-path"], text=True).strip()
    except (OSError, subprocess.CalledProcessError) as e:
        print("ios_symbol_gate: cannot run: %s" % e, file=sys.stderr)
        return 2
    imports = parse_nm_undefined(nm_text)
    found, unchecked = violations(imports, Headers(sdk).availability, args.min_os)
    verdict = {"min_os": args.min_os, "imports": len(imports), "unchecked": unchecked, "violations": found}
    if args.json:
        pathlib.Path(args.json).write_text(json.dumps(verdict, indent=2))
    for item in found:
        # "error: " first: the demo builder reports the first line that starts with it as the failure
        print("error: ios_symbol_gate: %s (from %s): %s" % (item["name"], item["lib"] or "?", item["why"]))
    print("ios_symbol_gate: %d import(s), %d unchecked, %d newer than iOS %s" % (len(imports), unchecked, len(found), args.min_os))
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
