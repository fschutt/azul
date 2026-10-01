#!/usr/bin/env python3
"""AzSetup end to end: the install wizard over the debug server.

    1. starts AzSetup headless (AZ_BACKEND=headless, the debug server on --debug-port);
    2. Welcome -> Next; on the License page Next is HELD until "I accept" is ticked (the reason
       shows, a click on Next changes nothing), then Next;
    3. Destination: Browse answers from the e2e mock store with a folder under the temp dir
       (`AZSETUP_PATH`), the required and the available space show; Next;
    4. Components: clearing "AzSlides" lowers the total from 547 MB to 477 MB (on screen and in
       `AZSETUP_TOTAL`); Next; Options; Next;
    5. Ready: Install; the copying reaches 100 % (`AZSETUP_DONE`, "100 %" on screen), "Show
       details" shows the log; Next;
    6. Finish: a screenshot in flora / dark too, then Finish (`AZSETUP_FINISHED launch=true
       settings=false`);
    7. starts AzSetup again with `--screen settings`: the categories and the General settings
       show, Appearance shows "Interface zoom", a screenshot.

Every state-changing op is followed by wait_frame. Screenshots go to --out (default: a
temporary folder printed at the end). Run ONE app at a time, through the capped runner:

    <scratchpad>/run_capped.sh --cap-mb 1500 --seconds 180 --log /tmp/azsetup.log -- \\
      env DYLD_LIBRARY_PATH=$PWD/target/azul-lib \\
      python3 scripts/azsetup_e2e.py --bin target/release/AzSetup --out /tmp/azsetup-shots

The debug-server client is AzShells' (examples/azul-shells/scripts/shells_e2e.py), not a copy.
"""

import argparse
import os
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, ".."))
sys.path.insert(0, os.path.join(REPO, "examples", "azul-shells", "scripts"))

from shells_e2e import App, Failure, repo_roots, tail  # noqa: E402

MB = 1024 * 1024


def log(line):
    print("[azsetup] %s" % line, flush=True)


def find_binary(explicit):
    candidates = []
    if explicit:
        candidates.append(explicit)
    if os.environ.get("AZSETUP_BIN"):
        candidates.append(os.environ["AZSETUP_BIN"])
    for root in repo_roots():
        for sub in ("release", "debug", "consumer/release", "consumer/debug"):
            candidates.append(os.path.join(root, "target", sub, "AzSetup"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no AzSetup binary; pass --bin or set AZSETUP_BIN (tried %s)" % candidates)


class Setup(App):
    """AzSetup under its debug server (AzShells' client, AzSetup's command line)."""

    def __init__(self, binary, args, port, env, logs, deadline, name):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "%s.stdout" % name)
        self.err_path = os.path.join(logs, "%s.stderr" % name)
        self.process = subprocess.Popen(
            [binary] + args, env=env, stdin=subprocess.DEVNULL,
            stdout=open(self.out_path, "wb"), stderr=open(self.err_path, "wb"),
        )

    def click_text(self, text):
        self.must("click", text=text)
        self.frame(2)

    def steps(self):
        return [int(s.split(" ", 1)[0]) for s in self.printed("AZSETUP_STEP", r"\d+ .+")]

    def step(self):
        steps = self.steps()
        return steps[-1] if steps else None

    def next_to(self, want, label="Next >"):
        self.click_text(label)
        self.until("step %d" % want, lambda: self.step() == want)


def walk_wizard(app, out, tmp):
    app.until("the welcome page", lambda: app.shows("Welcome to the AzOffice Setup Wizard"))
    app.frame(2)
    app.screenshot(os.path.join(out, "1-welcome.png"))
    app.next_to(1)

    # License: Next is held until "I accept" is ticked.
    app.until("the license", lambda: app.shows("AZOFFICE LICENSE AGREEMENT"))
    if not app.shows("Accept the license agreement to continue."):
        raise Failure("the held Next does not say why")
    app.screenshot(os.path.join(out, "2-license-held.png"))
    app.click_text("Next >")
    app.frame(2)
    if app.step() != 1:
        raise Failure("Next went on although the license is not accepted (step %s)" % app.step())
    app.click_text("I accept the terms of the license agreement")
    app.until("the reason to go", lambda: not app.shows("Accept the license agreement to continue."))
    app.next_to(2)

    # Destination: Browse answers from the mock store.
    folder = os.path.join(tmp, "AzOffice-e2e")
    app.must("mock", set={"file_open": {"path": folder}})
    app.click_text("Browse...")
    app.until("the picked folder", lambda: folder in app.printed("AZSETUP_PATH"))
    app.until("the space lines", lambda: app.shows("Space required:") and app.shows("Space available:"))
    app.screenshot(os.path.join(out, "3-destination.png"))
    app.next_to(3)

    # Components: clearing AzSlides lowers the total.
    app.until("the total", lambda: app.shows("Space required: 547 MB"))
    app.click_text("AzSlides")
    app.until("the new total", lambda: str(477 * MB) in app.printed("AZSETUP_TOTAL", r"\d+"))
    app.until("the total on screen", lambda: app.shows("Space required: 477 MB"))
    app.screenshot(os.path.join(out, "4-components.png"))
    app.next_to(4)
    app.until("the options", lambda: app.shows("Create a desktop shortcut"))
    app.next_to(5)

    # Ready, then the copying.
    app.until("the summary", lambda: app.shows(folder))
    texts = app.texts()
    components_row = [t for t in texts if "AzWriter" in t]
    if not components_row or "AzSlides" in components_row[0]:
        raise Failure("the summary lists the components wrongly: %s" % components_row)
    app.screenshot(os.path.join(out, "5-ready.png"))
    app.next_to(6, label="Install")
    app.until("the copying to end", lambda: app.printed("AZSETUP_DONE", r"\d+") or None)
    app.frame(2)
    app.until("100 % on screen", lambda: app.shows("100 %"))
    app.click_text("Show details")
    app.until("the log", lambda: app.shows("Copied "))
    app.screenshot(os.path.join(out, "6-installed.png"))
    app.next_to(7)

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
    log("wizard walked: %s" % " -> ".join(str(s) for s in app.steps()))


def check_settings(app, out):
    app.until("the settings window", lambda: app.shows("Reopen the last documents"))
    for want in ("General", "Editing", "Appearance", "Advanced", "Startup", "Documents folder"):
        if not app.shows(want):
            raise Failure("the settings window does not show %r" % want)
    app.screenshot(os.path.join(out, "8-settings.png"))
    app.click_text("Appearance")
    app.until("the appearance settings", lambda: app.shows("Interface zoom"))
    app.screenshot(os.path.join(out, "9-settings-appearance.png"))


def run(args, logs, out):
    binary = find_binary(args.bin)
    env = dict(os.environ)
    env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(args.debug_port)})
    tmp = tempfile.mkdtemp(prefix="azsetup-target-")

    app = Setup(binary, [], args.debug_port, env, logs, time.time() + args.timeout, "wizard")
    try:
        walk_wizard(app, out, tmp)
    except Failure as e:
        log("FAIL (wizard): %s" % e)
        print("---- stdout ----\n%s---- stderr ----\n%s" % (tail(app.out_path), tail(app.err_path)))
        raise
    finally:
        app.stop()

    app = Setup(binary, ["--screen", "settings"], args.debug_port, env, logs,
                time.time() + args.timeout, "settings")
    try:
        check_settings(app, out)
    except Failure as e:
        log("FAIL (settings): %s" % e)
        print("---- stdout ----\n%s---- stderr ----\n%s" % (tail(app.out_path), tail(app.err_path)))
        raise
    finally:
        app.stop()
    log("PASS: the wizard and the settings window, screenshots in %s" % out)
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8772)
    parser.add_argument("--timeout", type=int, default=150)
    parser.add_argument("--out")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azsetup-e2e-")
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    passed = False
    try:
        passed = run(args, logs, out)
    except Failure:
        passed = False
    finally:
        log("logs in %s" % logs)
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
