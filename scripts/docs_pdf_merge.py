#!/usr/bin/env python3
"""Merge the per-page guide PDFs into one, with a real PDF outline (bookmarks).

Usage: docs_pdf_merge.py <pdf_dir> <outline.json> <out.pdf>

<pdf_dir> holds NNNN.pdf files (one per rendered URL, zero-padded in URL order,
as written by docs_pdf_cdp.mjs). <outline.json> is the index-aligned manifest
from docs_pdf_book.mjs: a list of {"title", "level"} where level 0 is a
top-level entry (cover / book title) and level 1 nests under the most recent
level-0 entry. We append each source PDF and attach a bookmark to the first
physical page of each, so the merged PDF gets a navigable Contents tree —
something pdfunite cannot produce.
"""
import glob
import json
import os
import sys

from pypdf import PdfWriter


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: docs_pdf_merge.py <pdf_dir> <outline.json> <out.pdf>", file=sys.stderr)
        return 2
    pdf_dir, outline_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]

    pdfs = sorted(glob.glob(os.path.join(pdf_dir, "*.pdf")))
    if not pdfs:
        print(f"no PDFs in {pdf_dir}", file=sys.stderr)
        return 1

    outline = []
    if os.path.exists(outline_path):
        with open(outline_path, encoding="utf-8") as f:
            outline = json.load(f)
    else:
        print(f"warning: {outline_path} missing — merging without bookmarks", file=sys.stderr)

    writer = PdfWriter()
    last_top = None  # parent bookmark for level-1 entries
    for i, pdf in enumerate(pdfs):
        start = len(writer.pages)  # first physical page index of this source PDF
        writer.append(pdf)
        if i < len(outline):
            ent = outline[i]
            title = ent.get("title") or f"Page {i + 1}"
            level = int(ent.get("level", 0))
            if level <= 0:
                last_top = writer.add_outline_item(title, start)
            else:
                writer.add_outline_item(title, start, parent=last_top)

    with open(out_path, "wb") as f:
        writer.write(f)
    print(f"merged {len(pdfs)} page-PDFs -> {out_path} ({len(writer.pages)} pages, {len(outline)} bookmarks)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
