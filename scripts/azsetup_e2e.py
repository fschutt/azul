#!/usr/bin/env python3
"""AzSetup end to end: the install wizard over the debug server.

    1. starts AzSetup headless (AZ_BACKEND=headless, the debug server on --debug-port) on a fresh
       data folder (`--data-dir`);
    2. Welcome: Escape asks to exit Setup (`AZSETUP_BOXES question=true`), Escape again closes the
       question; F1 opens the About box, Escape closes it; Next;
    3. License: Next is HELD until "I accept" is ticked (the reason shows, a click on Next changes
       nothing), then Next;
    4. Destination: Browse answers from the e2e mock store with a folder under the temp dir
       (`AZSETUP_PATH`), the required and the available space show; Next;
    5. Components: the wizard's button row lies inside the window although the list is longer
       than the page (the list scrolls); clearing "AzSlides" lowers the total from 547 MB to
       477 MB (on screen and in `AZSETUP_TOTAL`); Next; Options; Next;
    6. Ready: Install; the copying reaches 100 % (`AZSETUP_DONE`, "100 %" on screen), "Show
       details" shows the log; Next;
    7. Finish: a screenshot in flora / dark too, then Finish (`AZSETUP_FINISHED launch=true
       settings=false`);
    8. starts AzSetup again with `--screen settings`: the categories and the General settings
       show, Appearance shows "Interface zoom"; Mode -> Dark (Apply where the dialog has one) is
       kept in setup/settings.json (`AZSETUP_SETTINGS_SAVED`);
    9. a third start with `--screen settings` opens in dark: the settings are remembered;
   10. the same wizard and settings with `--language de`: every page, the exit question, the
       About box, every settings category and a search show no English word (AzSetup's
       English resources that German says otherwise, the widgets' English defaults, the
       units "min" / "pt"), and the search finds Interface language by the German keyword
       "Übersetzung".

Every state-changing op is followed by wait_frame. Screenshots go to --out (default: a
temporary folder printed at the end). Run ONE app at a time, through the capped runner:

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/azsetup.log -- \\
      python3 scripts/azsetup_e2e.py --bin target/release/AzSetup --out /tmp/azsetup-shots

The debug-server client is the shared one (scripts/azlin_e2e.py).
"""

import json
import os
import shutil
import tempfile
import time

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azsetup"
MB = 1024 * 1024
# The classic wizard (640 x 480) plus the title row: AzSetup's window.
WINDOW_HEIGHT = 514.0
BUTTONS = ".__azul-native-wizard-layout-buttons"


def steps(app):
    return [int(s.split(" ", 1)[0]) for s in app.printed("AZSETUP_STEP", r"\d+ .+")]


def step(app):
    found = steps(app)
    return found[-1] if found else None


def next_to(app, want, label="Next >"):
    app.click(text=label)
    app.until("step %d" % want, lambda: step(app) == want)


def boxes(app):
    return app.last("AZSETUP_BOXES")


def check_boxes(app):
    """Escape asks to exit; Escape closes the question; F1 / Escape for About."""
    app.key("escape")
    app.expect_line("AZSETUP_BOXES", "about=false question=true", "Escape asks to exit Setup")
    app.screenshot(os.path.join(app.out, "1-exit-question.png"))
    app.key("escape")
    app.expect_line("AZSETUP_BOXES", "about=false question=false", "Escape closes the question")
    app.key("f1")
    app.expect_line("AZSETUP_BOXES", "about=true question=false", "F1 opens the About box")
    app.screenshot(os.path.join(app.out, "1-about.png"))
    app.key("escape")
    app.expect_line("AZSETUP_BOXES", "about=false question=false", "Escape closes the About box")
    if step(app) != 0:
        raise Failure("the boxes moved the wizard (step %s)" % step(app))


def buttons_in_the_window(app, page):
    r = app.value("get_node_layout", selector=BUTTONS).get("rect") or {}
    bottom = r.get("y", 0) + r.get("height", 0)
    if not r or bottom > WINDOW_HEIGHT + 0.5 or r.get("height", 0) < 10:
        raise Failure("%s: the button row is not inside the %dpx window: %s" % (page, WINDOW_HEIGHT, r))
    app.log("%s: the button row ends at y %.1f, inside the window" % (page, bottom))


def walk_wizard(app, out, tmp):
    app.until("the welcome page", lambda: app.shows("Welcome to the AzOffice Setup Wizard"))
    app.frame(2)
    app.screenshot(os.path.join(out, "1-welcome.png"))
    buttons_in_the_window(app, "Welcome")
    check_boxes(app)
    next_to(app, 1)

    # License: Next is held until "I accept" is ticked.
    app.until("the license", lambda: app.shows("AZOFFICE LICENSE AGREEMENT"))
    if not app.shows("Accept the license agreement to continue."):
        raise Failure("the held Next does not say why")
    app.screenshot(os.path.join(out, "2-license-held.png"))
    app.click(text="Next >")
    app.frame(2)
    if step(app) != 1:
        raise Failure("Next went on although the license is not accepted (step %s)" % step(app))
    app.click(text="I accept the terms of the license agreement")
    app.until("the reason to go", lambda: not app.shows("Accept the license agreement to continue."))
    next_to(app, 2)

    # Destination: Browse answers from the mock store.
    folder = os.path.join(tmp, "AzOffice-e2e")
    app.must("mock", set={"file_open": {"path": folder}})
    app.click(text="Browse...")
    app.until("the picked folder", lambda: folder in app.printed("AZSETUP_PATH"))
    app.until("the space lines", lambda: app.shows("Space required:") and app.shows("Space available:"))
    app.screenshot(os.path.join(out, "3-destination.png"))
    buttons_in_the_window(app, "Destination")
    next_to(app, 3)

    # Components: the list scrolls, the buttons stay; clearing AzSlides lowers the total.
    app.until("the total", lambda: app.shows("Space required: 547 MB"))
    buttons_in_the_window(app, "Components")
    app.click(text="AzSlides")
    app.until("the new total", lambda: str(477 * MB) in app.printed("AZSETUP_TOTAL", r"\d+"))
    app.until("the total on screen", lambda: app.shows("Space required: 477 MB"))
    app.screenshot(os.path.join(out, "4-components.png"))
    next_to(app, 4)
    app.until("the options", lambda: app.shows("Create a desktop shortcut"))
    app.screenshot(os.path.join(out, "5-options.png"))
    buttons_in_the_window(app, "Options")
    next_to(app, 5)

    # Ready, then the copying.
    app.until("the summary", lambda: app.shows(folder))
    components_row = [t for t in app.texts() if "AzWriter" in t]
    if not components_row or "AzSlides" in components_row[0]:
        raise Failure("the summary lists the components wrongly: %s" % components_row)
    app.screenshot(os.path.join(out, "5-ready.png"))
    next_to(app, 6, label="Install")
    app.until("the copying to end", lambda: app.printed("AZSETUP_DONE", r"\d+") or None)
    app.frame(2)
    app.until("100 % on screen", lambda: app.shows("100 %"))
    app.click(text="Show details")
    app.until("the log", lambda: app.shows("Copied "))
    app.screenshot(os.path.join(out, "6-installed.png"))
    next_to(app, 7)

    # Finish, also in flora / dark.
    app.until("the finish page", lambda: app.shows("Completing the AzOffice Setup Wizard"))
    app.screenshot(os.path.join(out, "7-finish.png"))
    app.must("set_theme", theme="flora")
    app.must("set_mode", mode="dark")
    app.frame(3)
    app.screenshot(os.path.join(out, "7-finish-flora-dark.png"))
    app.must("set_theme", theme="flat")
    app.must("set_mode", mode="light")
    app.frame(2)
    app.must("click", text="Finish")
    # Finish closes the window and the app exits: read its stdout, not the server.
    finished = None
    for _ in range(40):
        lines = app.printed("AZSETUP_FINISHED", r".+")
        if lines:
            finished = lines[-1]
            break
        time.sleep(0.25)
    if finished != "launch=true settings=false":
        raise Failure("Finish reported %r" % finished)
    app.log("wizard walked: %s" % " -> ".join(str(s) for s in steps(app)))


def check_settings(app, out, data_dir):
    app.until("the settings window", lambda: app.shows("Reopen the last documents"))
    for want in ("General", "Editing", "Appearance", "Advanced", "Startup", "Documents folder"):
        if not app.shows(want):
            raise Failure("the settings window does not show %r" % want)
    app.screenshot(os.path.join(out, "8-settings.png"))
    app.click(text="Appearance")
    app.until("the appearance settings", lambda: app.shows("Interface zoom"))
    app.screenshot(os.path.join(out, "9-settings-appearance.png"))
    # Mode -> Dark; the dialog applies at once on macOS, else Apply.
    app.click(text="Dark")
    if app.shows("Apply"):
        app.click(text="Apply")
    app.until("the settings to be saved", lambda: app.printed("AZSETUP_SETTINGS_SAVED", r".+"))
    path = os.path.join(data_dir, "setup", "settings.json")
    with open(path, "r", encoding="utf-8") as f:
        settings = json.load(f)
    if settings.get("mode") != "dark":
        raise Failure("setup/settings.json does not hold the dark mode: %s" % settings)
    app.log("settings.json: %s" % settings)


# ---- The German window (`--language de`) ----

HERE = os.path.dirname(os.path.abspath(__file__))
RESOURCES = os.path.join(HERE, "..", "examples", "azul-setup", "resources")
WIDGETS = os.path.join(HERE, "..", "layout", "src", "widgets")


def ftl(name):
    """The `key = value` lines of one of AzSetup's resource files."""
    found = {}
    with open(os.path.join(RESOURCES, name), "r", encoding="utf-8") as f:
        for line in f:
            if "=" in line and not line.startswith("#"):
                key, value = line.split("=", 1)
                found[key.strip()] = value.strip()
    return found


def widget_defaults():
    """The English words azul's widgets say when the app gives them none: the wizard pages',
    the settings dialog's and its layout's, the folder field's."""
    import re
    found = set()
    for name in ("wizard_pages.rs", "path_input.rs", os.path.join("shells", "settings_dialog.rs"),
                 os.path.join("shells", "settings_layout.rs")):
        with open(os.path.join(WIDGETS, name), "r", encoding="utf-8") as f:
            source = f.read()
        # The defaults of the widgets' constructors, not their tests' sample data.
        source = source.split("#[cfg(test)]", 1)[0]
        for text in re.findall(r'from_const_str\("([^"]*)"\)', source):
            # "OK" is German too; "warning" is an icon's name, "\u{...}" a glyph.
            if re.search(r"[A-Za-z]{2}", text) and text not in ("warning", "OK") \
                    and not text.startswith("\\u"):
                found.add(text)
    return found


def english_words():
    """Every text a German window must not show: AzSetup's English words that German says
    otherwise, and the widgets' English defaults."""
    en, de = ftl("en.ftl"), ftl("de.ftl")
    words = {v for k, v in en.items() if de.get(k) != v and "{" not in v}
    return words | widget_defaults()


def english_on_screen(app, what, english):
    """Fails when the window shows one of `english` as a text of its own."""
    import re
    shown = set()
    for text in app.texts(every_dom=True):
        for line in str(text).split("\n"):
            line = line.strip()
            if line in english:
                shown.add(line)
            # A number field's unit sits in the value's line ("10 min").
            m = re.fullmatch(r"[0-9.,]+ (\S+)", line)
            if m and m.group(1) in ("min", "pt"):
                shown.add(line)
    if shown:
        raise Failure("%s: the German window says %s" % (what, sorted(shown)))
    app.log("%s: no English word" % what)


def walk_wizard_german(app, out, tmp, english):
    de = ftl("de.ftl")
    app.until("the German welcome", lambda: app.shows(de["azsetup-welcome-title"]))
    app.frame(2)
    english_on_screen(app, "Welcome", english)
    app.key("escape")
    app.expect_line("AZSETUP_BOXES", "about=false question=true", "Escape asks to exit Setup")
    app.until("the German question", lambda: app.shows(de["azsetup-exit-title"]))
    english_on_screen(app, "the exit question", english)
    app.screenshot(os.path.join(out, "de-1-exit-question.png"))
    app.key("escape")
    app.key("f1")
    app.expect_line("AZSETUP_BOXES", "about=true question=false", "F1 opens the About box")
    english_on_screen(app, "the About box", english)
    app.screenshot(os.path.join(out, "de-1-about.png"))
    app.key("escape")
    nxt = de["azsetup-next"]
    next_to(app, 1, label=nxt)

    english_on_screen(app, "License", english)
    app.screenshot(os.path.join(out, "de-2-license.png"))
    accept = [t for t in app.texts() if "akzeptiere" in str(t)]
    if not accept:
        raise Failure("the license page has no German accept box")
    app.click(text=accept[0])
    next_to(app, 2, label=nxt)

    folder = os.path.join(tmp, "AzOffice-de")
    app.must("mock", set={"file_open": {"path": folder}})
    english_on_screen(app, "Destination", english)
    browse = [t for t in app.texts() if str(t).startswith("Durchsuchen")]
    if not browse:
        raise Failure("the destination page has no German Browse button")
    app.click(text=browse[0])
    app.until("the picked folder", lambda: folder in app.printed("AZSETUP_PATH"))
    app.frame(2)
    english_on_screen(app, "Destination with a folder", english)
    app.screenshot(os.path.join(out, "de-3-destination.png"))
    next_to(app, 3, label=nxt)
    english_on_screen(app, "Components", english)
    app.screenshot(os.path.join(out, "de-4-components.png"))
    next_to(app, 4, label=nxt)
    english_on_screen(app, "Options", english)
    next_to(app, 5, label=nxt)
    app.until("the summary", lambda: app.shows(folder))
    english_on_screen(app, "Ready", english)
    app.screenshot(os.path.join(out, "de-5-ready.png"))
    next_to(app, 6, label=de["azsetup-install"])
    app.until("the copying to end", lambda: app.printed("AZSETUP_DONE", r"\d+") or None)
    app.frame(2)
    details = [t for t in app.texts() if "Details" in str(t)]
    if details:
        app.click(text=details[0])
    app.frame(2)
    english_on_screen(app, "Installing", english)
    app.screenshot(os.path.join(out, "de-6-installed.png"))
    next_to(app, 7, label=nxt)
    app.until("the German finish page", lambda: app.shows(de["azsetup-finish-title"]))
    english_on_screen(app, "Finish", english)
    app.screenshot(os.path.join(out, "de-7-finish.png"))


def check_settings_german(app, out, english):
    de = ftl("de.ftl")
    app.until("the German settings", lambda: app.shows(de["azsetup-set-reopen"]))
    for key in ("azsetup-cat-general", "azsetup-cat-editing", "azsetup-cat-appearance",
                "azsetup-cat-advanced"):
        app.click(text=de[key])
        app.frame(2)
        english_on_screen(app, "Settings > %s" % de[key], english)
        app.screenshot(os.path.join(out, "de-8-settings-%s.png" % key.rsplit("-", 1)[1]))
    # The search finds a setting by a German word of its keywords, not only by its label.
    app.text_input("." + "__azul-native-settings-layout-search", "Übersetzung")
    app.until("the German keyword's match", lambda: app.shows(de["azsetup-set-language"]))
    english_on_screen(app, "Settings, searched", english)
    app.screenshot(os.path.join(out, "de-9-settings-search.png"))


def mode_of(app):
    value = app.value("get_mode")
    return value.get("mode") if isinstance(value, dict) else None


def body(args, logs, out):
    binary = e2e.find_binary("AzSetup", args.bin, "AZSETUP_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix="azsetup-target-")

    def start(name, extra):
        # The checks read English words, whatever the system's language.
        app = e2e.App(name, binary, ["--data-dir", data_dir, "--language", "en"] + extra,
                      args.debug_port, logs, args.timeout)
        app.out = out
        app.until("the debug server", lambda: app.op("wait_frame") is not None)
        return app

    for name, extra, check in [
        ("wizard", [], lambda app: walk_wizard(app, out, tmp)),
        ("settings", ["--screen", "settings"], lambda app: check_settings(app, out, data_dir)),
    ]:
        app = start(name, extra)
        try:
            check(app)
        except Failure:
            print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
            raise
        finally:
            app.stop()

    # German: every page, box and setting in the window's language.
    english = english_words()
    for name, extra, check in [
        ("wizard-de", [], lambda app: walk_wizard_german(app, out, tmp, english)),
        ("settings-de", ["--screen", "settings"], lambda app: check_settings_german(app, out, english)),
    ]:
        app = e2e.App(name, binary, ["--data-dir", data_dir, "--language", "de"] + extra,
                      args.debug_port, logs, args.timeout)
        app.out = out
        try:
            app.until("the debug server", lambda: app.op("wait_frame") is not None)
            check(app)
        except Failure:
            print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
            raise
        finally:
            app.stop()

    # The third start remembers the dark mode.
    app = start("remembered", ["--screen", "settings"])
    try:
        app.until("the settings window again", lambda: app.shows("Reopen the last documents"))
        if mode_of(app) != "dark":
            raise Failure("the restarted app is not in the remembered dark mode (%r)" % mode_of(app))
        app.screenshot(os.path.join(out, "10-settings-remembered-dark.png"))
    finally:
        app.stop()
        if not args.keep:
            shutil.rmtree(tmp, ignore_errors=True)
    app.log("PASS: the wizard, the settings window, the remembered settings; screenshots in %s" % out)
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8772)
