#!/usr/bin/env python3
"""AzPlayer end to end, headless, over the debug server.

    1. makes a 6-second test video with ffmpeg (an H.264 test pattern and an AAC 440 Hz tone in
       an MP4) - skipped (the empty library is checked instead) when ffmpeg is missing;
    2. starts AzPlayer on it (AZ_SYNTHETIC_DEVICES=audio_sink: the sound plays in real time on
       the synthetic output): AZPLAYER_OPEN, the stage and the controls bar; the picture plays
       (AZPLAYER_STATE playing) where this machine decodes H.264 (VideoToolbox on macOS) - a
       machine without a decoder shows the stage's note instead;
    3. Space pauses (AZPLAYER_STATE paused), Space plays on; M mutes (the OSD says "Muted");
    4. F goes fullscreen (the title row goes), Escape comes back;
    5. a second run without a file shows the library: "Continue watching" and the test video,
       with its progress, from player/history.json in the data tree;
    6. screenshots after each step, flat light; dark at the end.

Usage (after building libazul with the debug server and AzPlayer; ONE app at a time, through
scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azplayer_e2e.py [--bin target/release/AzPlayer] [--debug-port 8792]
        [--timeout 180] [--out <dir>] [--keep]
"""

import os
import shutil
import subprocess

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azplayer"


def make_video(path):
    """A 6 s 640x360 H.264 + AAC MP4, or None without ffmpeg."""
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        return None
    subprocess.run(
        [ffmpeg, "-y", "-loglevel", "error",
         "-f", "lavfi", "-i", "testsrc=size=640x360:rate=25:duration=6",
         "-f", "lavfi", "-i", "sine=frequency=440:duration=6",
         "-c:v", "libx264", "-pix_fmt", "yuv420p", "-profile:v", "main",
         "-c:a", "aac", "-shortest", path],
        check=True)
    return path


def phase(app):
    last = app.last("AZPLAYER_STATE")
    return last.split()[0] if last else None


def body(args, logs, out):
    binary = e2e.find_binary("AzPlayer", args.bin, "AZPLAYER_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    video = make_video(os.path.join(logs, "test-video.mp4"))
    env = {"AZ_SYNTHETIC_DEVICES": "audio_sink"}
    common = ["--data-dir", data_dir, "--size", "1100x700", "--theme", "flat", "--mode", "light"]

    if video:
        app = e2e.App(TAG, binary, common + [video], args.debug_port, logs, args.timeout,
                      extra_env=env)
        try:
            app.until("the file to open", lambda: app.last("AZPLAYER_OPEN"))
            app.frame(3)
            app.until("the stage", lambda: app.has_id("__azplayer_stage"))
            if not app.has_id("__azplayer_seek"):
                raise Failure("the controls bar (its seek bar) is not shown while loading")
            app.until("the picture to play or fail", lambda: phase(app) in ("playing", "failed"))
            if phase(app) == "failed":
                app.log("no H.264 decoder here: %s" % app.last("AZPLAYER_STATE"))
                if not app.has_id("__azplayer_note"):
                    raise Failure("a video that cannot play shows no note")
            else:
                app.screenshot(os.path.join(out, "1-playing.png"))
                app.key("space")
                app.until("paused", lambda: phase(app) == "paused")
                app.key("space")
                app.until("playing again", lambda: phase(app) == "playing")
                app.key("m")
                app.until("the mute OSD", lambda: app.has_id("__azplayer_osd") and app.shows("Muted"))
                app.screenshot(os.path.join(out, "2-muted.png"))
                app.key("f")
                app.frame(3)
                app.screenshot(os.path.join(out, "3-fullscreen.png"))
                app.key("escape")
                app.frame(3)
            app.must("set_mode", mode="dark")
            app.must("wait_settled")
            app.screenshot(os.path.join(out, "4-dark.png"))
        except Failure:
            print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
            raise
        finally:
            app.stop()
    else:
        app_log = "ffmpeg not found: the playback steps are skipped"
        print("[%s] %s" % (TAG, app_log))

    # ---- the library, from the history file ----
    app = e2e.App(TAG + "-library", binary, common, args.debug_port, logs, args.timeout,
                  extra_env=env)
    try:
        app.until("the history", lambda: app.last("AZPLAYER_HISTORY") is not None)
        app.frame(3)
        app.until("the library", lambda: app.has_id("__azplayer_library"))
        if video:
            if not app.shows("Continue watching") or not app.shows("test-video"):
                raise Failure("the library does not list the test video")
        elif not app.shows("No videos yet"):
            raise Failure("an empty library shows no empty state")
        if not app.has_id("__azplayer_open"):
            raise Failure("the library has no Open button")
        app.screenshot(os.path.join(out, "5-library.png"))
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8792)
