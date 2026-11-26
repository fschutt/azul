#!/usr/bin/env python3
"""AzReview end to end: ink on a page, the session archive in the data tree.

    1. writes a small folder to review (`src/lib.rs`, 120 lines; `README.md`) and starts
       `AzReview <folder>` headless (AZ_BACKEND=headless, the debug server on --debug-port) on a
       fresh data folder (`--data-dir`); it opens the first file (`AZREVIEW_FILE README.md`);
    2. the window is filled: the sheet strip reaches down to the window's bottom (there is no
       status bar);
    3. the file tree is the first column at the start, beside the pages (the sheet strip starts where the tree
       ends: the tree pushes the pages to the right, it does not cover them), the first file is open; the
       switch next to the traffic lights slides it away and back; a click on `lib.rs` in it opens it
       (`AZREVIEW_FILE src/lib.rs`) and the tree stays as it is, the pages where they were; the top bar has a
       labeled Open folder... button and names the folder, and a window started with no folder says so;
    4. a stroke drawn with the mouse on the first page is kept (`AZREVIEW_STROKES 1`) and the
       session is written on a Thread to review/src_lib.rs.azreview.zip (`AZREVIEW_SAVED ...`):
       a zip whose session.json holds the stroke;
    5. Mod+S and the Save button write it again; a meaning the marker cannot write (key 3) takes the
       pen, key 1 the marker again; Mod+Z takes the stroke back (the archive is written again, now
       without it) and Mod+Shift+Z brings it again; the record button mounts the microphone widget
       while a clip records and unmounts it on stop; the files have an Open folder button;
    6. the window names every control to a screen reader: azul prints no `[azul][a11y]` lint line;
    7. the gear opens azul-appkit's settings page (AzReview's own Dictation section: which pointers
       start a note, how long the pen rests, how long the quiet lasts; a choice is saved in
       settings.json), About the standard About box (`AZREVIEW_ABOUT open`), Escape closes the box,
       Escape the page;
    8. flora / dark: a screenshot (the chrome follows, the pages stay paper);
    9. the dictation, on a second AzReview (`--dwell-ms 400 --dwell-input any`: the mouse stands in
       for the pen, whose state this driver cannot inject): the pointer resting on a page starts a
       note that listens - a marker on the page, the note and the "listening" pill in the side
       column. A libazul built with `test-hooks` hears `AZ_SPEECH_HEARD` (the fake speech backend, run
       with AZ_LINK_PATH naming it so the script finds the marker): the words arrive in the column;
       a stroke, the stop button, Escape, a second rest and the pointer moving away each end the
       note; a tip held down without moving (contact dwell) starts one and lifting it in place
       neither ends it nor cycles the nib; the note is saved in the archive and can be deleted.
       Without the test hooks the real recogniser is asked and refuses (an unbundled binary has no
       Info.plist): the message is told over the pages, the empty note is not kept, a second rest
       starts nothing. No `[azul][a11y]` lint line may print in either run;
   10. the restart: a second AzReview on the same folder and data folder gets the saved session back
       when it opens the file (`AZREVIEW_LOADED 1 0`) and adds its next stroke to it.

Run ONE app at a time, through the capped runner:

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/azreview.log -- \\
      python3 scripts/azreview_e2e.py --bin target/release/AzReview --out /tmp/azreview-shots

The debug-server client is the shared one (scripts/azlin_e2e.py).
"""

import json
import os
import re
import time
import zipfile

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azreview"
WIDTH, HEIGHT = 1280, 820
STRIP = "__azreview_sheet-strip"
TREE = "__azreview_tree"
SIDEBAR_TOGGLE = "__azreview_sidebar-toggle"


def state(app):
    """What AzReview says its state is (the debug op `azreview_state`): pen, meaning, nib, panels."""
    answer = app.must("azreview_state")
    return answer["data"]["value"]["result"]["state"]


def settled(app, node_id, tries=40):
    """The rect of `node_id` once it stops moving (a panel slides for about 180 ms)."""
    last = None
    for _ in range(tries):
        now = app.rect(node_id)
        if now is not None and now == last:
            return now
        last = now
        time.sleep(0.1)
    raise Failure("%s keeps moving: %s" % (node_id, last))


def show_files(app):
    """The tree is out at the start; if it is not, the switch next to the traffic lights slides it out."""
    if not app.has_id(TREE):
        app.click(selector="#" + SIDEBAR_TOGGLE)
    app.until("the file tree is out", lambda: app.has_id(TREE))
ARCHIVE = "review/src_lib.rs.azreview.zip"


def fixture(root):
    os.makedirs(os.path.join(root, "src"), exist_ok=True)
    with open(os.path.join(root, "src", "lib.rs"), "w", encoding="utf-8") as f:
        for i in range(120):
            f.write("pub fn step_%d(x: u32) -> u32 { x + %d }\n" % (i, i))
    with open(os.path.join(root, "README.md"), "w", encoding="utf-8") as f:
        f.write("# A folder to review\n\nTwo files.\n")


def read_session(path):
    try:
        with zipfile.ZipFile(path) as z:
            return json.loads(z.read("session.json").decode("utf-8"))
    except (OSError, zipfile.BadZipFile, KeyError, ValueError):
        return {}


def saves(app):
    return app.printed("AZREVIEW_SAVED", r".+")


def body(args, logs, out):
    binary = e2e.find_binary("AzReview", args.bin, "AZREVIEW_BIN")
    data_dir = os.path.join(logs, "data")
    folder = os.path.join(logs, "folder")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)

    app = e2e.App("review", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en"],
                  args.debug_port, logs, args.timeout)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        app.frame(2)
        app.expect_line("AZREVIEW_FILE", "README.md", "the first file opens")
        strip = app.rect(STRIP)
        if not strip or strip.get("height", 0) < 300 or strip.get("y", 0) + strip.get("height", 0) > HEIGHT:
            raise Failure("the sheet strip does not fill the document: %s" % strip)
        app.screenshot(os.path.join(out, "1-readme.png"))
        # The file tree is out at the start, beside the pages: it pushes them to its right.
        if not app.has_id(TREE):
            raise Failure("the file tree is not out at the start")
        tree = app.rect(TREE)
        if not tree or tree.get("width", 0) <= 0:
            raise Failure("the file tree has no width: %s" % tree)
        if strip["x"] < tree["x"] + tree["width"] - 1.0:
            raise Failure("the tree covers the pages instead of pushing them: tree %s, strip %s" % (tree, strip))
        # The top bar says which folder it is and has the way to another one, in words.
        if not app.has_id("__azreview_open-folder-bar") or not app.has_id("__azreview_folder-chip"):
            raise Failure("the top bar has no Open folder... button or no folder name")
        # The switch slides the tree away and back; the pages follow it.
        under = strip
        app.click(selector="#" + SIDEBAR_TOGGLE)
        app.until("the tree is away", lambda: not app.has_id(TREE))
        away = settled(app, STRIP)
        if away["x"] >= under["x"] - 1.0:
            raise Failure("the pages did not move left when the tree slid away: %s -> %s" % (under, away))
        show_files(app)
        back = settled(app, STRIP)
        if abs(back["x"] - under["x"]) > 1.0:
            raise Failure("the pages did not come back with the tree: %s -> %s" % (under, back))

        # Open lib.rs from the tree: it opens and the tree stays.
        app.click(text="lib.rs")
        app.expect_line("AZREVIEW_FILE", os.path.join("src", "lib.rs"), "a click opens lib.rs")
        app.frame(2)
        if not app.has_id(TREE):
            raise Failure("a pick of a file folded the tree")
        folded = app.rect(STRIP)
        if abs(folded.get("x", 0) - under["x"]) > 1.0:
            raise Failure("the pages moved when a file was picked: %s -> %s" % (under, folded))

        # Draw a stroke on the first page.
        strip = app.rect(STRIP)
        x0, y0 = strip["x"] + 180.0, strip["y"] + 120.0
        app.must("mouse_move", x=x0, y=y0)
        app.must("mouse_down", x=x0, y=y0)
        for i in range(1, 13):
            app.must("mouse_move", x=x0 + i * 18.0, y=y0 + i * 3.0)
            app.frame(1)
        app.must("mouse_up", x=x0 + 12 * 18.0, y=y0 + 36.0)
        app.frame(2)
        app.expect_line("AZREVIEW_STROKES", "1", "the stroke is kept")
        app.until("the session written", lambda: ARCHIVE in saves(app))
        path = os.path.join(data_dir, ARCHIVE)
        with zipfile.ZipFile(path) as z:
            session = json.loads(z.read("session.json").decode("utf-8"))
        if session.get("file") != os.path.join("src", "lib.rs") or len(session.get("strokes", [])) != 1:
            raise Failure("the archive does not hold the stroke: %s" % {k: session.get(k) for k in ("file", "strokes")})
        # Every move between press and release is a point of the stroke (twelve moves 18 px apart):
        # movement is MouseMove, MouseOver fires only when the pointer enters the page.
        points = len(session["strokes"][0].get("points", []))
        if points < 12:
            raise Failure("the stroke kept %d point(s) of the 13 drawn: the moves between press and release are lost" % points)
        app.log("archive %s: %d stroke(s)" % (ARCHIVE, len(session["strokes"])))
        app.screenshot(os.path.join(out, "2-stroke.png"))

        # Mod+S and the Save button write it again.
        before = len(saves(app))
        app.key("s", primary=True)
        app.until("Mod+S writes", lambda: len(saves(app)) > before)
        before = len(saves(app))
        app.click(selector="#__azreview_save")
        app.until("Save writes", lambda: len(saves(app)) > before)

        # A meaning the marker cannot write (key 3, a question) takes the pen; scope (key 1) the marker again.
        app.key("3")
        app.until("the question pen with its pen nib",
                  lambda: (state(app)["meaning"], state(app)["nib"]) == ("question", "pen"))
        app.key("3")
        app.until("its next nib, the audio pen", lambda: state(app)["nib"] == "audio pen")
        app.key("1")
        app.until("the scope pen with the marker",
                  lambda: (state(app)["meaning"], state(app)["nib"]) == ("scope", "marker"))

        # Mod+Z takes the stroke back, Mod+Shift+Z brings it again; each is written to the archive.
        app.key("z", primary=True)
        app.until("the archive without the stroke",
                  lambda: len(read_session(os.path.join(data_dir, ARCHIVE)).get("strokes", [])) == 0)
        app.key("z", primary=True, shift=True)
        app.until("the archive with the stroke again",
                  lambda: len(read_session(os.path.join(data_dir, ARCHIVE)).get("strokes", [])) == 1)

        # Record mounts the microphone widget (the clip's samples come from it) and stopping removes it.
        app.click(selector="#__azreview_record")
        app.until("the microphone mounted", lambda: app.has_id("__azreview_microphone", every_dom=True)
                  or app.has_id("__azreview_microphone"))
        app.click(selector="#__azreview_record")
        app.until("the microphone gone", lambda: not app.has_id("__azreview_microphone"))

        # The window's own controls name themselves to a screen reader (azul's lint says when not).
        no_lint(app, "the review window")

        # Settings and About.
        app.click(selector="#__azreview_settings")
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        # AzReview's own category comes first: Dictation.
        for row in ("__azreview_setting-input", "__azreview_setting-rest", "__azreview_setting-quiet"):
            app.until("the dictation setting %s" % row, lambda row=row: app.has_id(row))
        app.click_within("#__azreview_setting-rest", "2 s")
        settings_path = os.path.join(data_dir, "review", "settings.json")
        app.until("the rest time saved",
                  lambda: os.path.exists(settings_path) and '"dictation-rest-ms": "2000"' in open(settings_path).read())
        app.screenshot(os.path.join(out, "3a-settings.png"))
        app.click(text="About")
        app.until("the About section", lambda: app.has_id("appkit-about-open"))
        app.click(selector="#appkit-about-open")
        app.expect_line("AZREVIEW_ABOUT", "open", "the About box opens")
        app.screenshot(os.path.join(out, "3-about.png"))
        app.key("escape")
        app.expect_line("AZREVIEW_ABOUT", "closed", "Escape closes the About box first")
        app.key("escape")
        app.until("the pages again", lambda: not app.has_id("appkit-settings") and app.has_id(STRIP))

        # Flora / dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.screenshot(os.path.join(out, "4-flora-dark.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: files, ink, the session archive, settings, About; screenshots in %s" % out)
    return True


HEARD = "this loop is off by one"
HOOKS_MARKER = b"AZLIN-BUILD-MARKER:test-hooks"


def fake_speech():
    """Whether the libazul the app loads has the test hooks (the fake speech backend)."""
    found = e2e.libazul_candidates()
    if not found:
        return False
    import mmap
    with open(found[0], "rb") as lib:
        with mmap.mmap(lib.fileno(), 0, access=mmap.ACCESS_READ) as data:
            return data.find(HOOKS_MARKER) >= 0


def no_lint(app, what):
    """No `[azul][a11y]` or `[azul][a11y-shape]` lint line on the app's output."""
    lines = []
    for path in (app.out_path, app.err_path):
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                lines += [l for l in f.read().splitlines() if "[azul][a11y" in l]
        except OSError:
            pass
    if lines:
        raise Failure("%s: azul's accessibility lint printed: %s" % (what, lines[:3]))


def dictation(args, logs, out, binary, fake):
    data_dir = os.path.join(logs, "data-dictation")
    folder = os.path.join(logs, "folder-dictation")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    extra = {"AZ_SPEECH_HEARD": HEARD} if fake else {}
    app = e2e.App("review-dictation", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-ms", "400", "--dwell-input", "any", "--silence-ms", "6000"],
                  args.debug_port + 1, logs, args.timeout, extra_env=extra)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        app.frame(2)
        show_files(app)
        app.click(text="lib.rs")
        app.frame(3)
        strip = app.rect(STRIP)
        base = (strip["x"] + 160.0, strip["y"] + 140.0)

        def rest_at(dx, dy, what, n):
            """The pointer moves to a new spot and stays: a dictation starts there."""
            x, y = base[0] + dx, base[1] + dy
            app.must("mouse_move", x=x, y=y)
            app.frame(1)
            app.until(what, lambda: (app.frame(1), app.count("AZREVIEW_DICTATION", r"started %d" % n) > 0)[1])

        def ended(n, cause):
            app.until("dictation %d ended (%s)" % (n, cause),
                      lambda: (app.frame(1), app.count("AZREVIEW_DICTATION", r"ended %d %s" % (n, cause)) > 0)[1])

        # 1. A rest starts a note that listens.
        rest_at(0.0, 0.0, "the first dictation to start", 1)
        app.until("the side column", lambda: app.has_id("__azreview_notes"))
        if fake:
            app.until("the marker on the page", lambda: app.has_id("__azreview_marker-1", every_dom=True))
            app.until("the words in the column", lambda: app.shows(HEARD))
            app.until("the listening pill", lambda: app.has_id("__azreview_listening"))
            app.until("the stop button", lambda: app.has_id("__azreview_dictation-stop"))
            app.screenshot(os.path.join(out, "5-listening.png"))
            # A stroke elsewhere on the page ends it.
            app.must("mouse_move", x=base[0] + 200.0, y=base[1] + 100.0)
            app.must("mouse_down", x=base[0] + 200.0, y=base[1] + 100.0)
            for i in range(1, 8):
                app.must("mouse_move", x=base[0] + 200.0 + i * 12.0, y=base[1] + 100.0 + i * 3.0)
                app.frame(1)
            app.must("mouse_up", x=base[0] + 200.0 + 84.0, y=base[1] + 121.0)
            app.frame(2)
            ended(1, "(?:moved|drew)")
            app.until("the note editable", lambda: app.has_id("__azreview_note-text-1"))
            app.screenshot(os.path.join(out, "6-note.png"))
            path = os.path.join(data_dir, ARCHIVE)
            app.until("the archive with the note", lambda: os.path.exists(path) and notes_in(path))
            notes = notes_in(path)
            if HEARD not in notes[0].get("text", ""):
                raise Failure("the archive's note does not hold the words: %s" % notes[0])
            if notes[0].get("page") != 0:
                raise Failure("the note's page: %s" % notes[0])

            # 2. The stop button.
            rest_at(0.0, 260.0, "the second dictation to start", 2)
            app.until("the stop button", lambda: app.has_id("__azreview_dictation-stop"))
            app.click(selector="#__azreview_dictation-stop")
            ended(2, "button")
            # 3. Escape.
            rest_at(0.0, 0.0, "the third dictation to start", 3)
            app.key("escape")
            ended(3, "key")
            # 4. A second rest, a little way off: the second dwell.
            rest_at(0.0, 100.0, "the fourth dictation to start", 4)
            app.must("mouse_move", x=base[0] + 20.0, y=base[1] + 100.0)
            app.frame(1)
            ended(4, "second-dwell")
            # 5. The pointer moving far away.
            rest_at(0.0, 300.0, "the fifth dictation to start", 5)
            app.must("mouse_move", x=base[0] + 400.0, y=base[1] + 300.0)
            app.frame(2)
            ended(5, "moved")
            # The pointer stays where it moved to: that rest starts a sixth note by itself. Escape ends it.
            app.until("the resting pointer's note", lambda: (app.frame(1), app.count("AZREVIEW_DICTATION", r"started 6") > 0)[1])
            app.key("escape")
            ended(6, "key")
            # 6. Contact dwell: the tip stays down without moving after the move - a note starts; lifting it
            #    in place neither ends it nor changes the nib (a tap would cycle the nib).
            x, y = base[0] + 300.0, base[1] + 420.0
            app.must("mouse_move", x=x, y=y)
            app.must("mouse_down", x=x, y=y)
            app.until("the contact dwell's note", lambda: (app.frame(1), app.count("AZREVIEW_DICTATION", r"started 7") > 0)[1])
            app.must("mouse_up", x=x, y=y)
            app.frame(3)
            if "marker" not in app.texts():
                raise Failure("lifting the tip in place after a contact dwell cycled the nib: %s" % [t for t in app.texts() if t in ("marker", "pen", "audio pen")])
            if app.count("AZREVIEW_DICTATION", r"ended 7 .*") > 0:
                raise Failure("lifting the tip in place ended the contact dwell's note")
            app.key("escape")
            ended(7, "key")
            # The notes: five cards (each said something), all editable.
            app.until("five notes", lambda: all(app.has_id("__azreview_note-%d" % i) for i in (1, 2, 3, 4, 5)))
            # Delete one with its button.
            app.click(selector="#__azreview_note-delete-5")
            app.until("the note gone", lambda: not app.has_id("__azreview_note-5"))
            app.screenshot(os.path.join(out, "7-notes.png"))
        else:
            # The real recogniser refuses (the binary has no Info.plist, or no microphone is there):
            # the user is told over the pages, and the empty note is not kept.
            app.until("the refusal line", lambda: app.count("AZREVIEW_DICTATION", r"refused .*") > 0)
            ended(1, "failed")
            app.until("the refusal told over the pages",
                      lambda: app.shows("Dictation") or app.shows("speech recognition"))
            if app.has_id("__azreview_note-1"):
                raise Failure("a note that heard nothing was kept")
            # No recogniser at all: it cannot change before the next start, so a second rest starts nothing
            # (it says so again over the pages) instead of asking the system once more.
            app.must("mouse_move", x=base[0] + 300.0, y=base[1] + 300.0)
            app.frame(1)
            time.sleep(1.2)
            app.frame(2)
            if app.count("AZREVIEW_DICTATION", r"started 2") > 0:
                raise Failure("a rest started a dictation although the recogniser is unavailable")
            app.screenshot(os.path.join(out, "5-refused.png"))
        no_lint(app, "the dictation run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: dictation (%s)" % ("fake speech: words, stroke, stop, Escape, second rest, moving away, saved"
                                       if fake else "the real recogniser refused, and said so"))


def restore(args, logs, out, binary):
    """A second AzReview on the folder and the data folder of the first: the saved session comes back
    when its file is opened (`AZREVIEW_LOADED 1 0`: the stroke), and a stroke drawn now is added to it -
    the archive is not written over (the first launch after a session used to lose it)."""
    data_dir = os.path.join(logs, "data")
    folder = os.path.join(logs, "folder")
    path = os.path.join(data_dir, ARCHIVE)
    before = len(read_session(path).get("strokes", []))
    if before != 1:
        raise Failure("the first run left %d stroke(s) in its archive, expected 1" % before)
    app = e2e.App("review-restore", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-input", "off"],
                  args.debug_port + 2, logs, args.timeout)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        app.frame(2)
        show_files(app)
        app.click(text="lib.rs")
        app.frame(3)
        app.until("the saved session back", lambda: app.count("AZREVIEW_LOADED", r"1 0") > 0)
        strip = app.rect(STRIP)
        x0, y0 = strip["x"] + 200.0, strip["y"] + 220.0
        app.must("mouse_move", x=x0, y=y0)
        app.must("mouse_down", x=x0, y=y0)
        for i in range(1, 12):
            app.must("mouse_move", x=x0 + i * 14.0, y=y0 + (i % 4) * 3.0)
            app.frame(1)
        app.must("mouse_up", x=x0 + 12 * 14.0, y=y0)
        app.frame(2)
        app.until("the archive with both strokes", lambda: len(read_session(path).get("strokes", [])) == 2)
        ids = [st["id"] for st in read_session(path)["strokes"]]
        if len(set(ids)) != len(ids):
            raise Failure("the two strokes share an id: %s" % ids)
        no_lint(app, "the restored session")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: the saved session came back and a new stroke was added to it (ids %s)" % ids)


def notes_in(path):
    with zipfile.ZipFile(path) as z:
        return json.loads(z.read("session.json").decode("utf-8")).get("notes", [])


def run_all(args, logs, out):
    binary = e2e.find_binary("AzReview", args.bin, "AZREVIEW_BIN")
    body(args, logs, out)
    # The dictation steps need the fake speech backend (a libazul with the test hooks). Without it the real recogniser
    # would be asked, and where the terminal holds the microphone and speech permissions it LISTENS: a test must never
    # open the microphone, so the steps are skipped.
    if fake_speech():
        dictation(args, logs, out, binary, True)
    else:
        print("[azreview] SKIPPED: the dictation steps (this libazul has no test hooks: the real recogniser would listen "
              "to the microphone)", flush=True)
    restore(args, logs, out, binary)
    return True


if __name__ == "__main__":
    e2e.run(TAG, run_all, default_port=8776)
