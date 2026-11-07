#!/usr/bin/env python3
"""check_no_test_hooks.py - does a binary carry the marker of a `test-hooks` build?

    scripts/check_no_test_hooks.py FILE...                 fail when a file holds the marker
    scripts/check_no_test_hooks.py --expect-marker FILE... fail when a file does NOT hold it

A build of an Azlin app with the cargo feature `test-hooks` (azul-appkit's) takes its update anchor, its
endpoints and the token server a drive token goes to from files, variables and switches. cargo unifies
features across the packages of one build, so `cargo build --release --workspace --all-targets` can turn a
dev-dependency's `test-hooks` on for a binary that is meant to ship. Such a binary holds the bytes
`AZLIN-BUILD-MARKER:test-hooks:` (azul-appkit's `anchors::TEST_HOOKS_MARKER`); a shipped one does not.

`scripts/sign-release.sh release` refuses an artifact that holds them, and CI builds the shipped binary and
the test binary of the bridge and checks both ways (`--expect-marker` is the negative control: a check
that cannot fail checks nothing). X-DELTA-AZDRIVE-KEYS-37.
"""
import sys

MARKER = b"AZLIN-BUILD-MARKER:test-hooks:"
CHUNK = 1 << 20


def holds_marker(path):
    """Whether the file at `path` holds the marker (read in chunks; a marker cut by a chunk's end is found)."""
    tail = b""
    with open(path, "rb") as f:
        while True:
            chunk = f.read(CHUNK)
            if not chunk:
                return False
            if MARKER in tail + chunk:
                return True
            tail = chunk[-(len(MARKER) - 1):]


def main(argv):
    expect = "--expect-marker" in argv
    files = [a for a in argv if a != "--expect-marker"]
    if not files:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: check_no_test_hooks.py [--expect-marker] FILE...", file=sys.stderr)
        return 2
    bad = 0
    for path in files:
        try:
            found = holds_marker(path)
        except OSError as e:
            print(f"check_no_test_hooks: {path}: {e}", file=sys.stderr)
            bad += 1
            continue
        if found and not expect:
            print(f"check_no_test_hooks: {path} is a test build (it holds the test-hooks marker)", file=sys.stderr)
            bad += 1
        elif expect and not found:
            print(f"check_no_test_hooks: {path} holds no test-hooks marker, and was expected to", file=sys.stderr)
            bad += 1
        else:
            print(f"check_no_test_hooks: {path}: {'marker present (expected)' if found else 'no marker'}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
