#!/usr/bin/env python3
"""REFACTOR13: prove that a split (or an extraction) moved code and changed nothing else.

    scripts/refactor/verify_moved.py --old-rev REV --old FILE --new FILE_OR_DIR [--new ...]
        [--plan PLAN.json] [--blocks BLOCKS.json] [--hand-written-ok]

OLD is read from git at REV, NEW from the working tree (a directory means every
`*.rs` directly in it). Lines are compared with trailing whitespace trimmed,
comments included.

1. Multiset: every line of OLD against every line of NEW. Each line found on one
   side only is printed and classified; a split may only produce
   * the generated preamble of a plan module's file (`//!` line, the copy of
     the old `use` items, the plan's `glob_preamble` lines, `use super::*;`) -
     rebuilt from the old file and the plan and compared line by line,
   * `mod x;` / `pub use x::*;`, the plan's `mod_rs_plumbing` lines, blank lines,
   * an outlined module's `mod x {` -> `mod x;` and its dropped closing `}`,
   * a line that differs from an old line only by an inserted `pub(super) `,
   * for an extraction (`--blocks MANIFEST` from extract.py): exactly the
     spec's hand-written head / tail / call lines, counted as a multiset.
   Anything else is UNCLASSIFIED and fails the run (`--hand-written-ok` only
   prints it).
2. Order: every top-level item of OLD (from `item_index`) must appear as ONE
   contiguous run of lines in one NEW file (`pub(super) ` tokens aside), so no
   line moved inside an item. `--blocks` lists further old line ranges that
   must stay contiguous (the blocks an extraction moved out of a function and
   the runs of it that stayed).
3. Forbidden: every NEW-only line outside a verified preamble is scanned for
   todo!/unimplemented!/unreachable!/panic!, `#[ignore]`, an `#[allow(` other
   than unused_imports, and temp / TEMP / HACK / "for now" - any hit fails.
"""
import argparse
import collections
import json
import os
import re
import subprocess
import sys
import tempfile

DEFAULT_INDEX_BIN = "/Users/fschutt/Development/azul-refactor-target/release/item_index"
FORBIDDEN = [
    (re.compile(r"\btodo!\s*\("), "todo!()"),
    (re.compile(r"\bunimplemented!\s*\("), "unimplemented!()"),
    (re.compile(r"\bunreachable!\s*\("), "unreachable!()"),
    (re.compile(r"\bpanic!\s*\("), "panic!()"),
    (re.compile(r"#\[\s*ignore\b"), "#[ignore]"),
    (re.compile(r"#!?\[\s*allow\s*\((?!\s*unused_imports\s*\))"), "#[allow(..)] other than unused_imports"),
    (re.compile(r"\btemp\b|\bTEMP\b|\bHACK\b|\bfor now\b", re.IGNORECASE), "temp / HACK / for now"),
]


def git_show(rev, path):
    out = subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(f"verify_moved: git show {rev}:{path} failed: {out.stderr.strip()}")
    return out.stdout


def index_items(index_bin, text):
    with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False) as fh:
        fh.write(text)
        tmp = fh.name
    try:
        out = subprocess.run([index_bin, tmp], capture_output=True, text=True)
    finally:
        os.unlink(tmp)
    if out.returncode != 0:
        sys.exit(f"verify_moved: item_index failed:\n{out.stderr}")
    items = []
    for line in out.stdout.splitlines():
        f = line.split("\t")
        items.append(
            {"kind": f[2], "name": f[3], "start": int(f[4]), "end": int(f[5]), "vis": f[6], "extra": f[8]}
        )
    return items


def new_files(paths):
    files = []
    for p in paths:
        if os.path.isdir(p):
            for name in sorted(os.listdir(p)):
                if name.endswith(".rs"):
                    files.append(os.path.join(p, name))
        else:
            files.append(p)
    return files


def strip_pub_super(line):
    return line.replace("pub(super) ", "")


def find_run(haystack, needle):
    """Index of `needle` as a contiguous run in `haystack`, or -1."""
    if not needle:
        return 0
    first = needle[0]
    for i in range(len(haystack) - len(needle) + 1):
        if haystack[i] == first and haystack[i : i + len(needle)] == needle:
            return i
    return -1


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--old-rev", default="HEAD")
    ap.add_argument("--old-root", help="read OLD from files under this directory instead of git")
    ap.add_argument("--old", required=True, action="append")
    ap.add_argument("--new", required=True, action="append")
    ap.add_argument("--plan")
    ap.add_argument("--blocks")
    ap.add_argument("--hand-written-ok", action="store_true")
    ap.add_argument("--index-bin", default=os.environ.get("ITEM_INDEX_BIN", DEFAULT_INDEX_BIN))
    args = ap.parse_args()

    plan = json.load(open(args.plan, encoding="utf-8")) if args.plan else {}
    if args.old_root:
        old_texts = {p: open(os.path.join(args.old_root, p), encoding="utf-8").read() for p in args.old}
        args.old_rev = f"(files under {args.old_root})"
    else:
        old_texts = {p: git_show(args.old_rev, p) for p in args.old}
    old_lines = []
    old_items = []
    use_block = []
    for p, text in old_texts.items():
        lines = [l.rstrip() for l in text.split("\n")]
        if lines and lines[-1] == "":
            lines.pop()
        items = index_items(args.index_bin, text)
        for it in items:
            it["file"] = p
            it["lines"] = lines[it["start"] - 1 : it["end"]]
            if it["kind"] == "use" and it["vis"] == "inherited":
                use_block += it["lines"]
        old_items += items
        old_lines += [(p, i + 1, l) for i, l in enumerate(lines)]

    new = {}
    for f in new_files(args.new):
        lines = [l.rstrip() for l in open(f, encoding="utf-8").read().split("\n")]
        if lines and lines[-1] == "":
            lines.pop()
        new[f] = lines

    classes = collections.Counter()
    problems = []

    # ---- 1. strip and check the generated preambles -------------------------
    # A plan module's file starts with exactly what split.py's `preamble_of`
    # writes: `//! <doc>`, a blank line, the old `use` items verbatim, the
    # plan's `glob_preamble` lines, `use super::*;`. Rebuilt here from the old
    # file and compared line by line; only an exact match is stripped.
    plan_modules = set(m["name"] for m in plan.get("modules", []))
    expected_tail = (use_block if plan.get("copy_use_block", True) else []) + [
        l.rstrip() for l in plan.get("glob_preamble", [])
    ] + ["use super::*;"]
    compared_new = []
    for f, lines in new.items():
        start = 0
        if os.path.basename(f)[:-3] in plan_modules:
            n = 2 + len(expected_tail)
            if lines[0].startswith("//! ") and lines[1] == "" and lines[2:n] == expected_tail:
                classes["preamble: //! doc line"] += 1
                classes["preamble: copy of the old use items (lines)"] += len(use_block) if plan.get("copy_use_block", True) else 0
                classes["preamble: glob_preamble lines (plan)"] += len(plan.get("glob_preamble", []))
                classes["preamble: use super::*;"] += 1
                classes["blank"] += 1
                start = n
            else:
                problems.append(f"{f}: its preamble is not the generated one (doc, use items, glob)")
        compared_new += [(f, i + 1, l) for i, l in enumerate(lines) if i >= start]

    # ---- 2. multiset ---------------------------------------------------------
    old_count = collections.Counter(l for _, _, l in old_lines)
    new_count = collections.Counter(l for _, _, l in compared_new)
    old_only = old_count - new_count
    new_only = new_count - old_count

    plumbing = set(l.rstrip() for l in plan.get("mod_rs_plumbing", []))
    outlined = set(plan.get("outline_modules", []))
    modules = set(m["name"] for m in plan.get("modules", []))
    hand_written = []

    def take(counter, line, n=1):
        counter[line] -= n
        if counter[line] <= 0:
            del counter[line]

    for line in list(new_only.elements()):
        if new_only[line] <= 0:
            continue
        m = re.fullmatch(r"(pub )?use (\w+)::\*;", line)
        if line == "":
            classes["blank"] += 1
            take(new_only, line)
            continue
        if line in plumbing:
            classes["plan plumbing (mod.rs)"] += 1
            take(new_only, line)
            continue
        md = re.fullmatch(r"mod (\w+);", line)
        if md and md.group(1) in outlined and f"mod {md.group(1)} {{" in old_only:
            classes["outlined module: `mod x {` -> `mod x;`"] += 1
            take(new_only, line)
            take(old_only, f"mod {md.group(1)} {{")
            continue
        if md and md.group(1) in modules:
            classes["mod declaration"] += 1
            take(new_only, line)
            continue
        if m and m.group(2) in modules:
            classes["glob re-export"] += 1
            take(new_only, line)
            continue
        if "pub(super) " in line:
            stripped = strip_pub_super(line)
            if old_only.get(stripped, 0) > 0:
                classes["visibility: + pub(super)"] += 1
                take(new_only, line)
                take(old_only, stripped)
                continue
    for line in list(old_only.elements()):
        if old_only[line] <= 0:
            continue
        if line == "":
            classes["blank (old side)"] += 1
            take(old_only, line)
        elif line == "}" and classes["outlined module: `mod x {` -> `mod x;`"] > classes["outlined module: dropped `}`"]:
            classes["outlined module: dropped `}`"] += 1
            take(old_only, line)

    # An extraction's hand-written lines (the spec's head / tail / call lines,
    # carried by the manifest): exactly those, as a multiset, may be new.
    if args.blocks:
        allowed = collections.Counter(l.rstrip() for l in json.load(open(args.blocks))["hand_written"])
        for line in list(new_only.elements()):
            if new_only[line] > 0 and allowed[line] > 0:
                classes["hand-written (the spec's head / tail / call lines)"] += 1
                take(new_only, line)
                take(allowed, line)

    def where(side, line):
        hits = [(f, n) for f, n, l in side if l == line]
        return ", ".join(f"{os.path.basename(f)}:{n}" for f, n in hits[:3]) + (" ..." if len(hits) > 3 else "")

    unclassified_new = sorted(new_only.elements())
    unclassified_old = sorted(old_only.elements())

    # ---- 3. order: every old item is one contiguous run ------------------------
    normalized = {f: [strip_pub_super(l) for l in lines] for f, lines in new.items()}
    order_failures = []
    for it in old_items:
        if it["kind"] == "mod" and it["name"] in outlined:
            body = it["extra"].split("=", 1)[1].split(":")
            o, c = int(body[0]), int(body[1])
            body_lines = [strip_pub_super(l) for l in it["lines"][o - it["start"] + 1 : c - it["start"]]]
            target = [f for f in new if os.path.basename(f) == f"{it['name']}.rs"]
            if not target or find_run(normalized[target[0]], body_lines) < 0:
                order_failures.append(f"outlined `mod {it['name']}`: its body is not one run in {it['name']}.rs")
            continue
        needle = [strip_pub_super(l) for l in it["lines"]]
        if args.blocks and it["kind"] == "fn" and it["name"] in json.load(open(args.blocks))["split_fns"]:
            continue  # checked block by block below
        if not any(find_run(lines, needle) >= 0 for lines in normalized.values()):
            order_failures.append(f"{it['kind']} {it['name']} ({it['file']}:{it['start']}-{it['end']}) is not one contiguous run")
    if args.blocks:
        spec = json.load(open(args.blocks, encoding="utf-8"))
        old_by_file = {p: [l.rstrip() for l in t.split("\n")] for p, t in old_texts.items()}
        for b in spec["blocks"]:
            lines = [strip_pub_super(l) for l in old_by_file[b["file"]][b["start"] - 1 : b["end"]]]
            if not any(find_run(l, lines) >= 0 for l in normalized.values()):
                order_failures.append(f"block {b['name']} ({b['file']}:{b['start']}-{b['end']}) is not one contiguous run")

    # ---- 4. forbidden ----------------------------------------------------------
    added = (new_count - old_count)
    forbidden_hits = []
    for line in added.elements():
        for rx, what in FORBIDDEN:
            if rx.search(line):
                forbidden_hits.append(f"{what}: {line.strip()}  ({where(compared_new, line)})")

    # ---- report ----------------------------------------------------------------
    print(f"old: {len(old_lines)} lines in {len(old_texts)} file(s) @ {args.old_rev}; "
          f"new: {sum(len(l) for l in new.values())} lines in {len(new)} file(s)")
    print(f"lines on both sides: {sum((old_count & new_count).values())}")
    print(f"old items checked for order: {len(old_items)}")
    for name, n in sorted(classes.items()):
        print(f"  plumbing  {n:>6}  {name}")
    for line in unclassified_new:
        tag = "HAND-WRITTEN" if args.hand_written_ok else "UNCLASSIFIED"
        hand_written.append(line)
        print(f"  {tag} new-only: {line!r}  ({where(compared_new, line)})")
    for line in unclassified_old:
        tag = "HAND-WRITTEN" if args.hand_written_ok else "UNCLASSIFIED"
        print(f"  {tag} old-only: {line!r}  ({where(old_lines, line)})")
    for p in problems:
        print(f"  PROBLEM {p}")
    for o in order_failures:
        print(f"  ORDER {o}")
    for h in forbidden_hits:
        print(f"  FORBIDDEN {h}")
    failed = bool(problems or order_failures or forbidden_hits)
    if not args.hand_written_ok and (unclassified_new or unclassified_old):
        failed = True
    print("RESULT:", "FAIL" if failed else "OK",
          f"({len(unclassified_new)} new-only / {len(unclassified_old)} old-only unclassified lines, "
          f"{len(order_failures)} order, {len(forbidden_hits)} forbidden)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
