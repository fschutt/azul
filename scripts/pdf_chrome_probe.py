#!/usr/bin/env python3
"""Page 1 of a few PDFs, drawn by Chrome and by AzPdf, compared (PDF9).

The user's next step is a reftest harness against Chrome's PDF rendering;
this is its probe. For every PDF it makes up to four pictures of page 1, all
the same width (816 px = a Letter page at 96 dpi by default):

  chrome_pdf     Chrome's own PDF viewer (PDFium), headless; the page is
                 cropped out of the viewer's grey background. THE REFERENCE.
  pdftoppm       poppler, if installed (a second reference: if it disagrees
                 with chrome_pdf, the crop went wrong, not azul).
  svg_in_chrome  azul's PDF -> SVG (`AzPdf --export-svg`, printpdf's page
                 renderer) shown by Chrome: how good the SVG is, whatever
                 azul's SVG renderer does with it.
  azpdf          `AzPdf --export-png`: the SVG drawn by azul's CPU SVG
                 renderer - exactly what the viewer shows.

and three comparisons, each as `mean abs diff` (0-255 per channel), `px
off` (% of pixels off by more than 32 in a channel) and `ink IoU` (overlap
of the dark pixels, 1.0 = the same places are inked):

  azpdf vs chrome_pdf          end to end
  svg_in_chrome vs chrome_pdf  printpdf's PDF -> SVG
  azpdf vs svg_in_chrome       azul's SVG renderer

The numbers are bad today (no <text> / <image> in azul's SVG renderer yet,
printpdf's cubic curves and CTMs - scripts/PDF9_2026_10_03.md); the probe
exists so they can only go up.

    python3 scripts/pdf_chrome_probe.py [--out DIR] [--width 816] [FILE.pdf ...]

Without files it writes its own fixtures (text and shapes; a page drawn
under a `cm` transform) and probes those; files given are probed too (e.g.
~/Development/calc.pdf). Writes DIR/report.md and the pictures. Needs
Chrome (CHROME=...), Pillow for the numbers (without it only the pictures),
AzPdf from target/release (AZPDF=...; without it the azul columns are
skipped). Nothing leaves the machine: Chrome's host resolver maps every name
to nothing (scripts/refci/cdp.py).
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(HERE, "refci"))

import cdp  # noqa: E402  (scripts/refci/cdp.py)

try:
    from PIL import Image, ImageChops
except ImportError:  # the pictures still get made
    Image = None
    ImageChops = None


# ==== Fixtures: small PDFs written by hand, so the probe needs no PDF library ====

def pdf_bytes(pages, title="probe"):
    """A PDF with one page per (width_pt, height_pt, content_stream) and the
    14-font Helvetica as /F1. Offsets in the xref table are exact."""
    objects = []

    def add(body):
        objects.append(body)
        return len(objects)

    catalog = add(None)
    pages_obj = add(None)
    font = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    kids = []
    for (w, h, content) in pages:
        stream = content.encode("latin-1")
        contents = add(b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"\nendstream")
        page = add(("<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %s %s] /Resources << /Font << /F1 %d 0 R >> >> "
                    "/Contents %d 0 R >>" % (pages_obj, w, h, font, contents)).encode("latin-1"))
        kids.append(page)
    info = add(("<< /Title (%s) /Producer (azul pdf_chrome_probe.py) >>" % title).encode("latin-1"))
    objects[catalog - 1] = ("<< /Type /Catalog /Pages %d 0 R >>" % pages_obj).encode("latin-1")
    objects[pages_obj - 1] = ("<< /Type /Pages /Kids [%s] /Count %d >>"
                              % (" ".join("%d 0 R" % k for k in kids), len(kids))).encode("latin-1")
    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for i, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % i + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += (b"trailer\n<< /Size %d /Root %d 0 R /Info %d 0 R >>\nstartxref\n%d\n%%%%EOF\n"
            % (len(objects) + 1, catalog, info, xref))
    return bytes(out)


TEXT_AND_SHAPES = """
1 0 0 rg 72 600 200 100 re f
0 0 1 RG 4 w 72 560 m 540 560 l S
0 0.5 0 RG 3 w 300 650 m 350 750 450 750 500 650 c S
0.2 0.2 0.2 rg 320 600 m 380 600 l 350 660 l h f
BT /F1 36 Tf 72 460 Td (Hello PDF9) Tj ET
BT /F1 14 Tf 72 420 Td (The quick brown fox jumps over the lazy dog.) Tj ET
"""

# The same kind of content placed through `cm` (translate and scale): the
# CTM path printpdf writes wrong today.
TRANSFORMED = """
q 1 0 0 1 100 300 cm
0 0.4 0.8 rg 0 0 150 150 re f
q 0.5 0 0 0.5 200 0 cm 0.8 0.1 0.1 rg 0 0 150 150 re f Q
BT /F1 24 Tf 0 180 Td (Inside a cm) Tj ET
Q
"""


def write_fixtures(folder):
    fixtures = {
        "text_and_shapes.pdf": pdf_bytes([(612, 792, TEXT_AND_SHAPES)], "text and shapes"),
        "transformed.pdf": pdf_bytes([(612, 792, TRANSFORMED)], "transformed"),
    }
    paths = []
    for name, data in fixtures.items():
        path = os.path.join(folder, name)
        with open(path, "wb") as f:
            f.write(data)
        paths.append(path)
    return paths


# ==== The renderers ====

def chrome_pdf_page1(chrome, pdf, width, out_png):
    """Chrome's PDF viewer, page 1 fitted to the width, cropped out of the
    viewer's background."""
    viewport_w = width + 120  # the viewer keeps margins either side
    page = chrome.open("file://" + os.path.abspath(pdf) + "#toolbar=0&navpanes=0&view=FitH",
                       width=viewport_w, height=int(viewport_w * 1.6))
    try:
        png = None
        last = None
        # PDFium paints asynchronously: wait until two screenshots agree.
        for _ in range(20):
            time.sleep(0.5)
            png = page.screenshot()
            if png == last:
                break
            last = png
    finally:
        page.close()
    raw = out_png + ".viewer.png"
    with open(raw, "wb") as f:
        f.write(png)
    if Image is None:
        shutil.copy(raw, out_png)
        return True
    img = Image.open(raw).convert("RGB")
    box = page_box(img)
    if box is None:
        return False
    img.crop(box).resize(scaled_size(box, width), Image.LANCZOS).save(out_png)
    return True


def scaled_size(box, width):
    w = box[2] - box[0]
    h = box[3] - box[1]
    return (width, max(1, round(h * width / max(1, w))))


def page_box(img):
    """The first page in a viewer screenshot: the first run of rows (from the
    top) that differ from the background, and the columns that do within it."""
    w, h = img.size
    px = img.load()
    bg = px[1, h - 2]

    def off(p):
        return sum(abs(a - b) for a, b in zip(p, bg)) > 60

    rows = [any(off(px[x, y]) for x in range(0, w, 2)) for y in range(h)]
    top = next((y for y, r in enumerate(rows) if r), None)
    if top is None:
        return None
    bottom = top
    while bottom + 1 < h and rows[bottom + 1]:
        bottom += 1
    cols = [any(off(px[x, y]) for y in range(top, bottom + 1, 2)) for x in range(w)]
    left = next((x for x, c in enumerate(cols) if c), 0)
    right = max((x for x, c in enumerate(cols) if c), default=w - 1)
    return (left, top, right + 1, bottom + 1)


def pdftoppm_page1(pdf, width, out_png):
    tool = shutil.which("pdftoppm")
    if not tool:
        return False
    stem = out_png[:-4]
    r = subprocess.run([tool, "-f", "1", "-l", "1", "-png", "-singlefile", "-scale-to-x", str(width),
                        "-scale-to-y", "-1", pdf, stem], capture_output=True, timeout=120)
    return r.returncode == 0 and os.path.exists(out_png)


def azpdf_binary():
    path = os.environ.get("AZPDF") or os.path.join(REPO, "target", "release", "AzPdf")
    return path if os.access(path, os.X_OK) else None


def azpdf_env():
    env = dict(os.environ)
    lib = os.environ.get("AZUL_LIB_DIR") or os.path.join(REPO, "target", "azul-lib")
    for var in ("DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"):
        env[var] = lib + (os.pathsep + env[var] if env.get(var) else "")
    return env


def azpdf_export(binary, switch, pdf, width, out):
    """`AzPdf --export-png|--export-svg OUT --page 1 --width W FILE`, capped at
    120 s (the Mac has 8 GB: one app at a time, never left running)."""
    try:
        r = subprocess.run([binary, switch, out, "--page", "1", "--width", str(width), pdf],
                           env=azpdf_env(), capture_output=True, timeout=120)
    except subprocess.TimeoutExpired:
        return False, "timed out"
    if r.returncode != 0:
        return False, (r.stderr.decode("utf-8", "replace").strip() or "exit %d" % r.returncode)[-300:]
    return os.path.exists(out), ""


def svg_in_chrome(chrome, svg, width, out_png):
    html = svg[:-4] + ".html"
    with open(html, "w") as f:
        f.write("<!doctype html><html><body style='margin:0;background:#fff'>"
                "<img src='%s' style='display:block;width:%dpx'></body></html>"
                % (os.path.basename(svg), width))
    page = chrome.open("file://" + os.path.abspath(html), width=width, height=width)
    try:
        time.sleep(1.0)
        png = page.screenshot(full_page=True)
    finally:
        page.close()
    with open(out_png, "wb") as f:
        f.write(png)
    return True


# ==== The numbers ====

def compare(a_png, b_png):
    if Image is None or not (os.path.exists(a_png) and os.path.exists(b_png)):
        return None
    a = Image.open(a_png).convert("RGB")
    b = Image.open(b_png).convert("RGB")
    h = min(a.size[1], b.size[1])
    w = min(a.size[0], b.size[0])
    a = a.crop((0, 0, w, h))
    b = b.crop((0, 0, w, h))
    diff = ImageChops.difference(a, b)
    hist = diff.histogram()
    total = w * h * 3
    mean = sum(i * hist[(c * 256) + i] for c in range(3) for i in range(256)) / max(1, total)
    pa, pb, pd = a.load(), b.load(), diff.load()
    off = 0
    ink_a = ink_b = ink_both = 0
    for y in range(0, h, 2):
        for x in range(0, w, 2):
            if max(pd[x, y]) > 32:
                off += 1
            da = sum(pa[x, y]) < 600
            db = sum(pb[x, y]) < 600
            ink_a += da
            ink_b += db
            ink_both += da and db
    samples = ((h + 1) // 2) * ((w + 1) // 2)
    union = ink_a + ink_b - ink_both
    return {
        "mean": mean,
        "off": 100.0 * off / max(1, samples),
        "iou": (ink_both / union) if union else 1.0,
    }


def fmt(m):
    if m is None:
        return "-"
    return "%.1f / %.1f%% / %.2f" % (m["mean"], m["off"], m["iou"])


# ==== Main ====

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default=os.path.join(tempfile.gettempdir(), "azpdf-probe"))
    ap.add_argument("--width", type=int, default=816)
    ap.add_argument("files", nargs="*")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    pdfs = write_fixtures(args.out) + [os.path.abspath(f) for f in args.files]
    binary = azpdf_binary()
    rows = []
    # The PDF viewer runs scripts: refci's Chrome turns them off by default.
    with cdp.Chrome(extra_args=["--blink-settings=scriptEnabled=true"]) as chrome:
        for pdf in pdfs:
            stem = os.path.join(args.out, os.path.splitext(os.path.basename(pdf))[0])
            pics = {k: "%s.%s.png" % (stem, k) for k in ("chrome_pdf", "pdftoppm", "svg_in_chrome", "azpdf")}
            notes = []
            if not chrome_pdf_page1(chrome, pdf, args.width, pics["chrome_pdf"]):
                notes.append("chrome: no page found")
            if not pdftoppm_page1(pdf, args.width, pics["pdftoppm"]):
                notes.append("pdftoppm: not installed or failed")
            if binary:
                svg = stem + ".page1.svg"
                ok, why = azpdf_export(binary, "--export-svg", pdf, args.width, svg)
                if ok:
                    svg_in_chrome(chrome, svg, args.width, pics["svg_in_chrome"])
                else:
                    notes.append("export-svg: " + why)
                ok, why = azpdf_export(binary, "--export-png", pdf, args.width, pics["azpdf"])
                if not ok:
                    notes.append("export-png: " + why)
            else:
                notes.append("AzPdf not built (target/release/AzPdf or AZPDF=...)")
            row = {
                "pdf": pdf,
                "end_to_end": compare(pics["azpdf"], pics["chrome_pdf"]),
                "svg_quality": compare(pics["svg_in_chrome"], pics["chrome_pdf"]),
                "azul_svg": compare(pics["azpdf"], pics["svg_in_chrome"]),
                "crop_check": compare(pics["pdftoppm"], pics["chrome_pdf"]),
                "notes": notes,
            }
            rows.append(row)
            print("%-28s e2e %-22s svg %-22s azul-svg %-22s crop %s %s" % (
                os.path.basename(pdf)[:28], fmt(row["end_to_end"]), fmt(row["svg_quality"]),
                fmt(row["azul_svg"]), fmt(row["crop_check"]), "; ".join(notes)))

    report = os.path.join(args.out, "report.md")
    with open(report, "w") as f:
        f.write("# AzPdf vs Chrome - page 1 at %d px\n\n" % args.width)
        f.write("Each cell: mean abs diff (0-255) / %% pixels off by > 32 / ink IoU.\n\n")
        f.write("| PDF | azpdf vs chrome_pdf | svg_in_chrome vs chrome_pdf | azpdf vs svg_in_chrome "
                "| pdftoppm vs chrome_pdf | notes |\n|---|---|---|---|---|---|\n")
        for r in rows:
            f.write("| %s | %s | %s | %s | %s | %s |\n" % (
                os.path.basename(r["pdf"]), fmt(r["end_to_end"]), fmt(r["svg_quality"]),
                fmt(r["azul_svg"]), fmt(r["crop_check"]), "; ".join(r["notes"]) or "-"))
    print("report:", report)
    return 0


if __name__ == "__main__":
    sys.exit(main())
