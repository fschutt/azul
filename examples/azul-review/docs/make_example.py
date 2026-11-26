#!/usr/bin/env python3
"""Writes azboard-1.example.json: a board of the Azul tree with two snapshots, made the way AzReview would make it.

    python3 examples/azul-review/docs/make_example.py <repository> <first rev> <second rev> > examples/azul-review/docs/azboard-1.example.json

For each cutout the lines are read from the repository at the revision of the snapshot (`git show <rev>:<path>`), so the code,
the SHA-256 of the lines, the revisions and the permalinks are real, and so are the dates of the revisions; the reasons are
what the commit messages of the two revisions say (2013be9136 and 6ffdbddfd2 of the Azul tree). The pictures are small stand-ins
that look like lines of code (AzReview draws the real ones with the CPU renderer at the screen's scale) and are made once,
when a cutout is: the second snapshot's cutouts keep the pictures of the first, which is what a board does - the reference
moves to the new revision, the picture stays what was circled. The board's name, the arrows and the layout are made up.

A snapshot's hash is the SHA-256 of its canonical form (RFC 8785): the same value, without the `hash` member, with the members
sorted by name and no white space. `json.dumps(sort_keys=True, separators=(",", ":"), ensure_ascii=False)` is that for the
numbers and texts of this file.
"""

import base64
import hashlib
import json
import struct
import subprocess
import sys
import zlib

WEB = "https://github.com/fschutt/azul"

# (id, path, the line that starts the cutout, the line that ends it (the first line at or after the start that begins with
# this), label, x, y, lines before the start to take along); a cutout the second snapshot adds has `first_rev` None.
CUTOUTS = [
    (1, "core/src/dom.rs", "pub struct Dom {", "    pub css: azul_css::css::CssVec,", "The DOM an app's layout callback returns", 0, 0, 2, True),
    (2, "core/src/styled_dom.rs", "pub struct StyledDom {", "    pub css_property_cache: CssPropertyCachePtr,", "The DOM with its CSS cascaded", 360, 150, 2, True),
    (3, "layout/src/solver3/mod.rs", "pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(", "    viewport: LogicalRect,", "Layout: boxes, text, positions", 720, 0, 0, True),
    (4, "webrender/core/src/renderer/mod.rs", "    pub fn render(", "    ) -> Result<RenderResults", "The display list becomes pixels", 1080, 150, 0, True),
    (5, "dll/src/desktop/shell2/mod.rs", "//! shell2 - Modern windowing system abstraction.", "//! └── headless/", "One shell per platform shows them", 1440, 0, 0, True),
    (6, "layout/src/solver3/cache.rs", "    /// How this document's passes build their display list.", "    pub display_list_options: DisplayListOptions,", "Where a pass finds its settings: the cache", 720, 300, 0, False),
]

# (id, from, to, label, bend, in the first snapshot)
ARROWS = [
    (7, 1, 2, "styled", 0.12, True),
    (8, 2, 3, "laid out", -0.12, True),
    (9, 3, 4, "display list", 0.12, True),
    (10, 4, 5, "drawn in a window", -0.12, True),
    (11, 5, 6, "the window writes its options here", 0.1, False),
    (12, 6, 3, "read by layout_document", 0.1, False),
]

WHY = [
    "The pipeline as it is: an app's layout callback returns a Dom, the cascade makes a StyledDom, solver3::layout_document turns it "
    "into boxes and a display list, WebRender draws that, a shell per platform shows the result.\n"
    "The weak spot is the solver's state: the display-list switches and counters are process-wide (the patching switch and the "
    "moved-run policy are atomics with a per-thread twin for tests, the counters of the patched builds are thread-locals), so a "
    "test that flips one leaves every other test of its binary with it.",
    "The solver's state has an owner now: its switches, memos and counters belong to the window, the cache or the pass, not to the "
    "process (2013be9136). The window writes the display-list options into the cache before each pass and layout_document reads "
    "them from there; since 6ffdbddfd2 the switches come from the settings and no longer from the environment.\n"
    "The boxes are the same; what moved is where the layout's state lives, which is why the cache has a cutout of its own and the "
    "window an arrow to it. Of the five references that were there, two point at code that changed (the fields of Dom and the "
    "parameters of layout_document); the other three are the same lines at the new revision.",
]


def git(repo, *args):
    return subprocess.run(["git", "-C", repo] + list(args), capture_output=True, check=True).stdout.decode("utf-8")


def rev_ms(repo, rev):
    return int(git(repo, "show", "-s", "--format=%ct", rev).strip()) * 1000


def span(lines, start, end, back):
    first = next((i for i, l in enumerate(lines) if l.startswith(start)), None)
    if first is None:
        return None
    last = next(i for i in range(first, len(lines)) if lines[i].startswith(end))
    first = max(0, first - back)
    return first + 1, last + 1  # lines are counted from 1


def png(width, height, code):
    """A PNG that looks like lines of code: a light page, a dark bar for each line (as long as the line), the way a small
    preview does."""
    rows = []
    bar_h, gap = 6, 4
    for y in range(height):
        row = bytearray([250, 249, 245, 255] * width)
        line = y // (bar_h + gap)
        if y % (bar_h + gap) < bar_h and line < len(code):
            n = min(width - 20, 12 + len(code[line].rstrip()) * 4)
            indent = 10 + (len(code[line]) - len(code[line].lstrip())) * 4
            for x in range(indent, min(width - 10, indent + n)):
                row[x * 4:x * 4 + 4] = bytes([58, 55, 47, 255])
        rows.append(b"\x00" + bytes(row))
    raw = b"".join(rows)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def cutout(repo, rev, spec, assets, keep_picture):
    id_, path, start, end, label, x, y, back, _ = spec
    lines = git(repo, "show", "%s:%s" % (rev, path)).split("\n")
    found = span(lines, start, end, back)
    if found is None:
        return None
    first, last = found
    code = lines[first - 1:last]
    digest = hashlib.sha256(("\n".join(l.rstrip("\r") for l in code) + "\n").encode("utf-8")).hexdigest()
    key = "img-%d" % id_
    width, height = 280, 112
    if key not in assets:
        assets[key] = {"type": "image/png", "width": width, "height": height, "src": "data:image/png;base64," + base64.b64encode(png(width, height, code)).decode("ascii")}
    return {
        "kind": "cutout", "id": id_, "x": x, "y": y, "width": width, "height": height, "label": label,
        "image": {"asset": key, "scale": 1},
        "source": {
            "path": path, "line_start": first, "line_end": last,
            "hash": "sha256:" + digest, "rev": rev,
            "permalink": "%s/blob/%s/%s#L%d-L%d" % (WEB, rev, path, first, last),
            "language": "rust", "code": code,
        },
    }


def arrow(spec):
    id_, a, b, label, bend, _ = spec
    return {"kind": "arrow", "id": id_, "from": {"item": a, "anchor": "auto"}, "to": {"item": b, "anchor": "auto"}, "bend": bend, "label": label}


def name_of(items, item):
    """What to call an item, as AzReview does: a cutout by its label, an arrow by its label."""
    return item.get("label") or ("%s:%d-%d" % (item["source"]["path"], item["source"]["line_start"], item["source"]["line_end"]) if item["kind"] == "cutout" else "")


def summary(before, after):
    old = {i["id"]: i for i in before}
    new = {i["id"]: i for i in after}
    s = {"added": [], "removed": [], "moved": [], "changed": []}
    for id_ in sorted(new):
        item = new[id_]
        if id_ not in old:
            s["added"].append({"id": id_, "kind": item["kind"], "name": name_of(after, item)})
            continue
        was = old[id_]
        if item["kind"] == "cutout":
            if (was["x"], was["y"]) != (item["x"], item["y"]):
                s["moved"].append({"id": id_, "from": [was["x"], was["y"]], "to": [item["x"], item["y"]]})
            what = []
            if was.get("label") != item.get("label"):
                what.append("label")
            if (was["width"], was["height"]) != (item["width"], item["height"]):
                what.append("size")
            keys = ("path", "line_start", "line_end", "hash")
            if [was["source"].get(k) for k in keys] != [item["source"].get(k) for k in keys]:
                what.append("reference")
            if what:
                s["changed"].append({"id": id_, "what": what})
    for id_ in sorted(old):
        if id_ not in new:
            s["removed"].append({"id": id_, "kind": old[id_]["kind"], "name": name_of(before, old[id_])})
    return s


def snapshot(repo, n, rev, parent, before, items, taken_ms, tags):
    code = {
        "repo": {"remote": WEB + ".git", "web_url": WEB, "host": "github"},
        "rev": rev, "branch": "master", "dirty": False, "rev_time_ms": rev_ms(repo, rev),
    }
    body = {
        "id": "s%d" % n,
        "taken_ms": taken_ms,
        "author": "Example Author",
        "code": code,
        "why": WHY[n - 1],
        "tags": tags,
        "summary": summary(before, items),
        "canvas": {"view": {"x": -40, "y": -40, "zoom": 0.8}},
        "next_id": max(i["id"] for i in items) + 1,
        "items": items,
    }
    if parent:
        body = {"id": body["id"], "parent": parent, **{k: v for k, v in body.items() if k != "id"}}
    canonical = json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return {**body, "hash": "sha256:" + hashlib.sha256(canonical.encode("utf-8")).hexdigest()}


def main():
    repo, rev1, rev2 = sys.argv[1], sys.argv[2], sys.argv[3]
    assets = {}
    first_items = [c for c in (cutout(repo, rev1, s, assets, False) for s in CUTOUTS if s[8]) if c] + [arrow(a) for a in ARROWS if a[5]]
    second_items = [c for c in (cutout(repo, rev2, s, assets, True) for s in CUTOUTS) if c] + [arrow(a) for a in ARROWS]
    t1, t2 = rev_ms(repo, rev1) + 5 * 60 * 1000, rev_ms(repo, rev2) + 3 * 60 * 60 * 1000
    s1 = snapshot(repo, 1, rev1, None, [], first_items, t1, [])
    s2 = snapshot(repo, 2, rev2, "s1", first_items, second_items, t2, ["2013be9136", "6ffdbddfd2"])
    doc = {
        "format": "azboard/1",
        "generator": "AzReview 0.1.0",
        "board": {"id": "b1", "name": "From the DOM to the pixels", "created_ms": t1 - 20 * 60 * 1000, "modified_ms": t2},
        "workspace": {
            "name": "azul",
            "repo": {"remote": WEB + ".git", "web_url": WEB, "host": "github", "rev": rev2, "branch": "master", "dirty": False},
        },
        "author": "Example Author",
        "canvas": {
            "unit": "px", "origin": "top-left",
            "view": {"x": -40, "y": -40, "zoom": 0.8},
            "bounds": {"x": 0, "y": 0, "width": 1720, "height": 412},
        },
        "next_id": max(i["id"] for i in second_items) + 1,
        "assets": assets,
        "items": second_items,
        "snapshots": [s1, s2],
    }
    print(json.dumps(doc, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
