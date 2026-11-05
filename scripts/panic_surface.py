#!/usr/bin/env python3
"""The panic surface of the crates that read bytes other people wrote (ARCH28 RF50 / AR5b-5).

AzDrive ships with `panic = "abort"`, so a panic reachable from a hostile file, answer or index
ends the program. This script counts, per crate and per file, the places a panic is WRITTEN in
the shipped code - `unwrap()`, `expect(`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`
and the asserts - leaving comments, strings and test items out, and compares the counts with the
committed list `scripts/panic_surface.json`. A count that went UP fails; a count that went down
asks for the list to be lowered, so the list only shrinks.

    scripts/panic_surface.py            check against the list (exit 1 on a rise)
    scripts/panic_surface.py --update   write the list from the tree (after a count went down)
    scripts/panic_surface.py --print    print the counts

What it is not: a proof of reach. A panic that nothing hostile can trigger is counted like one
that anything can; the lint set of the crates (`clippy::unwrap_used`, `expect_used`, `panic`,
`unreachable`, `todo`, `unimplemented`, denied outside tests in azlin-preflate and azlin-compress)
and the fuzz targets (`examples/azul-storage/fuzz`) are what keep the count honest.
"""

import json
import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
LIST = os.path.join(os.path.dirname(os.path.abspath(__file__)), "panic_surface.json")

# The crates, and under each the folders and files that are read (relative to the crate's src).
CRATES = {
    "azlin-preflate": ("examples/azlin-preflate/src", None),
    "azlin-compress": ("examples/azlin-compress/src", None),
    "azul-storage": (
        "examples/azul-storage/src",
        [
            "http1.rs",
            "s3.rs",
            "sharing.rs",
            "packing.rs",
            "encrypted.rs",
            "encrypted_upload.rs",
            "crypto/",
            "meta/",
            "rotation.rs",
            "migrate.rs",
            "recompress.rs",
            "xml.rs",
        ],
    ),
}

KINDS = {
    "unwrap": re.compile(r"\.unwrap\(\)"),
    # `.expect("..")` (the blanked message is spaces); a method `x.expect()` of some type is none
    "expect": re.compile(r"\.expect\((?!\))"),
    "panic": re.compile(r"\bpanic!\s*[(\[{]"),
    "unreachable": re.compile(r"\bunreachable!\s*[(\[{]"),
    "todo": re.compile(r"\btodo!\s*[(\[{]"),
    "unimplemented": re.compile(r"\bunimplemented!\s*[(\[{]"),
    "assert": re.compile(r"\bassert(?:_eq|_ne)?!\s*[(\[{]"),
}


def blank(src: str) -> str:
    """The source with comments, strings and char literals blanked (line breaks kept)."""
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        two = src[i : i + 2]
        if two == "//":
            while i < n and src[i] != "\n":
                out.append(" ")
                i += 1
        elif two == "/*":
            depth = 0
            while i < n:
                if src[i : i + 2] == "/*":
                    depth += 1
                    out.append("  ")
                    i += 2
                elif src[i : i + 2] == "*/":
                    depth -= 1
                    out.append("  ")
                    i += 2
                    if depth == 0:
                        break
                else:
                    out.append("\n" if src[i] == "\n" else " ")
                    i += 1
        elif c == "r" and re.match(r'r#*"', src[i:]):
            m = re.match(r'r(#*)"', src[i:])
            hashes = m.group(1)
            end = src.find('"' + hashes, i + len(m.group(0)))
            end = n if end < 0 else end + 1 + len(hashes)
            out.append("".join("\n" if ch == "\n" else " " for ch in src[i:end]))
            i = end
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            j = min(j + 1, n)
            out.append("".join("\n" if ch == "\n" else " " for ch in src[i:j]))
            i = j
        elif c == "'" and re.match(r"'(?:\\.[^']*|[^\\'])'", src[i:]):
            m = re.match(r"'(?:\\.[^']*|[^\\'])'", src[i:])
            out.append(" " * len(m.group(0)))
            i += len(m.group(0))
        else:
            out.append(c)
            i += 1
    return "".join(out)


def item_end(text: str, start: int) -> int:
    """The end of the item that starts at `start`: its matching brace, or its `;`."""
    depth = 0
    i = start
    while i < len(text):
        c = text[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth <= 0:
                return i + 1
        elif c == ";" and depth == 0:
            return i + 1
        i += 1
    return len(text)


def without_tests(text: str) -> str:
    """`text` with every item behind `#[cfg(test)]` and every `#[test]` fn blanked."""
    out = list(text)
    for m in re.finditer(r"#\[(?:cfg\(test\)|test)\]", text):
        end = item_end(text, m.end())
        for k in range(m.start(), end):
            if out[k] != "\n":
                out[k] = " "
    return "".join(out)


def is_test_file(rel: str) -> bool:
    parts = rel.split("/")
    return "tests" in parts[:-1] or parts[-1] in ("tests.rs",) or parts[-1].endswith("_tests.rs")


def counts_of(path: str) -> dict:
    with open(path, encoding="utf-8") as f:
        text = without_tests(blank(f.read()))
    found = {k: len(rx.findall(text)) for k, rx in KINDS.items()}
    return {k: v for k, v in found.items() if v}


def scan() -> dict:
    result = {}
    for crate, (src, only) in CRATES.items():
        base = os.path.join(ROOT, src)
        per_file = {}
        for dp, _dn, fn in os.walk(base):
            for f in sorted(fn):
                if not f.endswith(".rs"):
                    continue
                full = os.path.join(dp, f)
                rel = os.path.relpath(full, base).replace(os.sep, "/")
                if is_test_file(rel):
                    continue
                if only is not None and not any(
                    rel == o or (o.endswith("/") and rel.startswith(o)) for o in only
                ):
                    continue
                found = counts_of(full)
                if found:
                    per_file[rel] = found
        total = {}
        for found in per_file.values():
            for k, v in found.items():
                total[k] = total.get(k, 0) + v
        result[crate] = {"total": dict(sorted(total.items())), "files": dict(sorted(per_file.items()))}
    return result


def main() -> int:
    now = scan()
    if "--print" in sys.argv:
        print(json.dumps(now, indent=2))
        return 0
    if "--update" in sys.argv:
        with open(LIST, "w", encoding="utf-8") as f:
            json.dump(now, f, indent=2)
            f.write("\n")
        print(f"wrote {os.path.relpath(LIST, ROOT)}")
        return 0
    if not os.path.exists(LIST):
        print(f"{LIST} is missing: run scripts/panic_surface.py --update")
        return 1
    with open(LIST, encoding="utf-8") as f:
        listed = json.load(f)
    rose, fell = [], []
    for crate, data in now.items():
        old = listed.get(crate, {"files": {}})
        for rel in sorted(set(data["files"]) | set(old["files"])):
            have = data["files"].get(rel, {})
            want = old["files"].get(rel, {})
            for kind in sorted(set(have) | set(want)):
                a, b = have.get(kind, 0), want.get(kind, 0)
                if a > b:
                    rose.append(f"{crate}/{rel}: {kind} {b} -> {a}")
                elif a < b:
                    fell.append(f"{crate}/{rel}: {kind} {b} -> {a}")
    for line in fell:
        print("lower the list:", line)
    for line in rose:
        print("PANIC SURFACE GREW:", line)
    if rose:
        print(
            "A panic reachable from a hostile file, answer or index ends the program (panic = abort): "
            "return an error instead, or say in review why this one cannot be reached, "
            "then run scripts/panic_surface.py --update."
        )
        return 1
    if fell:
        print("run scripts/panic_surface.py --update to write the lower counts down")
        return 1
    print("the panic surface is the committed one")
    return 0


if __name__ == "__main__":
    sys.exit(main())
