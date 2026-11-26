#!/usr/bin/env python3
"""Writes azboard-1.example.json: a board of the Azul tree, made the way AzReview would make it.

    python3 examples/azul-review/docs/make_example.py <repository> <rev> > examples/azul-review/docs/azboard-1.example.json

For each cutout the lines are read from the repository at `rev` (`git show <rev>:<path>`), so the code, the SHA-256 of the lines
and the permalink are real; the pictures are small stand-ins that look like lines of code (AzReview draws the real ones with
the CPU renderer at the screen's scale), to keep the file short. The board's name, the arrows and the layout are made up.
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
# this), label, x, y)
CUTOUTS = [
    (1, "core/src/dom.rs", "pub struct Dom {", "    pub css: azul_css::css::CssVec,", "The DOM an app's layout callback returns", 0, 0, 2),
    (2, "core/src/styled_dom.rs", "pub struct StyledDom {", "    pub css_property_cache: CssPropertyCachePtr,", "The DOM with its CSS cascaded", 360, 150, 2),
    (3, "layout/src/solver3/mod.rs", "pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(", "    viewport: LogicalRect,", "Layout: boxes, text, positions", 720, 0, 0),
    (4, "webrender/core/src/renderer/mod.rs", "    pub fn render(", "    ) -> Result<RenderResults", "The display list becomes pixels", 1080, 150, 0),
    (5, "dll/src/desktop/shell2/mod.rs", "//! shell2 - Modern windowing system abstraction.", "//! └── headless/", "One shell per platform shows them", 1440, 0, 0),
]

ARROWS = [
    (6, 1, 2, "styled", 0.12),
    (7, 2, 3, "laid out", -0.12),
    (8, 3, 4, "display list", 0.12),
    (9, 4, 5, "drawn in a window", -0.12),
]


def git_lines(repo, rev, path):
    text = subprocess.run(["git", "-C", repo, "show", "%s:%s" % (rev, path)], capture_output=True, check=True).stdout.decode("utf-8")
    return text.split("\n")


def span(lines, start, end, back):
    first = next(i for i, l in enumerate(lines) if l.startswith(start))
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


def main():
    repo, rev = sys.argv[1], sys.argv[2]
    items, assets = [], {}
    for id_, path, start, end, label, x, y, back in CUTOUTS:
        lines = git_lines(repo, rev, path)
        first, last = span(lines, start, end, back)
        code = lines[first - 1:last]
        digest = hashlib.sha256(("\n".join(l.rstrip("\r") for l in code) + "\n").encode("utf-8")).hexdigest()
        link = "%s/blob/%s/%s#L%d-L%d" % (WEB, rev, path, first, last)
        key = "img-%d" % id_
        width, height = 280, 112
        assets[key] = {"type": "image/png", "width": width, "height": height, "src": "data:image/png;base64," + base64.b64encode(png(width, height, code)).decode("ascii")}
        items.append({
            "kind": "cutout", "id": id_, "x": x, "y": y, "width": width, "height": height, "label": label,
            "image": {"asset": key, "scale": 1},
            "source": {
                "path": path, "line_start": first, "line_end": last,
                "hash": "sha256:" + digest, "rev": rev, "permalink": link, "language": "rust",
                "code": code,
            },
        })
    for id_, a, b, label, bend in ARROWS:
        items.append({"kind": "arrow", "id": id_, "from": {"item": a, "anchor": "auto"}, "to": {"item": b, "anchor": "auto"}, "bend": bend, "label": label})
    doc = {
        "format": "azboard/1",
        "generator": "AzReview 0.1.0",
        "board": {"id": "b1", "name": "From the DOM to the pixels", "created_ms": 1764140000000, "modified_ms": 1764143600000},
        "workspace": {
            "name": "azul",
            "repo": {"remote": WEB + ".git", "web_url": WEB, "host": "github", "rev": rev, "branch": "master", "dirty": False},
        },
        "author": "Example Author",
        "canvas": {
            "unit": "px", "origin": "top-left",
            "view": {"x": -40, "y": -40, "zoom": 0.8},
            "bounds": {"x": 0, "y": 0, "width": 1720, "height": 262},
        },
        "next_id": 10,
        "assets": assets,
        "items": items,
        "snapshots": [],
    }
    print(json.dumps(doc, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
