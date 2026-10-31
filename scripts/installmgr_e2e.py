#!/usr/bin/env python3
"""AzInstallManager end to end: the first-run wizard, the installed apps, an update, a revert,
the rollout gate, the beta channel, an uninstall, refused releases, and AzWriter's banner, against
fake manifests on a local server, in a sandbox home folder.

    0. a sandbox: HOME, AZLIN_CONFIG (rollout group 60, no update source) and XDG_DATA_HOME
       under a temporary folder, a fake package manager (brew on macOS, apt
       elsewhere) first on the PATH; a local HTTP server with a manifest per app and channel and
       the artifacts: AzWriter is the real AzWriter program, the runtime and AzClock are bytes;
    0b. a first run through the package manager (the default when one is found): its page
       offers it (recommended), AzWriter and AzClock ticked, the Ready page shows its command,
       Install runs the fake package manager for them (pkexec on Linux) and shows its output;
       it installs nothing, so the next start is a first run again;
    1. the wizard, Azlin's own way picked on the package manager's page (Copy puts `...
       azlin-writer azlin-clock` on the clipboard); the destination (~/.azlin/apps, the space lines); the
       apps (the groups cleared, AzWriter and AzClock ticked: the total is 70 + 15 + 7 MB);
       the channel (stable, auto-update); no update source yet: the Updates page says so, the
       Ready page holds Install with a sentence; the local server's address typed into the
       source's field (Enter) goes into the config and the summary; Install: the runtime, AzWriter and AzClock 0.1.0 are
       fetched and put in place (`current/`, `current.json`, the .desktop file on Linux, the
       ~/.azlin/bin link and the .zshrc line on macOS); Finish shows the installed apps;
    2. the table: AzWriter 0.1.1 is published (no rollout) and AzClock 0.2.0 half rolled out
       (group 60 is not in it yet): Check for updates stages AzWriter 0.1.1 and shows nothing
       for AzClock; Update puts 0.1.1 in place and keeps 0.1.0; Revert goes back and skips
       0.1.1; the settings move AzClock to beta, where 0.3.0 is offered at once; Uninstall
       removes AzClock after its question; AzWriter 0.1.2 with a wrong checksum is refused;
    3. a root key in the config: the unsigned AzWriter 0.1.3 is refused;
    4. without it, 0.1.3 is staged; the installed AzWriter starts with the banner ("A new version
       of AzWriter is available (0.1.3)"); Update now writes the request, starts the manager
       with --apply azwriter (through a wrapper that logs it) and closes; the manager installs
       0.1.3 once AzWriter is gone and starts it again.

Screenshots go to --out. Run ONE app at a time, through the capped runner:

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 420 --log /tmp/installmgr.log -- \\
      python3 scripts/installmgr_e2e.py --bin target/release/AzInstallManager \\
        --writer-bin target/release/AzWriter --out /tmp/installmgr-shots

The debug-server client is the shared one (scripts/azlin_e2e.py).
"""

import hashlib
import http.server
import json
import os
import shutil
import signal
import subprocess
import sys
import threading
import time

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "installmgr"
MB = 1024 * 1024
DAY = 86400


class Releases:
    """The local update server: `<root>/<key>/<channel>.json` and the artifacts beside them."""

    def __init__(self, root):
        self.root = root
        os.makedirs(root, exist_ok=True)
        handler = lambda *a, **k: http.server.SimpleHTTPRequestHandler(*a, directory=root, **k)
        http.server.SimpleHTTPRequestHandler.log_message = lambda *a: None
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.server.server_address[1]
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def url(self, path):
        return "http://127.0.0.1:%d/%s" % (self.port, path)

    def publish(self, key, channel, version, data, slow=None, digest=None):
        """Publishes `data` as `version` of `key` on `channel`; `slow`: the rollout map, `digest`:
        a pin other than the data's (a tampered release)."""
        name = "%s-%s.bin" % (key, version)
        with open(os.path.join(self.root, name), "wb") as f:
            f.write(data)
        latest = {
            "version": version,
            "download_url": self.url(name),
            "digest": digest or "sha256:" + hashlib.sha256(data).hexdigest(),
        }
        if slow is not None:
            latest["release_date"] = int(time.time()) - DAY
            latest["slow"] = slow
        else:
            latest["slow"] = "off"
        os.makedirs(os.path.join(self.root, key), exist_ok=True)
        with open(os.path.join(self.root, key, "%s.json" % channel), "w") as f:
            json.dump({"latest": latest}, f)

    def stop(self):
        self.server.shutdown()


def read_json(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def edit_config(path, change):
    config = read_json(path) if os.path.exists(path) else {}
    change(config.setdefault("updates", {}))
    with open(path, "w", encoding="utf-8") as f:
        json.dump(config, f, indent=2)


def row(app, key):
    rows = [r for r in app.printed("AZINSTALLMGR_ROW", r"\S+ .+") if r.split(" ", 1)[0] == key]
    return rows[-1] if rows else None


def wait_row(app, key, want):
    app.until("the row %s %s" % (key, want), lambda: row(app, key) == "%s %s" % (key, want))


def step(app):
    steps = app.printed("AZINSTALLMGR_STEP", r"\d+ .+")
    return steps[-1].split(" ", 1)[1] if steps else None


def next_to(app, want, label="Next >"):
    app.click(text=label)
    app.until("the step %s" % want, lambda: step(app) == want)


def checked(app, before):
    app.until("the check", lambda: app.count("AZINSTALLMGR_CHECKED") > before)
    return app.last("AZINSTALLMGR_CHECKED")


def check(app):
    before = app.count("AZINSTALLMGR_CHECKED")
    app.click(selector="#installmgr-check")
    return checked(app, before)


class Sandbox:
    def __init__(self, logs):
        self.root = os.path.join(logs, "sandbox")
        self.home = os.path.join(self.root, "home")
        self.azlin = os.path.join(self.home, ".azlin")
        self.config = os.path.join(self.azlin, "config.json")
        self.data = os.path.join(self.root, "data")
        self.fakebin = os.path.join(self.root, "fakebin")
        for d in (self.azlin, self.data, self.fakebin):
            os.makedirs(d, exist_ok=True)
        self.pm = "brew" if sys.platform == "darwin" else "apt"
        # The fake package manager writes down how it was called; pkexec (Linux) runs what it
        # is given, as the real one does once the user said yes.
        self.pm_log = os.path.join(self.root, "pm.log")
        fake = os.path.join(self.fakebin, self.pm)
        with open(fake, "w") as f:
            f.write('#!/bin/sh\necho "%s $*" >> "%s"\nexit 0\n' % (self.pm, self.pm_log))
        os.chmod(fake, 0o755)
        pkexec = os.path.join(self.fakebin, "pkexec")
        with open(pkexec, "w") as f:
            f.write('#!/bin/sh\nexec "$@"\n')
        os.chmod(pkexec, 0o755)
        self.env = {
            "HOME": self.home,
            "AZLIN_CONFIG": self.config,
            "XDG_DATA_HOME": os.path.join(self.home, ".local", "share"),
            "PATH": self.fakebin + os.pathsep + os.environ.get("PATH", ""),
        }

    def app_dir(self, key):
        return os.path.join(self.azlin, "apps", key)

    def registration(self, key):
        return read_json(os.path.join(self.app_dir(key), "current.json"))


def set_source(app, box, rel, out):
    """The first run has no update source: the Updates page says so under the source's field,
    the Ready page holds Install with a sentence; the address typed into the field (Enter) is
    kept in the config, and the summary names it."""
    url = rel.url("{app}/{channel}.json")
    app.until("the source's field", lambda: app.has_id("installmgr-source"))
    if not app.shows("No update source is set"):
        raise Failure("the Updates page does not say that there is no update source")
    app.screenshot(os.path.join(out, "5a-no-source.png"))
    next_to(app, "ready")
    app.until("Install held", lambda: app.shows("Set an update source on the Updates page first"))
    app.screenshot(os.path.join(out, "5b-install-held.png"))
    app.click(text="Install")
    app.frame(3)
    if step(app) != "ready":
        raise Failure("Install went on without an update source: %s" % step(app))
    app.click(text="< Back")
    app.until("the Updates page again", lambda: step(app) == "channel")
    app.text_input("#installmgr-source", url)
    app.key("return")
    app.until("the source kept", lambda: app.last("AZINSTALLMGR_SETTING") == "source " + url)
    app.until("the source in the config",
              lambda: read_json(box.config).get("updates", {}).get("manifest_url") == url)
    app.until("the source's explanation", lambda: app.shows("The apps and their updates come from"))
    app.log("the update source typed on the Updates page is kept in the config")


def tick_writer_and_clock(app):
    """Every group cleared, then AzWriter and AzClock ticked: the runtime stays (required)."""
    for group in ("Office", "Mail, calendar and contacts", "Pictures, music and video", "Tools"):
        app.click(text=group)
    app.click(text="AzWriter")
    app.click(text="AzClock")


def through_package_manager(app, box, out):
    """With a package manager the wizard installs through it by default: its page says so,
    the Ready page shows the command, Install runs it (the fake one writes down how it was
    called) and shows what it said."""
    app.until("the welcome page", lambda: app.shows("Welcome to the Azlin apps"))
    next_to(app, "package-manager")
    app.until("the choice", lambda: app.shows("(recommended)"))
    if app.last("AZINSTALLMGR_THROUGH") not in (None, box.pm, "Homebrew", "apt"):
        raise Failure("the wizard does not start through the package manager")
    app.screenshot(os.path.join(out, "0a-through-manager.png"))
    next_to(app, "components")
    tick_writer_and_clock(app)
    next_to(app, "ready")
    app.until("the command on the Ready page", lambda: app.shows("azlin-writer"))
    app.screenshot(os.path.join(out, "0b-ready-through-manager.png"))
    next_to(app, "installing", label="Install")
    app.until("the package manager to end", lambda: app.last("AZINSTALLMGR_DONE"))
    if app.last("AZINSTALLMGR_DONE") != "installed=1 failed=0":
        raise Failure("the package manager's run reported %r" % app.last("AZINSTALLMGR_DONE"))
    with open(box.pm_log) as f:
        called = f.read()
    if "install" not in called or "azlin-writer" not in called or "azlin-clock" not in called:
        raise Failure("the package manager was not run for the ticked apps: %r" % called)
    app.log("the wizard ran %s for the ticked apps: %s" % (box.pm, called.strip()))
    next_to(app, "finish")


def walk_wizard(app, box, rel, out):
    app.until("the welcome page", lambda: app.shows("Welcome to the Azlin apps"))
    app.screenshot(os.path.join(out, "1-welcome.png"))
    next_to(app, "package-manager")
    # Azlin's own way: the install folder, the channel and the update source come back.
    app.click(text="Install Azlin's own way")
    app.until("Azlin's own way chosen", lambda: app.last("AZINSTALLMGR_THROUGH") == "azlin")
    app.until("the package manager's page", lambda: app.shows("run this in a terminal"))
    app.screenshot(os.path.join(out, "2-package-manager.png"))
    next_to(app, "destination")
    folder = os.path.join(box.azlin, "apps")
    app.until("the space lines", lambda: app.shows("Space required:") and app.shows("Space available:"))
    app.screenshot(os.path.join(out, "3-destination.png"))
    next_to(app, "components")

    tick_writer_and_clock(app)
    want = (70 + 15 + 7) * MB
    app.until("the total", lambda: app.last("AZINSTALLMGR_TOTAL") == str(want))
    app.screenshot(os.path.join(out, "4-components.png"))

    # The package manager's command follows the ticks.
    app.click(text="< Back")
    app.click(text="< Back")
    app.until("the package manager's page again", lambda: step(app) == "package-manager")
    app.click(selector="#installmgr-pm-copy")
    copied = app.last("AZINSTALLMGR_COPIED") or ""
    if "azlin-writer" not in copied or "azlin-clock" not in copied or "azlin-mail" in copied:
        raise Failure("the copied command is not the ticked apps': %r" % copied)
    app.log("copied: %s" % copied)
    next_to(app, "destination")
    next_to(app, "components")
    next_to(app, "channel")
    app.until("the channel page", lambda: app.shows("Stable") and app.shows("Beta"))
    app.screenshot(os.path.join(out, "5-channel.png"))
    set_source(app, box, rel, out)
    next_to(app, "ready")
    app.until("the summary", lambda: app.shows(folder))
    if not app.shows(rel.url("{app}/{channel}.json")):
        raise Failure("the summary does not name the update source")
    app.screenshot(os.path.join(out, "6-ready.png"))
    next_to(app, "installing", label="Install")
    app.until("the install to end", lambda: app.last("AZINSTALLMGR_DONE"))
    done = app.last("AZINSTALLMGR_DONE")
    if done != "installed=3 failed=0":
        raise Failure("the install reported %r: %s" % (done, app.printed("AZINSTALLMGR_INSTALL_FAILED")))
    app.click(text="Show details")
    app.screenshot(os.path.join(out, "7-installed.png"))
    next_to(app, "finish")
    app.until("the finish page", lambda: app.shows("The Azlin apps are installed"))
    app.must("click", text="Finish")
    app.until("the apps", lambda: app.last("AZINSTALLMGR_SCREEN") == "apps")


def check_installed_files(box, writer_bytes):
    for key, version in (("runtime", "0.1.0"), ("azwriter", "0.1.0"), ("azclock", "0.1.0")):
        reg = box.registration(key)
        if reg.get("version") != version or not reg.get("managed"):
            raise Failure("%s's registration: %s" % (key, reg))
        if not os.path.isfile(reg["binary"]):
            raise Failure("%s is not at %s" % (key, reg["binary"]))
    program = box.registration("azwriter")["binary"]
    with open(program, "rb") as f:
        if f.read() != writer_bytes:
            raise Failure("the installed AzWriter is not the published one")
    if not os.access(program, os.X_OK):
        raise Failure("the installed AzWriter is not executable")
    if sys.platform == "darwin":
        link = os.path.join(box.azlin, "bin", "azwriter")
        if os.path.realpath(link) != os.path.realpath(program):
            raise Failure("~/.azlin/bin/azwriter does not lead to AzWriter")
        with open(os.path.join(box.home, ".zshrc")) as f:
            if f.read().count("# Azlin apps (AzInstallManager)") != 1:
                raise Failure("the PATH line is not in .zshrc once")
    elif sys.platform.startswith("linux"):
        desktop = os.path.join(box.env["XDG_DATA_HOME"], "applications", "azlin-azwriter.desktop")
        with open(desktop) as f:
            if program not in f.read():
                raise Failure("the .desktop file does not start AzWriter")


def first_check(app):
    """With auto-update on, the apps' screen checks every app at its first tick: waited for, so
    a check asked for later is not answered by it."""
    app.until("the first automatic check", lambda: app.count("AZINSTALLMGR_CHECKED") >= 1)


def table(app, box, rel, writer_bytes, out):
    first_check(app)
    wait_row(app, "azwriter", "0.1.0 - up-to-date")
    app.screenshot(os.path.join(out, "8-table.png"))

    # AzWriter 0.1.1 for everyone; AzClock 0.2.0 has reached half the computers, not group 60.
    rel.publish("azwriter", "stable", "0.1.1", writer_bytes)
    now = int(time.time())
    rel.publish("azclock", "stable", "0.2.0", b"clock 0.2.0", slow={"50": now - DAY, "100": now + 3 * DAY})
    result = check(app)
    if result != "available=1 failed=0":
        raise Failure("the check found %r" % result)
    wait_row(app, "azwriter", "0.1.0 0.1.1 staged")
    wait_row(app, "azclock", "0.1.0 - up-to-date")
    if read_json(os.path.join(box.app_dir("azwriter"), "staged.json"))["release"]["version"] != "0.1.1":
        raise Failure("AzWriter 0.1.1 is not staged")
    app.screenshot(os.path.join(out, "9-update-available.png"))

    app.click(selector="#installmgr-update-azwriter")
    app.until("the update", lambda: app.last("AZINSTALLMGR_UPDATED") == "azwriter 0.1.1")
    prev = read_json(os.path.join(box.app_dir("azwriter"), "prev", "release.json"))
    if prev["version"] != "0.1.0":
        raise Failure("the previous version kept is %s" % prev)

    app.click(selector="#installmgr-revert-azwriter")
    app.until("the revert", lambda: app.last("AZINSTALLMGR_REVERTED") == "azwriter 0.1.0")
    skip = read_json(box.config)["updates"]["overrides"]["azwriter"].get("skip_version")
    if skip != "0.1.1":
        raise Failure("the reverted version is not skipped (%r)" % skip)
    if os.path.exists(os.path.join(box.app_dir("azwriter"), "prev")):
        raise Failure("a previous version is left after the revert")
    app.screenshot(os.path.join(out, "10-reverted.png"))

    # AzClock to beta: the beta's 0.3.0 at once, whatever the rollout.
    rel.publish("azclock", "beta", "0.3.0", b"clock 0.3.0", slow={"50": now + DAY, "100": now + 3 * DAY})
    app.click(selector="#installmgr-settings-open")
    app.until("the settings", lambda: app.last("AZINSTALLMGR_SCREEN") == "settings")
    app.click_within("#installmgr-channel-azclock", "Beta")
    app.until("the channel", lambda: app.last("AZINSTALLMGR_SETTING") == "channel azclock beta")
    app.screenshot(os.path.join(out, "11-settings.png"))
    app.click(selector="#installmgr-settings-back")
    app.until("the apps", lambda: app.last("AZINSTALLMGR_SCREEN") == "apps")
    check(app)
    wait_row(app, "azclock", "0.1.0 0.3.0 staged")

    app.click(selector="#installmgr-uninstall-azclock")
    app.until("the question", lambda: app.last("AZINSTALLMGR_ASK_UNINSTALL") == "azclock")
    app.until("the question's window", lambda: app.popup())
    app.click_exact("Uninstall", window=app.popup())
    app.until("the uninstall", lambda: app.last("AZINSTALLMGR_UNINSTALLED") == "azclock")
    if os.path.exists(box.app_dir("azclock")):
        raise Failure("AzClock's folder is left")

    # A release whose bytes are not the ones its manifest pins is refused.
    rel.publish("azwriter", "stable", "0.1.2", writer_bytes, digest="sha256:" + "0" * 64)
    result = check(app)
    if result != "available=0 failed=1":
        raise Failure("the tampered release was not refused: %r" % result)
    wait_row(app, "azwriter", "0.1.0 - failed")
    staged = os.path.join(box.app_dir("azwriter"), "staged.json")
    if os.path.exists(staged) and read_json(staged)["release"]["version"] == "0.1.2":
        raise Failure("the tampered release is staged")
    app.screenshot(os.path.join(out, "12-refused.png"))


def start_manager(name, binary, box, logs, args, extra=None):
    env = dict(box.env)
    env.update(extra or {})
    # In English whatever the machine's language: the checks read the English words.
    app = e2e.App(name, binary, ["--data-dir", box.data, "--language", "en"], args.debug_port,
                  logs, args.timeout, extra_env=env)
    app.until("the debug server", lambda: app.op("wait_frame") is not None)
    return app


def banner(args, logs, out, box, rel, writer_bytes, manager_bin):
    # The manager stages 0.1.3 (no root key), then stops.
    rel.publish("azwriter", "stable", "0.1.3", writer_bytes)
    app = start_manager("stage", manager_bin, box, logs, args)
    try:
        app.until("the apps", lambda: app.last("AZINSTALLMGR_SCREEN") == "apps")
        first_check(app)
        wait_row(app, "azwriter", "0.1.0 0.1.3 staged")
    finally:
        app.stop()

    # The manager the app starts: a wrapper that logs it, keeps its pid and runs it headless
    # without a debug server (AzWriter's port is still taken while it closes).
    wrapper = os.path.join(box.root, "manager.sh")
    apply_log = os.path.join(logs, "manager-apply.stdout")
    pid_file = os.path.join(box.root, "manager.pid")
    with open(wrapper, "w") as f:
        f.write("#!/bin/sh\necho $$ > '%s'\nunset AZ_DEBUG\nexec '%s' --data-dir '%s' \"$@\" > '%s' 2>&1\n"
                % (pid_file, manager_bin, box.data, apply_log))
    os.chmod(wrapper, 0o755)
    reg_path = os.path.join(box.app_dir("installmgr"), "current.json")
    reg = read_json(reg_path)
    reg["binary"] = wrapper
    with open(reg_path, "w") as f:
        json.dump(reg, f)

    program = box.registration("azwriter")["binary"]
    writer = e2e.App("azwriter", program, ["--data-dir", box.data, "--language", "en"],
                     args.debug_port + 1, logs, args.timeout, extra_env=box.env)
    try:
        writer.until("the debug server", lambda: writer.op("wait_frame") is not None)
        writer.until("the offer", lambda: writer.last("AZLIN_UPDATE_OFFER") == "azwriter 0.1.3 staged")
        writer.until("the banner", lambda: writer.shows("A new version of AzWriter is available (0.1.3)"))
        writer.screenshot(os.path.join(out, "13-azwriter-banner.png"))
        launched = box.registration("azwriter").get("launched_unix", 0)
        writer.must("click", selector="#kit-update-now")
        for _ in range(40):
            if writer.printed("AZLIN_UPDATE_REQUESTED"):
                break
            time.sleep(0.25)
        if writer.last("AZLIN_UPDATE_REQUESTED") != "azwriter 0.1.3":
            raise Failure("AzWriter did not ask for 0.1.3: %r" % writer.last("AZLIN_UPDATE_REQUESTED"))
    finally:
        writer.stop()

    deadline = time.time() + 90
    applied = None
    while time.time() < deadline:
        if os.path.exists(apply_log):
            with open(apply_log, encoding="utf-8", errors="replace") as f:
                lines = [l.split(" ", 1)[1].strip() for l in f if l.startswith("AZINSTALLMGR_REQUESTED ")]
            if lines:
                applied = lines[-1]
                break
        time.sleep(0.5)
    if applied != "azwriter 0.1.3":
        raise Failure("the manager did not carry out the request: %r (log %s)" % (applied, apply_log))
    current = read_json(os.path.join(box.app_dir("azwriter"), "current", "release.json"))
    if current["version"] != "0.1.3":
        raise Failure("AzWriter's current version is %s" % current)
    # Started again: the new process registered its start.
    e2e_deadline = time.time() + 60
    while time.time() < e2e_deadline:
        if box.registration("azwriter").get("launched_unix", 0) > launched:
            break
        time.sleep(0.5)
    else:
        raise Failure("AzWriter was not started again after its update")
    print("[%s] AzWriter 0.1.3 installed through its own Update now and started again" % TAG, flush=True)


def stop_leftovers(box):
    pid_file = os.path.join(box.root, "manager.pid")
    if os.path.exists(pid_file):
        try:
            os.kill(int(open(pid_file).read().strip()), signal.SIGTERM)
        except (OSError, ValueError):
            pass
    # The AzWriter the manager started again runs from the sandbox.
    subprocess.call(["pkill", "-f", os.path.join(box.azlin, "apps")], stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL)


def body(args, logs, out):
    manager_bin = e2e.find_binary("AzInstallManager", args.bin, "INSTALLMGR_BIN")
    writer_bin = e2e.find_binary("AzWriter", args.writer_bin, "AZWRITER_BIN")
    with open(writer_bin, "rb") as f:
        writer_bytes = f.read()
    box = Sandbox(logs)
    rel = Releases(os.path.join(box.root, "releases"))
    # No update source: the wizard asks for it (set_source), as on a fresh computer.
    edit_config(box.config, lambda u: u.update({
        "cohort": 60,
    }))
    rel.publish("runtime", "stable", "0.1.0", b"the azul runtime 0.1.0")
    rel.publish("azwriter", "stable", "0.1.0", writer_bytes)
    rel.publish("azclock", "stable", "0.1.0", b"clock 0.1.0")
    try:
        # The first start installs through the (fake) package manager; it installs nothing, so
        # the next start is a first run again.
        app = start_manager("through-manager", manager_bin, box, logs, args)
        try:
            through_package_manager(app, box, out)
        except Failure:
            print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
            raise
        finally:
            app.stop()

        app = start_manager("wizard", manager_bin, box, logs, args)
        try:
            walk_wizard(app, box, rel, out)
            check_installed_files(box, writer_bytes)
            table(app, box, rel, writer_bytes, out)
        except Failure:
            print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
            raise
        finally:
            app.stop()

        # A root key in the config: the unsigned release is refused.
        rel.publish("azwriter", "stable", "0.1.3", writer_bytes)
        edit_config(box.config, lambda u: u.update({
            "root_public_key": "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3",
        }))
        app = start_manager("signed", manager_bin, box, logs, args)
        try:
            app.until("the apps", lambda: app.last("AZINSTALLMGR_SCREEN") == "apps")
            first_check(app)
            result = app.last("AZINSTALLMGR_CHECKED")
            if result != "available=0 failed=1":
                raise Failure("the unsigned release was not refused: %r" % result)
            if not app.shows("UNSIGNED"):
                raise Failure("the refusal does not say the release is unsigned")
            app.screenshot(os.path.join(out, "12b-unsigned.png"))
        finally:
            app.stop()
        edit_config(box.config, lambda u: u.pop("root_public_key", None))

        banner(args, logs, out, box, rel, writer_bytes, manager_bin)
    finally:
        stop_leftovers(box)
        rel.stop()
        if not args.keep:
            shutil.rmtree(box.home, ignore_errors=True)
    print("[%s] PASS: the wizard, the table, update / revert / beta / uninstall, refused releases, "
          "the banner and Update now; screenshots in %s" % (TAG, out), flush=True)
    return True


if __name__ == "__main__":
    # --writer-bin: the AzWriter program published as the AzWriter release.
    argv = sys.argv[1:]
    writer = None
    if "--writer-bin" in argv:
        i = argv.index("--writer-bin")
        writer = argv[i + 1]
        del argv[i:i + 2]

    def run_body(args, logs, out):
        args.writer_bin = writer
        return body(args, logs, out)

    e2e.run(TAG, run_body, argv=argv, default_port=8774)
