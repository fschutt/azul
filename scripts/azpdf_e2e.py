#!/usr/bin/env python3
"""AzPdf end to end: open a PDF, its pages drawn, page turns, zoom, search,
the recent list in the data tree (PDF9).

    1. writes a three-page PDF (Helvetica text, shapes; "needle" on pages 2
       and 3) and starts `AzPdf <file>` headless on a fresh data folder;
       it opens (`AZPDF_OPENED 3 <path>`) and page 1's DOM is made
       (`AZPDF_RENDERED 1 dom`); the page view fills the document;
    2. the page frame and its thumbnail exist (`__azpdf_page-1`,
       `__azpdf_thumb-1`); the page is a DOM: its text is in the page's
       document, a double click on a word of it selects the word, a triple
       click the line;
    3. Next goes to page 2 (`AZPDF_PAGE 2`), Page Down to page 3;
    4. Zoom in makes the page wider (`AZPDF_ZOOM <percent>`);
    5. "needle" + Return in the search field: two hits (`AZPDF_HITS 2`)
       in the side pane; a click on the first goes to its page;
    6. the recent list is written to pdf/recent.json and names the file;
    7. the Outline tab says there is no outline; the gear opens azul-appkit's
       settings page, Escape closes it;
    8. flora / dark: a screenshot (the chrome follows, the pages stay paper);
    9. a PDF with a form (a text field and a check box): an input over each
       field; typing and a click change the values (`AZPDF_FIELD`), "Export
       filled PDF" saves a flattened copy (`AZPDF_FILLED`); the
       `--export-filled` switch writes one whose page draws the values and
       that has no form left.

Run ONE app at a time, through the capped runner:

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/azpdf.log -- \\
      python3 scripts/azpdf_e2e.py --bin target/release/AzPdf --out /tmp/azpdf-shots

The debug-server client is the shared one (scripts/azlin_e2e.py); the PDF
writer is the probe's (scripts/pdf_chrome_probe.py).
"""

import json
import os
import subprocess

import azlin_e2e as e2e
from azlin_e2e import Failure
from pdf_chrome_probe import pdf_bytes

TAG = "azpdf"
WIDTH, HEIGHT = 1280, 860
PAGES = "__azpdf_pages"


def page_content(n, needle):
    text = "BT /F1 28 Tf 72 700 Td (Page %d of the AzPdf test) Tj ET\n" % n
    if needle:
        text += "BT /F1 14 Tf 72 660 Td (Look for the needle on this page.) Tj ET\n"
    shapes = "%.1f 0.3 0.3 rg 72 400 %d 120 re f\n0 0 0 RG 2 w 72 380 m 540 380 l S\n" % (0.2 * n, 100 + 60 * n)
    return text + shapes


def fixture(path):
    pages = [(612, 792, page_content(n, n >= 2)) for n in (1, 2, 3)]
    with open(path, "wb") as f:
        f.write(pdf_bytes(pages, "AzPdf E2E"))


def form_pdf(path):
    """A Letter page with a form: a text field `name` (empty, 12 pt) and a
    check box `agree` (unchecked), labels drawn on the page."""
    content = ("BT /F1 12 Tf 72 726 Td (Name:) Tj ET\n"
               "BT /F1 12 Tf 92 652 Td (I agree) Tj ET\n")
    objects = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R] "
        "/DA (/Helv 0 Tf 0 g) >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R "
        "/Resources << /Font << /F1 7 0 R >> >> /Annots [4 0 R 5 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /P 3 0 R /T (name) /FT /Tx "
        "/DA (/Helv 12 Tf 0 g) /Rect [72 696 300 716] >>",
        "<< /Type /Annot /Subtype /Widget /P 3 0 R /T (agree) /FT /Btn /V /Off /AS /Off "
        "/Rect [72 648 86 662] >>",
        "<< /Length %d >>\nstream\n%sendstream" % (len(content), content),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    data = b"%PDF-1.7\n"
    offsets = []
    for i, body in enumerate(objects):
        offsets.append(len(data))
        data += ("%d 0 obj\n%s\nendobj\n" % (i + 1, body)).encode("latin-1")
    xref = len(data)
    data += ("xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)).encode()
    for offset in offsets:
        data += ("%010d 00000 n \n" % offset).encode()
    data += ("trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n"
             % (len(objects) + 1, xref)).encode()
    with open(path, "wb") as f:
        f.write(data)


def form_session(binary, args, logs, out):
    """A PDF with a form: fill it in, export the filled copy (step 9)."""
    pdf = os.path.join(logs, "form.pdf")
    form_pdf(pdf)
    data_dir = os.path.join(logs, "form-data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App("pdf-form", binary, [pdf, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT)],
                  args.debug_port, logs, args.timeout)
    try:
        app.expect_line("AZPDF_FORM", "2", "the form's two fields are read")
        app.until("page 1's DOM made", lambda: app.printed("AZPDF_RENDERED", r"1 dom"))
        app.until("an input over the text field",
                  lambda: app.has_id("__azpdf_field-0-0", every_dom=True))
        app.click(selector="#__azpdf_field-0-0", every_dom=True)
        app.must("text_input", text="Ada")
        app.frame(2)
        app.until("the typed name", lambda: app.printed("AZPDF_FIELD", r"name=Ada"))
        app.click(selector="#__azpdf_field-1-0", every_dom=True)
        app.until("the check box checked", lambda: app.printed("AZPDF_FIELD", r"agree=\S+"))
        if app.printed("AZPDF_FIELD", r"agree=Off"):
            raise Failure("a click on the unchecked box unchecked it")
        app.screenshot(os.path.join(out, "4-form.png"))
        app.must("mock", set={"save_bytes": {"accept": True}})
        app.click(selector="#__azpdf_export-filled")
        app.until("the filled copy saved", lambda: app.printed("AZPDF_FILLED", r"\d+"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # The same fill without a window: the copy's page draws the values, no form left.
    filled = os.path.join(logs, "filled.pdf")
    result = subprocess.run([binary, "--export-filled", filled, "--set", "name=Ada Lovelace",
                             "--set", "agree=Yes", pdf], capture_output=True, text=True, timeout=120)
    if result.returncode != 0:
        raise Failure("--export-filled failed: %s %s" % (result.stdout, result.stderr))
    with open(filled, "rb") as f:
        copy = f.read()
    if b"(Ada Lovelace) Tj" not in copy:
        raise Failure("the filled copy does not draw the typed name")
    if b"/AcroForm" in copy:
        raise Failure("the flattened copy still has a form")
    app.log("PASS: form fields filled and exported")


def body(args, logs, out):
    binary = e2e.find_binary("AzPdf", args.bin, "AZPDF_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    pdf = os.path.join(logs, "three pages.pdf")
    fixture(pdf)

    app = e2e.App("pdf", binary, [pdf, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT)],
                  args.debug_port, logs, args.timeout)
    try:
        app.until("AzPdf's page view", lambda: app.has_id(PAGES))
        app.expect_line("AZPDF_OPENED", "3 %s" % pdf, "the PDF opens with its three pages")
        app.until("page 1's DOM made", lambda: app.printed("AZPDF_RENDERED", r"1 dom"))
        app.frame(3)
        view = app.rect(PAGES)
        if not view or view.get("height", 0) < 300 or view.get("y", 0) + view.get("height", 0) > HEIGHT:
            raise Failure("the page view does not fill the document: %s" % view)
        # The pages and the thumbnails are VirtualViews: DOMs of their own.
        page1 = app.rect("__azpdf_page-1", every_dom=True)
        if not page1 or page1.get("width", 0) < 300:
            raise Failure("page 1 is not laid out in the view: %s" % page1)
        if not app.has_id("__azpdf_thumb-1", every_dom=True):
            raise Failure("no thumbnail of page 1 in the rail")
        # The page is its SVG read into a DOM: the text is text, not pixels.
        app.until("page 1's text in its DOM",
                  lambda: app.shows("Page 1 of the AzPdf test", every_dom=True))
        # A double click on "AzPdf" (x 72 pt + 13 characters of 28 pt
        # Helvetica, the baseline at 700 pt of 792) selects the word.
        page1 = app.rect("__azpdf_page-1", every_dom=True)
        x = page1["x"] + page1["width"] * (72.0 + 200.0) / 612.0
        y = page1["y"] + page1["height"] * (792.0 - 700.0 - 9.0) / 792.0
        app.must("double_click", x=x, y=y)
        app.frame(2)
        state = app.value("get_selection_state") or {}
        selected = [r for sel in state.get("selections") or [] if sel.get("dom_id") != 0
                    for r in sel.get("ranges") or []
                    if r.get("selection_type") != "cursor" and r.get("start") != r.get("end")]
        if not selected:
            raise Failure("a double click on the page's text selects no word: %s"
                          % json.dumps(state)[:400])
        # A triple click selects the paragraph: the page's whole line.
        line = "Page 1 of the AzPdf test"
        for _ in range(3):
            app.must("click", x=x, y=y)
        app.frame(2)
        state = app.value("get_selection_state") or {}
        spans = [(r.get("start"), r.get("end")) for sel in state.get("selections") or []
                 if sel.get("dom_id") != 0 for r in sel.get("ranges") or []
                 if r.get("selection_type") != "cursor"]
        if not any(sorted(span) == [0, len(line)] for span in spans):
            raise Failure("a triple click on the page's text does not select its line: %s"
                          % json.dumps(state)[:400])
        app.screenshot(os.path.join(out, "1-opened.png"))

        # Page turns.
        app.click(selector="#__azpdf_next")
        app.expect_line("AZPDF_PAGE", "2", "Next goes to page 2")
        app.key("pagedown")
        app.expect_line("AZPDF_PAGE", "3", "Page Down goes to page 3")

        # Zoom in: the page gets wider.
        before = app.rect("__azpdf_page-3", every_dom=True) or {}
        app.click(selector="#__azpdf_zoom-in")
        app.until("the zoom changed", lambda: app.printed("AZPDF_ZOOM", r"\d+"))
        app.frame(4)
        after = app.rect("__azpdf_page-3", every_dom=True) or {}
        if after.get("width", 0) <= before.get("width", 0):
            raise Failure("zoom in did not widen the page: %s -> %s" % (before, after))

        # Search.
        app.text_input("#__azpdf_search", "needle")
        app.key("return")
        app.expect_line("AZPDF_HITS", "2", "two pages hold the needle")
        app.until("the hits pane", lambda: app.has_id("__azpdf_hit-0"))
        app.screenshot(os.path.join(out, "2-search.png"))
        app.click(selector="#__azpdf_hit-0")
        app.frame(3)

        # The recent list in the data tree.
        app.until("the recent list saved", lambda: app.printed("AZPDF_RECENT_SAVED", r".*"))
        recent_path = os.path.join(data_dir, "pdf", "recent.json")
        with open(recent_path, "r", encoding="utf-8") as f:
            recent = json.load(f)
        paths = [d.get("path") for d in recent.get("docs", [])]
        if pdf not in paths:
            raise Failure("pdf/recent.json does not name the file: %s" % paths)

        # The outline tab, the settings page.
        app.click(text="Outline")
        app.until("the outline", lambda: app.has_id("__azpdf_outline"))
        if not app.shows("This document has no outline."):
            raise Failure("the outline of an outline-less PDF does not say so")
        app.click(text="Pages")
        app.click(selector="#__azpdf_settings")
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        app.key("escape")
        app.until("the pages again", lambda: not app.has_id("appkit-settings") and app.has_id(PAGES))

        # Flora / dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.frame(3)
        app.screenshot(os.path.join(out, "3-flora-dark.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    form_session(binary, args, logs, out)
    app.log("PASS: open, render, page turns, zoom, search, recent list, outline, settings, form; "
            "screenshots in %s" % out)
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
