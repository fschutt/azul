#!/usr/bin/env python3
"""gen_jni_stubs.py <dir of `javac -h` headers> <out.c>: an empty C body for every JNI function the headers declare.

The cold-start lab's control app (scripts/startup-control/build.sh) is the REAL Java side of an Azul APK (scripts/android/*.java:
AzulActivity and the bridges) over a stand-in native library. The Java classes declare their natives and the engine implements them;
the stand-in implements none of them, and a native method with no function throws UnsatisfiedLinkError at its first call. So every
declared native gets a stub that does nothing and answers 0 / NULL (`AzulActivity.nativeGetWindowPointer` answers 0: "the engine has
no window yet", which every caller already handles).
"""
import pathlib
import re
import sys

PROTOTYPE = re.compile(r"JNIEXPORT\s+(\w+)\s+JNICALL\s+(\w+)\s*\(([^)]*)\)\s*;", re.S)
NUMERIC = {"jboolean", "jbyte", "jchar", "jshort", "jint", "jlong", "jfloat", "jdouble"}


def stubs(text):
    """The C text of the stubs for the prototypes in `text`."""
    out = []
    for ret, name, args in PROTOTYPE.findall(text):
        params = []
        for i, raw in enumerate(a.strip() for a in args.split(",") if a.strip()):
            params.append(f"{raw}a{i}" if raw.endswith("*") else f"{raw} a{i}")
        body = "" if ret == "void" else ("return 0;" if ret in NUMERIC else "return NULL;")
        out.append(f"JNIEXPORT {ret} JNICALL {name}({', '.join(params) or 'void'}) {{ {body} }}")
    return "\n".join(out)


def main(argv):
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    headers = sorted(pathlib.Path(argv[0]).glob("*.h"))
    body = "\n".join(stubs(h.read_text()) for h in headers)
    pathlib.Path(argv[1]).write_text("#include <jni.h>\n#include <stddef.h>\n" + body + "\n")
    print(f"{len(headers)} header(s), {body.count('JNIEXPORT')} stub(s) -> {argv[1]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
