#!/usr/bin/env python3
"""AzPlayer end to end, headless, over the debug server (the Windows Media Center look).

    1. makes a media tree with ffmpeg: Videos/test-video.mp4 (6 s, an H.264 test pattern and an
       AAC 440 Hz tone), Music/E2E Album/01 First Tone.m4a + 02 Second Tone.m4a (tagged AAC),
       Pictures/Trip/card-1.png + card-2.png, an empty Recorded TV folder - without ffmpeg the
       folders stay empty and the empty states are checked instead;
    2. starts AzPlayer on it (`--music-dir`, `--pictures-dir`, `--videos-dir`, `--tv-dir`: every
       setting is a switch; AZ_SYNTHETIC_DEVICES=audio_sink - the engine's - lets the sound play
       in real time on the synthetic output): the scans report every library, the START STRIP
       shows, the arrows move its focus (AZPLAYER_FOCUS);
    3. every section opens from the strip and Back (Backspace) comes back: music (its views move:
       AZPLAYER_VIEW), pictures (a folder, a picture, the next one, the slide show), videos,
       movies, recorded tv, recently played; a disabled item (radio, extras library) says why
       (AZPLAYER_NOTICE);
    3b. NO TITLE ROW: the top band is the window's drag region (-azul-app-region: drag), the back
       button and the clock are no-drag, on macOS the back button clears the traffic lights;
    3c. MORE INFO (Ctrl+D) on a tile opens Media Center's panel (AZPLAYER_OVERLAY more-info open,
       #__azplayer_overlay-more-info), Back closes it and stays on the page;
    3d. TAB walks the page's zones: the back button (AZPLAYER_ZONE corner) STAYS shown while it
       has the keys - longer than the 2 s the pointer's chrome waits (AZPLAYER_CHROME, its
       opacity) - Tab goes back to the page, Shift+Tab to the button, Enter presses it;
    3e. THE SETTINGS are AzPlayer's own pages (page-settings, page-settings-general): "show the
       clock" turned off (AZPLAYER_OPTION), save (AZPLAYER_SETTINGS saved, the file written),
       the clock is gone; library setup lists the music folder; Escape leaves;
    4. music: play all -> now playing; the transport buttons pause, play, skip; Space pauses,
       Back asks (AZPLAYER_OVERLAY music-paused open) and Escape keeps it paused; on the start
       strip the inset shows, Tab reaches it, its more info's "stop" ends the music and the
       inset goes;
    5. THE VIDEO'S CURTAIN: a video opens behind the menus - the first picture and the first sound
       are ready (AZPLAYER_PREROLL picture / sound) BEFORE the menus fade (AZPLAYER_CURTAIN
       fade-out), picture and sound start together (AZPLAYER_CURTAIN play, AZPLAYER_AUDIO ready),
       the video plays (AZPLAYER_STATE playing) and the curtain opens; the order of the markers is
       checked; then Space pauses, the round play button plays, M mutes (the OSD), F / Escape
       fullscreen, Backspace closes it (AZPLAYER_CLOSE) and the page under it comes back;
    5b. NETWORK STREAMING: the test video is served by a local file server answering range
       requests; "open an address" (Media Center's dialog with the field: AZPLAYER_OVERLAY
       address open) plays it behind the same curtain (the picture and the sound read while
       they download: AZPLAYER_AUDIO ready, AZPLAYER_STATE playing);
    6. a second run on the same data folder, `--screen recent`: the test video is in recently
       played (player/history.json), the clock stays off (the saved setting), no now-playing
       inset (the stopped music stays gone); the video's more info removes it from recently
       played;
    7. screenshots of the start strip, a library, now playing, the picture viewer, the curtain
       mid-fade, the playing video, the recent page.

Usage (after building libazul with the debug server and AzPlayer; ONE app at a time, through
scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azplayer_e2e.py [--bin target/release/AzPlayer] [--debug-port 8792]
        [--timeout 240] [--out <dir>] [--keep]
"""

import base64
import http.server
import os
import re
import shutil
import subprocess
import sys
import threading
import time

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azplayer"
# The width macOS's traffic lights take at the window's top left (AzMusic's check).
TRAFFIC_LIGHTS_W = 70.0


def ffmpeg():
    return shutil.which("ffmpeg")


def run_ffmpeg(args):
    subprocess.run([ffmpeg(), "-y", "-loglevel", "error"] + args, check=True)


def make_media(root):
    """The media tree under `root`; returns the folders and whether media were made."""
    folders = {
        "music": os.path.join(root, "Music"),
        "pictures": os.path.join(root, "Pictures"),
        "videos": os.path.join(root, "Videos"),
        "tv": os.path.join(root, "Videos", "Recorded TV"),
    }
    for path in folders.values():
        os.makedirs(path, exist_ok=True)
    if not ffmpeg():
        return folders, False
    run_ffmpeg(["-f", "lavfi", "-i", "testsrc=size=640x360:rate=25:duration=6",
                "-f", "lavfi", "-i", "sine=frequency=440:duration=6",
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-profile:v", "main",
                "-c:a", "aac", "-shortest", os.path.join(folders["videos"], "test-video.mp4")])
    album = os.path.join(folders["music"], "E2E Album")
    os.makedirs(album, exist_ok=True)
    for number, (title, freq) in enumerate((("First Tone", 440), ("Second Tone", 660)), 1):
        run_ffmpeg(["-f", "lavfi", "-i", "sine=frequency=%d:duration=8" % freq,
                    "-c:a", "aac",
                    "-metadata", "title=%s" % title,
                    "-metadata", "artist=AzPlayer",
                    "-metadata", "album=E2E Album",
                    "-metadata", "track=%d" % number,
                    os.path.join(album, "%02d %s.m4a" % (number, title))])
    trip = os.path.join(folders["pictures"], "Trip")
    os.makedirs(trip, exist_ok=True)
    for number, pattern in ((1, "testsrc"), (2, "smptebars")):
        run_ffmpeg(["-f", "lavfi", "-i", "%s=size=800x600:rate=1" % pattern, "-frames:v", "1",
                    os.path.join(trip, "card-%d.png" % number)])
    return folders, True


def page(app):
    return app.last("AZPLAYER_PAGE")


def wait_page(app, name):
    app.until("the page %s" % name, lambda: page(app) == name)
    app.until("#__azplayer_%s in the window" % name, lambda: app.has_id("__azplayer_" + name))


def key_to_page(app, key, name):
    app.key(key)
    wait_page(app, name)


def back_to_start(app):
    for _ in range(6):
        if page(app) == "page-start":
            break
        app.key("backspace")
    wait_page(app, "page-start")


def focus(app):
    return app.last("AZPLAYER_FOCUS")


def move_strip(app, key, expected):
    app.key(key)
    app.until("the strip's focus on %s" % expected, lambda: focus(app) == expected)


def shot_now(app, path):
    """A screenshot without settling first: a moment of a transition."""
    value = app.value("take_screenshot")
    data = value.get("data") if isinstance(value, dict) else None
    if isinstance(data, str) and "base64," in data:
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        app.log("screenshot %s (mid-transition)" % path)


def line_index(app, key, pattern):
    """The line number of the first `<key> <pattern>` line of the app's stdout (-1: none)."""
    try:
        with open(app.out_path, "r", encoding="utf-8", errors="replace") as f:
            lines = f.read().splitlines()
    except OSError:
        return -1
    for i, line in enumerate(lines):
        if re.fullmatch(r"%s %s" % (re.escape(key), pattern), line):
            return i
    return -1


def phase(app):
    last = app.last("AZPLAYER_STATE")
    return last.split()[0] if last else None


def overlay(app):
    """The last `AZPLAYER_OVERLAY <name> <open|closed>`."""
    return app.last("AZPLAYER_OVERLAY")


def option(app, key):
    """The last value AzPlayer printed for the option `key` (`AZPLAYER_OPTION <key> <value>`:
    every option at the start, then a settings page's changes)."""
    values = app.printed("AZPLAYER_OPTION", r"%s \S+" % re.escape(key))
    return values[-1].split(" ", 1)[1] if values else None


def css_of(app, selector):
    """A node's computed CSS (`get_node_css_properties`) as {property: value}."""
    value = app.value("get_node_css_properties", selector=selector)
    props = value.get("properties") if isinstance(value, dict) else None
    found = {}
    for line in props or []:
        if isinstance(line, str) and ":" in line:
            key, val = line.split(":", 1)
            found[key.strip()] = val.strip()
    return found


def opacity_of(app, selector):
    """A node's computed opacity, 0..1 (1 when the server reports none)."""
    raw = css_of(app, selector).get("opacity")
    if not raw:
        return 1.0
    try:
        number = float(raw.rstrip("%").strip())
    except ValueError:
        return 1.0
    return number / 100.0 if raw.endswith("%") or number > 1.0 else number


def check_band(app):
    """No title row: the top band is the window's drag region, the back button and the clock
    keep their presses (`no-drag`), and on macOS the back button clears the traffic lights."""
    if app.has(".csd-title"):
        raise Failure("a title row is back over the media center")
    region = css_of(app, "#__azplayer_band").get("-azul-app-region")
    if region != "drag":
        raise Failure("the top band is not the window's drag region (-azul-app-region %r)"
                      % region)
    for control in ("back", "clock"):
        got = css_of(app, "#__azplayer_%s" % control).get("-azul-app-region")
        if got != "no-drag":
            raise Failure("a press on #__azplayer_%s would move the window (%r)" % (control, got))
    if sys.platform == "darwin":
        back = app.rect("__azplayer_back")
        if float(back.get("x", 0.0)) < TRAFFIC_LIGHTS_W:
            raise Failure("the back button sits under the traffic lights: %r" % back)
    app.log("the top band moves the window, its controls are their own")


class RangeHandler(http.server.SimpleHTTPRequestHandler):
    """A file server that answers range requests (`206`, `Content-Range`), as a video host
    does: what AzPlayer's streaming reads a window ahead with."""

    def log_message(self, *args):
        pass

    def do_HEAD(self):
        self.serve(head=True)

    def do_GET(self):
        self.serve(head=False)

    def serve(self, head):
        path = self.translate_path(self.path)
        if not os.path.isfile(path):
            self.send_error(404)
            return
        size = os.path.getsize(path)
        start, end = 0, size - 1
        ranged = False
        match = re.match(r"bytes=(\d*)-(\d*)$", self.headers.get("Range", ""))
        if match and (match.group(1) or match.group(2)):
            ranged = True
            if match.group(1):
                start = int(match.group(1))
                end = int(match.group(2)) if match.group(2) else size - 1
            else:
                start = max(0, size - int(match.group(2)))
            end = min(end, size - 1)
            if start > end:
                self.send_response(416)
                self.send_header("Content-Range", "bytes */%d" % size)
                self.end_headers()
                return
        self.send_response(206 if ranged else 200)
        self.send_header("Content-Type", "video/mp4")
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(end - start + 1))
        if ranged:
            self.send_header("Content-Range", "bytes %d-%d/%d" % (start, end, size))
        self.end_headers()
        if head:
            return
        with open(path, "rb") as f:
            f.seek(start)
            left = end - start + 1
            while left > 0:
                block = f.read(min(65536, left))
                if not block:
                    break
                self.wfile.write(block)
                left -= len(block)


def serve_folder(folder):
    """A range-answering file server for `folder` on a free local port: (server, base URL)."""
    handler = lambda *a, **k: RangeHandler(*a, directory=folder, **k)  # noqa: E731
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, "http://127.0.0.1:%d" % server.server_address[1]


def check_strip_and_sections(app, out, have_media, folders):
    wait_page(app, "page-start")
    if not app.has_id("__azplayer_strip"):
        raise Failure("the start strip has no strip")
    for word in ("music", "pictures + videos", "movies", "tasks"):
        if not app.shows(word):
            raise Failure("the start strip does not show %r" % word)
    app.screenshot(os.path.join(out, "1-start-strip.png"))

    # The strip opens on music; the arrows move between the categories and along the items.
    move_strip(app, "up", "pictures + videos / picture library")
    move_strip(app, "down", "music / music library")
    move_strip(app, "right", "music / play all")
    move_strip(app, "left", "music / music library")

    # MUSIC: its page, its views.
    key_to_page(app, "return", "page-music")
    for word in ("albums", "artists", "genres", "songs"):
        if not app.shows(word):
            raise Failure("the music page's views row has no %r" % word)
    if have_media and not app.shows("E2E Album"):
        app.until("the album in the music gallery", lambda: app.shows("E2E Album"))
    app.key("up")
    app.key("right")
    app.until("the artists view", lambda: app.last("AZPLAYER_VIEW") == "artists")
    app.screenshot(os.path.join(out, "2-music.png"))

    # NO TITLE ROW: the band moves the window, its controls are their own.
    check_band(app)

    # MORE INFO (Ctrl+D, Media Center's button) on a tile: its panel; Back closes it.
    if have_media:
        app.key("down")
        app.until("a tile of the gallery focused", lambda: (focus(app) or "").startswith("tile "))
        app.key("d", ctrl=True)
        app.until("the more-info panel", lambda: overlay(app) == "more-info open")
        app.until("#__azplayer_overlay-more-info in the window",
                  lambda: app.has_id("__azplayer_overlay-more-info"))
        if not app.shows("add to queue"):
            raise Failure("an artist's more info offers no 'add to queue'")
        app.screenshot(os.path.join(out, "2b-more-info.png"))
        app.key("backspace")
        app.until("Back closes more info", lambda: overlay(app) == "more-info closed")
        if page(app) != "page-music":
            raise Failure("Back on more info left the page: %r" % page(app))

    # TAB walks the page's zones: the back button STAYS while it has the keys, however long the
    # pointer rests ("pressing tab can also make the back button disappear").
    app.key("tab")
    app.until("Tab: the keys on the back button", lambda: app.last("AZPLAYER_ZONE") == "corner")
    time.sleep(2.6)
    app.frame(3)
    if app.last("AZPLAYER_CHROME") == "hidden":
        raise Failure("the back button hid while it had the keys")
    seen = opacity_of(app, "#__azplayer_corner")
    if seen < 0.5:
        raise Failure("the back button is not shown while it has the keys (opacity %.2f)" % seen)
    if not app.laid_out("#__azplayer_back"):
        raise Failure("no back button to have the keys")
    app.screenshot(os.path.join(out, "2c-tab-back.png"))
    app.key("tab")
    app.until("Tab: the keys back on the page", lambda: app.last("AZPLAYER_ZONE") == "content")
    app.key("tab", shift=True)
    app.until("Shift+Tab: the back button", lambda: app.last("AZPLAYER_ZONE") == "corner")
    # Enter presses the button the keys are on.
    key_to_page(app, "return", "page-start")

    # A disabled item says why: radio.
    move_strip(app, "right", "music / play all")
    move_strip(app, "right", "music / radio")
    app.key("return")
    app.until("radio says why", lambda: (app.last("AZPLAYER_NOTICE") or "").startswith(
        "There is no radio tuner"))
    move_strip(app, "left", "music / play all")
    move_strip(app, "left", "music / music library")

    # PICTURES: the library, a folder, a picture, the next one, the slide show.
    move_strip(app, "up", "pictures + videos / picture library")
    key_to_page(app, "return", "page-pictures")
    if have_media:
        app.until("the Trip folder", lambda: app.shows("Trip"))
        app.key("return")
        app.until("the folder's page", lambda: app.last("AZPLAYER_GROUP") == "Trip")
        app.key("return")
        app.until("the picture viewer", lambda: app.last("AZPLAYER_PICTURE") == "0 still")
        app.key("right")
        app.until("the next picture", lambda: app.last("AZPLAYER_PICTURE") == "1 still")
        app.screenshot(os.path.join(out, "3-picture.png"))
        app.key("space")
        app.until("the slide show", lambda: app.last("AZPLAYER_SLIDESHOW") == "playing")
        app.key("space")
        app.until("the slide show paused", lambda: app.last("AZPLAYER_SLIDESHOW") == "paused")
    else:
        app.until("the empty pictures page", lambda: app.shows("no pictures"))
    back_to_start(app)

    # VIDEOS, MOVIES, RECORDED TV. Back kept the strip's focus where it was left: on
    # pictures + videos (Media Center's start strip remembers it).
    app.until("the strip's focus back on pictures + videos",
              lambda: focus(app) == "pictures + videos / picture library")
    move_strip(app, "right", "pictures + videos / play favorites")
    move_strip(app, "right", "pictures + videos / video library")
    key_to_page(app, "return", "page-videos")
    key_to_page(app, "backspace", "page-start")
    move_strip(app, "down", "music / music library")
    move_strip(app, "down", "movies / movie library")
    key_to_page(app, "return", "page-movies")
    if not app.shows("40 minutes"):
        app.until("the movies' empty state (no video is 40 minutes long)",
                  lambda: app.shows("40 minutes"))
    key_to_page(app, "backspace", "page-start")
    move_strip(app, "down", "tv / recorded tv")
    key_to_page(app, "return", "page-tv")
    app.until("recorded TV's empty state", lambda: app.shows("no recorded TV"))
    key_to_page(app, "backspace", "page-start")

    # TASKS: the settings, AzPlayer's own pages.
    check_settings(app, out, folders)

    # EXTRAS: the extras library says why; recently played opens.
    for _ in range(5):
        app.key("up")
    app.until("extras", lambda: (focus(app) or "").startswith("extras /"))
    app.key("return")
    app.until("extras says why", lambda: (app.last("AZPLAYER_NOTICE") or "").startswith(
        "No extras"))
    move_strip(app, "right", "extras / recently played")
    key_to_page(app, "return", "page-recent")
    if not app.has_id("__azplayer_open"):
        raise Failure("recently played has no open tile")
    key_to_page(app, "backspace", "page-start")
    # Back to music.
    move_strip(app, "down", "pictures + videos / video library")
    move_strip(app, "down", "music / music library")


def check_settings(app, out, folders):
    """THE SETTINGS, AzPlayer's own ten-foot pages: the categories; general's "show the clock"
    turned off and saved (the settings file is written: a second run sees it) - the clock goes;
    library setup lists the music folder; Escape cancels a page and leaves the settings."""
    move_strip(app, "down", "tasks / settings")
    key_to_page(app, "return", "page-settings")
    for word in ("general", "library setup", "start-up & window", "about"):
        if not app.shows(word):
            raise Failure("the settings do not list %r" % word)
    app.screenshot(os.path.join(out, "9-settings.png"))
    key_to_page(app, "return", "page-settings-general")
    app.key("down")
    app.until("the clock's check box", lambda: focus(app) == "setting show the clock")
    app.key("return")
    app.until("the clock turned off in the draft",
              lambda: option(app, "player.show_clock") == "false")
    app.key("down")
    app.until("save", lambda: focus(app) == "setting save")
    app.screenshot(os.path.join(out, "9b-settings-general.png"))
    written = app.count("AZPLAYER_SETTINGS_SAVED")
    app.key("return")
    app.until("saved", lambda: app.last("AZPLAYER_SETTINGS") == "saved general 1")
    app.until("the settings file written",
              lambda: app.count("AZPLAYER_SETTINGS_SAVED") > written)
    wait_page(app, "page-settings")
    app.until("the clock gone", lambda: not app.has_id("__azplayer_clock"))
    # Library setup lists each library's folders.
    for _ in range(5):
        app.key("down")
    app.until("library setup", lambda: focus(app) == "category library setup")
    key_to_page(app, "return", "page-settings-library")
    app.until("the music folder in library setup", lambda: app.shows(folders["music"]))
    app.screenshot(os.path.join(out, "9c-library-setup.png"))
    key_to_page(app, "escape", "page-settings")
    if app.last("AZPLAYER_SETTINGS") != "cancel library":
        raise Failure("Escape on a settings page is not cancel: %r"
                      % app.last("AZPLAYER_SETTINGS"))
    key_to_page(app, "escape", "page-start")


def check_music(app, out, have_media):
    move_strip(app, "right", "music / play all")
    app.key("return")
    if not have_media:
        app.until("play all says why", lambda: (app.last("AZPLAYER_NOTICE") or "").startswith(
            "There is no music"))
        move_strip(app, "left", "music / music library")
        return
    wait_page(app, "page-now-playing")
    app.until("the music plays", lambda: app.last("AZPLAYER_MUSIC") == "playing")
    app.screenshot(os.path.join(out, "4-now-playing.png"))
    app.click(selector="#__azplayer_play")
    app.until("paused", lambda: app.last("AZPLAYER_MUSIC") == "paused")
    app.click(selector="#__azplayer_play")
    app.until("playing again", lambda: app.last("AZPLAYER_MUSIC") == "playing")
    songs = app.count("AZPLAYER_MUSIC", r"play .*")
    app.click(selector="#__azplayer_next")
    app.until("the next song", lambda: app.count("AZPLAYER_MUSIC", r"play .*") > songs)

    # PAUSED, then Back: Media Center asks - stop it, or keep it paused (Escape keeps it).
    app.key("space")
    app.until("paused by Space", lambda: app.last("AZPLAYER_MUSIC") == "paused")
    app.key("backspace")
    app.until("Back asks about the paused music", lambda: overlay(app) == "music-paused open")
    app.screenshot(os.path.join(out, "4b-music-paused.png"))
    app.key("escape")
    app.until("kept paused", lambda: overlay(app) == "music-paused closed")
    wait_page(app, "page-start")
    app.until("the paused song's inset", lambda: app.has_id("__azplayer_inset"))

    # The inset can be dismissed: Tab reaches it, its more info's stop ends the music.
    app.key("tab")
    app.until("Tab: the inset", lambda: app.last("AZPLAYER_ZONE") == "inset")
    app.key("d", ctrl=True)
    app.until("the inset's more info", lambda: overlay(app) == "more-info open")
    for _ in range(4):
        if focus(app) == "choice stop":
            break
        app.key("down")
    app.until("stop focused", lambda: focus(app) == "choice stop")
    app.key("return")
    app.until("stopped", lambda: app.last("AZPLAYER_MUSIC") == "stopped")
    app.until("the inset gone", lambda: not app.has_id("__azplayer_inset"))
    back_to_start(app)
    move_strip(app, "left", "music / music library")


def check_video(app, out):
    """The curtain: the picture and the sound ready before the menus fade, both started
    together, the picture fading in; then the transport."""
    # The category kept its item: the video library.
    move_strip(app, "up", "pictures + videos / video library")
    key_to_page(app, "return", "page-videos")
    # The "title" view lists the videos themselves.
    app.key("up")
    app.key("right")
    app.key("right")
    app.until("the title view", lambda: app.last("AZPLAYER_VIEW") == "title")
    app.key("down")
    app.until("the test video in the gallery", lambda: app.shows("test-video"))
    app.key("return")
    app.until("the video opening", lambda: app.last("AZPLAYER_OPEN"))
    app.until("the fade to black", lambda: app.last("AZPLAYER_CURTAIN") in
              ("fade-out", "play", "open") or phase(app) == "failed")
    if phase(app) == "failed":
        app.log("no H.264 decoder here: %s" % app.last("AZPLAYER_STATE"))
        app.until("the failed video's note", lambda: app.has_id("__azplayer_note"))
        app.key("backspace")
        wait_page(app, "page-videos")
        back_to_start(app)
        return
    shot_now(app, os.path.join(out, "5-curtain.png"))
    app.until("the curtain open", lambda: app.last("AZPLAYER_CURTAIN") == "open")
    app.until("the picture plays", lambda: phase(app) == "playing")
    # The order: nothing moved before both were ready, the play after the fade.
    preroll = line_index(app, "AZPLAYER_CURTAIN", "preroll")
    picture = line_index(app, "AZPLAYER_PREROLL", r"picture .*")
    sound = line_index(app, "AZPLAYER_PREROLL", r"sound .*")
    fade = line_index(app, "AZPLAYER_CURTAIN", "fade-out")
    play = line_index(app, "AZPLAYER_CURTAIN", "play")
    opened = line_index(app, "AZPLAYER_CURTAIN", "open")
    playing = line_index(app, "AZPLAYER_STATE", r"playing .*")
    order = [preroll, picture, sound, fade, play, opened]
    if min(order) < 0:
        raise Failure("a curtain marker is missing: %r" % order)
    if not (preroll < picture < fade and preroll < sound < fade and fade < play < opened):
        raise Failure("the curtain's order is wrong: preroll %d picture %d sound %d fade %d "
                      "play %d open %d" % tuple(order))
    if playing < play:
        raise Failure("the picture played before the curtain said play (%d < %d)" % (playing, play))
    audio = app.last("AZPLAYER_AUDIO") or ""
    if not audio.startswith("ready"):
        raise Failure("the sound was not ready when the picture started: %r" % audio)
    app.screenshot(os.path.join(out, "6-video-playing.png"))
    for stem in ("seek", "bar", "top", "play", "back", "fullscreen", "osd"):
        app.until("#__azplayer_%s on the stage" % stem, lambda: app.has_id("__azplayer_" + stem))
    app.key("space")
    app.until("paused", lambda: phase(app) == "paused")
    app.click(selector="#__azplayer_play")
    app.until("playing again", lambda: phase(app) == "playing")
    app.key("m")
    app.until("the mute OSD", lambda: app.shows("Muted"))
    app.key("f")
    app.until("fullscreen", lambda: app.last("AZPLAYER_FULLSCREEN") == "on")
    app.key("escape")
    app.until("fullscreen left", lambda: app.last("AZPLAYER_FULLSCREEN") == "off")
    closes = app.count("AZPLAYER_CLOSE")
    app.key("backspace")
    app.until("the video closed", lambda: app.count("AZPLAYER_CLOSE") > closes)
    wait_page(app, "page-videos")
    back_to_start(app)


def check_address(app, out, base_url):
    """Open an address: the test video from a local server answering range requests plays
    behind the same curtain - the picture and the sound read while they download."""
    move_strip(app, "down", "music / music library")
    move_strip(app, "down", "movies / movie library")
    move_strip(app, "right", "movies / open a file")
    move_strip(app, "right", "movies / open an address")
    # Media Center's dialog over the strip: the field (it has the keys), play, a sample.
    app.key("return")
    app.until("the address dialog", lambda: overlay(app) == "address open")
    app.until("its field", lambda: app.has_id("__azplayer_address-field"))
    app.screenshot(os.path.join(out, "6a-address-dialog.png"))
    url = base_url + "/test-video.mp4"
    app.text_input("#__azplayer_address-field", url)
    opens = app.count("AZPLAYER_CURTAIN", "open")
    playing = app.count("AZPLAYER_STATE", r"playing .*")
    app.key("return")
    app.until("the address played", lambda: app.last("AZPLAYER_ADDRESS") == url)
    app.until("the address's curtain open", lambda: app.count("AZPLAYER_CURTAIN", "open") > opens
              or phase(app) == "failed")
    if phase(app) == "failed":
        raise Failure("the video at %s does not play: %s" % (url, app.last("AZPLAYER_STATE")))
    app.until("the address's picture plays",
              lambda: app.count("AZPLAYER_STATE", r"playing .*") > playing)
    audio = app.last("AZPLAYER_AUDIO") or ""
    if not audio.startswith("ready"):
        raise Failure("the sound at the address was not ready with the picture: %r" % audio)
    app.screenshot(os.path.join(out, "6b-address-playing.png"))
    closes = app.count("AZPLAYER_CLOSE")
    app.key("backspace")
    app.until("the address's video closed", lambda: app.count("AZPLAYER_CLOSE") > closes)
    # The dialog closed when the video opened: the strip under it comes back.
    wait_page(app, "page-start")
    if app.has_id("__azplayer_overlay-address"):
        raise Failure("the address dialog is still there after its video")
    move_strip(app, "up", "music / music library")


def body(args, logs, out):
    binary = e2e.find_binary("AzPlayer", args.bin, "AZPLAYER_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    folders, have_media = make_media(os.path.join(logs, "media"))
    if not have_media:
        print("[%s] ffmpeg not found: the libraries stay empty, the empty states are checked"
              % TAG)
    env = {"AZ_SYNTHETIC_DEVICES": "audio_sink"}
    common = ["--data-dir", data_dir, "--size", "1100x700", "--theme", "flat", "--mode", "light",
              "--music-dir", folders["music"], "--pictures-dir", folders["pictures"],
              "--videos-dir", folders["videos"], "--tv-dir", folders["tv"]]

    app = e2e.App(TAG, binary, common, args.debug_port, logs, args.timeout, extra_env=env)
    try:
        app.until("the history", lambda: app.last("AZPLAYER_HISTORY") is not None)
        for library in ("music", "pictures", "videos", "recorded-tv"):
            app.until("the %s scan" % library,
                      lambda library=library: app.printed("AZPLAYER_SCAN", r"%s \d+ \w+" % library))
        app.frame(3)
        check_strip_and_sections(app, out, have_media, folders)
        check_music(app, out, have_media)
        if have_media:
            check_video(app, out)
            server, base_url = serve_folder(folders["videos"])
            try:
                check_address(app, out, base_url)
            finally:
                server.shutdown()
        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "7-dark.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # ---- recently played, from the history file ----
    app = e2e.App(TAG + "-recent", binary, common + ["--screen", "recent"], args.debug_port, logs,
                  args.timeout, extra_env=env)
    try:
        app.until("the history", lambda: app.last("AZPLAYER_HISTORY") is not None)
        app.frame(3)
        wait_page(app, "page-recent")
        if not app.has_id("__azplayer_open"):
            raise Failure("recently played has no open tile")
        # The saved setting holds: the clock stays off. The music stopped in the first run
        # stays gone: no now-playing inset.
        if option(app, "player.show_clock") != "false":
            raise Failure("the clock setting was not kept: %r" % option(app, "player.show_clock"))
        if app.has_id("__azplayer_clock"):
            raise Failure("the clock shows though the settings turned it off")
        if app.has_id("__azplayer_inset"):
            raise Failure("the stopped music's inset is back after a restart")
        if have_media:
            if app.last("AZPLAYER_HISTORY") == "0":
                raise Failure("the history is empty after a video played")
            app.until("the test video in recently played", lambda: app.shows("test-video"))
        app.screenshot(os.path.join(out, "8-recent.png"))
        if have_media:
            # The video's more info (below the open tile) removes it from recently played.
            app.key("down")
            app.until("the recent video focused", lambda: focus(app) == "tile 1 column 0")
            app.key("d", ctrl=True)
            app.until("the video's more info", lambda: overlay(app) == "more-info open")
            for _ in range(4):
                if focus(app) == "choice remove from recently played":
                    break
                app.key("down")
            app.until("remove focused",
                      lambda: focus(app) == "choice remove from recently played")
            # (The file and the address of the same clip are both there: one goes.)
            before = int(app.last("AZPLAYER_HISTORY") or "0")
            app.key("return")
            app.until("forgotten",
                      lambda: app.last("AZPLAYER_HISTORY") == str(max(before - 1, 0)))
            app.until("more info closed", lambda: overlay(app) == "more-info closed")
        app.key("backspace")
        wait_page(app, "page-start")
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8792)
