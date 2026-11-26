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

import glob
import json
import os
import re
import subprocess
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


def pick(app, folder, name):
    """Opens `name` from the tree, opening its `folder` first: the tree lists a folder when it is opened, and its rows
    are a VirtualView's (a DOM of their own)."""
    if not app.shows(name, every_dom=True):
        app.click(text=folder, every_dom=True)
        app.until("the files of %s" % folder, lambda: app.shows(name, every_dom=True))
    app.click(text=name, every_dom=True)


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


def read_text(path):
    """A text file as it is now ('' where it is not there yet)."""
    try:
        with open(path, encoding="utf-8", newline="") as f:
            return f.read()
    except OSError:
        return ""


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
        pick(app, "src", "lib.rs")
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
    extra = {}
    if fake:
        # what the recogniser hears: one utterance, then quiet (the engine's stream walks the steps of a script; the old
        # AZ_SPEECH_HEARD answers at once on every call, which a stream would take as utterance after utterance without a pause)
        script = os.path.join(logs, "speech-dictation.json")
        with open(script, "w", encoding="utf-8") as f:
            json.dump({"builtin": [{"say": HEARD, "delay_ms": 300, "language": "en"}, {"silence": True}], "then": "silence"}, f)
        extra = {"AZ_SPEECH_SCRIPT": script}
    app = e2e.App("review-dictation", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-ms", "400", "--dwell-input", "any", "--silence-ms", "6000",
                   "--standby", "near", "--standby-idle-ms", "5000",
                   # a dictation that ends asks the agent by itself: no agent of this machine is ever called by a test
                   "--agent", "mock", "--agent-auto", "off"],
                  args.debug_port + 1, logs, args.timeout, extra_env=extra)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        app.frame(2)
        show_files(app)
        pick(app, "src", "lib.rs")
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

        # 0. Nothing is open before the pointer comes near a page (the policy only counts under the e2e server: no
        # test opens a microphone).
        before = state(app)["standby"]
        if not before["enabled"] or before["armed"] or before["arms"] != 0 or before["mode"] != "near":
            raise Failure("the standby before the pointer came near a page: %s" % before)
        # 1. A rest starts a note that listens.
        rest_at(0.0, 0.0, "the first dictation to start", 1)
        app.until("the side column", lambda: app.has_id("__azreview_notes"))
        near = state(app)["standby"]
        if not near["armed"] or near["arms"] != 1:
            raise Failure("the pointer came near a page and the microphone was not asked to open once: %s" % near)
        app.until("AZREVIEW_STANDBY armed", lambda: app.count("AZREVIEW_STANDBY", r"armed") == 1)
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
            # 8. The pointer has been away for the idle time (5 s, from the end of the last dictation): the microphone
            # is asked to close, once, and the standby's timer ends with it.
            app.until("the microphone closed after the idle time", lambda: not state(app)["standby"]["armed"])
            closed = state(app)["standby"]
            if closed["disarms"] != 1:
                raise Failure("the idle time closed the microphone %s times: %s" % (closed["disarms"], closed))
            app.until("AZREVIEW_STANDBY disarmed", lambda: app.count("AZREVIEW_STANDBY", r"disarmed") == 1)
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


def pixels_of(png_path):
    """A screenshot's pixels as a height x width x 3 array of RGB: the system's `sips` turns the PNG into a BMP (no decoder of
    our own; the BMP it writes is top-down, 32 bits, blue first)."""
    import struct
    import numpy as np
    bmp = png_path + ".bmp"
    done = subprocess.run(["/usr/bin/sips", "-s", "format", "bmp", png_path, "--out", bmp], capture_output=True, text=True, check=False)
    if done.returncode != 0:
        raise Failure("sips could not read %s: %s" % (png_path, done.stderr))
    with open(bmp, "rb") as f:
        data = f.read()
    os.remove(bmp)
    offset, = struct.unpack("<I", data[10:14])
    width, height = struct.unpack("<ii", data[18:26])
    bits, = struct.unpack("<H", data[28:30])
    if bits != 32:
        raise Failure("the BMP of %s has %d bits" % (png_path, bits))
    rows = abs(height)
    a = np.frombuffer(data, dtype=np.uint8, count=width * rows * 4, offset=offset).reshape(rows, width, 4)
    if height > 0:
        a = a[::-1]
    return a[:, :, [2, 1, 0]]


def marker_bands(png_path, region):
    """The rows, in the picture's px, where the marker's ink (a pink wash: 255, 182, 216) is, as bands of rows that touch:
    `region` is (x0, y0, x1, y1) in the picture's px."""
    import numpy as np
    a = pixels_of(png_path)
    x0, y0, x1, y1 = [int(v) for v in region]
    part = a[y0:y1, x0:x1].astype(int)
    pink = (part[:, :, 0] >= 245) & (part[:, :, 1] >= 165) & (part[:, :, 1] <= 200) & (part[:, :, 2] >= 200) & (part[:, :, 2] <= 232)
    rows = np.where(pink.sum(axis=1) >= 30)[0]
    bands = []
    for r in rows:
        if bands and r - bands[-1][1] <= 1:
            bands[-1][1] = int(r)
        else:
            bands.append([int(r), int(r)])
    return [(a_ + y0, b_ + y0) for a_, b_ in bands]


def page_growth(args, logs, out, binary):
    """Page breaks are stable and the ink stays with its line: a marker stroke drawn on line 24 goes down with that line when an
    agent's suggestion puts a line in above it (the stroke is kept where the page was first drawn, and painted where its line
    is), and a stroke drawn on the page that has grown is where the pen was."""
    data_dir = os.path.join(logs, "data-grow")
    folder = os.path.join(logs, "folder-grow")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    app = e2e.App("review-grow", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en", "--dwell-input", "off",
                   "--standby", "off", "--agent", "mock", "--agent-auto", "off", "--agent-delay-ms", "300"],
                  args.debug_port + 8, logs, args.timeout)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        show_files(app)
        pick(app, "src", "lib.rs")
        app.until("lib.rs open", lambda: state(app)["tree"]["open"] == os.path.join("src", "lib.rs"))
        app.frame(3)
        strip = app.rect(STRIP)

        def draw(y, x0, x1):
            app.must("mouse_move", x=x0, y=y)
            app.must("mouse_down", x=x0, y=y)
            for i in range(1, 11):
                app.must("mouse_move", x=x0 + (x1 - x0) * i / 10.0, y=y)
                app.frame(1)
            app.must("mouse_up", x=x1, y=y)
            app.frame(3)

        def centre(n):
            """The middle of line `n`'s row in the window's px (the rect of a node of the pages' DOM is in that DOM's own
            coordinates, which start at the strip's corner)."""
            r = app.rect("__azreview_line-%d" % n, every_dom=True)
            return strip["y"] + r["y"] + r["height"] / 2.0, strip["x"] + r["x"]

        def band_near(png, y):
            """The one band of marker ink within 10 px (logical) of `y`: its centre in logical px."""
            shot = pixels_of(png)
            scale = shot.shape[1] / float(WIDTH)
            region = ((strip["x"] + 100) * scale, (strip["y"] + 30) * scale, (strip["x"] + 700) * scale, (strip["y"] + 700) * scale)
            found = [(a + b) / 2.0 / scale for a, b in marker_bands(png, region)]
            near = [c for c in found if abs(c - y) <= 10.0]
            if len(near) != 1:
                raise Failure("expected one band of marker ink near y=%.1f, found bands at %s" % (y, found))
            return near[0]

        # The pixels are read from headless pictures only: a real window's picture is the whole window as the system composited it
        # (its frame, its shadow, the display's scale), which has no fixed place for a logical px; there the lines' places stand
        # in for the pixels and the pictures are the evidence for the eye.
        pixels = e2e.BACKEND == "headless"
        # (the note's anchor is on line 4 and an agent is told of the ink within 160 px of it, so the strokes are on lines 24 and 32:
        # the mock agent answers the words, and the suggestion is the line after line 4 whatever is drawn)
        y8, x8 = centre(24)
        draw(y8, x8 + 150.0, x8 + 330.0)
        before = os.path.join(out, "30-ink-before.png")
        app.screenshot(before)
        a_y = band_near(before, y8) if pixels else y8
        if abs(a_y - y8) > 4.0:
            raise Failure("the stroke is not where the pen was: its band is at %.1f, the pen drew at %.1f" % (a_y, y8))

        # a suggestion that puts a line in after the fourth line: the seventh line and the stroke on it go down a row
        note = add_note(app, "rename step_3 to first_step", CONV_ANCHOR_Y, ask=True)
        app.until("a suggestion waits", lambda: talk_of(app, note)["phase"] == "suggested")
        app.until("the line is a row of the page", lambda: app.has_id("__azreview_insert-%d-4" % note, every_dom=True))
        app.until("line 24 is a row lower", lambda: abs(centre(24)[0] - (y8 + 15.0)) < 1.0)
        after = os.path.join(out, "31-ink-grown.png")
        app.screenshot(after)
        b_y = band_near(after, y8 + 15.0) if pixels else a_y + 15.0
        if abs(b_y - a_y - 15.0) > 2.5:
            raise Failure("the ink did not go down with its line: the band was at %.1f and is at %.1f, a row is 15" % (a_y, b_y))

        # a stroke drawn on the page that has grown is where the pen was (kept where the page was first drawn, painted where
        # its line is)
        y20, x20 = centre(32)
        draw(y20, x20 + 150.0, x20 + 330.0)
        third = os.path.join(out, "32-ink-on-grown-page.png")
        app.screenshot(third)
        c_y = band_near(third, y20) if pixels else y20
        if abs(c_y - y20) > 4.0:
            raise Failure("a stroke drawn on the grown page is not where the pen was: its band is at %.1f, the pen drew at %.1f" % (c_y, y20))
        if pixels and abs(band_near(third, y8 + 15.0) - b_y) > 1.0:
            raise Failure("the first stroke moved when the second was drawn")
        # it is kept where the page was first drawn: the archive's second stroke is a row (15 px) higher than the pen was
        app.key("s", primary=True)
        path = os.path.join(data_dir, ARCHIVE)
        app.until("the archive", lambda: len(read_session(path).get("strokes", [])) == 2)
        strokes = read_session(path)["strokes"]
        ys = [sum(p["y"] for p in st["points"]) / len(st["points"]) for st in strokes]
        want = [(y8 - strip["y"]) , (y20 - strip["y"]) - 15.0]
        if abs((ys[1] - ys[0]) - (want[1] - want[0])) > 3.0:
            raise Failure("the kept strokes are %s px apart, the pen drew %.1f apart less a row put in" % (ys[1] - ys[0], y20 - y8))
        no_lint(app, "the page growth run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: a line put in grows its page, the ink goes down with its line, and ink drawn on the grown page is where the pen was")


def ink_to_agent(args, logs, out, binary):
    """The ink an agent is told of: a marker stroke over a word, a note beside it that asks the mock agent to rename "this", and
    the suggestion is about the word under the stroke (the mock replaces the first word the strokes cover): it is struck on the
    page and the new word is yellow after it. Without the stroke in the request the mock would put a line in instead."""
    data_dir = os.path.join(logs, "data-ink-agent")
    folder = os.path.join(logs, "folder-ink-agent")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    app = e2e.App("review-ink-agent", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en", "--dwell-input", "off",
                   "--standby", "off", "--agent", "mock", "--agent-auto", "off", "--agent-delay-ms", "300"],
                  args.debug_port + 9, logs, args.timeout)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        show_files(app)
        pick(app, "src", "lib.rs")
        app.until("lib.rs open", lambda: state(app)["tree"]["open"] == os.path.join("src", "lib.rs"))
        app.frame(3)
        strip = app.rect(STRIP)
        # the word step_6 of line 7 ("pub fn step_6(x: u32) ..."): columns 7 to 12 of the line
        line = app.rect("__azreview_line-7-text", every_dom=True)
        y = strip["y"] + line["y"] + line["height"] / 2.0
        x0 = strip["x"] + line["x"] + 7 * 6.6 + 3.0
        x1 = strip["x"] + line["x"] + 13 * 6.6 - 3.0
        app.must("mouse_move", x=x0, y=y)
        app.must("mouse_down", x=x0, y=y)
        for i in range(1, 9):
            app.must("mouse_move", x=x0 + (x1 - x0) * i / 8.0, y=y)
            app.frame(1)
        app.must("mouse_up", x=x1, y=y)
        app.frame(3)
        if state(app)["strokes"] != 1:
            raise Failure("the stroke was not kept: %s" % state(app)["strokes"])
        note = add_note(app, "rename this", 32.0 + 6 * 15.0 + 2.0, ask=True)
        app.until("a suggestion waits", lambda: talk_of(app, note)["phase"] == "suggested")
        struck = "__azreview_struck-7-7"
        if not app.has_id(struck, every_dom=True):
            raise Failure("the suggestion is not about the word under the stroke: no #%s; the answer: %s" % (struck, talk_of(app, note)))
        app.until("the new word after it", lambda: app.has_id("__azreview_added-7-0", every_dom=True))
        if not app.shows("renamed", every_dom=True):
            raise Failure("the card does not say what the word becomes")
        app.screenshot(os.path.join(out, "33-ink-told-to-the-agent.png"))
        no_lint(app, "the ink to the agent run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: the strokes near a conversation reach the agent: the word under the stroke is the one it answers")


def dictation_stream(args, logs, out, binary):
    """A dictation through the engine's stream (a test-hooks libazul, which scripts the recogniser: nothing opens a microphone):
    the words come as they are said (the words so far, then each utterance), the microphone is told, the quiet ends the stream
    where it is (the last words are not lost), and a stop ends it at once (not after a chunk of seconds)."""
    data_dir = os.path.join(logs, "data-stream")
    folder = os.path.join(logs, "folder-stream")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    first, second = "rename this loop", "and then check the test"
    script = os.path.join(logs, "speech-stream.json")
    with open(script, "w", encoding="utf-8") as f:
        json.dump({"builtin": [{"say": first, "delay_ms": 400, "language": "en"}, {"say": second, "delay_ms": 600, "language": "en"},
                               {"silence": True}], "then": "silence"}, f)
    app = e2e.App("review-stream", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en", "--agent", "mock",
                   "--agent-auto", "off", "--dwell-ms", "400", "--dwell-input", "any", "--silence-ms", "3000", "--endpoint-ms", "800",
                   "--standby", "off"],
                  args.debug_port + 7, logs, args.timeout, extra_env={"AZ_SPEECH_SCRIPT": script})
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        show_files(app)
        pick(app, "src", "lib.rs")
        app.frame(3)
        strip = app.rect(STRIP)
        base = (strip["x"] + 160.0, strip["y"] + 140.0)
        lines = lambda pattern: app.printed("AZREVIEW_DICTATION", pattern)

        # 1. a rest starts it; the words come as utterances; the quiet ends it where it is
        app.must("mouse_move", x=base[0], y=base[1])
        app.until("the dictation to start", lambda: (app.frame(1), app.count("AZREVIEW_DICTATION", r"started 1") > 0)[1])
        app.until("the microphone is open", lambda: app.count("AZREVIEW_DICTATION", r"ready 1") > 0)
        app.until("the first utterance", lambda: app.count("AZREVIEW_DICTATION", r"text 1 %s" % re.escape(first)) > 0)
        app.until("the second", lambda: app.count("AZREVIEW_DICTATION", r"text 1 %s" % re.escape(second)) > 0)
        if app.count("AZREVIEW_DICTATION", r"partial 1 .+") < 1:
            raise Failure("no words so far were told while the words were said: %s" % lines(r".*"))
        began = time.perf_counter()
        app.until("the quiet ends it", lambda: app.count("AZREVIEW_DICTATION", r"ended 1 silence") > 0)
        waited = time.perf_counter() - began
        if waited > 3.0 + 2.5:
            raise Failure("the quiet (3 s) ended the stream after %.1f s" % waited)
        if app.count("AZREVIEW_MIC", r"listening") < 1:
            raise Failure("the microphone was never told as listening: %s" % app.printed("AZREVIEW_MIC", r".*"))
        notes = state(app)["conversations"]["notes"]
        if not notes or notes[0]["said"] < 2:
            raise Failure("the two utterances are not the conversation's: %s" % notes)
        app.screenshot(os.path.join(out, "29-dictation-stream.png"))

        # 2. a stop ends it at once: the script's utterances are over, a stream waits in the quiet, and the stop key ends it
        app.must("mouse_move", x=base[0] + 300.0, y=base[1] + 200.0)
        app.until("the second dictation to start", lambda: (app.frame(1), app.count("AZREVIEW_DICTATION", r"started 2") > 0)[1])
        app.until("its stop key", lambda: app.has_id("__azreview_dictation-stop"))
        began = time.perf_counter()
        app.click(selector="#__azreview_dictation-stop")
        app.until("the stop ends it", lambda: app.count("AZREVIEW_DICTATION", r"ended 2 button") > 0)
        stopped = time.perf_counter() - began
        if stopped > 5.0:
            raise Failure("a stop took %.1f s to end the stream" % stopped)
        no_lint(app, "the dictation stream run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: a dictation through the engine's stream: the words as they are said, the quiet ends it, a stop ends it at once")


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
        pick(app, "src", "lib.rs")
        app.frame(3)
        app.until("the saved session back", lambda: app.count("AZREVIEW_LOADED", r"1 0") > 0)
        strip = app.rect(STRIP)
        x0, y0 = strip["x"] + 200.0, strip["y"] + 220.0
        app.must("mouse_move", x=x0, y=y0)
        app.must("mouse_down", x=x0, y=y0)
        for i in range(1, 12):
            # a zigzag 18 px tall: ink, not a quick level slash across words (which takes them)
            app.must("mouse_move", x=x0 + i * 14.0, y=y0 + (i % 4) * 6.0)
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


SLASH_LINE = 3
SLASH_TEXT = "pub fn step_2(x: u32) -> u32 { x + 2 }"
# the words of that line a slash from column 3.8 to column 24.2 takes (each covered fully; "pub" and the second "u32" not at all)
SLASH_TAKEN = ["fn", "step_2", "(", "x", ":", "u32", ")", "->"]
SLASH_COLS = (3.8, 24.2)


def line_rects(app, first, last):
    """The rect of the text of every line from `first` to `last` that is on the screen."""
    out = {}
    for n in range(first, last + 1):
        node = "__azreview_line-%d-text" % n
        if app.has_id(node, every_dom=True):
            out[n] = app.rect(node, every_dom=True)
    return out


def css_of(app, node):
    """The computed CSS the debug server tells of `#node`, in whichever of the window's DOMs it is (the pages are a
    VirtualView's)."""
    for dom in app.dom_ids():
        if app._has_in("#%s" % node, dom):
            return app.value("get_node_css_properties", selector="#%s" % node, dom_id=dom)
    raise Failure("no DOM of the window has #%s" % node)


def slash(args, logs, out, binary):
    """The quick slash, in a real window: a quick level stroke across words takes them - they are drawn in the
    colour of the page and every line keeps its rect - Mod+Z gives them back, Mod+Shift+Z takes them again. No
    microphone: the dwell is off."""
    data_dir = os.path.join(logs, "data-slash")
    folder = os.path.join(logs, "folder-slash")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    app = e2e.App("review-slash", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-input", "off", "--standby", "off"],
                  args.debug_port + 3, logs, args.timeout)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        app.frame(2)
        show_files(app)
        pick(app, "src", "lib.rs")
        app.frame(3)
        text = "__azreview_line-%d-text" % SLASH_LINE
        app.until("the third line of lib.rs", lambda: app.has_id(text, every_dom=True))
        before = line_rects(app, 1, 30)
        line = before[SLASH_LINE]
        cw = line["width"] / len(SLASH_TEXT)
        app.log("the page's columns are %.3f px wide (the slash finds words by 6.6)" % cw)
        if abs(cw - 6.6) > 0.08:
            raise Failure("a column of the page is %.3f px, not 6.6: a word is found %.1f px off at column 40"
                          % (cw, abs(cw - 6.6) * 40))

        # The slash: down, two moves, up - as quick as the debug server's requests are. The rects of the lines are in the
        # pages' own DOM (a VirtualView's), whose origin is the strip's corner: the pointer is in the window's.
        strip = app.rect(STRIP)
        x0 = strip["x"] + line["x"] + SLASH_COLS[0] * cw
        x1 = strip["x"] + line["x"] + SLASH_COLS[1] * cw
        y = strip["y"] + line["y"] + line["height"] / 2.0
        app.must("mouse_move", x=x0, y=y)
        app.must("mouse_down", x=x0, y=y)
        app.must("mouse_move", x=(x0 + x1) / 2.0, y=y - 1.0)
        app.must("mouse_move", x=x1 - 4.0, y=y)
        app.must("mouse_up", x=x1, y=y)
        app.frame(3)
        log = [e for e in app.must("azreview_pointer_log")["data"]["value"]["result"]["events"]
               if e["kind"] in ("down", "up")]
        if len(log) >= 2:
            app.log("the stroke took %d ms between the press and the release (the log's clock)" % (log[-1]["ms"] - log[-2]["ms"]))
        st = state(app)
        if st["slashes"] != 1 or st["deleted_words"] != len(SLASH_TAKEN):
            raise Failure("the slash took %s word(s) in %s slash(es), expected %d: %s"
                          % (st["deleted_words"], st["slashes"], len(SLASH_TAKEN), log))
        app.until("AZREVIEW_SLASH %d" % len(SLASH_TAKEN), lambda: app.count("AZREVIEW_SLASH", r"%d" % len(SLASH_TAKEN)) == 1)
        if st["tree_open"] is not True:
            raise Failure("the slash moved a panel: %s" % st)

        # The words are drawn in the colour of the page, where they were; nothing else moved. Words that touch are one
        # piece of the line: "step_2(x:" is one, "u32)" another.
        pieces = [(4, 2), (7, 9), (17, 4), (22, 2)]  # (the column it starts in, how many columns it takes)
        for start, cols in pieces:
            node = "__azreview_hidden-%d-%d" % (SLASH_LINE, start)
            app.until("the hidden piece at column %d" % start, lambda node=node: app.has_id(node, every_dom=True))
            said = json.dumps(css_of(app, node)).lower()
            if "ffffff" not in said and "255, 255, 255" not in said and "255,255,255" not in said:
                raise Failure("the hidden piece at column %d is not in the colour of the page: %s" % (start, said[:300]))
            rect = app.rect(node, every_dom=True)
            want_x = line["x"] + start * cw
            if abs(rect["x"] - want_x) > 0.6 or abs(rect["y"] - line["y"]) > 0.6 or abs(rect["width"] - cols * cw) > 0.8:
                raise Failure("the hidden piece at column %d moved or changed its size: %s, it was to be at x %.1f, %d columns"
                              % (start, rect, want_x, cols))
        after = line_rects(app, 1, 30)
        for n, was in before.items():
            now = after.get(n)
            # a line cut into pieces is the sum of the widths of its pieces: a 32nd of a pixel off at most, no more
            if not now or any(abs(now[k] - was[k]) > 0.1 for k in ("x", "y", "width", "height")):
                raise Failure("line %d moved or changed its size: %s -> %s" % (n, was, now))
        app.screenshot(os.path.join(out, "8-slash.png"))

        # Mod+Z gives the words back; Mod+Shift+Z takes them again.
        app.key("z", primary=True)
        app.until("the words back", lambda: state(app)["slashes"] == 0 and not app.has_id("__azreview_hidden-%d-7" % SLASH_LINE, every_dom=True))
        again = line_rects(app, 1, 30)
        if again != before:
            raise Failure("undo changed the lines' rects")
        app.screenshot(os.path.join(out, "9-slash-undone.png"))
        app.key("z", primary=True, shift=True)
        app.until("the words taken again", lambda: state(app)["slashes"] == 1 and app.has_id("__azreview_hidden-%d-7" % SLASH_LINE, every_dom=True))

        # It is in the archive: a second window on the same data folder has the words hidden.
        path = os.path.join(data_dir, ARCHIVE)
        app.until("the archive with the deletion", lambda: os.path.exists(path) and len(read_session(path).get("slashes", [])) == 1)
        words = [w["text"] for w in read_session(path)["slashes"][0]["words"]]
        if words != SLASH_TAKEN:
            raise Failure("the archive holds %s, not %s" % (words, SLASH_TAKEN))
        no_lint(app, "the slash run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: a quick slash took %d words, drew them in the colour of the page, moved no line, and undo, redo and the archive have them" % len(SLASH_TAKEN))


def right_click_in_tree(app, name):
    """A right click on the row of the tree named `name`: the pointer goes to the middle of the row and presses the right
    button there (the rows are a VirtualView's: their rects are in its own DOM, whose corner is the tree list's)."""
    row = next((r for r in state(app)["tree"]["visible"] if r["name"] == name), None)
    if row is None:
        raise Failure("no row of the tree is named %r: %s" % (name, state(app)["tree"]["visible"][:12]))
    node = "__azreview_tree-row-%d" % row["id"]
    rect = app.rect(node, every_dom=True)
    corner = app.rect("__azreview_files")
    x = corner["x"] + rect["x"] + min(rect["width"], 120.0) / 2.0
    y = corner["y"] + rect["y"] + rect["height"] / 2.0
    app.must("mouse_move", x=x, y=y)
    app.must("mouse_down", x=x, y=y, button="right")
    app.must("mouse_up", x=x, y=y, button="right")
    app.frame(2)


def reviewed_state(app):
    return state(app)["progress"]


def progress(args, logs, out, binary):
    """What has been looked at, in a window: the page in view for a second and a half is seen, a file is reviewed when every
    page of it is, the totals of the workspace follow (the whole workspace counted behind the window), a right click holds a
    file back, and a second window on the same data folder has all of it from the start. No microphone: the dwell is off."""
    data_dir = os.path.join(logs, "data-progress")
    folder = os.path.join(logs, "folder-progress")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    argv = [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en", "--dwell-input", "off",
            "--standby", "off"]
    app = e2e.App("review-progress", binary, argv, args.debug_port + 4, logs, args.timeout)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        app.frame(2)
        # README.md is open: one page, three lines. Not seen before a second and a half has passed.
        if reviewed_state(app)["seen_pages"] != 0:
            raise Failure("a page was seen before it was looked at: %s" % reviewed_state(app))
        app.until("the README's page seen", lambda: reviewed_state(app)["seen_pages"] == 1)
        app.until("the README reviewed", lambda: reviewed_state(app)["reviewed"])
        # The workspace is counted behind the window: 3 lines of README, 120 of lib.rs.
        app.until("the workspace counted", lambda: not reviewed_state(app)["counting"])
        w = reviewed_state(app)["workspace"]
        if (w["files"], w["lines"], w["files_reviewed"], w["lines_reviewed"]) != (2, 123, 1, 3):
            raise Failure("the totals after the README: %s" % w)
        app.until("the sentence at the foot of the tree", lambda: app.shows("3 of 123 lines reviewed (2%)", every_dom=False))
        app.screenshot(os.path.join(out, "10-progress-readme.png"))

        # lib.rs: three pages; the first is looked at, the others are not.
        pick(app, "src", "lib.rs")
        app.until("lib.rs open", lambda: state(app)["tree"]["open"] == os.path.join("src", "lib.rs"))
        app.until("its first page seen", lambda: reviewed_state(app)["seen_pages"] == 1)
        time.sleep(2.2)
        p = reviewed_state(app)
        if p["seen_pages"] != 1 or p["reviewed"]:
            raise Failure("a page next to the one looked at was seen, or the file is reviewed after one page of three: %s" % p)
        app.click(selector="#__azreview_page-pill-2")
        app.until("page 2 seen", lambda: reviewed_state(app)["seen_pages"] == 2)
        app.click(selector="#__azreview_page-pill-3")
        app.until("lib.rs reviewed", lambda: reviewed_state(app)["reviewed"])
        w = reviewed_state(app)["workspace"]
        if (w["files_reviewed"], w["lines_reviewed"], w["lines_seen"]) != (2, 123, 123):
            raise Failure("the totals after lib.rs: %s" % w)
        app.until("the sentence says everything is reviewed", lambda: app.shows("123 of 123 lines reviewed (100%)"))
        app.until("the tree marks the file green", lambda: app.has_id("__azreview_file-mark-done", every_dom=True))
        app.screenshot(os.path.join(out, "11-progress-reviewed.png"))

        # A right click on the file in the tree: it is not reviewed, whatever its pages say; again, and it is.
        right_click_in_tree(app, "lib.rs")
        app.until("lib.rs held back", lambda: reviewed_state(app)["held"] and not reviewed_state(app)["reviewed"])
        app.until("the sentence follows", lambda: app.shows("3 of 123 lines reviewed (2%)"))
        right_click_in_tree(app, "lib.rs")
        app.until("lib.rs let go", lambda: not reviewed_state(app)["held"] and reviewed_state(app)["reviewed"])
        # what was seen is written: the toggle asks for it at once
        app.until("the progress file written", lambda: app.count("AZREVIEW_SAVED", r"review/progress-[0-9a-f]{16}\.json") > 0)
        no_lint(app, "the progress run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # A second window on the same data folder: the README is open again, lib.rs is reviewed without being looked at.
    again = e2e.App("review-progress-again", binary, argv, args.debug_port + 5, logs, args.timeout)
    try:
        again.until("AzReview's window", lambda: again.has_id(STRIP))
        again.until("what was seen came back", lambda: again.count("AZREVIEW_PROGRESS_LOADED", r"2") > 0)
        again.until("both files reviewed from the start", lambda: reviewed_state(again)["workspace"]["files_reviewed"] == 2)
        if reviewed_state(again)["seen_pages"] != 1 or not reviewed_state(again)["reviewed"]:
            raise Failure("the README is not reviewed straight away: %s" % reviewed_state(again))
        again.screenshot(os.path.join(out, "12-progress-restored.png"))
        no_lint(again, "the second progress run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(again.out_path), e2e.tail(again.err_path)))
        raise
    finally:
        again.stop()
    app.log("PASS: pages seen after a second and a half, a file reviewed when all are, the totals, a right click holds a file back, and a second window has all of it")


def add_note(app, text, y, page=0, ask=False, open_it=False):
    """A note at `y` of `page` of the open file, as a dictation would make one (the debug op: no microphone): its id."""
    answer = app.must("azreview_add_note", text=text, page=page, y=y, ask=ask, open=open_it)["data"]["value"]["result"]
    if not answer.get("success"):
        raise Failure("azreview_add_note: %s" % answer)
    return answer["id"]


def talk_of(app, note):
    """What the state says of conversation `note`."""
    for n in state(app)["conversations"]["notes"]:
        if n["id"] == note:
            return n
    raise Failure("no conversation %d in %s" % (note, state(app)["conversations"]))


CONV_ANCHOR_Y = 32.0 + 3 * 15.0 + 2.0  # the fourth line of a page: under its padding and three lines


def conversations(args, logs, out, binary):
    """The conversations of a page with the mock agent: none, one, and many; a chip in the margin at the anchor, the
    popover that opens from it, the agent that is asked and answers a suggestion (yellow), words that are nonsense and
    end quietly (nothing is shown), resolving, the archive, and fifty of them. No microphone: the notes are made by the
    debug op and the dwell is off."""
    data_dir = os.path.join(logs, "data-conv")
    folder = os.path.join(logs, "folder-conv")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    argv = [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en", "--dwell-input", "off",
            "--standby", "off", "--agent", "mock", "--agent-delay-ms", "1500"]
    app = e2e.App("review-conv", binary, argv, args.debug_port + 6, logs, args.timeout)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        show_files(app)
        pick(app, "src", "lib.rs")
        app.until("lib.rs open", lambda: state(app)["tree"]["open"] == os.path.join("src", "lib.rs"))
        app.frame(2)

        # none: no chip, no margin, the bar says so
        if state(app)["conversations"]["notes"] or state(app)["chips"] != 0:
            raise Failure("conversations before anybody said anything: %s" % state(app)["conversations"])
        if app.has_id("__azreview_margin-1", every_dom=True):
            raise Failure("a margin with no conversation")
        app.until("the bar says no agent works", lambda: app.shows("No agents are working."))
        if state(app)["conversations"]["agents"]["backend"] != "mock":
            raise Failure("the agents are not the mock: %s" % state(app)["conversations"]["agents"])

        # one: a note on the fourth line of page 1; its chip stands in the margin of that page
        first = add_note(app, "rename step_3 to first_step", CONV_ANCHOR_Y)
        app.until("its chip in the margin", lambda: app.has_id("__azreview_conv-%d" % first, every_dom=True))
        # where the lines of the first two pages are before anything is put in (page breaks are stable: see below)
        line_y = lambda n: app.rect("__azreview_line-%d" % n, every_dom=True)["y"]
        rows_before = {n: line_y(n) for n in (1, 4, 5, 46, 47, 48)}
        if talk_of(app, first)["phase"] != "draft" or state(app)["chips"] != 1:
            raise Failure("a note nobody asked about: %s, %d chips" % (talk_of(app, first), state(app)["chips"]))
        chip = app.rect("__azreview_conv-%d" % first, every_dom=True)
        margin = app.rect("__azreview_margin-1", every_dom=True)
        # the chip is as high as the anchor: the margin starts at the page's padding, the chip is centred on the anchor
        want = CONV_ANCHOR_Y - 32.0 - 14.0
        if abs(chip["y"] - margin["y"] - max(want, 0.0)) > 1.5:
            raise Failure("the chip is not at the height of its anchor: chip %s, margin %s, wanted %.1f under the margin's top" % (chip, margin, want))
        app.screenshot(os.path.join(out, "20-conversation-chip.png"))

        # a click on the chip opens it (and is no stroke of the pen under it); a click on its close key closes it
        strokes = state(app)["strokes"]
        app.click(selector="#__azreview_conv-%d" % first, every_dom=True)
        app.until("the open conversation", lambda: app.has_id("__azreview_conv-pop-%d" % first, every_dom=True))
        if state(app)["conversations"]["open"] != first or state(app)["strokes"] != strokes:
            raise Failure("opening a chip: open %s, strokes %d -> %d" % (state(app)["conversations"]["open"], strokes, state(app)["strokes"]))
        if not app.shows("Page 1, line 4", every_dom=True):
            raise Failure("the open conversation does not say where it is")
        app.screenshot(os.path.join(out, "21-conversation-open.png"))
        app.click(selector="#__azreview_conv-close-%d" % first, every_dom=True)
        app.until("it closed", lambda: not app.has_id("__azreview_conv-pop-%d" % first, every_dom=True))
        app.click(selector="#__azreview_conv-%d" % first, every_dom=True)
        app.until("it opened again", lambda: app.has_id("__azreview_conv-pop-%d" % first, every_dom=True))
        # Escape closes it too
        app.key("Escape")
        app.until("Escape closed it", lambda: not app.has_id("__azreview_conv-pop-%d" % first, every_dom=True))
        app.click(selector="#__azreview_conv-%d" % first, every_dom=True)
        app.until("open for the agent", lambda: app.has_id("__azreview_conv-send-%d" % first, every_dom=True))

        # ask: the spinner while the agent works, then the mock's suggestion waits (yellow)
        app.click(selector="#__azreview_conv-send-%d" % first, every_dom=True)
        app.until("working", lambda: talk_of(app, first)["phase"] == "working")
        app.until("the spinner in the conversation", lambda: app.has_id("__azreview_conv-working-%d" % first, every_dom=True))
        app.until("the tree marks the file as worked on", lambda: app.has_id("__azreview_file-mark-working", every_dom=True))
        app.screenshot(os.path.join(out, "22-conversation-working.png"))
        app.until("a suggestion waits", lambda: talk_of(app, first)["phase"] == "suggested")
        t = talk_of(app, first)
        if t["pending"] != 1 or t["turns"] != 1:
            raise Failure("the mock's answer: %s" % t)
        if not app.shows("Mock agent: rename step_3 to first_step", every_dom=True):
            raise Failure("the agent's words are not in the conversation")
        if app.count("AZREVIEW_AGENT", r"done %d editing mock items=1 ms=\d+" % first) < 1:
            raise Failure("no done line for the agent's answer: %s" % app.printed("AZREVIEW_AGENT", r".*"))
        app.until("the tree marks the file yellow", lambda: app.has_id("__azreview_file-mark-suggested", every_dom=True))
        app.screenshot(os.path.join(out, "23-conversation-suggested.png"))

        # the suggestion: a card in the conversation and a yellow rule on the page where the line would go; taken (green),
        # taken back, refused, taken again
        card = "__azreview_sugg-%d-0" % first
        rule = "__azreview_insert-%d-4" % first

        def moved_by(down, why):
            """The first page's lines after the fourth are `down` px lower than before, the others where they were (the second
            page's too: page breaks are stable, a page that has lines put in grows and nothing else moves)."""
            for n, want in ((1, 0.0), (4, 0.0), (5, down), (46, down), (47, 0.0), (48, 0.0)):
                got = line_y(n) - rows_before[n]
                if abs(got - want) > 1.0:
                    raise Failure("%s: line %d is %.1f px lower than before, not %.1f" % (why, n, got, want))

        app.until("the suggestion's card", lambda: app.has_id(card, every_dom=True))
        app.until("the line on the page", lambda: app.has_id(rule, every_dom=True))
        app.until("the lines after it are a row lower", lambda: abs(line_y(5) - rows_before[5] - 15.0) < 1.0)
        moved_by(15.0, "a suggestion that waits")
        if not app.shows("Insert one line after line 4", every_dom=True):
            raise Failure("the card does not say what it would do")
        app.click(selector="#__azreview_sugg-accept-%d-0" % first, every_dom=True)
        app.until("taken", lambda: talk_of(app, first)["phase"] == "done" and talk_of(app, first)["accepted"] == 1)
        app.until("it says taken", lambda: app.has_id("__azreview_sugg-decided-%d-0" % first, every_dom=True))
        app.until("the file is no longer yellow", lambda: not app.has_id("__azreview_file-mark-suggested", every_dom=True))
        app.screenshot(os.path.join(out, "23b-suggestion-taken.png"))
        app.click(selector="#__azreview_sugg-undo-%d-0" % first, every_dom=True)
        app.until("it waits again", lambda: talk_of(app, first)["phase"] == "suggested" and talk_of(app, first)["pending"] == 1)
        app.click(selector="#__azreview_sugg-reject-%d-0" % first, every_dom=True)
        app.until("refused", lambda: talk_of(app, first)["phase"] == "done" and talk_of(app, first)["accepted"] == 0)
        app.until("no line where nothing goes", lambda: not app.has_id(rule, every_dom=True))
        app.until("the lines are where they were", lambda: abs(line_y(5) - rows_before[5]) < 1.0)
        moved_by(0.0, "a suggestion that was refused")
        app.click(selector="#__azreview_sugg-undo-%d-0" % first, every_dom=True)
        app.click(selector="#__azreview_sugg-accept-%d-0" % first, every_dom=True)
        app.until("taken at last", lambda: talk_of(app, first)["accepted"] == 1 and talk_of(app, first)["phase"] == "done")
        app.until("the line is back", lambda: app.has_id(rule, every_dom=True))
        app.until("the lines after it are a row lower again", lambda: abs(line_y(5) - rows_before[5] - 15.0) < 1.0)
        moved_by(15.0, "a suggestion that was taken")
        app.screenshot(os.path.join(out, "23c-page-grew.png"))

        # the resolved conversation is ONE patch file in the data root (the one it wrote before was replaced each time): the
        # header says which conversation it is and how it ended, the diff is what was taken, and it applies to the file as it
        # is on the disk
        patches = lambda c: sorted(glob.glob(os.path.join(data_dir, "review", "patches-*", "*-c%d.patch" % c)))
        app.until("its patch file", lambda: len(patches(first)) == 1 and "Outcome: accepted" in read_text(patches(first)[0]))
        found = patches(first)
        if len(found) != 1:
            raise Failure("a conversation resolved three times has one patch file: %s" % found)
        text = read_text(found[0])
        for want in ("AzReview-Patch: 1", "Conversation: %d" % first, "Outcome: accepted", "Base: none", "Said: rename step_3 to first_step",
                     "Rationale: Mock agent: rename step_3 to first_step", "Files: src/lib.rs",
                     "diff --git a/src/lib.rs b/src/lib.rs", "@@ -2,6 +2,7 @@", "+// mock: rename step_3 to first_step"):
            if want not in text:
                raise Failure("the patch does not say %r:\n%s" % (want, text))
        if app.count("AZREVIEW_PATCH", r"made c%d outcome=accepted changed=true left_out=0 name=.*" % first) < 1:
            raise Failure("no line for the patch made: %s" % app.printed("AZREVIEW_PATCH", r".*"))
        applied = subprocess.run(["patch", "-p1", "--dry-run", "-d", folder, "-i", found[0]], capture_output=True, text=True, check=False)
        if applied.returncode != 0:
            raise Failure("the patch does not apply to the file: %s%s" % (applied.stdout, applied.stderr))

        # nonsense: the agent does nothing, and nothing is shown of it (the reason is in the log line only)
        nonsense = add_note(app, "testing one two", CONV_ANCHOR_Y + 15.0 * 12, ask=True)
        app.until("asked", lambda: talk_of(app, nonsense)["asked"] == 1)
        app.until("it ended quietly", lambda: talk_of(app, nonsense)["phase"] == "draft"
                  and app.count("AZREVIEW_AGENT", r"done %d editing mock items=0 ms=\d+ no_action=.*" % nonsense) > 0)
        t = talk_of(app, nonsense)
        if t["turns"] != 0 or t["problem"] is not None or t["pending"] != 0:
            raise Failure("something was shown of an agent that did nothing: %s" % t)

        # an answer that comes while another file is open: the tree marks the file at once, the conversation has it when the
        # file is opened again (the mock takes 1.5 s: the reviewer is in the README by then)
        late = add_note(app, "what does step_5 add to x", CONV_ANCHOR_Y + 15.0 * 20, ask=True)
        app.until("the agent works on it", lambda: talk_of(app, late)["phase"] == "working")
        app.click(text="README.md", every_dom=True)
        app.until("the README is open", lambda: state(app)["tree"]["open"] == "README.md")
        if state(app)["conversations"]["notes"]:
            raise Failure("the conversations of the file that was left are still here: %s" % state(app)["conversations"]["notes"])
        app.until("the file that was left is yellow while it waits", lambda: app.has_id("__azreview_file-mark-suggested", every_dom=True)
                  or app.has_id("__azreview_file-mark-working", every_dom=True))
        app.until("the answer came", lambda: app.count("AZREVIEW_AGENT", r"done %d editing mock items=1 ms=\d+" % late) > 0)
        app.until("lib.rs is yellow", lambda: app.has_id("__azreview_file-mark-suggested", every_dom=True))
        app.click(text="lib.rs", every_dom=True)
        app.until("lib.rs is open again", lambda: state(app)["tree"]["open"] == os.path.join("src", "lib.rs"))
        app.until("its conversations are back", lambda: any(n["id"] == late for n in state(app)["conversations"]["notes"]))
        if talk_of(app, late)["phase"] != "suggested" or talk_of(app, late)["turns"] != 1:
            raise Failure("the answer that came while the file was away: %s" % talk_of(app, late))
        if state(app)["conversations"]["agents"]["late"] != 0:
            raise Failure("an answer is still waiting: %s" % state(app)["conversations"]["agents"])

        # a conversation whose suggestions were all decided is done and offers to be opened again
        app.click(selector="#__azreview_conv-%d" % first, every_dom=True)
        app.until("the first one open", lambda: state(app)["conversations"]["open"] == first)
        app.until("the reopen key", lambda: app.has_id("__azreview_conv-reopen-%d" % first, every_dom=True))
        # closing a conversation that waits: what waits is no longer waited for, and the file is not yellow any more
        app.until("the one that waits is a suggestion still", lambda: talk_of(app, late)["phase"] == "suggested")
        # an open conversation covers the margin below its chip, as it does for a reviewer: it is closed before the next chip
        app.click(selector="#__azreview_conv-close-%d" % first, every_dom=True)
        app.until("the first one closed", lambda: state(app)["conversations"]["open"] is None)
        app.click(selector="#__azreview_conv-%d" % late, every_dom=True)
        app.until("the other one open", lambda: state(app)["conversations"]["open"] == late)
        app.click(selector="#__azreview_conv-resolve-%d" % late, every_dom=True)
        app.until("it is resolved too", lambda: talk_of(app, late)["phase"] == "done")
        # closed with a suggestion waiting: a header alone, that says nothing was taken (and a conversation nobody resolved has none)
        app.until("the patch of the one that was closed", lambda: len(patches(late)) == 1)
        closed = read_text(patches(late)[0])
        for want in ("Conversation: %d" % late, "Outcome: rejected", "Files: none", "Said: what does step_5 add to x"):
            if want not in closed:
                raise Failure("the patch of the conversation that was closed does not say %r:\n%s" % (want, closed))
        if "diff --git" in closed:
            raise Failure("a diff where nothing was taken:\n%s" % closed)
        if patches(nonsense):
            raise Failure("a patch for a conversation nobody resolved: %s" % patches(nonsense))
        app.until("the file is no longer yellow", lambda: not app.has_id("__azreview_file-mark-suggested", every_dom=True))

        # the archive keeps the conversation: the phase and the turn are in it
        app.key("s", primary=True)
        path = os.path.join(data_dir, ARCHIVE)
        app.until("the archive", lambda: os.path.exists(path) and notes_in(path))
        mine = [n for n in notes_in(path) if n.get("id") == first]
        if not mine or mine[0].get("phase") != "done" or not mine[0].get("turns"):
            raise Failure("the archive's conversation: %s" % mine)

        # many: fifty conversations over the three pages; each page's margin holds what it can and says how many it does not
        for i in range(50):
            add_note(app, "conversation number %d about this line" % i, 40.0 + (i % 9) * 15.0 * 4, page=i % 3)
        app.until("fifty and the three", lambda: len(state(app)["conversations"]["notes"]) == 53)
        app.until("the margin of page 1", lambda: app.has_id("__azreview_margin-1", every_dom=True))
        app.screenshot(os.path.join(out, "24-fifty-conversations.png"))
        no_lint(app, "the conversations run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: chips in the margin at the height of their anchors, a popover from a chip, the mock agent's suggestion (yellow), "
            "nonsense that ends quietly, resolving, the archive, and fifty conversations")


def conversations_failing(args, logs, out, binary):
    """An agent that cannot answer (a program that is not there): the conversation says why, in words, and has a key to ask
    again. The settings file names a program that does not exist, so that no agent of this machine is ever called."""
    data_dir = os.path.join(logs, "data-conv-fail")
    folder = os.path.join(logs, "folder-conv-fail")
    os.makedirs(os.path.join(data_dir, "review"), exist_ok=True)
    with open(os.path.join(data_dir, "review", "settings.json"), "w", encoding="utf-8") as f:
        json.dump({"theme": "", "mode": "", "values": {"agent-editing-backend": "claude-print",
                                                        "agent-editing-program": "/nonexistent/azreview-no-such-agent"}}, f)
    fixture(folder)
    argv = [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en", "--dwell-input", "off",
            "--standby", "off"]
    app = e2e.App("review-conv-fail", binary, argv, args.debug_port + 7, logs, args.timeout)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        show_files(app)
        pick(app, "src", "lib.rs")
        app.until("lib.rs open", lambda: state(app)["tree"]["open"] == os.path.join("src", "lib.rs"))
        if state(app)["conversations"]["agents"]["backend"] != "claude-print":
            raise Failure("the settings file was not read: %s" % state(app)["conversations"]["agents"])
        note = add_note(app, "rename step_3 to first_step", CONV_ANCHOR_Y, ask=True, open_it=True)
        app.until("it failed", lambda: talk_of(app, note)["phase"] == "failed")
        why = talk_of(app, note)["problem"] or ""
        if "claude" not in why or "not installed" not in why:
            raise Failure("the reason is not in words: %r" % why)
        app.until("the reason is shown", lambda: app.has_id("__azreview_conv-problem-%d" % note, every_dom=True))
        app.until("the key to ask again", lambda: app.has_id("__azreview_conv-send-%d" % note, every_dom=True))
        if app.count("AZREVIEW_AGENT", r"failed %d editing not-installed .*" % note) < 1:
            raise Failure("no failed line: %s" % app.printed("AZREVIEW_AGENT", r".*"))
        app.screenshot(os.path.join(out, "25-conversation-failed.png"))
        no_lint(app, "the failing run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: an agent that is not there is a conversation that says so and can be asked again")

def conversations_dictation(args, logs, out, binary):
    """A dictation that goes the whole way without a microphone (the debug op says what it is given): the note and its
    conversation open in the margin of the page, the last three lines said are shown while it goes, the silence ends it,
    and the agent is asked by itself when it ends."""
    data_dir = os.path.join(logs, "data-conv-dictate")
    folder = os.path.join(logs, "folder-conv-dictate")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    argv = [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en", "--dwell-input", "off",
            "--standby", "off", "--silence-ms", "2000", "--agent", "mock", "--agent-delay-ms", "600"]
    app = e2e.App("review-conv-dictate", binary, argv, args.debug_port + 8, logs, args.timeout)
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        show_files(app)
        pick(app, "src", "lib.rs")
        app.until("lib.rs open", lambda: state(app)["tree"]["open"] == os.path.join("src", "lib.rs"))
        # the symbol index of the folder is built behind the window and says when it is done
        app.until("the symbol index is ready", lambda: app.count("AZREVIEW_SYMBOLS", r"ready found=\d+ done=\d+ dirs=\d+") > 0)
        sym = state(app)["symbols"]
        if not sym["running"] or not sym["progress"]["idle"] or sym["progress"]["found"] < 1:
            raise Failure("the symbol index: %s" % sym)
        said = ["rename step_3", "to first_step", "please", "right now"]
        answer = app.must("azreview_dictate", words=said, page=0, y=CONV_ANCHOR_Y, gap_ms=700)["data"]["value"]["result"]
        if not answer.get("success"):
            raise Failure("azreview_dictate: %s" % answer)
        note = answer["id"]
        app.until("the dictation runs and its conversation is open", lambda: state(app)["dictating"] and state(app)["conversations"]["open"] == note)
        app.until("its chip and its popover", lambda: app.has_id("__azreview_conv-pop-%d" % note, every_dom=True))
        app.until("the listening pill", lambda: app.has_id("__azreview_listening", every_dom=True))
        # three lines are shown, the last three said: after the fourth utterance the first is gone from the live lines
        app.until("four utterances heard", lambda: talk_of(app, note)["said"] == 4)
        app.frame(2)
        if not app.has_id("__azreview_conv-live-%d" % note, every_dom=True):
            raise Failure("no live lines while it listens")
        live = [k for k in range(6) if app.has_id("__azreview_conv-live-%d-%d" % (note, k), every_dom=True)]
        if live != [0, 1, 2]:
            raise Failure("the live lines are %s, not the last three" % live)
        for words in said[1:]:
            if not app.shows(words, every_dom=True):
                raise Failure("%r is not shown among the last three lines" % words)
        app.screenshot(os.path.join(out, "26-dictation-live.png"))
        # the silence ends it, and the agent is asked by itself
        app.until("the dictation ended by silence", lambda: app.count("AZREVIEW_DICTATION", r"ended %d silence" % note) > 0)
        app.until("it asked the agent", lambda: talk_of(app, note)["asked"] == 1)
        app.until("a suggestion waits", lambda: talk_of(app, note)["phase"] == "suggested")
        t = talk_of(app, note)
        if t["said"] != 4 or t["turns"] != 1:
            raise Failure("the conversation after the dictation: %s" % t)
        # the name the reviewer said is a definition of the folder, and it was found and kept for the next utterance
        if state(app)["symbols"]["recent"] < 1:
            raise Failure("the name step_3 was not found in the index: %s" % state(app)["symbols"])
        if not app.shows("rename step_3 to first_step please right now", every_dom=True):
            raise Failure("the words the dictation heard are not the note's")
        app.screenshot(os.path.join(out, "27-dictation-answered.png"))
        # the archive has what was heard, utterance by utterance
        app.key("s", primary=True)
        path = os.path.join(data_dir, ARCHIVE)
        app.until("the archive", lambda: os.path.exists(path) and notes_in(path))
        mine = [n for n in notes_in(path) if n.get("id") == note]
        if not mine or len(mine[0].get("said", [])) != 4 or mine[0].get("phase") != "suggested":
            raise Failure("the archive's conversation: %s" % mine)
        no_lint(app, "the dictation run")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: a dictation without a microphone: its conversation opens in the margin, the last three lines show, the silence ends it, the agent answers and the archive has it all")


def conversations_return(args, logs, out, binary):
    """Continue reviewing: a jump to a conversation (a chip of the bottom bar) remembers where the reviewer was, the first
    jump only; the key (or Mod+Shift+C) comes back and clears it; a restart still has it. No microphone, the mock agent."""
    data_dir = os.path.join(logs, "data-conv-return")
    folder = os.path.join(logs, "folder-conv-return")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    argv = [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en", "--dwell-input", "off",
            "--standby", "off", "--agent", "mock"]
    app = e2e.App("review-conv-return", binary, argv, args.debug_port + 9, logs, args.timeout)
    place = {"file": os.path.join("src", "lib.rs"), "page": 0, "line": 1}
    try:
        app.until("AzReview's window", lambda: app.has_id(STRIP))
        show_files(app)
        pick(app, "src", "lib.rs")
        app.until("lib.rs open", lambda: state(app)["tree"]["open"] == place["file"])
        near = add_note(app, "near the top", CONV_ANCHOR_Y, page=0)
        far = add_note(app, "far down the file", CONV_ANCHOR_Y, page=2)
        app.until("the bar has them", lambda: app.shows("far down the file") and app.shows("near the top"))
        if state(app)["progress"]["back"] is not None or app.has_id("__azreview_back"):
            raise Failure("a place to come back to before any jump: %s" % state(app)["progress"]["back"])

        # the first jump: to the far conversation, remembering where the reviewer was (page 1, line 1)
        app.click(text="far down the file", frames=2)
        app.until("it opened", lambda: state(app)["conversations"]["open"] == far)
        app.until("the place is kept", lambda: state(app)["progress"]["back"] == place)
        app.until("the key to continue", lambda: app.has_id("__azreview_back"))
        if state(app)["visible_page"] != 2:
            raise Failure("the jump did not take the pages to the conversation: page %s" % state(app)["visible_page"])
        if not app.shows("Back to src/lib.rs p1 l1"):
            raise Failure("the key does not say where it goes back to")
        app.screenshot(os.path.join(out, "28-continue-reviewing.png"))

        # a jump from one conversation to another does not overwrite it (the reviewer is on page 3 now)
        app.click(text="near the top", frames=2)
        app.until("the other opened", lambda: state(app)["conversations"]["open"] == near)
        if state(app)["progress"]["back"] != place:
            raise Failure("the second jump changed the place: %s" % state(app)["progress"]["back"])

        # what the place is, is kept in the data root: leave the file (that writes it), stop, and start again
        app.click(text="README.md", every_dom=True, frames=1)
        app.until("the progress written", lambda: app.count("AZREVIEW_SAVED", r"review/progress-[0-9a-f]{16}\.json") > 0)
        app.stop()
        again = e2e.App("review-conv-return-again", binary, argv, args.debug_port + 10, logs, args.timeout)
        try:
            again.until("AzReview's window", lambda: again.has_id(STRIP))
            again.until("the place came back with the window", lambda: state(again)["progress"]["back"] == place)
            again.until("and its key", lambda: again.has_id("__azreview_back"))
            # the shortcut comes back: the place opens, and is none
            again.key("c", primary=True, shift=True)
            again.until("back at the place", lambda: state(again)["tree"]["open"] == place["file"] and state(again)["visible_page"] == 0)
            again.until("no place any more", lambda: state(again)["progress"]["back"] is None and not again.has_id("__azreview_back"))
            no_lint(again, "the continue-reviewing run")
        finally:
            again.stop()
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: a jump to a conversation remembers where the reviewer was (the first one only), the key and Mod+Shift+C come back, and a restart still has it")


def notes_in(path):
    with zipfile.ZipFile(path) as z:
        return json.loads(z.read("session.json").decode("utf-8")).get("notes", [])


def run_all(args, logs, out):
    binary = e2e.find_binary("AzReview", args.bin, "AZREVIEW_BIN")
    # AZREVIEW_E2E_ONLY=slash, progress, growth, ink, dictation, stream or conversations runs that step alone (the others each take minutes).
    only = os.environ.get("AZREVIEW_E2E_ONLY", "")
    if only == "slash":
        slash(args, logs, out, binary)
        return True
    if only == "progress":
        progress(args, logs, out, binary)
        return True
    if only == "ink":
        ink_to_agent(args, logs, out, binary)
        return True
    if only == "growth":
        page_growth(args, logs, out, binary)
        return True
    if only == "dictation":
        if not fake_speech():
            print("[azreview] SKIPPED: the dictation (this libazul has no test hooks: the real recogniser would listen to the "
                  "microphone)", flush=True)
            return True
        dictation(args, logs, out, binary, True)
        return True
    if only == "stream":
        if not fake_speech():
            print("[azreview] SKIPPED: the dictation stream (this libazul has no test hooks: the real recogniser would listen "
                  "to the microphone)", flush=True)
            return True
        dictation_stream(args, logs, out, binary)
        return True
    if only == "conversations":
        conversations(args, logs, out, binary)
        conversations_failing(args, logs, out, binary)
        conversations_dictation(args, logs, out, binary)
        conversations_return(args, logs, out, binary)
        return True
    body(args, logs, out)
    # The dictation steps need the fake speech backend (a libazul with the test hooks). Without it the real recogniser
    # would be asked, and where the terminal holds the microphone and speech permissions it LISTENS: a test must never
    # open the microphone, so the steps are skipped.
    if fake_speech():
        dictation(args, logs, out, binary, True)
        dictation_stream(args, logs, out, binary)
    else:
        print("[azreview] SKIPPED: the dictation steps (this libazul has no test hooks: the real recogniser would listen "
              "to the microphone)", flush=True)
    slash(args, logs, out, binary)
    progress(args, logs, out, binary)
    conversations(args, logs, out, binary)
    conversations_failing(args, logs, out, binary)
    conversations_dictation(args, logs, out, binary)
    conversations_return(args, logs, out, binary)
    page_growth(args, logs, out, binary)
    ink_to_agent(args, logs, out, binary)
    restore(args, logs, out, binary)
    return True


if __name__ == "__main__":
    e2e.run(TAG, run_all, default_port=8776)
