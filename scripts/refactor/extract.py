#!/usr/bin/env python3
"""REFACTOR13: move blocks of one function VERBATIM into new helper functions.

    scripts/refactor/extract.py SPEC.json [--root DIR] [--index-bin PATH] [--manifest OUT.json]

The spec (see bfc_extract.json) names the function and, per helper, the block
to move: from the line `from` up to (not including) the line `to_before`, both
given as their exact text (trailing whitespace ignored) and each required to
occur exactly once inside the function - so the spec re-runs on a newer
version of the file. `to_before: "<end of fn>"` means up to the function's
closing brace.

The block's lines move byte for byte into the helper's body. What is written
by hand - and lives in the spec, for review and re-runs - is only:
* `head`: the helper's doc, attributes and signature (and, if needed, the
  destructuring of a parameter struct),
* `tail`: how its results come back (the closing `Ok(..)` and `}`),
* `call`: the lines that replace the block in the function.
Helpers are inserted after the function, in spec order.

`--manifest` writes the old line ranges of every moved block and of every
kept run of the function, for `verify_moved.py --blocks` (each must still be
one contiguous run) and the spec's hand-written lines (the only new-only
lines allowed).
"""
import argparse
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
DEFAULT_INDEX_BIN = "/Users/fschutt/Development/azul-refactor-target/release/item_index"
END_OF_FN = "<end of fn>"


class SpecError(Exception):
    pass


def fn_range(index_bin, path, name):
    out = subprocess.run([index_bin, path], capture_output=True, text=True)
    if out.returncode != 0:
        raise SpecError(f"item_index failed on {path}:\n{out.stderr}")
    hits = []
    for line in out.stdout.splitlines():
        f = line.split("\t")
        if f[2] == "fn" and f[3] == name:
            hits.append((int(f[4]), int(f[5])))
    if len(hits) != 1:
        raise SpecError(f"`fn {name}` occurs {len(hits)} times in {path}")
    return hits[0]


def find_once(lines, lo, hi, text, what):
    """1-based line number of the one line in [lo, hi] whose text is `text`."""
    want = text.rstrip()
    hits = [i for i in range(lo, hi + 1) if lines[i - 1].rstrip() == want]
    if len(hits) != 1:
        raise SpecError(f"{what}: {text!r} occurs {len(hits)} times in the function (needs 1)")
    return hits[0]


def extract(spec, root, index_bin):
    path = os.path.join(root, spec["file"])
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    lines = text.splitlines(keepends=True)
    start, end = fn_range(index_bin, path, spec["function"])
    if lines[end - 1].rstrip() != "}":
        raise SpecError(f"the last line of `fn {spec['function']}` is not its closing brace")

    blocks = []
    for ex in spec["extractions"]:
        a = find_once(lines, start, end, ex["from"], f"{ex['helper_name']}.from")
        b = end if ex["to_before"] == END_OF_FN else find_once(lines, start, end, ex["to_before"], f"{ex['helper_name']}.to_before")
        if not (start < a < b <= end):
            raise SpecError(f"{ex['helper_name']}: empty or inverted block ({a}..{b})")
        blocks.append((a, b - 1, ex))
    blocks.sort(key=lambda t: t[0])
    for (a1, b1, e1), (a2, b2, e2) in zip(blocks, blocks[1:]):
        if a2 <= b1:
            raise SpecError(f"blocks {e1['helper_name']} and {e2['helper_name']} overlap")

    new_fn, kept, cursor = [], [], start
    for a, b, ex in blocks:
        if cursor <= a - 1:
            kept.append((cursor, a - 1))
        new_fn += lines[cursor - 1 : a - 1]
        new_fn += [l + "\n" for l in ex["call"]]
        cursor = b + 1
    kept.append((cursor, end))
    new_fn += lines[cursor - 1 : end]

    helpers = []
    for ex in spec["extractions"]:
        a, b = next((a, b) for a, b, e in blocks if e is ex)
        helpers.append("\n")
        helpers += [l + "\n" for l in ex["head"]]
        helpers += lines[a - 1 : b]
        helpers += [l + "\n" for l in ex["tail"]]

    out = lines[: start - 1] + new_fn + helpers + lines[end:]
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("".join(out))

    manifest = {
        "split_fns": [spec["function"]],
        "blocks": [
            {"name": f"moved:{ex['helper_name']}", "file": spec["file"], "start": a, "end": b}
            for a, b, ex in blocks
        ]
        + [{"name": f"kept:{i}", "file": spec["file"], "start": a, "end": b} for i, (a, b) in enumerate(kept)],
        "hand_written": [l for ex in spec["extractions"] for l in ex["head"] + ex["tail"] + ex["call"]],
    }
    summary = [f"extract from `fn {spec['function']}` ({spec['file']}:{start}-{end}):"]
    for a, b, ex in blocks:
        summary.append(f"  {ex['helper_name']:<36} lines {a}-{b} ({b - a + 1} moved verbatim)")
    summary.append(f"  hand-written lines: {len(manifest['hand_written'])}")
    return summary, manifest


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("spec")
    ap.add_argument("--root", default=REPO)
    ap.add_argument("--index-bin", default=os.environ.get("ITEM_INDEX_BIN", DEFAULT_INDEX_BIN))
    ap.add_argument("--manifest")
    args = ap.parse_args()
    with open(args.spec, encoding="utf-8") as fh:
        spec = json.load(fh)
    try:
        summary, manifest = extract(spec, args.root, args.index_bin)
    except SpecError as e:
        print(f"extract.py: {e}", file=sys.stderr)
        return 1
    print("\n".join(summary))
    if args.manifest:
        with open(args.manifest, "w", encoding="utf-8") as fh:
            json.dump(manifest, fh, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
