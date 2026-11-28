#!/usr/bin/env python3
"""jni_signature_check.py: does every framework call the Rust JNI code makes exist, with that signature, in the Android SDK?
(standard library only; `javap` and `android.jar` of the platform come from the runner's JDK and SDK)

    python3 scripts/ci/jni_signature_check.py <android.jar> <file.rs> [<file.rs> ...]

The Android backends call the framework from Rust by NAME: `env.call_method(&resolver, "query", "(Landroid/net/Uri;...)...", ...)`. A typo
in a name or a descriptor compiles and fails when it runs, on a phone, as a `NoSuchMethodError` the code has to turn into an error.
This reads the string literals of the calls out of the Rust files, asks `javap -s` for the signatures of the classes the files name
(and of the classes a call may be made on: Activity, Context, ...), and reports every (name, descriptor) pair that no such class has.

What it is: a CHECK of spelling, run where an SDK is (a runner, not the Mac). What it is not: a type check of the call's receiver.
A method that exists on a class the file never calls it on passes; the emulator proof is what proves the calls.

The pure parts (reading the calls, reading javap) are unit-tested in scripts/ci/test_jni_signature_check.py.
"""
import re
import subprocess
import sys

# classes a call may be made on without the file naming them (the activity and its supertypes, the system's collections)
SUPERTYPES = [
    "android/app/Activity", "android/content/ContextWrapper", "android/content/Context", "android/view/ContextThemeWrapper",
    "java/lang/Object", "java/lang/Throwable", "java/lang/String", "java/lang/CharSequence", "java/util/List", "java/util/ArrayList",
    "java/util/Collection", "java/io/File",
]

CONST = re.compile(r'const\s+([A-Z][A-Z0-9_]*)\s*:\s*&str\s*=\s*\n?\s*"((?:[^"\\]|\\.)*)"\s*;')
LITERAL = r'"((?:[^"\\]|\\.)*)"'
# a name literal or a const, then a descriptor literal or a const, as in `"query", RESOLVER_QUERY` / `"getFd", "()I"`
# a lookahead: the descriptor of one pair may be the name of the next ("android/net/Uri", "parse", "(...)...", ...)
PAIR = re.compile(r'(?=(?:' + LITERAL + r'|\b([A-Z][A-Z0-9_]*)\b)\s*,\s*(?:' + LITERAL + r'|\b([A-Z][A-Z0-9_]*)\b)\s*,)')
CLASS = re.compile(r'"((?:android|java)/[A-Za-z0-9_/$]+)"')
_TYPE = r"\[*(?:[BCDFIJSZ]|L[^;()\[]+;)"
DESCRIPTOR = re.compile(r"\((?:" + _TYPE + r")*\)(?:" + _TYPE + r"|V)")


def unescape(text):
    return text.replace('\\"', '"').replace("\\\\", "\\")


def consts_of(source):
    """{NAME: value} of the `const NAME: &str = "..."` items of a Rust file."""
    return {m.group(1): unescape(m.group(2)) for m in CONST.finditer(source)}


def calls_of(source):
    """[(name, descriptor)] of the framework calls of a Rust file: every `"name", "(descriptor)ret"` pair (either side may be a const of
    the file), and every constructor `new_object("class", "(descriptor)V", ...)` as `("<init>", descriptor)`."""
    consts = consts_of(source)
    out = []
    for m in PAIR.finditer(source):
        name = unescape(m.group(1)) if m.group(1) is not None else consts.get(m.group(2))
        desc = unescape(m.group(3)) if m.group(3) is not None else consts.get(m.group(4))
        if name and desc and desc.startswith("(") and DESCRIPTOR.fullmatch(desc) and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
            out.append((name, desc))
    for m in re.finditer(r'new_object\(\s*"[^"]+"\s*,\s*(?:' + LITERAL + r'|\b([A-Z][A-Z0-9_]*)\b)', source):
        desc = unescape(m.group(1)) if m.group(1) is not None else consts.get(m.group(2))
        if desc and desc.startswith("("):
            out.append(("<init>", desc))
    return out


def fields_of(source):
    """[(name, descriptor)] of `get_static_field(class, "NAME", "sig")` / `get_field(obj, "name", "sig")` reads."""
    return [(unescape(m.group(1)), unescape(m.group(2)))
            for m in re.finditer(r'get_(?:static_)?field\([^,]+,\s*' + LITERAL + r'\s*,\s*' + LITERAL, source)]


def constants_of(source):
    """The names of the SDK constants a file reads by a string it passes on (`"FLAG_SUPPORTS_RENAME"`, `"COLUMN_DISPLAY_NAME"`): every
    ALL-CAPS literal with an underscore that is not the value of a `const` of the file (those are the strings of actions and extras)."""
    values = set(consts_of(source).values())
    return sorted({lit for lit in re.findall(r'"([A-Z][A-Z0-9]*_[A-Z0-9_]+)"', source) if lit not in values})


def classes_of(source):
    """The `android/...` and `java/...` classes a Rust file names (as a class literal or inside a descriptor), plus the supertypes a call
    may be made on."""
    in_descriptors = {c for c in re.findall(r"L((?:android|java)/[A-Za-z0-9_/$]+);", source)}
    return sorted(set(CLASS.findall(source)) | in_descriptors | set(SUPERTYPES))


def parse_javap(text):
    """`javap -s -p` output -> (methods, fields): sets of (name, descriptor) over all the classes of the text."""
    methods, fields = set(), set()
    last = None
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("descriptor:"):
            desc = stripped[len("descriptor:"):].strip()
            if last is not None:
                (methods if desc.startswith("(") else fields).add((last, desc))
            last = None
        elif stripped.endswith(";") and ("(" in stripped or not stripped.startswith(("public class", "public final class", "public abstract class", "public interface"))):
            head = stripped[:-1]
            if "(" in head:
                head = head[: head.index("(")]
                name = head.split()[-1] if head.split() else ""
                # a constructor is printed with the class name: `public android.content.Intent(java.lang.String);`
                last = "<init>" if "." in name else name
            else:
                last = head.split()[-1] if head.split() else None
    return methods, fields


def missing(calls, fields, methods, field_set, constants=()):
    """The (name, descriptor) pairs no class of the SDK listing has; a constant that is no field of any listed class is `(name, "constant")`."""
    named = {name for name, _ in field_set}
    return sorted(
        {c for c in calls if c not in methods}
        | {f for f in fields if f not in field_set}
        | {(c, "constant") for c in constants if c not in named}
    )


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    jar, files = argv[0], argv[1:]
    calls, fields, classes, constants = [], [], set(), set()
    for path in files:
        source = open(path, encoding="utf-8").read()
        calls += calls_of(source)
        fields += fields_of(source)
        constants |= set(constants_of(source))
        classes |= set(classes_of(source))
    names = sorted(c.replace("/", ".") for c in classes)
    text = ""
    absent = []
    for name in names:
        # one class at a time: a class the SDK does not list (a hidden one) must not hide the others
        listing = subprocess.run(["javap", "-s", "-p", "-classpath", jar, name], capture_output=True, text=True)
        if listing.returncode != 0:
            absent.append(name)
        text += listing.stdout
    if not text:
        print(f"javap listed nothing: {listing.stderr[-400:]}")
        return 2
    for name in absent:
        print(f"note: javap does not know the class {name}")
    methods, field_set = parse_javap(text)
    bad = missing(set(calls), set(fields), methods, field_set, constants)
    print(f"{len(set(calls))} method call(s), {len(set(fields))} field read(s), {len(constants)} constant(s) of {len(files)} file(s); {len(methods)} methods and {len(field_set)} fields known from {len(names)} classes")
    for name, desc in bad:
        print(f"NOT IN THE SDK: {name} {desc}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
