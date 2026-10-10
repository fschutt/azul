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
    9. a third start with `--screen settings` opens in dark: the settings are remembered.

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


def mode_of(app):
    value = app.value("get_mode")
    return value.get("mode") if isinstance(value, dict) else None


def body(args, logs, out):
    binary = e2e.find_binary("AzSetup", args.bin, "AZSETUP_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix="azsetup-target-")

    def start(name, extra):
        app = e2e.App(name, binary, ["--data-dir", data_dir] + extra, args.debug_port, logs, args.timeout)
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
