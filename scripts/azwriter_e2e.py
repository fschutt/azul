#!/usr/bin/env python3
"""AzWriter end to end over the debug server (headless).

    1. starts AzWriter on an empty, temporary data folder with --sample: the sample document opens,
       is saved as writer/<uuid>.md and spans two A4 pages (a page break), each page an editing
       host of the shared editor (#__azwriter_doc-page-<first block>);
    2. types into the second block, saves (Ctrl/Cmd+S) and reads the FILE back;
    3. undoes the typing with Ctrl/Cmd+Z (the editor's ONE history, not the engine's text undo),
       saves, and the file no longer has the text;
    4. makes the caret's block a heading 2 (Ctrl/Cmd+2), saves: "## " in the file;
    5. exports a PDF (Ctrl/Cmd+P) into writer/exports/ and checks the bytes;
    6. opens File > Open (Ctrl/Cmd+O): the document is listed; Escape leaves it;
    7. types again and closes the window: the close guard asks "Save changes?", Save saves and
       closes; the file has the text;
    8. starts AzWriter again on the same folder, opens the document from File > Open and finds the
       text; screenshots in flat / flora x light / dark.

Usage (from the azul repository, after building libazul with the debug server and AzWriter):

    python3 scripts/azwriter_e2e.py [--bin target/release/AzWriter] [--debug-port 8794]
        [--timeout 180] [--out <dir>] [--keep]

Run it through the capped runner (scripts/waves/tools/run_capped.sh) on a small machine.
"""

import glob
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as e  # noqa: E402

PAGE0 = "#__azwriter_doc-page-0"
BLOCK = "#__azwriter_doc-%d"


def read(path):
    try:
        with open(path, "r", encoding="utf-8") as f:
            return f.read()
    except OSError:
        return ""


def doc_file(data, doc_id):
    return os.path.join(data, "writer", "%s.md" % doc_id)


def saved_count(app, doc_id):
    return len([v for v in app.printed("AZWRITER_SAVED", r"[0-9a-f-]+") if v == doc_id])


DIALOG_BUTTONS = "__azul-native-standard-dialog-buttons"


def click_dialog_button(app, label):
    """Clicks the button `label` of the standard dialog showing (the text "Save" is in the
    document too, so a click by text could land in the page)."""
    answer = app.op("get_node_hierarchy")
    nodes = [d for d in e.dicts(answer) if "index" in d and "parent" in d]
    by_index = {d["index"]: d for d in nodes}

    def in_buttons(node):
        seen = 0
        while node is not None and seen < 64:
            if DIALOG_BUTTONS in (node.get("classes") or []):
                return True
            node = by_index.get(node.get("parent"))
            seen += 1
        return False

    for node in nodes:
        if (node.get("text") or "").strip() == label and in_buttons(node):
            # A text node may have no box of its own: click its nearest ancestor that has one.
            target = node
            while target is not None and not target.get("rect"):
                target = by_index.get(target.get("parent"))
            app.must("click", node_id=(target or node)["index"])
            app.frame(2)
            return
    raise e.Failure("no dialog button %r" % label)


def save_and_wait(app, doc_id):
    before = saved_count(app, doc_id)
    app.key("s", primary=True)
    app.until("the save of %s" % doc_id, lambda: saved_count(app, doc_id) > before)


def first_session(app, data, out):
    app.until("the window", lambda: app.printed("AZWRITER_READY", r".*"))
    app.must("resize", width=1280, height=800)
    app.frame(2)
    doc_id = app.until("the sample document", lambda: (app.printed("AZWRITER_OPENED", r"[0-9a-f-]+") or [None])[-1])
    app.until("the sample's first save", lambda: saved_count(app, doc_id) >= 1)
    text = read(doc_file(data, doc_id))
    if "# Welcome to AzWriter" not in text:
        raise e.Failure("writer/%s.md lacks the sample's heading:\n%s" % (doc_id, text))
    app.log("sample saved as writer/%s.md" % doc_id)

    # Two pages: the pagination answers, the second page's host is there.
    app.until("the pagination", lambda: [int(n) for n in app.printed("AZWRITER_PAGES", r"\d+") if int(n) >= 2])
    app.frame(2)
    if not app.has_id(PAGE0[1:]):
        raise e.Failure("no first page host %s" % PAGE0)
    app.log("pages laid out (%s)" % app.printed("AZWRITER_PAGES", r"\d+")[-1])

    # Type into the second block (the first paragraph), save, read the file.
    app.click(BLOCK % 1)
    app.must("text_input", text="QUILL ")
    app.frame(2)
    save_and_wait(app, doc_id)
    if "QUILL" not in read(doc_file(data, doc_id)):
        raise e.Failure("the typed text is not in the saved file:\n%s" % read(doc_file(data, doc_id)))
    app.log("typing saved")

    # Ctrl/Cmd+Z: the editor's own history takes the typing back.
    app.key("z", primary=True)
    app.frame(2)
    save_and_wait(app, doc_id)
    if "QUILL" in read(doc_file(data, doc_id)):
        raise e.Failure("Ctrl/Cmd+Z did not undo the typing in the document")
    app.log("undo is the editor's history")

    # Ctrl/Cmd+2: the caret's block becomes a heading 2.
    app.click(BLOCK % 1)
    app.key("2", primary=True)
    app.frame(2)
    save_and_wait(app, doc_id)
    if "\n## AzWriter keeps" not in read(doc_file(data, doc_id)):
        raise e.Failure("Ctrl/Cmd+2 made no heading:\n%s" % read(doc_file(data, doc_id)))
    app.key("z", primary=True)
    app.log("heading 2 and back")

    # Export as PDF into the data tree.
    before = len(app.printed("AZWRITER_EXPORTED", r".+"))
    app.key("p", primary=True)
    app.until("the PDF export", lambda: len(app.printed("AZWRITER_EXPORTED", r".+")) > before)
    pdfs = glob.glob(os.path.join(data, "writer", "exports", "*.pdf"))
    if not pdfs or not open(pdfs[0], "rb").read(5) == b"%PDF-":
        raise e.Failure("no PDF in writer/exports: %s" % pdfs)
    app.log("exported %s" % os.path.basename(pdfs[0]))

    # File > Open lists the document.
    app.key("o", primary=True)
    app.until("the document list", lambda: app.has_id("__azwriter_open-0"))
    if not app.shows("Welcome to AzWriter"):
        raise e.Failure("File > Open does not list the document")
    app.screenshot(os.path.join(out, "backstage-open.png"))
    app.key("escape")
    app.until("the pages again", lambda: app.has_id(PAGE0[1:]))

    # Looks.
    for theme in ("flat", "flora"):
        for mode in ("light", "dark"):
            app.must("set_theme", theme=theme)
            app.must("set_mode", mode=mode)
            app.frame(3)
            app.screenshot(os.path.join(out, "%s-%s.png" % (theme, mode)))
    app.must("set_theme", theme="flat")
    app.must("set_mode", mode="light")
    app.frame(2)

    # Unsaved typing, then close: the guard asks, Save saves and closes.
    app.click(BLOCK % 1)
    app.must("text_input", text="CLOSING ")
    app.frame(2)
    before = saved_count(app, doc_id)
    app.op("close")
    app.frame(2)
    app.until("the save-changes question", lambda: app.shows("Save changes"))
    app.screenshot(os.path.join(out, "close-guard.png"))
    click_dialog_button(app, "Save")
    app.until("the save on close", lambda: saved_count(app, doc_id) > before)
    end = time.time() + 20
    while app.process.poll() is None and time.time() < end:
        time.sleep(0.25)
    if app.process.poll() is None:
        raise e.Failure("the window did not close after Save")
    if "CLOSING" not in read(doc_file(data, doc_id)):
        raise e.Failure("the save on close did not write the text")
    app.log("the close guard saved and closed")
    return doc_id


def second_session(app, doc_id, out):
    app.until("the window", lambda: app.printed("AZWRITER_READY", r".*"))
    app.until("the list", lambda: app.printed("AZWRITER_LISTED", r"\d+"))
    app.key("o", primary=True)
    app.until("the document list", lambda: app.has_id("__azwriter_open-0"))
    app.click("#__azwriter_open-0")
    app.until("the document to open", lambda: doc_id in app.printed("AZWRITER_OPENED", r"[0-9a-f-]+"))
    app.frame(3)
    app.until("the saved text after the restart", lambda: app.shows("CLOSING"))
    app.screenshot(os.path.join(out, "restart.png"))
    app.log("the restart reads the document back from its file")


def body(args, logs, out):
    binary = e.find_binary("AzWriter", args.bin, "AZWRITER_BIN")
    data = os.path.join(logs, "data")
    os.makedirs(data, exist_ok=True)
    common = ["--data-dir", data, "--size", "1280x800"]
    app = e.App("azwriter-1", binary, common + ["--sample"], args.debug_port, logs, args.timeout)
    try:
        doc_id = first_session(app, data, out)
    except e.Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e.tail(app.out_path), e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app = e.App("azwriter-2", binary, common, args.debug_port, logs, args.timeout)
    try:
        second_session(app, doc_id, out)
    except e.Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e.tail(app.out_path), e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: screenshots in %s" % out)
    return True


if __name__ == "__main__":
    e.run("azwriter", body, default_port=8794)
