#!/usr/bin/env python3
"""check_no_test_hooks.py - does a binary carry the marker of a test build?

    scripts/check_no_test_hooks.py FILE...                       fail when a file holds a build marker
    scripts/check_no_test_hooks.py --expect-marker FILE...       fail when a file holds NO build marker
    scripts/check_no_test_hooks.py --expect-marker=NAME FILE...  fail when a file does not hold the marker of the feature NAME

A build of an Azlin app with a cargo feature that is for tests only puts a marker into its binary: the bytes
`AZLIN-BUILD-MARKER:<feature>:` and a sentence saying what the build is. Two features have one today -
`test-hooks` (azul-appkit's `anchors::TEST_HOOKS_MARKER` and libazul's `run_mode::TEST_HOOKS_MARKER`: the update
anchor, the endpoints and the token server a drive token goes to come from files, variables and switches) and
`memory-keyring` (azul-storage's `azul_keyring::MEMORY_KEYRING_MARKER`: the Azlin keyring is a map of the process
and every drive key is gone at exit). cargo unifies features across the packages of one build, so
`cargo build --release --workspace --all-targets` can turn a dev-dependency's feature on for a binary that is meant
to ship. ANY string that starts with `AZLIN-BUILD-MARKER:` makes a file a test build, whatever the feature's name:
a feature that gets its marker tomorrow is refused with no change here. A shipped build never refers to a marker's
constant, so its binary does not hold the bytes.

`scripts/sign-release.sh release` refuses an artifact that holds one, and CI builds the shipped binary and the test
binary of the bridge and checks both ways (`--expect-marker` is the negative control: a check that cannot fail
checks nothing; `--expect-marker=NAME` is the one for a single feature, which a build with another marker does not
satisfy). X-DELTA-AZDRIVE-KEYS-37, X-DELTA-STRUCT3-5; tested by scripts/test_sign_release.py.
"""
import re
import sys

PREFIX = b"AZLIN-BUILD-MARKER:"
# The name a marker gives its feature: between the prefix and the next colon. A marker with no readable name is a
# marker all the same (its name is empty).
NAME_MAX = 64
MARKER = re.compile(re.escape(PREFIX) + rb"([A-Za-z0-9._-]{0,%d})" % NAME_MAX)
CHUNK = 1 << 20
# A marker that starts within the last WINDOW bytes of a chunk is whole in the next read.
WINDOW = len(PREFIX) + NAME_MAX


def build_markers(path):
    """The names of the build markers the file at `path` holds ('' for one with no readable name), each once, in the
    order they are first met. The file is read in chunks; a marker cut by a chunk's end is found, and its name is read
    whole."""
    names = []
    tail = b""
    with open(path, "rb") as f:
        while True:
            chunk = f.read(CHUNK)
            data = tail + chunk
            for found in MARKER.finditer(data):
                if chunk and found.end() == len(data):
                    break  # its name may go on in the next chunk: it is read again with it
                name = found.group(1).decode("ascii")
                if name not in names:
                    names.append(name)
            if not chunk:
                return names
            tail = data[-WINDOW:]


def named(names):
    """The markers in words, for a message."""
    return "build marker%s %s" % ("s" if len(names) > 1 else "", ", ".join(n or "(no name)" for n in names))


def main(argv):
    expect = None  # None: a marker is refused; "": any marker is expected; else the name of the feature expected
    files = []
    for arg in argv:
        if arg == "--expect-marker":
            expect = ""
        elif arg.startswith("--expect-marker="):
            expect = arg[len("--expect-marker="):]
            if not expect:
                print("check_no_test_hooks: --expect-marker= wants the name of a feature", file=sys.stderr)
                return 2
        else:
            files.append(arg)
    if not files:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: check_no_test_hooks.py [--expect-marker[=NAME]] FILE...", file=sys.stderr)
        return 2
    bad = 0
    for path in files:
        try:
            names = build_markers(path)
        except OSError as e:
            print(f"check_no_test_hooks: {path}: {e}", file=sys.stderr)
            bad += 1
            continue
        if expect is None:
            if names:
                print(f"check_no_test_hooks: {path} is a test build (it holds the {named(names)})", file=sys.stderr)
                bad += 1
            else:
                print(f"check_no_test_hooks: {path}: no marker")
        elif (expect in names) if expect else bool(names):
            print(f"check_no_test_hooks: {path}: marker present (expected): {', '.join(n or '(no name)' for n in names)}")
        else:
            what = f"{expect} marker" if expect else "build marker"
            held = f" (it holds the {named(names)})" if names else ""
            print(f"check_no_test_hooks: {path} holds no {what}{held}, and was expected to", file=sys.stderr)
            bad += 1
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
