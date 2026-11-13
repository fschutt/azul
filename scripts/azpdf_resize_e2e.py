#!/usr/bin/env python3
"""AzPdf: dragging the window edge does not lay the pages out again at every
size, and the benchmark of one drag step.

At Fit width every window size is a new scale. A page is its SVG read into a
DOM, and its text is laid out at the page's scale, so laying a page out at a
new scale runs all of its text through shaping and line layout again. While
the window is resized the pages keep the size they were laid out at and are
drawn scaled onto the new one; once the size stays put they are laid out at
it, once. AzPdf prints `AZPDF_LAID_OUT <percent>` each time the page view
lays its pages out at a new scale.

    1. writes a four-page PDF of dense text (kerned TJ arrays, as TeX writes
       them) and starts `AzPdf <file>` headless at Fit width; the page's art
       (`#__azpdf_art-1`, the box that holds the page's DOM) is laid out at
       its frame's width;
    2. drags the right window edge in 12 steps of 24 px, one `resize` and one
       frame each: the page frame follows the window, and the pages are not
       laid out at a new scale during the drag (no `AZPDF_LAID_OUT` line - a
       step slower than the settle time may legitimately settle once);
    3. after the drag the art is laid out at the frame's width, with one
       `AZPDF_LAID_OUT` line (crisp at the final size);
    4. the same drag at a fixed zoom (Zoom in): the scale never changes, no
       line at all; its steps are the floor a Fit width step is compared with;
    5. during both drags each page shows its picture (`#__azpdf_picture-1`,
       drawn at the settled scale once the view settled: `AZPDF_PICTURED
       <page> <width>`) instead of its DOM, and no step draws a page it has
       a picture of again (a page coming into view may be drawn once).

It prints the median and the slowest step of both drags (wall clock) and
fails on 1 to 5. Run ONE app at a time:

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/azpdf-resize.log -- \\
      python3 scripts/azpdf_resize_e2e.py --bin target/release/AzPdf
"""

import os
import statistics
import time

import azlin_e2e as e2e
from azlin_e2e import Failure
from pdf_chrome_probe import pdf_bytes

TAG = "azpdf-resize"
WIDTH, HEIGHT = 1280, 860
PAGES = "__azpdf_pages"
FRAME = "__azpdf_page-1"
ART = "__azpdf_art-1"
PICTURE = "__azpdf_picture-1"
STEPS = 12
STEP_PX = 24
# How long the view has to keep its size before the pages are laid out at it
# (`RESIZE_SETTLE_MS` in examples/azul-pdf/src/model.rs), less a margin for
# the two clocks: a step at least this slow may settle the drag once.
SETTLE_MS = 150 - 20
WORDS = ("the quick brown fox jumps over a lazy dog while seven wizards box "
         "and quietly judge the hexagonal sphinx of black quartz").split()


def dense_page(n):
    """A Letter page of 9 pt text, 64 lines, each line a TJ array of kerned words."""
    out = []
    for line in range(64):
        y = 750 - line * 11
        parts = []
        for i in range(14):
            word = WORDS[(n * 7 + line * 3 + i) % len(WORDS)]
            parts.append("(%s ) %d" % (word, -15 - (i % 4) * 5))
        out.append("BT /F1 9 Tf 54 %d Td [%s] TJ ET" % (y, " ".join(parts)))
    out.append("0.2 0.4 0.8 rg 54 40 %d 12 re f" % (100 + 40 * n))
    return "\n".join(out) + "\n"


def fixture(path):
    with open(path, "wb") as f:
        f.write(pdf_bytes([(612, 792, dense_page(n)) for n in (1, 2, 3, 4)], "AzPdf resize"))


def width_of(app, node_id):
    if not app.has_id(node_id, every_dom=True):
        return None
    return float(app.rect(node_id, every_dom=True).get("width", 0))


def within(app, seconds, check):
    """Whether `check` comes true within `seconds` (a frame between tries). A bounded wait: a
    build without the art (one from before the fix) still reaches the benchmark."""
    end = time.time() + seconds
    while time.time() < end:
        if check():
            return True
        app.frame(1)
        time.sleep(0.05)
    return check()


def drag(app, what, findings):
    """Drags the right window edge in STEPS steps of STEP_PX px, one `resize` and one frame
    each and nothing in between; the step times in ms and the `AZPDF_LAID_OUT` lines the
    drag printed. A step that shows page 1's DOM instead of its picture, and a page drawn
    during the drag, are findings."""
    before = app.count("AZPDF_LAID_OUT")
    drawn_before = app.printed("AZPDF_PICTURED")
    times = []
    without_picture = 0
    for step in range(1, STEPS + 1):
        t0 = time.perf_counter()
        app.must("resize", width=float(WIDTH - step * STEP_PX), height=float(HEIGHT))
        app.frame(1)
        times.append((time.perf_counter() - t0) * 1000.0)
        if not app.has_id(PICTURE, every_dom=True):
            without_picture += 1
    laid_out = app.count("AZPDF_LAID_OUT") - before
    during = app.printed("AZPDF_PICTURED")[len(drawn_before):]
    drawn = sum(1 for picture in during if picture in drawn_before)
    app.log("%s: drag step median %.1f ms, slowest %.1f ms (%d steps of %d px), %d relayouts, "
            "%d steps without the picture, %d pictures drawn, %d of them again"
            % (what, statistics.median(times), max(times), STEPS, STEP_PX, laid_out,
               without_picture, len(during), drawn))
    if without_picture:
        findings.append("%s: %d of %d drag steps showed page 1's DOM instead of its picture "
                        "(the page's text laid out again at those steps)"
                        % (what, without_picture, STEPS))
    if drawn:
        findings.append("%s: the drag drew %d pages again at a size they had a picture of"
                        % (what, drawn))
    return times, laid_out


def body(args, logs, out):
    binary = e2e.find_binary("AzPdf", args.bin, "AZPDF_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    pdf = os.path.join(logs, "dense.pdf")
    fixture(pdf)
    app = e2e.App(TAG, binary,
                  [pdf, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT),
                   "--language", "en"],
                  args.debug_port, logs, args.timeout)
    findings = []
    try:
        app.until("AzPdf's page view", lambda: app.has_id(PAGES))
        app.until("pages 1 and 2 made", lambda: app.printed("AZPDF_RENDERED", r"1 dom")
                  and app.printed("AZPDF_RENDERED", r"2 dom"))
        app.until("page 1 laid out", lambda: (width_of(app, FRAME) or 0) > 300)
        app.until("page 1's picture", lambda: app.printed("AZPDF_PICTURED", r"1 \d+"))
        app.frame(3)
        app.screenshot(os.path.join(out, "1-before.png"))
        frame0, art0 = width_of(app, FRAME), width_of(app, ART)
        if art0 is None or abs(art0 - frame0) > 1.0:
            findings.append("before the drag the page's art (#%s) is not laid out at its frame's "
                            "width: frame %s, art %s" % (ART, frame0, art0))

        fit_times, laid_out = drag(app, "Fit width", findings)
        frame1 = width_of(app, FRAME)
        if frame1 is None or frame1 >= frame0 - 1.0:
            findings.append("the page frame does not follow the window at Fit width: %s -> %s"
                            % (frame0, frame1))
        may_settle = sum(1 for t in fit_times if t >= SETTLE_MS)
        if laid_out > may_settle:
            findings.append("the pages were laid out at %d new scales during a drag of %d steps "
                            "(%d of them slower than the settle time): every step lays the "
                            "pages' text out again" % (laid_out, STEPS, may_settle))
        app.screenshot(os.path.join(out, "2-dragged.png"))

        after_drag = app.count("AZPDF_LAID_OUT")

        def settled():
            frame, art = width_of(app, FRAME), width_of(app, ART)
            return frame is not None and art is not None and abs(frame - art) <= 1.0
        if not within(app, 3.0, settled):
            findings.append("after the drag the art is not laid out at the frame's width: "
                            "frame %s, art %s" % (width_of(app, FRAME), width_of(app, ART)))
        app.frame(2)
        if may_settle == 0 and app.count("AZPDF_LAID_OUT") - after_drag != 1:
            findings.append("the end of the drag laid the pages out %d times, not once"
                            % (app.count("AZPDF_LAID_OUT") - after_drag))
        app.screenshot(os.path.join(out, "3-settled.png"))

        app.must("resize", width=float(WIDTH), height=float(HEIGHT))
        app.frame(2)
        app.click(selector="#__azpdf_zoom-in")
        app.until("the zoom changed", lambda: app.printed("AZPDF_ZOOM", r"\d+"))
        within(app, 3.0, settled)
        zoom_pictures = app.count("AZPDF_PICTURED")
        within(app, 5.0, lambda: app.count("AZPDF_PICTURED") > zoom_pictures)
        app.frame(4)
        fixed_times, fixed_laid_out = drag(app, "fixed zoom", findings)
        app.frame(2)
        time.sleep(0.5)
        if fixed_laid_out or not settled():
            findings.append("at a fixed zoom a drag laid the pages out at %d new scales (art "
                            "settled: %s)" % (fixed_laid_out, settled()))
        app.log("BENCH fit-width median %.1f ms max %.1f ms; fixed-zoom median %.1f ms max %.1f ms"
                % (statistics.median(fit_times), max(fit_times),
                   statistics.median(fixed_times), max(fixed_times)))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s"
              % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    if findings:
        raise Failure("; ".join(findings))
    app.log("PASS: a drag keeps the pages' layout, the final size lays them out once")
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8798)
