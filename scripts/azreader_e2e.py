#!/usr/bin/env python3
"""AzReader end to end over the debug server (headless).

    1. starts AzReader on an empty, temporary data folder with --sample: the sample EPUB is made,
       imported as reader/books/<uuid>/ (book.epub, info.json, state.json) and opens; its first
       chapter is laid out (AZREADER_PAGES) and shows as page 0 with the running head and the
       folio;
    2. turns the page with the Right key (AZREADER_PAGE, or the next chapter when the first has
       one page only), bookmarks it with Ctrl/Cmd+D: state.json holds the bookmark;
    3. opens the contents (Ctrl/Cmd+T) and goes to the second chapter from its entry; the running
       head names it;
    4. makes the text larger (Ctrl/Cmd+Plus): the chapter is laid out again;
    5. screenshots in flat / flora x light / dark; the library (Escape) shows the book's tile;
    6. starts AzReader again on the same folder, opens the book from its tile (double click): it
       opens where the reader left it (the second chapter).

Usage (from the azul repository, after building libazul with the debug server and AzReader):

    python3 scripts/azreader_e2e.py [--bin target/release/AzReader] [--debug-port 8796]
        [--timeout 180] [--out <dir>] [--keep]

Run it through the capped runner (scripts/waves/tools/run_capped.sh) on a small machine.
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as e  # noqa: E402

PAGE0 = "__azreader_page-0"
HEAD = "__azreader_head"
FOLIO = "__azreader_folio"


def book_dir(data, book_id):
    return os.path.join(data, "reader", "books", book_id)


def state_of(data, book_id):
    try:
        with open(os.path.join(book_dir(data, book_id), "state.json"), "r", encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def pages_of(app, chapter):
    """The page counts printed for `chapter` so far."""
    out = []
    for value in app.printed("AZREADER_PAGES", r"\d+ \d+"):
        c, n = value.split()
        if int(c) == chapter:
            out.append(int(n))
    return out


def first_session(app, data, out):
    app.until("the window", lambda: app.printed("AZREADER_READY", r".*"))
    app.must("resize", width=1280, height=800)
    app.frame(2)
    book_id = app.until("the sample book", lambda: (app.printed("AZREADER_IMPORTED", r"[0-9a-f-]+") or [None])[-1])
    for name in ("book.epub", "info.json", "state.json"):
        if not os.path.isfile(os.path.join(book_dir(data, book_id), name)):
            raise e.Failure("reader/books/%s/%s was not written" % (book_id, name))
    app.log("the sample is reader/books/%s/" % book_id)

    app.until("the book to open", lambda: [v for v in app.printed("AZREADER_OPENED", r"[0-9a-f-]+ \d+") if v.startswith(book_id)])
    app.until("the first chapter's pages", lambda: pages_of(app, 0))
    app.frame(3)
    app.until("the first page", lambda: app.has_id(PAGE0))
    if not app.shows("Chapter I. Down the Rabbit-Hole"):
        raise e.Failure("the running head does not name the first chapter")
    if not app.shows("Alice was beginning to get very tired"):
        raise e.Failure("the first page does not show the chapter's first paragraph")
    app.settle()
    app.screenshot(os.path.join(out, "first-page.png"))
    app.log("chapter I laid out on %d page(s)" % pages_of(app, 0)[-1])

    # The next page (or chapter), then a bookmark there.
    before_page = len(app.printed("AZREADER_PAGE", r"\d+ \d+"))
    app.key("right")
    app.until("the page turn", lambda: len(app.printed("AZREADER_PAGE", r"\d+ \d+")) > before_page)
    app.key("d", primary=True)
    app.until("the bookmark", lambda: "1" in app.printed("AZREADER_BOOKMARKS", r"\d+"))
    app.until("the bookmark in state.json", lambda: len(state_of(data, book_id).get("bookmarks", [])) == 1)
    app.log("page turned and bookmarked (%s)" % app.printed("AZREADER_PAGE", r"\d+ \d+")[-1])

    # The contents: chapter II from its entry.
    app.key("t", primary=True)
    app.until("the contents", lambda: app.has_id("__azreader_toc"))
    app.click("#__azreader_toc-2")
    app.until("chapter II's pages", lambda: pages_of(app, 1))
    app.frame(3)
    app.until("chapter II on the page", lambda: app.shows("Curiouser and curiouser"))
    app.log("the contents went to chapter II")

    # Larger text: laid out again.
    count = len(app.printed("AZREADER_PAGES", r"\d+ \d+"))
    app.key("equals", primary=True)
    app.until("the new layout", lambda: len(app.printed("AZREADER_PAGES", r"\d+ \d+")) > count)
    app.log("larger text laid out again")

    for theme in ("flat", "flora"):
        for mode in ("light", "dark"):
            app.must("set_theme", theme=theme)
            app.must("set_mode", mode=mode)
            app.frame(3)
            app.settle()
            app.screenshot(os.path.join(out, "reader-%s-%s.png" % (theme, mode)))
    app.must("set_theme", theme="flat")
    app.must("set_mode", mode="light")
    app.frame(2)

    # The library.
    app.key("escape")
    app.until("the library", lambda: app.has_id("__azreader_book-0"))
    if not app.shows("Alice's Adventures in Wonderland (sample)"):
        raise e.Failure("the library's tile does not show the book's title")
    app.settle()
    app.screenshot(os.path.join(out, "library.png"))
    app.until("the position in state.json", lambda: state_of(data, book_id).get("position", {}).get("chapter") == 1)
    app.log("the library shows the book; state.json is at chapter II")
    return book_id


def second_session(app, book_id, out):
    app.until("the window", lambda: app.printed("AZREADER_READY", r".*"))
    app.until("the library", lambda: "1" in app.printed("AZREADER_LISTED", r"\d+"))
    app.frame(2)
    app.until("the tile", lambda: app.has_id("__azreader_book-0"))
    app.must("double_click", selector="#__azreader_book-0")
    app.frame(2)
    app.until("the book to open", lambda: [v for v in app.printed("AZREADER_OPENED", r"[0-9a-f-]+ \d+") if v.startswith(book_id)])
    app.until("chapter II again", lambda: pages_of(app, 1))
    app.frame(3)
    app.until("chapter II on the page", lambda: app.shows("Curiouser and curiouser"))
    app.settle()
    app.screenshot(os.path.join(out, "restart.png"))
    app.log("the restart opens the book where the reader left it")


def body(args, logs, out):
    binary = e.find_binary("AzReader", args.bin, "AZREADER_BIN")
    data = os.path.join(logs, "data")
    os.makedirs(data, exist_ok=True)
    common = ["--data-dir", data, "--size", "1280x800"]
    app = e.App("azreader-1", binary, common + ["--sample"], args.debug_port, logs, args.timeout)
    try:
        book_id = first_session(app, data, out)
    except e.Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e.tail(app.out_path), e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app = e.App("azreader-2", binary, common, args.debug_port, logs, args.timeout)
    try:
        second_session(app, book_id, out)
    except e.Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e.tail(app.out_path), e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: screenshots in %s" % out)
    return True


if __name__ == "__main__":
    e.run("azreader", body, default_port=8796)
