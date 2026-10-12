#!/usr/bin/env python3
"""AzPdf: dragging the window edge does not lay the pages out again at every
size, and the benchmark of one drag step.

At Fit width every window size is a new scale. A page is its SVG read into a
DOM, and its text is laid out at the page's scale, so laying a page out at a
new scale runs all of its text through shaping and line layout again. While
the window is resized the pages keep the size they were laid out at and are
drawn scaled onto the new one; once the size stays put they are laid out at
it, once.

    1. writes a four-page PDF of dense text (kerned TJ arrays, as TeX writes
       them) and starts `AzPdf <file>` headless at Fit width;
    2. drags the right window edge in 12 steps of 24 px: the page frame
       follows the window every step, the page's laid-out art
       (`__azpdf_art-1`, the page's DOM) keeps the width it had before the
       drag;
    3. after the drag the art is laid out at the frame's width again (crisp
       at the final size);
    4. the same drag at a fixed zoom (Zoom in): there the scale never changes,
       the floor every Fit width step is compared with.

It prints the median and the slowest step of both drags (`resize` + one
frame, wall clock) and fails on 2 or 3. Run ONE app at a time:

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
STEPS = 12
STEP_PX = 24
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


def drag(app, what):
    """Drags the right window edge in STEPS steps of STEP_PX px (one `resize` and one frame
    each); the step times in ms and the (frame, art) widths after each step."""
    times, widths = [], []
    for step in range(1, STEPS + 1):
        t0 = time.perf_counter()
        app.must("resize", width=float(WIDTH - step * STEP_PX), height=float(HEIGHT))
        app.frame(1)
        times.append((time.perf_counter() - t0) * 1000.0)
        widths.append((width_of(app, FRAME), width_of(app, ART)))
    app.log("%s: drag step median %.1f ms, slowest %.1f ms (%d steps of %d px)"
            % (what, statistics.median(times), max(times), STEPS, STEP_PX))
    return times, widths


def grow_back(app):
    app.must("resize", width=float(WIDTH), height=float(HEIGHT))
    app.frame(2)


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
        app.frame(3)
        app.screenshot(os.path.join(out, "1-before.png"))
        frame0, art0 = width_of(app, FRAME), width_of(app, ART)
        if art0 is None or abs(art0 - frame0) > 1.0:
            findings.append("before the drag the page's art (#%s) is not laid out at its frame's "
                            "width: frame %s, art %s" % (ART, frame0, art0))

        fit_times, widths = drag(app, "Fit width")
        if not all(f is not None and f < frame0 - 1.0 for f, _ in widths):
            findings.append("the page frame does not follow the window at Fit width: %s"
                            % [f for f, _ in widths])
        moved = [a for _, a in widths if a is None or abs(a - (art0 or 0)) > 1.0]
        if moved:
            findings.append("the page's art was laid out again during the drag (widths %s, "
                            "before %s): every step reshaped the page's text"
                            % ([a for _, a in widths], art0))
        app.screenshot(os.path.join(out, "2-dragged.png"))

        def settled():
            frame, art = width_of(app, FRAME), width_of(app, ART)
            return frame is not None and art is not None and abs(frame - art) <= 1.0
        try:
            app.until("the art laid out at the final size", settled)
        except Failure:
            findings.append("after the drag the art is not laid out at the frame's width: "
                            "frame %s, art %s" % (width_of(app, FRAME), width_of(app, ART)))
        app.screenshot(os.path.join(out, "3-settled.png"))

        grow_back(app)
        app.click(selector="#__azpdf_zoom-in")
        app.until("the zoom changed", lambda: app.printed("AZPDF_ZOOM", r"\d+"))
        app.frame(4)
        fixed_times, _ = drag(app, "fixed zoom")
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
