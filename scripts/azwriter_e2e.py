#!/usr/bin/env python3
"""AzWriter end to end over the debug server (headless).

    1. starts AzWriter on an empty, temporary data folder with --sample: the sample document opens,
       is saved as writer/<uuid>.md and spans two A4 pages (a page break), each page an editing
       host of the shared editor (#__azwriter_doc-page-<first block>);
    2. the first page has the keyboard as it opens (its autofocus); types into the second block, saves (Ctrl/Cmd+S) and reads the FILE back;
    3. undoes the typing with Ctrl/Cmd+Z (the editor's ONE history, not the engine's text undo),
       saves, and the file no longer has the text;
    4. makes the caret's block a heading 2 (Ctrl/Cmd+2), saves: "## " in the file;
    5. exports a PDF (Ctrl/Cmd+P) into writer/exports/ and checks the bytes;
    6. opens File > Open (Ctrl/Cmd+O): the document is listed; Escape leaves it;
    7. types again and closes the window: the close guard asks "Save changes?"; Cancel keeps the
       window and the typing, nothing is written; the next close asks again, Save saves and
       closes; the file has the text;
    8. starts AzWriter again on the same folder, opens the document from File > Open and finds the
       text; screenshots in flat / flora x light / dark;
    9. types once more and closes: Don't Save closes the window (and the app) without writing,
       the file keeps what it had;
   10. starts AzWriter in German (--language de) on the same folder: the File tab says DATEI,
       and a close over unsaved typing asks in German ("... speichern?", Speichern / Nicht
       speichern / Abbrechen - no English word); Abbrechen keeps the window.

Usage (from the azul repository, after building libazul with the debug server and AzWriter):

    python3 scripts/azwriter_e2e.py [--bin target/release/AzWriter] [--debug-port 8794]
        [--timeout 180] [--out <dir>] [--keep]

Run it through the capped runner (scripts/waves/tools/run_capped.sh) on a small machine.
"""

import glob
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as e  # noqa: E402

PAGE0 = "#__azwriter_doc-page-0"


def first_page_shown(app):
    """Whether the first page's host is there: `<doc>-page-0`, with `~<block>@<byte>` after it
    when the page ends inside a paragraph (the next page goes on with its tail)."""
    return any(re.fullmatch(r"__azwriter_doc-page-0(~\d+@\d+)?", n.get("id") or "")
               for n in app.hierarchy())
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
    if not first_page_shown(app):
        raise e.Failure("no first page host %s" % PAGE0)
    app.log("pages laid out (%s)" % app.printed("AZWRITER_PAGES", r"\d+")[-1])

    # The first page has the keyboard as the document opens (its autofocus): the caret is in
    # the text without a click.
    def focused():
        answer = app.must("get_focus_state", seat=0)
        data_ = answer.get("data") if isinstance(answer, dict) else None
        value = data_.get("value") if isinstance(data_, dict) and "value" in data_ else data_
        return ((value or {}).get("focused_node") or {}).get("selector") or ""
    app.until("the first page to take the keyboard", lambda: "doc-page-0" in focused())
    app.log("the document opens with the caret in its first page")

    # HOME fits a 1280 px window: the styles gallery shows a row of a few
    # styles (More shows all), so the Editing group's Undo is on screen.
    # The label's box: `get_node_layout text=` answers the text node, which has no rect.
    rect = app.text_rect("Undo")
    if not rect or rect.get("x", 0) + rect.get("width", 0) > 1280:
        raise e.Failure("HOME runs out of the 1280 px window: Undo at %s" % rect)
    app.log("HOME fits the window (Undo at x %.0f)" % rect["x"])

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
    app.until("the pages again", lambda: first_page_shown(app))

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

    # Unsaved typing, then close: the guard asks. Cancel keeps the window and the typing.
    app.click(BLOCK % 1)
    app.must("text_input", text="CLOSING ")
    app.frame(2)
    before = saved_count(app, doc_id)
    app.op("close")
    app.frame(2)
    app.until("the save-changes question", lambda: app.shows("Save changes"))
    app.screenshot(os.path.join(out, "close-guard.png"))
    e.click_dialog_button(app, "Cancel")
    app.until("the question to go", lambda: not app.shows("Save changes"))
    app.frame(2)
    if not app.alive():
        raise e.Failure("Cancel closed the window")
    if not app.shows("CLOSING"):
        raise e.Failure("Cancel lost the typing")
    if saved_count(app, doc_id) != before or "CLOSING" in read(doc_file(data, doc_id)):
        raise e.Failure("Cancel wrote the document")
    app.log("Cancel keeps the window and the unsaved typing")

    # The next close asks again: Save saves and closes.
    app.op("close")
    app.frame(2)
    app.until("the save-changes question again", lambda: app.shows("Save changes"))
    e.click_dialog_button(app, "Save", closes=True)
    # The save runs on a thread and its write-back closes the window - and the app: wait for
    # the exit (`until` gives up on an exited app), then read what it printed.
    if not e.exits(app):
        raise e.Failure("the window did not close after Save")
    if saved_count(app, doc_id) <= before:
        raise e.Failure("the window closed without the save on close")
    if "CLOSING" not in read(doc_file(data, doc_id)):
        raise e.Failure("the save on close did not write the text")
    app.log("the close guard saved and closed")
    return doc_id


def second_session(app, data, doc_id, out):
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

    # Unsaved typing, then close: Don't Save closes the window without writing.
    on_disk = read(doc_file(data, doc_id))
    before = saved_count(app, doc_id)
    app.click(BLOCK % 1)
    app.must("text_input", text="DISCARDED ")
    app.frame(2)
    app.op("close")
    app.frame(2)
    app.until("the save-changes question", lambda: app.shows("Save changes"))
    e.click_dialog_button(app, "Don't Save", closes=True)
    if not e.exits(app):
        asked = app.shows("Save changes")
        raise e.Failure("the window did not close after Don't Save (the question %s)"
                        % ("is back" if asked else "is gone"))
    if saved_count(app, doc_id) != before or read(doc_file(data, doc_id)) != on_disk:
        raise e.Failure("Don't Save wrote the document")
    app.log("Don't Save closes without writing")


def german_session(app, doc_id, out):
    app.until("the window", lambda: app.printed("AZWRITER_READY", r".*"))
    app.until("the File tab in German", lambda: app.shows("DATEI"))
    app.until("the list", lambda: app.printed("AZWRITER_LISTED", r"\d+"))
    app.key("o", primary=True)
    app.until("the document list", lambda: app.has_id("__azwriter_open-0"))
    app.click("#__azwriter_open-0")
    app.until("the document to open", lambda: doc_id in app.printed("AZWRITER_OPENED", r"[0-9a-f-]+"))
    app.frame(3)
    app.click(BLOCK % 1)
    app.must("text_input", text="DEUTSCH ")
    app.frame(2)
    app.op("close")
    app.frame(2)
    app.until("the save-changes question in German", lambda: app.shows("speichern?"))
    app.screenshot(os.path.join(out, "close-guard-de.png"))
    for word in ("Nicht speichern", "Abbrechen"):
        if not app.shows(word):
            raise e.Failure("the German close question has no %r button" % word)
    for word in ("Save changes", "Don't Save"):
        if app.shows(word):
            raise e.Failure("the close question of a German window says %r" % word)
    e.click_dialog_button(app, "Abbrechen")
    app.until("the question to go", lambda: not app.shows("speichern?"))
    if not app.alive():
        raise e.Failure("Abbrechen closed the window")
    app.log("German: the File tab says DATEI, the close question and its buttons are German")


def body(args, logs, out):
    binary = e.find_binary("AzWriter", args.bin, "AZWRITER_BIN")
    data = os.path.join(logs, "data")
    os.makedirs(data, exist_ok=True)
    # The checks read English words (the system may be German).
    common = ["--data-dir", data, "--size", "1280x800", "--language", "en"]
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
        second_session(app, data, doc_id, out)
    except e.Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e.tail(app.out_path), e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    german = ["--data-dir", data, "--size", "1280x800", "--language", "de"]
    app = e.App("azwriter-3", binary, german, args.debug_port, logs, args.timeout)
    try:
        german_session(app, doc_id, out)
    except e.Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e.tail(app.out_path), e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: screenshots in %s" % out)
    return True


if __name__ == "__main__":
    e.run("azwriter", body, default_port=8794)
