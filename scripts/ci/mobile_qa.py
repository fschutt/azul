#!/usr/bin/env python3
"""mobile_qa.py: drive ONE phone app through the lifecycle on a running emulator or simulator and say, per step,
whether it is alive, in the foreground and showing something (standard library only).

    mobile_qa.py android --out DIR --apk app.apk [--package P] [--activity A] [--label L] [--gpu MODE]
    mobile_qa.py ios     --out DIR --udid UDID --bundle B [--label L] [--landscape-app app.app]

It runs after the device booted (scripts/ci/android_qa.sh / ios_qa.sh start it) and writes
    DIR/qa.json      every step: {name, ok, seconds, checks:[{what, ok, detail}], notes, shots}
    DIR/qa.md        the same as a table a person reads
    DIR/shots/*.png  one picture per step (written from the device's raw bitmap: no lossy step)
    DIR/logs/*       logcat / crash buffer / dumpsys (Android), console + simulator log (iOS)

WHAT A PASS MEANS (the first boot tests said "alive" about an app that had never drawn; a launcher screen is not
blank): a step passes when the app's process is alive AND the app is the foreground app (Android:
mCurrentFocus names the package; iOS: the screen differs from the home screen taken before the launch) AND the
screenshot is not one flat colour AND it is not the launcher (its signature differs from the home screen's by
more than a threshold) AND the crash log holds nothing of the app since the step began. A step that is SUPPOSED
to leave the app (home) is judged by the app still being alive afterwards, a step that resumes it by the app
being back with the same checks.

Screenshots are compared as SIGNATURES: a grid of 40 x 80 sampled pixels (RGB) taken from the raw bitmap, so no
image library is needed. flat = one colour in 99.9 % of the samples; same = the mean channel difference of two
signatures under 1.5 %.
"""
import argparse
import json
import os
import re
import statistics
import struct
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import zlib
from pathlib import Path

COLS, ROWS = 40, 80
FLAT_FRACTION = 0.999
SAME_BELOW = 0.015          # mean |difference| / 255 under which two screens count as the same picture
LAUNCHER_BELOW = 0.02       # an app screen this close to the home screen is the home screen


# ---------------------------------------------------------------------------------------------
# pictures: signatures from raw bitmaps, a PNG writer (no image library on a runner)
# ---------------------------------------------------------------------------------------------
def sample_grid(width, height):
    xs = [min(width - 1, int((i + 0.5) * width / COLS)) for i in range(COLS)]
    ys = [min(height - 1, int((j + 0.5) * height / ROWS)) for j in range(ROWS)]
    return xs, ys


def signature_rgba(pixels, width, height, offset=0, stride=None, order="rgba", bottom_up=False):
    """RGB samples (bytes, 3 per sample) of a 32-bit bitmap. `order` is the byte order of a pixel ('rgba' or
    'bgra'); `bottom_up` is a BMP's row order."""
    stride = stride or width * 4
    xs, ys = sample_grid(width, height)
    out = bytearray()
    for y in ys:
        row = (height - 1 - y) if bottom_up else y
        base = offset + row * stride
        for x in xs:
            p = base + x * 4
            if order == "bgra":
                out += bytes((pixels[p + 2], pixels[p + 1], pixels[p]))
            else:
                out += pixels[p:p + 3]
    return bytes(out)


def signature_android_raw(raw):
    """`adb exec-out screencap` (no -p): u32 width, height, format (+ a u32 colour space on Android 10+), then the
    pixels. -> (signature, width, height, rgba bytes) or raises ValueError."""
    if len(raw) < 16:
        raise ValueError("screencap output too short")
    width, height, fmt = struct.unpack_from("<III", raw, 0)
    body = width * height * 4
    header = len(raw) - body
    if header not in (12, 16) or fmt not in (1, 2):
        raise ValueError(f"unexpected screencap layout: {width}x{height} format {fmt}, header {header} bytes")
    pixels = raw[header:]
    return signature_rgba(pixels, width, height), width, height, pixels


def parse_bmp(data):
    """A 24- or 32-bit BMP (`sips -s format bmp` writes a 32-bit V5 one, top-down, with channel masks).
    -> (signature, width, height, rgba bytes)."""
    if data[:2] != b"BM":
        raise ValueError("not a BMP")
    offset = struct.unpack_from("<I", data, 10)[0]
    dib = struct.unpack_from("<I", data, 14)[0]
    width, height, _planes, bpp = struct.unpack_from("<iihh", data, 18)
    compression = struct.unpack_from("<I", data, 30)[0] if dib >= 40 else 0
    bottom_up = height > 0
    height = abs(height)
    if bpp not in (24, 32):
        raise ValueError(f"BMP with {bpp} bits per pixel")
    # where each channel sits in a pixel's bytes (little endian): from the masks when there are any
    index = {"r": 2, "g": 1, "b": 0, "a": 3}
    if bpp == 32 and compression == 3 and dib >= 52:
        masks = struct.unpack_from("<IIII", data, 54) if dib >= 56 else struct.unpack_from("<III", data, 54) + (0,)
        for name, mask in zip("rgba", masks):
            if mask and mask.bit_length() % 8 == 0 and (mask & (mask - 1) == 0 or mask in (0xFF, 0xFF00, 0xFF0000, 0xFF000000)):
                index[name] = (mask.bit_length() - 1) // 8
    step = bpp // 8
    stride = (width * step + 3) // 4 * 4
    xs, ys = sample_grid(width, height)
    sig = bytearray()
    for y in ys:
        row = offset + ((height - 1 - y) if bottom_up else y) * stride
        for x in xs:
            p = row + x * step
            sig += bytes((data[p + index["r"]], data[p + index["g"]], data[p + index["b"]]))
    rgba = bytearray(width * height * 4)
    for y in range(height):
        src = offset + ((height - 1 - y) if bottom_up else y) * stride
        row = data[src:src + width * step]
        dst = y * width * 4
        rgba[dst + 0:dst + width * 4:4] = row[index["r"]::step]
        rgba[dst + 1:dst + width * 4:4] = row[index["g"]::step]
        rgba[dst + 2:dst + width * 4:4] = row[index["b"]::step]
        rgba[dst + 3:dst + width * 4:4] = (row[index["a"]::step] if bpp == 32 else b"\xff" * width)
    return bytes(sig), width, height, bytes(rgba)


def png_size(path):
    """(width, height) of a PNG file from its IHDR."""
    with open(path, "rb") as fh:
        head = fh.read(24)
    if head[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    return struct.unpack(">II", head[16:24])


def write_png(path, width, height, rgba):
    def chunk(kind, body):
        crc = zlib.crc32(kind + body) & 0xFFFFFFFF
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)
    stride = width * 4
    rows = b"".join(b"\x00" + rgba[y * stride:(y + 1) * stride] for y in range(height))
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(rows, 6)) + chunk(b"IEND", b""))
    Path(path).write_bytes(png)


def flat_fraction(sig):
    """The share of the samples that has the most common colour (channels quantised to 16 levels)."""
    counts = {}
    for i in range(0, len(sig), 3):
        key = (sig[i] >> 4, sig[i + 1] >> 4, sig[i + 2] >> 4)
        counts[key] = counts.get(key, 0) + 1
    return max(counts.values()) / (len(sig) // 3) if sig else 1.0


def is_flat(sig):
    return flat_fraction(sig) >= FLAT_FRACTION


def sig_diff(a, b):
    """Mean |difference| of two signatures as a share of 255 (0 = identical, 1 = black against white)."""
    if not a or not b or len(a) != len(b):
        return 1.0
    return sum(abs(x - y) for x, y in zip(a, b)) / (255.0 * len(a))


# the stated thresholds of the CPU-against-GPU picture comparison (renderers may differ in anti-aliasing, not in layout)
PIXEL_TOLERANCE = 24          # a pixel DIFFERS when any of its channels differs by more than this (of 255)
MAX_DIFFERING_SHARE = 0.03    # the comparison passes below this share of differing pixels ...
MAX_MEAN_DIFF = 2.0           # ... and below this mean |channel difference| (of 255)


def diff_pixels(a, b, width, height, tolerance=PIXEL_TOLERANCE):
    """Compare two RGBA bitmaps of the same size pixel by pixel.
    -> {differing, share, mean_abs, max_abs, bbox: (x0, y0, x1, y1) | None, image: RGBA bytes}; the image is the first
    picture dimmed to a third with every differing pixel red, so a person sees WHERE the renderers disagree."""
    stride = width * 4
    differing = 0
    total_abs = 0
    max_abs = 0
    x0, y0, x1, y1 = width, height, -1, -1
    out = bytearray(len(a))
    for y in range(height):
        ra, rb = a[y * stride:(y + 1) * stride], b[y * stride:(y + 1) * stride]
        base = y * stride
        if ra == rb:
            row = bytearray(ra)
            row[0::4] = bytes(v // 3 for v in ra[0::4])
            row[1::4] = bytes(v // 3 for v in ra[1::4])
            row[2::4] = bytes(v // 3 for v in ra[2::4])
            out[base:base + stride] = row
            continue
        row = bytearray(ra)
        for x in range(width):
            i = x * 4
            d0, d1, d2 = abs(ra[i] - rb[i]), abs(ra[i + 1] - rb[i + 1]), abs(ra[i + 2] - rb[i + 2])
            total_abs += d0 + d1 + d2
            m = max(d0, d1, d2)
            if m > max_abs:
                max_abs = m
            if m > tolerance:
                differing += 1
                row[i], row[i + 1], row[i + 2] = 255, 0, 0
                if x < x0:
                    x0 = x
                if x > x1:
                    x1 = x
                if y < y0:
                    y0 = y
                y1 = y
            else:
                row[i], row[i + 1], row[i + 2] = ra[i] // 3, ra[i + 1] // 3, ra[i + 2] // 3
        out[base:base + stride] = row
    return {"differing": differing, "share": differing / float(width * height), "mean_abs": total_abs / float(width * height * 3),
            "max_abs": max_abs, "bbox": (x0, y0, x1, y1) if differing else None, "image": bytes(out)}


# The one line a phone shell writes when its window gets a renderer (the contract the GPU work is held to):
#     [render] backend=<cpu|gpu> requested=<cpu|gpu|auto> device=<free text>
# `AZ_LOG=info` shows it in logcat (RustStdoutStderr) / on the console. A build that has no such line is "unknown".
BACKEND_LINE = re.compile(r"\[render\][^\n]*?\bbackend=(\w+)(?:[^\n]*?\brequested=(\w+))?(?:[^\n]*?\bdevice=([^\n]*))?")


def backend_ran(text):
    """-> (ran, requested, device) from the engine's `[render] backend=` line (the LAST one), or ("unknown", None, None)."""
    found = None
    for line in text.splitlines():
        m = BACKEND_LINE.search(line)
        if m:
            found = (m.group(1).lower(), (m.group(2) or "").lower() or None, (m.group(3) or "").strip() or None)
    return found or ("unknown", None, None)


# The one diagnostic of a request for the GPU that fell back (the engine writes it BEFORE the contract line, once):
#     [render] gpu unavailable at <stage>: <cause>; drawing on the CPU
FALLBACK_LINE = re.compile(r"\[render\] gpu unavailable at (\w+): (.*?); drawing on the CPU")


def gpu_fallbacks(text):
    """-> [(stage, cause)]: every fallback line of the engine's log, in order."""
    found = []
    for line in text.splitlines():
        m = FALLBACK_LINE.search(line)
        if m:
            found.append((m.group(1), m.group(2).strip()))
    return found


def gpu_fail_stage(spec):
    """The stage the engine's fallback line names when `AZ_GPU_FAIL=<spec>` is set (`<stage>` or `<stage>:<n>`): the stage itself,
    except `frame` (a frame that cannot go out is a lost context, and a second one with no good frame between ends the GPU
    path at `lost`) and `lost`."""
    word = spec.split(":", 1)[0].strip().lower().replace("-", "_")
    return "lost" if word in ("frame", "lost") else word


# The frame times the engine measures of its own presents (shell2/common/frame_stats.rs): while frames come one after another,
# one line a second, and one for a burst that ends:
#     [render] frames n=62 over_ms=1003 interval_ms median=16.7 p95=17.9 max=33.4 draw_ms median=4.1 p95=7.9
FRAMES_LINE = re.compile(r"\[render\] frames n=(\d+) over_ms=(\d+) interval_ms median=([\d.]+) p95=([\d.]+) max=([\d.]+)"
                         r"(?: draw_ms median=([\d.]+) p95=([\d.]+))?")


def engine_frames(text):
    """-> [{n, over_ms, median, p95, max, draw_median, draw_p95}] for every frames line of the engine's log, in order
    (`draw_*` are None for the CPU path, which does not time its draw)."""
    found = []
    for line in text.splitlines():
        m = FRAMES_LINE.search(line)
        if m:
            found.append({"n": int(m.group(1)), "over_ms": int(m.group(2)), "median": float(m.group(3)), "p95": float(m.group(4)),
                          "max": float(m.group(5)), "draw_median": float(m.group(6)) if m.group(6) else None,
                          "draw_p95": float(m.group(7)) if m.group(7) else None})
    return found


def swipe_windows(text):
    """The frames lines of a scripted swipe that report a FULL window (the engine writes one when a burst has run for a second:
    `over_ms` >= 1000). The line that closes a burst on the next frame after an idle gap is the tail of a burst BEFORE the swipe
    (under a second, since a burst that long was reported already): it is not the swipe's. -> [the dicts of `engine_frames`]"""
    return [l for l in engine_frames(text) if l["over_ms"] >= 1000]


def summarize_frames(lines):
    """The frame times of a whole scripted swipe from the engine's frames lines (`engine_frames`): the MEDIAN OF THE MEDIANS of the
    lines inside it, the WORST p95 and the number of frames: {"median", "p95", "n"} (the three keys `report_modes` and the
    "Render backends" table read), plus `draw_median` / `draw_p95` when the lines carry the draw times (the GPU path). None for no line."""
    if not lines:
        return None
    out = {"median": round(statistics.median(l["median"] for l in lines), 1), "p95": max(l["p95"] for l in lines),
           "n": sum(l["n"] for l in lines)}
    draws = [l for l in lines if l.get("draw_median") is not None]
    if draws:
        out["draw_median"] = round(statistics.median(l["draw_median"] for l in draws), 1)
        out["draw_p95"] = max(l["draw_p95"] for l in draws)
    return out


# What proves that no GL command went out while the app was in the background: the GPU window writes a line when its GL commands
# stop (after the last command of the answer to `didEnterBackground`) and one when they are allowed again (before the first command
# of the answer to the return). A frame or a failed action between the two lines is a command in the background.
GL_STOPPED = re.compile(r"\[iOS GPU\] GL commands stopped")
GL_ALLOWED = re.compile(r"\[iOS GPU\] GL commands allowed again")
GL_WORK = re.compile(r"\[render\] frames n=|\[iOS GPU\] frame \d+x\d+|\[iOS GPU\] .* failed at ")
MEMORY_RELEASED = re.compile(r"\[iOS GPU\] memory pressure: the renderer's caches are released")


def gl_in_background(lines):
    """The console lines of ONE stay in the background -> {"stopped": n, "allowed": n, "inside": [the lines of GL work between a
    `stopped` and the next `allowed again`, or the end of the lines]}. `inside` must be empty."""
    stopped = allowed = 0
    off = False
    inside = []
    for line in lines:
        if GL_STOPPED.search(line):
            stopped += 1
            off = True
        elif GL_ALLOWED.search(line):
            allowed += 1
            off = False
        elif off and GL_WORK.search(line):
            inside.append(line.strip()[:200])
    return {"stopped": stopped, "allowed": allowed, "inside": inside}


def find_key(obj, key):
    """The first value under `key` anywhere in a decoded JSON answer (the debug server nests what it says), or None."""
    if isinstance(obj, dict):
        if key in obj:
            return obj[key]
        for v in obj.values():
            found = find_key(v, key)
            if found is not None:
                return found
    elif isinstance(obj, list):
        for v in obj:
            found = find_key(v, key)
            if found is not None:
                return found
    return None


def percentile(values, p):
    v = sorted(values)
    if not v:
        return None
    k = max(0, min(len(v) - 1, int(round(p / 100.0 * (len(v) - 1)))))
    return v[k]


def frame_intervals_ms(present_times_ns):
    """Frame times (ms) from the sorted present timestamps (ns) of SurfaceFlinger's --latency: zero and the
    9223372036854775807 'pending' marker are skipped."""
    ts = sorted(t for t in present_times_ns if 0 < t < 9223372036854775807)
    return [(b - a) / 1e6 for a, b in zip(ts, ts[1:])]


def parse_sf_latency(text):
    """`dumpsys SurfaceFlinger --latency <layer>`: line 1 the refresh period (ns), then rows `desired actual ready`.
    -> (refresh_ms, [actual present timestamps in ns])"""
    lines = [l.split() for l in text.strip().splitlines() if l.strip()]
    if not lines or len(lines[0]) != 1:
        return None, []
    refresh = int(lines[0][0]) / 1e6
    return refresh, [int(l[1]) for l in lines[1:] if len(l) == 3]


# ---------------------------------------------------------------------------------------------
# parsing what the platforms say
# ---------------------------------------------------------------------------------------------
def parse_focus(text):
    """The first `mCurrentFocus=Window{... u0 pkg/activity}` line of `dumpsys window`: the window's name, or ''."""
    for line in text.splitlines():
        m = re.search(r"mCurrentFocus=Window\{[0-9a-f]+ u\d+ ([^}]+)\}", line)
        if m:
            return m.group(1).strip()
    return ""


def parse_ime_shown(text):
    """True when the soft keyboard is up (`mInputShown=true` / `isInputViewShown=true` in `dumpsys input_method`)."""
    return bool(re.search(r"(mInputShown|isInputViewShown|mIsInputViewShown)=true", text))


def parse_ime_inset(text):
    """The IME's inset source in `dumpsys window` (API 30+): `InsetsSource type=ime frame=[0,1360][1080,2400] visible=true`
    (older: `mType=ITYPE_IME ... mVisible=true`). -> True / False, None when no such line is there. The inset is the
    window system's own account of the keyboard, which `dumpsys input_method` stopped giving a stable field for (API 34:
    the keyboard was on the screen while `mInputShown` said nothing)."""
    seen = None
    for line in text.splitlines():
        if re.search(r"type=ime\b|ITYPE_IME", line):
            m = re.search(r"\b(?:visible|mVisible)=(true|false)", line)
            if m:
                seen = (m.group(1) == "true") or bool(seen)
    return seen


def parse_bounds(xml_text):
    """[(label, cx, cy)] for the nodes of a `uiautomator dump` that carry a text or a content description."""
    found = []
    for m in re.finditer(r"<node\b([^>]*)>", xml_text):
        attrs = dict(re.findall(r'([\w-]+)="([^"]*)"', m.group(1)))
        label = attrs.get("text") or attrs.get("content-desc") or ""
        b = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", attrs.get("bounds", ""))
        if label and b:
            l, t, r, bt = (int(v) for v in b.groups())
            found.append((label, (l + r) // 2, (t + bt) // 2))
    return found


# "has died" is NOT here: the Android shell ends its own process after BACK on purpose (libazul is not re-entrant), and the
# system logs that as `Process ... has died`. A real crash says FATAL EXCEPTION / Fatal signal / a panic, and the exit
# reasons (parse_exit_reasons) name it: APP CRASH, CRASH_NATIVE, ANR.
FATAL_PATTERNS = ("FATAL EXCEPTION", "Fatal signal", "SIGSEGV", "SIGABRT", "SIGBUS", "UnsatisfiedLinkError",
                  "dlopen failed", "panicked at", "An unexpected panic", "ANR in")
CRASH_EXIT_REASONS = (4, 5, 6, 7)       # ApplicationExitInfo: CRASH, CRASH_NATIVE, ANR, INITIALIZATION_FAILURE


def parse_exit_reasons(text):
    """The entries of `dumpsys activity exit-info <package>` -> [(timestamp, pid, reason code, reason name)], newest first."""
    out = []
    for m in re.finditer(r"timestamp=(\S+ \S+) pid=(\d+)[^\n]*\n\s*process=\S+ reason=(\d+) \((.*?)\) subreason=", text):
        out.append((m.group(1), int(m.group(2)), int(m.group(3)), m.group(4)))
    return out


def create_activity_lines(events_text, package):
    """The `am_create_activity` / `wm_create_activity` events of the package in the events buffer."""
    return [l for l in events_text.splitlines() if ("am_create_activity" in l or "wm_create_activity" in l) and package in l]


def fatal_lines(text, package):
    """Lines of a log that say the app died (a crash of another package is not this app's verdict)."""
    out = []
    for line in text.splitlines():
        if any(p in line for p in FATAL_PATTERNS) and (package in line or "azul" in line.lower()
                                                         or "RustStdoutStderr" in line or "libazul" in line
                                                         or "DEBUG" in line):
            out.append(line.strip())
    return out


# ---------------------------------------------------------------------------------------------
# the record of a run
# ---------------------------------------------------------------------------------------------
class Step:
    def __init__(self, run, name, what):
        self.run, self.name, self.what = run, name, what
        self.checks, self.notes, self.shots = [], [], []
        self.started = time.time()
        self.seconds = 0.0
        self.error = ""

    def check(self, what, ok, detail="", required=True):
        self.checks.append({"what": what, "ok": bool(ok), "detail": str(detail), "required": required})
        return bool(ok)

    def note(self, text):
        self.notes.append(str(text))

    @property
    def ok(self):
        return not self.error and all(c["ok"] or not c["required"] for c in self.checks)

    def as_json(self):
        return {"name": self.name, "what": self.what, "ok": self.ok, "seconds": round(self.seconds, 1),
                "error": self.error, "checks": self.checks, "notes": self.notes, "shots": self.shots}


class Run:
    def __init__(self, out, platform, meta):
        self.out = Path(out)
        self.shots_dir = self.out / "shots"
        self.logs_dir = self.out / "logs"
        for d in (self.out, self.shots_dir, self.logs_dir):
            d.mkdir(parents=True, exist_ok=True)
        self.platform, self.meta, self.steps = platform, dict(meta), []
        self.t0 = time.time()
        self.recover = None
        self.render_table = []       # rows of the "Render backends" section of qa.md (filled by report_modes)
        self.render_diff = None
        self.only = None             # {"03", "08", ...}: run only these steps (00-02, the device and the install, always run)
        self.extra_shots = 100       # diff images are numbered from 100 so they never collide with a step's own pictures

    def next_shot_number(self):
        self.extra_shots += 1
        return self.extra_shots

    def step(self, name, what, fn, needs_app=False):
        """Run `fn(step)`; an exception is the step's failure, never the run's. A step that `needs_app` is
        preceded by `self.recover(step)` (when one is set): a step is judged on a live app, so the crash of an
        earlier step is reported once, by that step, and does not fail every step after it."""
        if self.only is not None:
            prefix = name.split("-")[0]
            if prefix not in ("00", "01", "02", "e00", "e01") and prefix not in self.only and prefix.rstrip("abcdefghijklmnopqrstuvwxyz") not in self.only:
                return None
        s = Step(self, name, what)
        self.steps.append(s)
        print(f"--- {name}: {what}", flush=True)
        try:
            if needs_app and self.recover is not None:
                self.recover(s)
            fn(s)
        except Exception as e:  # noqa: BLE001
            s.error = f"{type(e).__name__}: {e}"
            print(f"    EXCEPTION {s.error}", flush=True)
        s.seconds = time.time() - s.started
        for c in s.checks:
            print(f"    [{'ok' if c['ok'] else ('FAIL' if c['required'] else 'info')}] {c['what']}"
                  f"{': ' + c['detail'] if c['detail'] else ''}", flush=True)
        self.write()
        return s

    def write(self):
        doc = {"platform": self.platform, "meta": self.meta, "ok": all(s.ok for s in self.steps),
               "render": {"table": self.render_table, "cpu_against_gpu": self.render_diff},
               "steps": [s.as_json() for s in self.steps]}
        (self.out / "qa.json").write_text(json.dumps(doc, indent=2), encoding="utf-8")
        lines = [f"# {self.platform} QA: {self.meta.get('label', '')}", "",
                 "  ".join(f"{k}={v}" for k, v in sorted(self.meta.items()) if k != "label"), "",
                 "| step | result | seconds | what it checked | notes |", "|---|---|---|---|---|"]
        for s in self.steps:
            bad = [c for c in s.checks if not c["ok"] and c["required"]]
            res = "PASS" if s.ok else "FAIL"
            why = "; ".join(f"{c['what']} ({c['detail']})" if c["detail"] else c["what"] for c in bad) or s.error
            lines.append(f"| {s.name} | {res} | {s.seconds:.0f} | {s.what} | {(why if not s.ok else '; '.join(s.notes))[:300]} |")
        # The standing section every run fills the same way (the GPU work reads and writes the same columns):
        # one row per AZ_BACKEND request, which renderer RAN, and the numbers per backend
        if self.render_table:
            lines += ["", "## Render backends: requested against ran", "",
                      "| requested | ran | device | Displayed ms | first frame seen s | scroll frame ms median | p95 | frames |",
                      "|---|---|---|---|---|---|---|---|"]
            for r in self.render_table:
                lines.append("| {requested} | {ran} | {device} | {displayed} | {first} | {median} | {p95} | {n} |".format(**r))
            if self.render_diff:
                d = self.render_diff
                lines += ["", f"cpu against gpu, pixel by pixel (a pixel differs above {PIXEL_TOLERANCE}/255; pass below {MAX_DIFFERING_SHARE * 100:.0f} % "
                          f"of the pixels and a mean difference below {MAX_MEAN_DIFF}/255): {d['share'] * 100:.2f} % of the pixels differ, "
                          f"mean {d['mean_abs']:.2f}/255, max {d['max_abs']}" + (f", box {d['bbox']}" if d.get("bbox") else "")
                          + f" - {'PASS' if d['ok'] else 'FAIL'}; the diff image is shots/{d['image']}"]
        (self.out / "qa.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def displayed_ms(text, package=""):
    """The system's own first-frame time: `ActivityTaskManager: Displayed pkg/act: +1s304ms` -> 1304 (the last one)."""
    found = None
    for line in text.splitlines():
        m = re.search(r"Displayed \S*" + re.escape(package) + r"\S*(?: for user \d+)?: \+(?:(\d+)s)?(\d+)ms", line)
        if m:
            found = int(m.group(1) or 0) * 1000 + int(m.group(2))
    return found


LOGCAT_LINE = re.compile(r"^(\d\d)-(\d\d) (\d\d):(\d\d):(\d\d)\.(\d{3})\s+(\d+)\s+(\d+)\s+\w\s+(\S.*?):\s(.*)$")


def _logcat_ms(m):
    """Milliseconds since the start of the month of a `logcat -v threadtime` line (a start never spans a month)."""
    day, hour, minute, second, milli = (int(m.group(i)) for i in (2, 3, 4, 5, 6))
    return (((day * 24 + hour) * 60 + minute) * 60 + second) * 1000 + milli


def start_timelines(text, package):
    """One record per start of `package` in a `logcat -v threadtime` dump, in milliseconds after the system's `START u0 {cmp=<package>/..}`:
    `proc` (the fork was asked for), `main` (the engine's `android_main entered`, its first line), `displayed` (the system's
    own `Displayed ... +NNNms`: the window's first frame, which can be a blank one) and `present` (the first `present full`
    of the engine, AZ_DUMP_PRESENT=1: the first picture of the app). Keys are missing for what the log does not show."""
    records, by_pid = [], {}
    for line in text.splitlines():
        m = LOGCAT_LINE.match(line)
        if not m:
            continue
        pid, tag, msg, now = m.group(7), m.group(9), m.group(10), _logcat_ms(m)
        if tag == "ActivityTaskManager" and msg.startswith("START u0 ") and f"cmp={package}/" in msg:
            records.append({"t0": now})
        elif tag == "ActivityManager" and records and (forked := re.match(r"Start proc (\d+):" + re.escape(package) + "/", msg)):
            rec = next((r for r in reversed(records) if "proc" not in r), None)
            if rec is not None:
                rec["proc"] = now - rec["t0"]
                by_pid[forked.group(1)] = rec
        elif tag == "ActivityTaskManager" and msg.startswith(f"Displayed {package}/"):
            shown = re.search(r": \+(?:(\d+)s)?(\d+)ms", msg)
            rec = next((r for r in reversed(records) if "displayed" not in r), None)
            if shown and rec is not None:
                rec["displayed"] = int(shown.group(1) or 0) * 1000 + int(shown.group(2))
        elif tag == "RustStdoutStderr" and pid in by_pid:
            rec = by_pid[pid]
            if "android_main entered" in msg and "main" not in rec:
                rec["main"] = now - rec["t0"]
            elif "present full" in msg and "present" not in rec and "main" in rec:
                rec["present"] = now - rec["t0"]
    return [{k: v for k, v in r.items() if k != "t0"} for r in records]


def summarize_timelines(records):
    """The median of each mark over the records that have it: {mark: {"median", "min", "max", "n"}}."""
    out = {}
    for key in ("proc", "main", "displayed", "present"):
        values = sorted(r[key] for r in records if key in r)
        if values:
            out[key] = {"median": values[len(values) // 2], "min": values[0], "max": values[-1], "n": len(values)}
    gaps = sorted(r["present"] - r["main"] for r in records if "present" in r and "main" in r)
    if gaps:
        out["main_to_present"] = {"median": gaps[len(gaps) // 2], "min": gaps[0], "max": gaps[-1], "n": len(gaps)}
    return out


SOFTWARE_DEVICE = re.compile(r"swiftshader|llvmpipe|softpipe|swrast|software renderer|android emulator|apple software|angle.*(swiftshader|software)", re.I)


def device_is_software(device):
    """Does the contract line's `device=` text name a software GL renderer (SwiftShader, llvmpipe, 'Apple Software Renderer', ...)?"""
    return bool(device and SOFTWARE_DEVICE.search(device))


GPU_FIRST_FRAME_SECONDS = 120
IDLE_SECONDS = 4.0         # an idle window is watched this long for frames lines (a window that draws while nothing moves)
SWIPE_SECONDS = 4.0        # the scripted swipe of the render-mode step: at least three full seconds of frames lines


def first_frame_timeout_for(mode, base):
    """How long to wait for the first frame of an AZ_BACKEND run. A `gpu` request on a software GL (every CI emulator and Simulator)
    compiles each WebRender shader at its first use (an LLVM JIT) and can take about two minutes to the first frame; waiting
    less would call a slow start a missing picture. The other modes keep the normal wait."""
    return max(base, GPU_FIRST_FRAME_SECONDS) if mode == "gpu" else base


def report_modes(run, step, results, expect_gpu, platform, hardware=False):
    """The verdict of the AZ_BACKEND runs. `results[mode]` = {ran, requested, device, rgba, dims, first_frame_s,
    displayed_ms, frames: {median, p95, n} | None}. Says, per mode, WHICH backend ran: a request for the GPU that ran
    the CPU renderer is a FINDING (informational while the build has no GPU path, a FAILURE once `--expect-gpu` says it
    has), and compares the cpu and gpu pictures pixel by pixel with the stated thresholds, keeping the diff image."""
    summary = []
    for mode in ("cpu", "gpu", "auto"):
        r = results.get(mode)
        if not r:
            continue
        ran = r["ran"]
        bits = [f"ran {ran}" + (f" on {r['device']}" if r.get("device") else "")]
        if r.get("displayed_ms") is not None:
            bits.append(f"Displayed {r['displayed_ms']} ms")
        if r.get("first_frame_s") is not None:
            bits.append(f"first frame seen after {r['first_frame_s']:.1f} s")
        f = r.get("frames")
        if f:
            bits.append(f"scroll frame time median {f['median']:.1f} / p95 {f['p95']:.1f} ms (n={f['n']})")
        summary.append(f"{mode}: " + ", ".join(bits))
        step.note(f"AZ_BACKEND={mode}: " + ", ".join(bits))
        software = (not hardware) or device_is_software(r.get("device"))
        if mode == "cpu":
            step.check("requested cpu: the CPU renderer ran", ran in ("cpu", "unknown"), f"ran {ran}", required=True)
        elif mode == "gpu":
            # `gpu` FORCES the GL path on any renderer that works, a software GL included (SwiftShader, 'Apple Software Renderer')
            if ran == "gpu":
                step.check("requested gpu: the GL renderer ran", True, r.get("device") or "")
            elif ran == "cpu":
                step.check("requested gpu: the GL renderer ran", False,
                           "requested gpu, ran cpu: the CPU renderer drew the page (finding while the build has no GL path; a failure with --expect-gpu)",
                           required=expect_gpu)
            else:
                step.check("requested gpu: the engine says which renderer ran", False,
                           "no `[render] backend=` line in the engine's log: it cannot be told which renderer drew", required=expect_gpu)
        else:
            # `auto` takes the GL path ONLY on a hardware renderer with ES >= 3.0; on a software GL (every CI emulator and
            # Simulator) it runs the CPU renderer with one diagnostic line: 'requested auto, ran cpu' is then the DESIGN
            if ran == "unknown":
                step.check("requested auto: the engine says which renderer ran", False,
                           "no `[render] backend=` line in the engine's log: it cannot be told which renderer drew", required=False)
            elif software:
                step.check("requested auto on a software GL: the CPU renderer runs (by design)", ran == "cpu",
                           f"ran {ran}" + ("" if ran == "cpu" else ": auto took the GL path on a software renderer, it must wait for hardware"),
                           required=True)
            else:
                step.check("requested auto on a hardware GPU: the GL renderer runs", ran == "gpu",
                           f"ran {ran} on {r.get('device') or '?'}", required=expect_gpu)
    run.meta["render_modes"] = " | ".join(summary)
    run.render_table = []
    for mode in ("cpu", "gpu", "auto"):
        r = results.get(mode)
        if not r:
            continue
        f = r.get("frames") or {}
        run.render_table.append({
            "requested": mode, "ran": r["ran"], "device": (r.get("device") or "-")[:60],
            "displayed": r["displayed_ms"] if r.get("displayed_ms") is not None else "n/a",
            "first": f"{r['first_frame_s']:.1f}" if r.get("first_frame_s") is not None else "n/a",
            "median": f"{f['median']:.1f}" if f else "n/a", "p95": f"{f['p95']:.1f}" if f else "n/a", "n": f["n"] if f else "n/a"})
    cpu, gpu = results.get("cpu"), results.get("gpu")
    if cpu and gpu and cpu.get("rgba") and gpu.get("rgba") and cpu["dims"] == gpu["dims"]:
        w, h = cpu["dims"]
        d = diff_pixels(cpu["rgba"], gpu["rgba"], w, h)
        fname = f"{run.next_shot_number():02d}-diff-cpu-vs-gpu.png"
        write_png(run.shots_dir / fname, w, h, d["image"])
        step.shots.append(fname)
        verdict = d["share"] < MAX_DIFFERING_SHARE and d["mean_abs"] < MAX_MEAN_DIFF
        run.render_diff = {"share": d["share"], "mean_abs": d["mean_abs"], "max_abs": d["max_abs"], "bbox": d["bbox"], "ok": verdict, "image": fname}
        step.check("the cpu and gpu pictures agree", verdict,
                   f"{d['share'] * 100:.2f} % of the pixels differ by more than {PIXEL_TOLERANCE}/255 (limit {MAX_DIFFERING_SHARE * 100:.0f} %), "
                   f"mean |diff| {d['mean_abs']:.2f}/255 (limit {MAX_MEAN_DIFF}), max {d['max_abs']}"
                   + (f", box {d['bbox']}" if d["bbox"] else ""))
        step.note("the cpu-vs-gpu comparison runs on " + (f"{ran_pair(cpu, gpu)}" ))
    elif cpu and gpu:
        step.check("the cpu and gpu pictures could be compared", False, "no pixels or different sizes", required=False)


def ran_pair(cpu, gpu):
    same = cpu["ran"] == gpu["ran"]
    return (f"two runs of the SAME renderer ({cpu['ran']}): it proves nothing about a GPU path until the backends differ" if same
            else f"the {cpu['ran']} and the {gpu['ran']} renderer")


# ---------------------------------------------------------------------------------------------
# Android
# ---------------------------------------------------------------------------------------------
class Android:
    def __init__(self, run, package, activity="", serial=""):
        self.run, self.package, self.activity, self.serial = run, package, activity, serial
        self.home_sig = None
        self.last_sig = None
        self.last_rgba = None
        self.dims = (0, 0)
        self.shot_count = 0
        self.ime_dumps = 0

    # -- plumbing
    def adb(self, *args, timeout=90, binary=False):
        cmd = ["adb"] + (["-s", self.serial] if self.serial else []) + list(args)
        try:
            p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        except subprocess.TimeoutExpired:
            return (b"" if binary else "")
        return p.stdout if binary else p.stdout.decode("utf-8", "replace").replace("\r", "")

    def sh(self, command, timeout=90):
        return self.adb("shell", command, timeout=timeout)

    # -- state
    def pid(self):
        return self.sh(f"pidof {self.package}").strip().split(" ")[0]

    def alive(self):
        return bool(self.pid())

    def focus(self):
        return parse_focus(self.sh("dumpsys window | grep -E 'mCurrentFocus'"))

    def foreground(self):
        return self.package in self.focus()

    def ime_shown(self):
        """Is the soft keyboard up? The window system's IME inset first, the input method service's own fields as a
        fallback; the raw dumps of the first calls go to logs/ime-*.txt so the detection can be checked against them."""
        inset = parse_ime_inset(self.sh("dumpsys window | grep -iE 'type=ime|ITYPE_IME'"))
        if self.ime_dumps < 3:
            self.ime_dumps += 1
            self.save_log(f"ime-{self.ime_dumps}.txt", "== dumpsys window (ime lines)\n" + self.sh("dumpsys window | grep -iE 'type=ime|ITYPE_IME|InputMethod'")
                          + "\n== dumpsys input_method\n" + self.sh("dumpsys input_method | head -n 120"))
        if inset is not None:
            return inset
        return parse_ime_shown(self.sh("dumpsys input_method | grep -E 'mInputShown|isInputViewShown'"))

    def find_label(self, label):
        """The centre of the first on-screen accessibility node named `label` (uiautomator dump of the window), or None."""
        self.sh("uiautomator dump /sdcard/qa-ui.xml", timeout=40)
        for name, cx, cy in parse_bounds(self.sh("cat /sdcard/qa-ui.xml")):
            if name == label and cx > 0 and cy > 0:
                return cx, cy
        return None

    def tap_label(self, label, fallback=None):
        """Tap the node named `label`; when the tree does not show it, tap `fallback` (x, y) instead. -> (x, y, how)"""
        found = self.find_label(label)
        if found:
            self.tap(*found)
            return found[0], found[1], "accessibility tree"
        if fallback:
            self.tap(*fallback)
            return fallback[0], fallback[1], "fixed position"
        return None, None, "not found"

    def hide_ime(self):
        """BACK closes the keyboard when it is up (and only then: BACK on a closed keyboard leaves the app)."""
        if self.ime_shown():
            self.back()
            time.sleep(1.5)

    def resolve_activity(self):
        if self.activity:
            return self.activity
        out = self.sh(f"cmd package resolve-activity --brief -c android.intent.category.LAUNCHER {self.package}")
        last = [l for l in out.splitlines() if "/" in l]
        if last:
            comp = last[-1].strip()
            self.activity = comp.split("/", 1)[1]
            if self.activity.startswith("."):
                self.activity = self.package + self.activity
        return self.activity

    # -- the picture
    def capture(self, name):
        """One raw capture -> PNG file + signature. -> (signature, width, height)."""
        raw = self.adb("exec-out", "screencap", binary=True, timeout=60)
        sig, w, h, rgba = signature_android_raw(raw)
        self.shot_count += 1
        fname = f"{self.shot_count:02d}-{name}.png"
        write_png(self.run.shots_dir / fname, w, h, rgba)
        self.last_sig = sig
        self.last_rgba = rgba
        self.dims = (w, h)
        return sig, w, h, fname

    def look(self, step, name, expect_changed_from=None, allow_launcher=False):
        """Take a screenshot as part of `step` and judge it. -> the signature."""
        sig, w, h, fname = self.capture(name)
        step.shots.append(fname)
        flat = flat_fraction(sig)
        step.check("the screen is not one flat colour", flat < FLAT_FRACTION, f"{flat:.4f} of the samples share one colour ({w}x{h})")
        if self.home_sig is not None and not allow_launcher:
            d = sig_diff(sig, self.home_sig)
            step.check("the screen is not the launcher", d > LAUNCHER_BELOW, f"differs {d:.3f} from the home screen")
        if expect_changed_from is not None:
            d = sig_diff(sig, expect_changed_from)
            step.check("the picture changed", d > SAME_BELOW, f"differs {d:.3f} from the earlier one")
        return sig

    def alive_and_front(self, step, label=""):
        step.check(f"the process is alive{label}", self.alive(), f"pid {self.pid() or '-'}")
        step.check(f"the app has the focus{label}", self.foreground(), self.focus() or "no focused window")

    def no_crash_since(self, step, mark):
        crash = self.adb("logcat", "-d", "-b", "crash", "-v", "threadtime", timeout=60)
        mine = [l for l in crash.splitlines() if self.package in l or "libazul" in l or "azul" in l.lower()]
        new = [l for l in mine if l[:18] >= mark]
        step.check("the crash log holds nothing of the app", not new, new[0][:200] if new else "")

    def no_recreation_since(self, step, mark):
        """A configuration change the manifest handles does not recreate the activity: no new create_activity event."""
        ev = self.adb("logcat", "-b", "events", "-d", "-v", "threadtime", timeout=60)
        new = [l for l in create_activity_lines(ev, self.package) if l[:18] >= mark]
        step.check("the activity was not recreated by this change (the manifest lists it)", not new, new[0].split(":", 3)[-1].strip()[:120] if new else "")

    def now_mark(self):
        t = self.sh("date '+%m-%d %H:%M:%S'").strip()
        return t

    # -- actions
    def launch(self, cold=False):
        act = self.resolve_activity()
        flags = "-S " if cold else ""
        return self.sh(f"am start -W {flags}-n {self.package}/{act}", timeout=120)

    def resume(self):
        """What a tap on the launcher icon does: the same task comes to the front."""
        return self.sh(f"monkey -p {self.package} -c android.intent.category.LAUNCHER 1", timeout=60)

    def home(self):
        self.sh("input keyevent KEYCODE_HOME")

    def back(self):
        self.sh("input keyevent KEYCODE_BACK")

    def tap(self, x, y):
        self.sh(f"input tap {int(x)} {int(y)}")

    def swipe(self, x1, y1, x2, y2, ms=350):
        self.sh(f"input swipe {int(x1)} {int(y1)} {int(x2)} {int(y2)} {ms}")

    def rotate(self, quarter_turns):
        self.sh("settings put system accelerometer_rotation 0")
        self.sh(f"settings put system user_rotation {quarter_turns}")

    def wait_for_front(self, timeout=40):
        """Seconds until the app is alive, has the focus and its screen is neither flat nor the launcher; None on timeout."""
        t0 = time.time()
        seen_alive = False
        while time.time() - t0 < timeout:
            alive = self.alive()
            if alive and self.foreground():
                seen_alive = True
                try:
                    sig, _w, _h, _rgba = signature_android_raw(self.adb("exec-out", "screencap", binary=True, timeout=60))
                except ValueError:
                    sig = None
                if sig and not is_flat(sig) and (self.home_sig is None or sig_diff(sig, self.home_sig) > LAUNCHER_BELOW):
                    self.last_sig = sig
                    return time.time() - t0
            elif alive:
                seen_alive = True
            elif seen_alive:
                return None           # it was running and is gone: it died, waiting longer shows nothing
            time.sleep(1.0)
        return None

    def sf_layers(self):
        """The SurfaceFlinger layers of this app's window (`dumpsys SurfaceFlinger --list`)."""
        out = self.sh("dumpsys SurfaceFlinger --list")
        return [l.strip() for l in out.splitlines() if self.package in l and "Surface(" not in l and "Task=" not in l
                and "Splash" not in l and "Background" not in l]

    def scroll_frame_stats(self, seconds=3.0):
        """Frame times while a finger scrolls for `seconds`: SurfaceFlinger keeps the present timestamps of the last 128
        frames of every layer (`--latency`), whichever renderer drew them. -> {median, p95, max, n, refresh_ms, layer} | None"""
        layers = self.sf_layers()
        if not layers:
            return None
        for layer in layers:
            self.sh(f"dumpsys SurfaceFlinger --latency-clear '{layer}'")
        w, h = self.dims
        self.swipe(w * 0.5, h * 0.72, w * 0.5, h * 0.28, int(seconds * 1000))
        best = None
        for layer in layers:
            refresh, times = parse_sf_latency(self.sh(f"dumpsys SurfaceFlinger --latency '{layer}'"))
            iv = frame_intervals_ms(times)
            if iv and (best is None or len(iv) > best[0]):
                best = (len(iv), layer, refresh, iv)
        if not best:
            return None
        n, layer, refresh, iv = best
        return {"median": percentile(iv, 50), "p95": percentile(iv, 95), "max": max(iv), "n": n, "refresh_ms": refresh, "layer": layer}

    def settle(self, seconds=2.0):
        time.sleep(seconds)

    def save_log(self, name, text):
        (self.run.logs_dir / name).write_text(text, encoding="utf-8", errors="replace")


def android_scenarios(args):
    run = Run(args.out, "android", {"label": args.label, "package": args.package, "gpu": args.gpu, "apk": os.path.basename(args.apk or "")})
    run.only = set(args.only.split(",")) if args.only else None
    dev = Android(run, args.package, args.activity, args.serial)
    pts = json.loads(args.points) if args.points else {}
    P = {  # fractions of the screen, from the Widget Showcase in portrait (AzWidgets)
        "dark": (0.511, 0.113), "system": (0.234, 0.113), "text_input": (0.50, 0.332), "switch": (0.15, 0.68),
        "checkbox": (0.126, 0.844), "scroll_from": (0.5, 0.75), "scroll_to": (0.5, 0.25),
    }
    P.update({k: tuple(v) for k, v in pts.items()})
    size = {"w": 1080, "h": 2400}
    state = {"portrait": None}

    def at(key):
        fx, fy = P[key]
        return fx * size["w"], fy * size["h"]

    # ---- 0. the device
    def s_device(s):
        info = {k: dev.sh(f"getprop {p}").strip() for k, p in
                (("model", "ro.product.model"), ("abi", "ro.product.cpu.abi"), ("api", "ro.build.version.sdk"),
                 ("release", "ro.build.version.release"), ("gles", "ro.opengles.version"),
                 ("renderer", "ro.hardware.egl"), ("qemu_gles", "ro.kernel.qemu.gles"))}
        run.meta.update({f"device_{k}": v for k, v in info.items()})
        s.note(", ".join(f"{k}={v}" for k, v in info.items()))
        dev.sh("settings put global window_animation_scale 0")
        dev.sh("settings put global transition_animation_scale 0")
        dev.sh("settings put global animator_duration_scale 0")
        dev.sh("settings put secure show_ime_with_hard_keyboard 1")
        dev.sh("settings put system accelerometer_rotation 0")
        dev.sh("settings put system user_rotation 0")
        dev.sh("input keyevent KEYCODE_WAKEUP")
        dev.sh("input keyevent 82")
        dev.sh("am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS")
        s.check("the device answers", bool(info["api"]), info["api"])

    run.step("00-device", "the emulator is up and quiet (animations off, rotation locked to portrait)", s_device)

    def recover(s):
        """Before a step: the device settings back to normal, and the app running in front (started again when an
        earlier step left it dead or away: that earlier step has already said so)."""
        dev.sh("settings put system user_rotation 0")
        dev.sh("settings put system font_scale 1.0")
        dev.sh("wm density reset")
        dev.sh("cmd uimode night no")
        if dev.alive() and dev.foreground():
            dev.hide_ime()           # a keyboard an earlier step left up covers half of every later picture
            return
        s.note(f"recovered: the app was {'not in front' if dev.alive() else 'dead'} before this step; started it again")
        dev.launch(cold=True)
        t = dev.wait_for_front(timeout=args.first_frame_timeout)
        s.check("the app could be started again before this step", t is not None, f"first frame after {t:.1f} s" if t is not None else "no frame")
        dev.settle(2.0)
        state["portrait"] = dev.last_sig or state["portrait"]

    run.recover = recover

    # ---- 1. the launcher baseline (what 'not the app' looks like)
    def s_home(s):
        dev.home()
        dev.settle(2.5)
        sig, w, h, fname = dev.capture("home-baseline")
        size["w"], size["h"] = w, h
        s.shots.append(fname)
        dev.home_sig = sig
        s.check("the home screen is a picture", not is_flat(sig), f"{flat_fraction(sig):.3f}")
        s.note(f"screen {w}x{h}")

    run.step("01-home-baseline", "the launcher screen before the app starts: the picture an app must differ from", s_home)

    # ---- 2. install + cold start
    def s_install(s):
        if args.apk:
            out = dev.adb("install", "-r", "-t", args.apk, timeout=240)
            s.check("adb install succeeds", "Success" in out, out.strip()[-160:])
        dev.adb("logcat", "-c")
        # the engine reads debug.az.env (KEY=VALUE;KEY=VALUE) at its start: AZ_LOG=info for its own lines, AZ_DUMP_PRESENT=1
        # for one line per presented frame ("[Android] present partial - N of M rows")
        dev.sh("setprop debug.az.env 'AZ_LOG=info;AZ_DUMP_PRESENT=1'")
        dev.sh(f"pm grant {args.package} android.permission.POST_NOTIFICATIONS")  # harmless when not requested
        info = dev.sh(f"dumpsys package {args.package} | grep -E 'versionName|versionCode|targetSdk|requested permissions' -A0")
        dev.save_log("package.txt", dev.sh(f"dumpsys package {args.package}"))
        s.note(" ".join(info.split())[:200])

    run.step("02-install", "install the APK, clear the log", s_install)

    def s_cold(s):
        mark = dev.now_mark()
        start = dev.launch(cold=True)
        total = re.search(r"TotalTime: (\d+)", start)
        s.note(f"am start -W: TotalTime {total.group(1) if total else '?'} ms")
        if total:
            run.meta["cold_start_ms"] = int(total.group(1))      # the number that has a history across re-runs
        t = dev.wait_for_front(timeout=args.first_frame_timeout)
        s.check("a first frame appears (alive, focus, not flat, not the launcher)", t is not None,
                f"after {t:.1f} s" if t is not None else f"nothing after {args.first_frame_timeout} s")
        dev.settle(3.0)
        state["portrait"] = dev.look(s, "portrait")
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)
        # what the app costs when nothing happens
        pid = dev.pid()
        if pid:
            top = dev.sh(f"top -b -n 2 -d 2 -p {pid}", timeout=60)
            row = [l for l in top.splitlines() if l.strip().startswith(pid)]
            if row:
                s.note("idle top: " + " ".join(row[-1].split()))
            mem = dev.sh(f"dumpsys meminfo {pid} | grep -E 'TOTAL PSS|TOTAL:|Native Heap|Graphics' | head -4")
            s.note("meminfo: " + " | ".join(l.strip() for l in mem.splitlines()))

    run.step("03-cold-start", "start the app from nothing: first frame, portrait picture, cost at rest", s_cold)

    def s_modes(s):
        """AZ_BACKEND=cpu / gpu / auto through the system property the engine reads, one cold start each. Per mode: which
        renderer RAN (the engine's `[render] backend=` line), the first-frame time (the system's `Displayed`), the frame
        times of a 3 s scroll (SurfaceFlinger), the picture; then the cpu and gpu pictures compared pixel by pixel."""
        results = {}
        for mode in ("cpu", "gpu", "auto"):
            dev.sh(f"setprop debug.az.env 'AZ_LOG=info;AZ_DUMP_PRESENT=1;AZ_BACKEND={mode}'")
            dev.adb("logcat", "-c")
            dev.launch(cold=True)
            t = dev.wait_for_front(timeout=first_frame_timeout_for(mode, args.first_frame_timeout))
            dev.settle(2.5)
            sig = dev.look(s, f"AZ_BACKEND-{mode}")
            rgba, dims = dev.last_rgba, dev.dims
            text = dev.adb("logcat", "-d", "-v", "threadtime", timeout=120)
            ran, requested, device = backend_ran(text)
            if ran == "unknown" and any("[Android] present" in l for l in text.splitlines()):
                ran, device = "cpu", "inferred: the CPU present path (ANativeWindow_lock) logged its frames"
            s.check(f"AZ_BACKEND={mode}: the app draws", t is not None and not is_flat(sig), f"first frame after {t:.1f} s" if t is not None else "no frame")
            frames = dev.scroll_frame_stats(3.0)
            dev.settle(1.0)
            results[mode] = {"ran": ran, "requested": requested, "device": device, "rgba": rgba, "dims": dims, "first_frame_s": t,
                             "displayed_ms": displayed_ms(text, args.package), "frames": frames}
        report_modes(run, s, results, args.expect_gpu, "android", hardware=args.hardware)
        dev.sh("setprop debug.az.env 'AZ_LOG=info;AZ_DUMP_PRESENT=1'")
        dev.launch(cold=True)       # the default start again: what the later steps compare with
        dev.wait_for_front(timeout=args.first_frame_timeout)
        dev.settle(2.5)
        state["portrait"] = dev.look(s, "default-again")

    run.step("03b-render-modes", "AZ_BACKEND=cpu, gpu, auto: which renderer ran, first frame, scroll frame times, cpu against gpu pixel by pixel", s_modes, needs_app=True)

    def s_series(s):
        """N cold starts of the app (and of the floor APK, an empty Java Activity, when one is given), each from the
        launcher with the process gone: `am start -W` TotalTime / ThisTime / WaitTime, and the system's own events
        (am_proc_start, am_create_activity, wm_activity_launch_time) kept for the startup timeline work."""
        def stats(values):
            v = sorted(values)
            return {"min": v[0], "median": v[len(v) // 2], "max": v[-1], "n": len(v)}

        def series(pkg, act, label):
            times, timelines = [], []
            dev.adb("logcat", "-b", "events", "-c")
            for i in range(args.series + 1):            # the first run only warms the page cache: not counted
                dev.sh(f"am force-stop {pkg}")
                dev.home()
                time.sleep(2.5)
                mark = dev.now_mark()
                out = dev.sh(f"am start -W -n {pkg}/{act}", timeout=120)
                total = re.search(r"TotalTime: (\d+)", out)
                this = re.search(r"ThisTime: (\d+)", out)
                wait = re.search(r"WaitTime: (\d+)", out)
                if i > 0 and total:
                    times.append({"total": int(total.group(1)), "this": int(this.group(1)) if this else None,
                                  "wait": int(wait.group(1)) if wait else None})
                time.sleep(3.0)
                if i > 0:
                    # what the user waits for: the system's START of the activity -> the engine's first line -> the first picture
                    # (the logcat of this start only; the engine logs at AZ_LOG=info with AZ_DUMP_PRESENT=1 on the QA's default env)
                    text = dev.adb("logcat", "-d", "-v", "threadtime", timeout=120)
                    timelines += start_timelines("\n".join(l for l in text.splitlines() if l[:18] >= mark), pkg)[-1:]
            events = dev.adb("logcat", "-b", "events", "-d", "-v", "threadtime", timeout=60)
            dev.save_log(f"events-{label}.txt", "\n".join(l for l in events.splitlines() if pkg in l))
            dev.save_log(f"start-timeline-{label}.json", json.dumps({"records": timelines, "summary": summarize_timelines(timelines)}, indent=1))
            dev.sh(f"am force-stop {pkg}")
            return times, timelines

        mine, mine_timelines = series(args.package, dev.resolve_activity(), "app")
        tl = summarize_timelines(mine_timelines)
        if tl:
            def at(key):
                return f"{tl[key]['median']} ms" if key in tl else "n/a"
            run.meta["start_timeline_ms"] = (f"from the system's START: fork {at('proc')}, android_main {at('main')}, system Displayed {at('displayed')}, "
                                             f"first full present of the engine {at('present')}; the engine's share (android_main to first present) {at('main_to_present')} "
                                             f"(medians of n={tl.get('present', {}).get('n', 0)})")
            s.note(run.meta["start_timeline_ms"])
        if mine:
            st = stats([t["total"] for t in mine])
            run.meta["cold_start_series_ms"] = f"median {st['median']} min {st['min']} max {st['max']} (n={st['n']})"
            s.note(f"{args.package}: TotalTime ms {[t['total'] for t in mine]}; median {st['median']}")
        s.check(f"{args.series} cold starts of the app were measured", len(mine) == args.series, f"{len(mine)} measured")
        if args.floor_apk:
            out = dev.adb("install", "-r", "-t", args.floor_apk, timeout=120)
            s.check("the floor APK installs", "Success" in out, out.strip()[-120:], required=False)
            floor, _floor_timelines = series("com.azul.floor", ".FloorActivity", "floor")
            if floor:
                fs = stats([t["total"] for t in floor])
                run.meta["floor_start_series_ms"] = f"median {fs['median']} min {fs['min']} max {fs['max']} (n={fs['n']})"
                s.note(f"floor (an empty Java Activity): TotalTime ms {[t['total'] for t in floor]}; median {fs['median']}")
                if mine:
                    s.note(f"the app costs {st['median'] - fs['median']} ms more than the floor at the median")
            dev.sh("pm uninstall com.azul.floor")
        dev.launch(cold=True)
        dev.wait_for_front(timeout=args.first_frame_timeout)
        dev.settle(2.5)
        state["portrait"] = dev.look(s, "after-series")

    if args.series:
        run.step("03d-cold-start-series", "N cold starts from the launcher: the app against the floor (an empty Java Activity)", s_series, needs_app=True)

    def s_tap_label(s):
        """Press the button the accessibility tree names `--tap-label`: a tap, then a long press, then a tap 40 px higher, each
        followed by 3 s and a picture; the step passes when ANY of them changes the screen, and says which did. (AzCompanion's
        'Try it without a Mac' changed nothing on a tap: is it the touch path or the button?)"""
        mark = dev.now_mark()
        before = dev.look(s, "before-label-tap")
        found = dev.find_label(args.tap_label)
        s.check(f"the accessibility tree names {args.tap_label!r} with a position", bool(found), str(found))
        if not found:
            return
        x, y = found
        run.meta["tap_label_at"] = f"{args.tap_label!r} ({x}, {y})"
        s.note(f"{args.tap_label!r} is at ({x}, {y}) in the accessibility tree")
        outcomes = []
        for how, action in (("tap", lambda: dev.tap(x, y)),
                            ("long press 800 ms", lambda: dev.swipe(x, y, x, y, 800)),
                            ("tap 40 px higher", lambda: dev.tap(x, y - 40))):
            action()
            dev.settle(3.0)
            sig = dev.look(s, "after-" + how.split(" ")[0] + ("-" + how.split(" ")[1] if how.startswith("tap 40") else ""))
            d = sig_diff(sig, before)
            outcomes.append((how, d))
            if d > SAME_BELOW:
                break
        s.check("pressing the button changes the screen", any(d > SAME_BELOW for _h, d in outcomes),
                "; ".join(f"{h}: differs {d:.3f}" for h, d in outcomes))
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    if args.tap_label:
        run.step("03e-tap-label", f"press the button {args.tap_label!r} (a tap, a long press, a tap 40 px higher): the screen must change", s_tap_label, needs_app=True)

    def s_ui_dump(s):
        dev.sh("uiautomator dump /sdcard/qa-ui.xml", timeout=40)
        xml = dev.sh("cat /sdcard/qa-ui.xml")
        dev.save_log("uiautomator.xml", xml)
        nodes = parse_bounds(xml)
        s.note(f"{len(nodes)} labelled accessibility nodes; first: " + ", ".join(repr(n[0]) for n in nodes[:8]))
        s.check("the app exposes labelled nodes to the accessibility service (TalkBack / uiautomator)", len(nodes) > 0,
                f"{len(nodes)} nodes", required=False)

    run.step("04-accessibility-tree", "what uiautomator sees of the window", s_ui_dump, needs_app=True)

    # ---- 3. touch
    def s_scroll(s):
        mark = dev.now_mark()
        x1, y1 = at("scroll_from")
        x2, y2 = at("scroll_to")
        dev.swipe(x1, y1, x2, y2, 400)
        dev.settle(1.5)
        after = dev.look(s, "scrolled-down", expect_changed_from=state["portrait"])
        dev.swipe(x2, y2, x1, y1, 400)
        dev.swipe(x2, y2, x1, y1, 400)
        dev.settle(1.5)
        back = dev.look(s, "scrolled-back-up")
        s.check("scrolling back returns to the first picture", sig_diff(back, state["portrait"]) < 0.05,
                f"differs {sig_diff(back, state['portrait']):.3f}", required=False)
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("05-swipe-scroll", "a finger swipe scrolls the page and back", s_scroll, needs_app=True)

    def s_tap_dark(s):
        mark = dev.now_mark()
        before = dev.look(s, "before-tap")
        x, y, how = dev.tap_label("Dark", fallback=at("dark"))
        s.note(f"tapped Dark at ({x:.0f}, {y:.0f}) from the {how}")
        dev.settle(1.5)
        after = dev.look(s, "after-tap-dark", expect_changed_from=before)
        dev.tap_label("System", fallback=at("system"))   # back to the system scheme
        dev.settle(1.0)
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("06-tap-mode-button", "a tap on the Mode switch (Dark) changes the page", s_tap_dark, needs_app=True)

    def s_keyboard(s):
        mark = dev.now_mark()
        s.check("the keyboard is hidden before the tap", not dev.ime_shown(), "", required=False)
        x, y, how = dev.tap_label("TextInput", fallback=at("text_input"))
        s.note(f"tapped the TextInput field at ({x:.0f}, {y:.0f}) from the {how}")
        dev.settle(2.5)
        shown = dev.ime_shown()
        s.check("tapping a text field shows the soft keyboard", shown, "the window system says " + ("shown" if shown else "hidden"))
        dev.sh("input text hello%sazul")
        dev.settle(1.0)
        typed = dev.look(s, "keyboard-and-text")
        if shown:
            dev.back()
            dev.settle(1.5)
            s.check("the keyboard hides again", not dev.ime_shown(), "")
        dev.look(s, "keyboard-hidden")
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("07-keyboard", "tap a text field, type, hide the keyboard", s_keyboard, needs_app=True)

    # ---- 4. rotation
    def s_landscape(s):
        mark = dev.now_mark()
        dev.rotate(1)
        dev.settle(3.0)
        sig = dev.look(s, "landscape")
        w, h = dev.dims
        s.check("the screen is wider than tall", w > h, f"{w}x{h}")
        dev.alive_and_front(s)
        s.check("the layout changed with the rotation", sig_diff(sig, state["portrait"]) > SAME_BELOW, f"{sig_diff(sig, state['portrait']):.3f}")
        dev.tap(w * 0.5, h * 0.5)   # a tap in landscape lands somewhere sensible
        dev.settle(1.0)
        dev.look(s, "landscape-after-tap")
        dev.rotate(0)
        dev.settle(3.0)
        again = dev.look(s, "portrait-again")
        w2, h2 = dev.dims
        s.check("back in portrait the screen is taller than wide", h2 > w2, f"{w2}x{h2}")
        s.check("portrait looks like portrait did", sig_diff(again, state["portrait"]) < 0.10, f"differs {sig_diff(again, state['portrait']):.3f}", required=False)
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("08-rotate", "landscape, a tap, portrait again", s_landscape, needs_app=True)

    # ---- 5. leaving and coming back
    def s_home_resume(s):
        mark = dev.now_mark()
        before = dev.look(s, "before-home")
        dev.home()
        dev.settle(2.5)
        away, *_ = dev.capture("home-pressed")
        s.shots.append(f"{dev.shot_count:02d}-home-pressed.png")
        s.check("the launcher is showing", sig_diff(away, dev.home_sig) < 0.05, f"differs {sig_diff(away, dev.home_sig):.3f} from the baseline")
        s.check("the process survives going home", dev.alive(), f"pid {dev.pid() or '-'}")
        dev.resume()
        t = dev.wait_for_front(timeout=30)
        s.check("the app is back in front", t is not None, f"after {t:.1f} s" if t is not None else "not back after 30 s")
        dev.settle(1.5)
        after = dev.look(s, "resumed")
        # REQUIRED: a window that came back black (the surface was recreated and nothing asked for a repaint) is a failure
        s.check("the window shows the picture it had before (it was repainted after the return)", sig_diff(after, before) < 0.08, f"differs {sig_diff(after, before):.3f}")
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("09-home-and-resume", "home, then the launcher icon: same process, same picture", s_home_resume, needs_app=True)

    def s_background(s):
        mark = dev.now_mark()
        pid0 = dev.pid()
        before = dev.look(s, "before-background")
        dev.home()
        time.sleep(6)
        # an app in the background draws nothing and costs nothing: CPU over 3 s at 6 s in (a battery question, a finding when high)
        pid_bg = dev.pid()
        if pid_bg:
            top = dev.sh(f"top -b -n 2 -d 3 -p {pid_bg}", timeout=60)
            rows = [l for l in top.splitlines() if l.strip().startswith(pid_bg)]
            if rows:
                cols = rows[-1].split()
                cpu = next((c for c in cols if re.fullmatch(r"\d+(\.\d+)?", c) and "." in c), None)
                if cpu is not None:
                    s.check("the app is quiet in the background (CPU under 5 %)", float(cpu) < 5.0, f"{cpu} % of one core over 3 s", required=False)
                s.note("background top: " + " ".join(cols))
        time.sleep(1)
        s.check("alive after 10 s in the background", dev.alive(), f"pid {dev.pid() or '-'}")
        dev.resume()
        t = dev.wait_for_front(timeout=30)
        s.check("the app is back in front", t is not None, f"after {t:.1f} s" if t is not None else "not back after 30 s")
        dev.settle(1.5)
        after = dev.look(s, "after-10s-background")
        s.check("same process as before", dev.pid() == pid0, f"{pid0} -> {dev.pid()}", required=False)
        s.check("the window shows the picture it had before (it was repainted after the return)", sig_diff(after, before) < 0.08, f"differs {sig_diff(after, before):.3f}")
        dev.no_crash_since(s, mark)

    run.step("10-background-10s", "ten seconds in the background, then back", s_background, needs_app=True)

    def s_memory(s):
        mark = dev.now_mark()
        before = dev.look(s, "before-trim")
        out = dev.sh(f"am send-trim-memory {dev.pid()} RUNNING_CRITICAL")
        s.note(f"am send-trim-memory RUNNING_CRITICAL: {out.strip()[:100] or 'ok'}")
        dev.settle(2.0)
        after = dev.look(s, "after-trim")
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("11-memory-pressure", "onTrimMemory(RUNNING_CRITICAL) while in the foreground", s_memory, needs_app=True)

    # ---- 6. configuration changes
    def s_dark(s):
        mark = dev.now_mark()
        base = dev.look(s, "light-system")
        dev.sh("cmd uimode night yes")
        # SLOW or MISSING? the picture after 3 s and again after 6 s: a change by 3 s is fine, a change only by 6 s says the
        # theme arrives late, no change at 6 s says the app never took the system's night mode
        dev.settle(3.0)
        sig3, *_r = dev.capture("system-dark-3s")
        d3 = sig_diff(sig3, base)
        dev.settle(3.0)
        dark = dev.look(s, "system-dark", expect_changed_from=base)
        d6 = sig_diff(dark, base)
        verdict = "changed by 3 s" if d3 > SAME_BELOW else ("changed only by 6 s: SLOW" if d6 > SAME_BELOW else "did not change in 6 s: MISSING")
        s.note(f"the app's picture against the light one: {d3:.3f} at 3 s, {d6:.3f} at 6 s: {verdict}")
        dev.alive_and_front(s)
        dev.sh("cmd uimode night no")
        dev.settle(2.5)
        dev.look(s, "system-light-again")
        dev.no_recreation_since(s, mark)
        dev.no_crash_since(s, mark)

    run.step("12-system-dark-mode", "the system switches to dark mode and back (a configuration change)", s_dark, needs_app=True)

    def s_font(s):
        mark = dev.now_mark()
        base = dev.look(s, "font-1.0")
        dev.sh("settings put system font_scale 1.5")
        dev.settle(3.0)
        big = dev.look(s, "font-1.5")
        s.check("the page reacts to the font scale", sig_diff(big, base) > SAME_BELOW, f"differs {sig_diff(big, base):.3f}", required=False)
        dev.alive_and_front(s)
        dev.sh("settings put system font_scale 1.0")
        dev.settle(2.5)
        dev.no_recreation_since(s, mark)
        dev.no_crash_since(s, mark)

    run.step("13-font-scale", "the system font scale goes to 1.5 and back", s_font, needs_app=True)

    def s_density(s):
        mark = dev.now_mark()
        dev.sh("wm density 480")
        dev.settle(3.0)
        dense = dev.look(s, "density-480")
        dev.alive_and_front(s)
        dev.sh("wm density reset")
        dev.settle(3.0)
        dev.look(s, "density-reset")
        dev.alive_and_front(s, " after the reset")
        dev.no_recreation_since(s, mark)
        dev.no_crash_since(s, mark)

    run.step("14-display-density", "the display density changes under the running app and back", s_density, needs_app=True)

    # ---- 7. leaving for good and coming back
    def s_back(s):
        mark = dev.now_mark()
        pid0 = dev.pid()
        dev.back()
        dev.settle(3.0)
        s.note(f"after BACK: alive={dev.alive()} focus={dev.focus() or '-'}")
        dev.look(s, "after-back", allow_launcher=True)
        dev.resume()
        t = dev.wait_for_front(timeout=40)
        s.check("a launch after BACK shows the app again (frame in time)", t is not None,
                f"after {t:.1f} s" if t is not None else "no frame after 40 s")
        dev.settle(2.0)
        dev.look(s, "relaunch-after-back")
        dev.alive_and_front(s)
        s.note(f"pid {pid0} -> {dev.pid() or '-'}")
        dev.no_crash_since(s, mark)

    run.step("15-back-then-relaunch", "BACK ends the app (its process too, on purpose); starting it again is a cold start that shows it", s_back, needs_app=True)

    def s_cold2(s):
        mark = dev.now_mark()
        dev.sh(f"am force-stop {args.package}")
        dev.settle(1.0)
        s.check("the process is gone after force-stop", not dev.alive(), "")
        dev.launch(cold=False)
        t = dev.wait_for_front(timeout=40)
        s.check("a cold start after force-stop shows a frame", t is not None, f"after {t:.1f} s" if t is not None else "no frame after 40 s")
        dev.settle(2.0)
        dev.look(s, "cold-start-2")
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("16-force-stop-cold-start", "kill the process, start again", s_cold2, needs_app=True)

    def s_kill_bg(s):
        mark = dev.now_mark()
        dev.home()
        dev.settle(2.0)
        out = dev.sh(f"am kill {args.package}")
        dev.settle(1.5)
        s.note(f"am kill (what the system does to a background app under memory pressure): alive afterwards = {dev.alive()}")
        dev.resume()
        t = dev.wait_for_front(timeout=40)
        s.check("coming back after the system killed it shows a frame", t is not None, f"after {t:.1f} s" if t is not None else "no frame after 40 s")
        dev.settle(2.0)
        dev.look(s, "after-system-kill")
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("17-killed-in-background", "the system kills the app while it is in the background; the user returns", s_kill_bg, needs_app=True)

    def s_monkey(s):
        mark = dev.now_mark()
        out = dev.sh(f"monkey -p {args.package} --pct-touch 90 --pct-motion 10 --pct-nav 0 --pct-majornav 0 --pct-syskeys 0 "
                     f"--pct-appswitch 0 --throttle 40 -s 42 300", timeout=240)
        s.note("monkey: " + " ".join(out.strip().splitlines()[-2:])[:200])
        dev.settle(2.0)
        dev.look(s, "after-monkey")
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("18-monkey", "300 random touches and drags (fixed seed)", s_monkey, needs_app=True)

    def pss_kb(pid):
        text = dev.sh(f"dumpsys meminfo {pid} | grep -E 'TOTAL PSS|TOTAL:' | head -n 2")
        m = re.search(r"TOTAL PSS:\s+(\d+)", text) or re.search(r"TOTAL:?\s+(\d+)", text)
        return int(m.group(1)) if m else None

    def s_soak(s):
        """Rotate eight times (portrait, landscape, portrait ...) and look at what the process holds: a leak per rotation (a
        pixmap, a layout, a window never dropped) shows as growth. Required: alive, in front, growth under 100 MB; the number is
        in the notes."""
        mark = dev.now_mark()
        pid = dev.pid()
        before = pss_kb(pid) if pid else None
        for _ in range(8):
            dev.rotate(1)
            dev.settle(1.5)
            dev.rotate(0)
            dev.settle(1.5)
        dev.look(s, "after-rotation-soak")
        after = pss_kb(dev.pid() or pid)
        if before and after:
            s.note(f"PSS {before // 1024} MB before, {after // 1024} MB after eight rotation pairs ({(after - before) // 1024:+d} MB)")
            s.check("eight rotation pairs leave the process's memory within 100 MB", after - before < 100 * 1024, f"{(after - before) // 1024:+d} MB")
        dev.alive_and_front(s)
        dev.no_crash_since(s, mark)

    run.step("18b-rotation-soak", "eight rotations there and back: alive, in front, memory growth", s_soak, needs_app=True)

    # ---- the logs
    def s_logs(s):
        full = dev.adb("logcat", "-d", "-v", "threadtime", timeout=120)
        dev.save_log("logcat.txt", full)
        dev.save_log("logcat-crash.txt", dev.adb("logcat", "-d", "-b", "crash", "-v", "threadtime"))
        dev.save_log("exit-info.txt", dev.sh(f"dumpsys activity exit-info {args.package}"))
        dev.save_log("meminfo.txt", dev.sh(f"dumpsys meminfo {args.package}"))
        dev.save_log("window.txt", dev.sh("dumpsys window | grep -E 'mCurrentFocus|mFocusedApp|Rotation|mDisplayRotation' | head -20"))
        bad = fatal_lines(full, args.package)
        s.check("no fatal line of the app in the whole log", not bad, bad[0][:240] if bad else "")
        exits = parse_exit_reasons(dev.sh(f"dumpsys activity exit-info {args.package}"))
        crashes = [e for e in exits if e[2] in CRASH_EXIT_REASONS]
        s.check("no exit of the app was a crash or an ANR (the system's own exit reasons)", not crashes,
                "; ".join(f"{e[0]} pid {e[1]}: {e[3]}" for e in crashes[:3]))
        s.note("exit reasons, newest first: " + ", ".join(f"{e[3]}" for e in exits[:8]))
        shown = [l.split("ActivityTaskManager:")[-1].strip() for l in full.splitlines() if "Displayed" in l and run.meta.get("package", "") in l]
        if shown:
            s.note("Displayed: " + " | ".join(x[:110] for x in shown[:6]))
        present = [l for l in full.splitlines() if "present" in l and "rows" in l and "[Android]" in l]
        partial = sum(1 for l in present if "present partial" in l)
        s.note(f"{len(present)} present lines: {partial} partial, {len(present) - partial} full" + (f"; first: {present[0].split('RustStdoutStderr:')[-1].strip()[:100]}" if present else ""))
        rust = [l for l in full.splitlines() if "RustStdoutStderr" in l]
        s.note(f"{len(rust)} lines from the app's stdout/stderr; {len(full.splitlines())} log lines in all")
        warn = [l for l in rust if " W " in l or "WARN" in l or "ERROR" in l]
        s.note(f"{len(warn)} warning/error lines from the engine (first: {(warn[0].split('RustStdoutStderr:')[-1].strip()[:140] if warn else '-')})")

    run.step("19-logs", "the whole log: nothing fatal, what the engine said", s_logs)
    run.write()
    return run



# ---------------------------------------------------------------------------------------------
# iOS Simulator
# ---------------------------------------------------------------------------------------------
class IOS:
    """One app on a booted simulator, driven with `xcrun simctl` (no idb: nothing to install on the runner)."""

    def __init__(self, run, udid, bundle, exe, app_dir=None):
        self.run, self.udid, self.bundle, self.exe, self.app_dir = run, udid, bundle, exe, app_dir
        self.home_sig = None
        self.last_sig = None
        self.dims = (0, 0)
        self.shot_count = 0
        self.launch_count = 0
        # the launch that STARTED the running process: its console is the process's (a warm launch of a running app writes none)
        self.process_launch = 0
        self.started = time.strftime("%Y-%m-%dT%H:%M:%S")
        self.gui = False
        # what EVERY launch of this run sets in the app's environment (`--backend gpu`: AZ_BACKEND=gpu), under the extra_env of a launch
        self.base_env = {}

    def x(self, *cmd, timeout=120, env=None):
        try:
            p = subprocess.run(list(cmd), capture_output=True, timeout=timeout, env=env)
        except subprocess.TimeoutExpired:
            return 124, "", "timeout"
        return p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")

    def simctl(self, *args, timeout=120, env=None):
        return self.x("xcrun", "simctl", *args, timeout=timeout, env=env)

    def to_file(self, log_name, *cmd, timeout=600):
        """Run a command with its output going straight into logs/<log_name> (a FILE, not a pipe: the first trial run of this
        harness hung for an hour in `log show` because a killed command's children kept a pipe open). -> (rc, the file's text)"""
        path = self.run.logs_dir / log_name
        with open(path, "wb") as fh:
            try:
                rc = subprocess.run(list(cmd), stdout=fh, stderr=subprocess.STDOUT, timeout=timeout).returncode
            except subprocess.TimeoutExpired:
                rc = 124
        return rc, path.read_text(encoding="utf-8", errors="replace")

    def fire(self, *args, wait=25):
        """`simctl <args>` that is allowed to run on in the background: waits `wait` seconds for it, never kills it, never
        blocks longer (launches and the like). -> its output so far"""
        proc = subprocess.Popen(["xcrun", "simctl"] + list(args), stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        try:
            out, _e = proc.communicate(timeout=wait)
        except subprocess.TimeoutExpired:
            return "(still running after %d s)" % wait
        return out.decode("utf-8", "replace")

    # -- state
    def pid(self):
        _rc, out, _err = self.simctl("spawn", self.udid, "launchctl", "list")
        for line in out.splitlines():
            if f"UIKitApplication:{self.bundle}" in line:
                first = line.split()[0] if line.split() else "-"
                return "" if first in ("-", "0") else first
        return ""

    def alive(self):
        return bool(self.pid())

    def rss_mb(self):
        """The resident memory of the app's process in MB (a Simulator app is a process of the host: `ps` reads it), or None."""
        pid = self.pid()
        if not pid:
            return None
        rc, out, _e = self.x("ps", "-o", "rss=", "-p", str(pid), timeout=20)
        try:
            return int(out.strip()) / 1024.0 if rc == 0 else None
        except ValueError:
            return None

    # -- the picture
    def capture(self, name, keep=True):
        """PNG from simctl; the signature from a small BMP made of it with sips. -> (signature, w, h, filename).
        `keep=False` (the polls of wait_for_front) writes into the system temp folder and counts no picture: nothing of
        this script deletes a file, temp files stay in the runner's temp folder."""
        if keep:
            self.shot_count += 1
            fname = f"{self.shot_count:02d}-{name}.png"
            path = self.run.shots_dir / fname
        else:
            fname = ""
            path = Path(tempfile.gettempdir()) / f"azqa-{os.getpid()}-poll.png"
        rc, _o, err = self.simctl("io", self.udid, "screenshot", "--type=png", str(path), timeout=90)
        if rc != 0 or not path.exists():
            raise RuntimeError(f"simctl io screenshot failed: {err.strip()[:200]}")
        w, h = png_size(path)
        small = Path(tempfile.gettempdir()) / f"azqa-{os.getpid()}-sig-{time.time_ns()}.bmp"
        rc, _o, err = self.x("sips", "-z", "240", "120", "--setProperty", "format", "bmp", str(path), "--out", str(small), timeout=60)
        if rc != 0 or not small.exists():
            raise RuntimeError(f"sips could not reduce the picture: {err.strip()[:200]}")
        sig, _sw, _sh, _px = parse_bmp(small.read_bytes())
        self.dims = (w, h)
        if keep:
            self.last_png = path
        return sig, w, h, fname

    def last_rgba_full(self):
        """The full-resolution pixels of the last picture (sips makes a BMP of the PNG). -> (rgba, w, h) | None"""
        if not getattr(self, "last_png", None):
            return None
        big = Path(tempfile.gettempdir()) / f"azqa-{os.getpid()}-full-{time.time_ns()}.bmp"
        rc, _o, _e = self.x("sips", "--setProperty", "format", "bmp", str(self.last_png), "--out", str(big), timeout=120)
        if rc != 0 or not big.exists():
            return None
        _sig, w, h, rgba = parse_bmp(big.read_bytes())
        return rgba, w, h

    def console_text(self, launch_no=None):
        """What the app wrote to stdout and stderr in launch number `launch_no` (default: the last)."""
        n = launch_no or self.launch_count
        out = ""
        for suffix in ("out", "err"):
            f = self.run.logs_dir / f"console-{n}.{suffix}.txt"
            if f.exists():
                out += f.read_text(errors="replace") + "\n"
        return out

    def process_console(self):
        """What the RUNNING process wrote since it started: the console of the cold launch that started it."""
        return self.console_text(self.process_launch or None)

    # -- the engine's debug server (a `mobile-e2e` app; the Simulator shares the host's loopback)
    def server_op(self, port, op, timeout=30, **params):
        """One request to the debug server. -> the decoded answer; raises OSError / URLError when nothing answers."""
        body = json.dumps(dict(params, op=op)).encode("utf-8")
        req = urllib.request.Request(f"http://127.0.0.1:{port}/", data=body, method="POST")
        with urllib.request.urlopen(req, timeout=timeout) as response:
            text = response.read().decode("utf-8") or "{}"
        try:
            return json.loads(text)
        except ValueError:
            return {"status": "error", "raw": text[:200]}

    def server_up(self, port, seconds=8):
        """Does a debug server answer `get_state` on `port` within `seconds`? -> its answer, or None (a build without the server)."""
        t0 = time.time()
        while True:
            try:
                answer = self.server_op(port, "get_state", timeout=3)
                if isinstance(answer, dict) and answer.get("status") != "error":
                    return answer
            except (urllib.error.URLError, OSError, TimeoutError, ValueError):
                pass
            if time.time() - t0 >= seconds:
                return None
            time.sleep(1.0)

    def swipe_frame_stats(self, port, state, seconds=4.0):
        """A finger drags the page up and down for `seconds` through the server's in-process touch events (one move per frame: the
        engine reads the touch once a frame), and the engine's own `[render] frames` lines written during it say how the frames came
        (`swipe_windows`: the full seconds of the swipe, not the tail of a burst before it). The page scrolls both ways, so the swipe
        never runs out of page. -> (summarize_frames of those lines | None, the touch moves made, the seconds they took)"""
        scale = (find_key(state, "dpi") or 288.0) / 96.0          # an iPhone: 3 device pixels per logical one
        width = find_key(state, "logical_width") or self.dims[0] / scale
        height = find_key(state, "logical_height") or self.dims[1] / scale
        x, top, bottom = width * 0.5, height * 0.3, height * 0.7
        before = len(self.process_console().splitlines())
        self.server_op(port, "touch_start", id=1, x=x, y=bottom)
        self.server_op(port, "wait_frame")
        t0 = time.time()
        made, pos, direction = 0, 0.0, 1.0
        while time.time() - t0 < seconds:
            self.server_op(port, "touch_move", id=1, x=x, y=bottom + (top - bottom) * pos)
            self.server_op(port, "wait_frame")
            made += 1
            pos += 0.04 * direction
            if pos >= 1.0 or pos <= 0.0:                          # the finger turns round at the ends
                direction = -direction
                pos = max(0.0, min(1.0, pos))
        took = time.time() - t0
        self.server_op(port, "touch_end", id=1)
        self.server_op(port, "wait_frame")
        self.settle(1.5)
        lines = self.process_console().splitlines()[before:]
        return summarize_frames(swipe_windows("\n".join(lines))), made, took

    def idle_windows(self, seconds=4.0):
        """Nothing touches the app for `seconds`: the engine's frames lines of full seconds written meanwhile. A window that is not
        animating presents nothing, so there must be none: one is a window that draws continuously while it shows the same picture
        (power). -> [the dicts of `engine_frames`]"""
        before = len(self.process_console().splitlines())
        time.sleep(seconds)
        return swipe_windows("\n".join(self.process_console().splitlines()[before:]))

    def look(self, step, name, expect_changed_from=None, allow_launcher=False):
        sig, w, h, fname = self.capture(name)
        step.shots.append(fname)
        flat = flat_fraction(sig)
        step.check("the screen is not one flat colour", flat < FLAT_FRACTION, f"{flat:.4f} of the samples share one colour ({w}x{h})")
        if self.home_sig is not None and not allow_launcher:
            d = sig_diff(sig, self.home_sig)
            step.check("the screen is not the home screen", d > LAUNCHER_BELOW, f"differs {d:.3f} from the home screen")
        if expect_changed_from is not None:
            d = sig_diff(sig, expect_changed_from)
            step.check("the picture changed", d > SAME_BELOW, f"differs {d:.3f} from the earlier one")
        return sig

    def alive_step(self, step, label=""):
        step.check(f"the process is alive{label}", self.alive(), f"pid {self.pid() or '-'}")

    # -- actions
    def launch(self, extra_env=None, cold=True):
        self.launch_count += 1
        if cold:
            self.process_launch = self.launch_count
        env = dict(os.environ)
        env["SIMCTL_CHILD_AZ_LOG"] = "info"
        env["SIMCTL_CHILD_RUST_BACKTRACE"] = "1"
        for k, v in self.base_env.items():
            env["SIMCTL_CHILD_" + k] = v
        for k, v in (extra_env or {}).items():
            env["SIMCTL_CHILD_" + k] = v
        out_f = self.run.logs_dir / f"console-{self.launch_count}.out.txt"
        err_f = self.run.logs_dir / f"console-{self.launch_count}.err.txt"
        cmd = ["launch"] + (["--terminate-running-process"] if cold else []) + [f"--stdout={out_f}", f"--stderr={err_f}", self.udid, self.bundle]
        # `simctl launch` prints "<bundle>: <pid>" and returns; when it does not (it can wait for the app to exit when the
        # output is redirected) it is left running after 25 s: waiting for it would block every step that follows
        proc = subprocess.Popen(["xcrun", "simctl"] + cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
        try:
            raw_out, raw_err = proc.communicate(timeout=25)
            rc = proc.returncode
        except subprocess.TimeoutExpired:
            raw_out, raw_err, rc = b"", b"", None
        out = raw_out.decode("utf-8", "replace")
        err = raw_err.decode("utf-8", "replace")
        m = re.search(r":\s*(\d+)\s*$", out.strip())
        return {"rc": rc, "pid": m.group(1) if m else self.pid(), "out": out.strip() or "(simctl launch still running after 25 s)",
                "err": err.strip(), "console": (out_f, err_f)}

    def terminate(self):
        self.simctl("terminate", self.udid, self.bundle)

    def open_other_app(self, bundle="com.apple.Preferences"):
        return self.fire("launch", self.udid, bundle)

    def wait_for_front(self, timeout=45):
        t0 = time.time()
        seen_alive = False
        while time.time() - t0 < timeout:
            if self.alive():
                seen_alive = True
                try:
                    sig, _w, _h, _f = self.capture("poll", keep=False)
                except RuntimeError:
                    sig = None
                if sig and not is_flat(sig) and (self.home_sig is None or sig_diff(sig, self.home_sig) > LAUNCHER_BELOW):
                    self.last_sig = sig
                    return time.time() - t0
            elif seen_alive:
                return None           # it was running and is gone: it died, waiting longer shows nothing
            elif time.time() - t0 > 15:
                return None           # it never started
            time.sleep(1.0)
        return None

    def settle(self, seconds=2.0):
        time.sleep(seconds)

    def crash_reports(self, since_stamp=None):
        """The .ips files the host wrote for this app since the run began (copied into logs/crash/)."""
        found = []
        d = Path.home() / "Library" / "Logs" / "DiagnosticReports"
        out = self.run.logs_dir / "crash"
        for f in sorted(d.glob(f"{self.exe}*.ips")) + sorted(d.glob(f"{self.exe}*.crash")):
            try:
                if f.stat().st_mtime >= self.run.t0:
                    out.mkdir(exist_ok=True)
                    (out / f.name).write_bytes(f.read_bytes())
                    found.append(f.name)
            except OSError:
                pass
        return found

    def landscape_copy(self):
        """A copy of the .app whose Info.plist allows only the two landscape orientations, signed ad hoc again (a
        Simulator app needs a signature, not an identity): when an app only supports landscape the Simulator starts it
        in landscape. Tests the landscape LAYOUT where the rotation itself cannot be pressed. -> the new .app path."""
        import plistlib
        import shutil
        # a folder of its own in the runner's temp area: nothing is deleted, nothing of the copy lands in the artifacts
        dst = Path(tempfile.mkdtemp(prefix="azqa-landscape-")) / Path(self.app_dir).name
        shutil.copytree(self.app_dir, dst, symlinks=True)
        info_path = dst / "Info.plist"
        info = plistlib.loads(info_path.read_bytes())
        both = ["UIInterfaceOrientationLandscapeLeft", "UIInterfaceOrientationLandscapeRight"]
        info["UISupportedInterfaceOrientations"] = both
        info["UISupportedInterfaceOrientations~ipad"] = both
        info_path.write_bytes(plistlib.dumps(info))
        rc, _o, err = self.x("codesign", "--force", "--sign", "-", "--timestamp=none", str(dst), timeout=120)
        if rc != 0:
            raise RuntimeError(f"codesign: {err.strip()[:200]}")
        return dst

    def gui_keystroke(self, char_code):
        """Cmd + arrow in the Simulator app (Device > Rotate Left = Cmd-Left, Rotate Right = Cmd-Right). Needs the
        GUI app and the runner's accessibility permission for osascript. OFF unless AZ_QA_GUI=1: it is the one part
        of the QA that touches the runner's window server. -> (ok, message)"""
        if os.environ.get("AZ_QA_GUI") != "1":
            return False, "the Simulator GUI steps are off (AZ_QA_GUI=1 turns them on)"
        if not self.gui:
            self.x("open", "-a", "Simulator", "--args", "-CurrentDeviceUDID", self.udid, timeout=60)
            time.sleep(8)
            self.gui = True
        rc, out, err = self.x("osascript", "-e", 'tell application "Simulator" to activate', "-e", "delay 1",
                              "-e", f'tell application "System Events" to keystroke (ASCII character {char_code}) using {{command down}}',
                              timeout=60)
        return rc == 0, (err or out).strip()[:200]

    def gui_menu(self, menu, item):
        """A Simulator menu item by name (Debug > Simulate Memory Warning ...). OFF unless AZ_QA_GUI=1. -> (ok, message)"""
        if os.environ.get("AZ_QA_GUI") != "1":
            return False, "the Simulator GUI steps are off (AZ_QA_GUI=1 turns them on)"
        if not self.gui:
            self.x("open", "-a", "Simulator", "--args", "-CurrentDeviceUDID", self.udid, timeout=60)
            time.sleep(8)
            self.gui = True
        script = (f'tell application "System Events" to tell process "Simulator" to click menu item "{item}" of menu "{menu}" of menu bar 1')
        rc, out, err = self.x("osascript", "-e", 'tell application "Simulator" to activate', "-e", "delay 1", "-e", script, timeout=60)
        return rc == 0, (err or out).strip()[:200]


def ios_scenarios(args):
    run = Run(args.out, "ios", {"label": args.label, "bundle": args.bundle})
    run.only = set(args.only.split(",")) if args.only else None
    run.t0 = time.time()
    exe = args.exe or args.bundle.rsplit(".", 1)[-1]
    dev = IOS(run, args.udid, args.bundle, exe, args.app)
    state = {"portrait": None}
    # --backend cpu|gpu|auto: every launch of this run (the lifecycle steps of each backend) starts the app with that renderer
    gpu_run = args.backend == "gpu"
    if args.backend:
        dev.base_env["AZ_BACKEND"] = args.backend
        run.meta["backend"] = args.backend
    if gpu_run:
        # the first frame of a GL renderer on a software GL compiles every shader (about two minutes): every start of the
        # lifecycle, the ones that bring the app back before a step included, waits as long as the render-mode step does
        args.first_frame_timeout = first_frame_timeout_for("gpu", args.first_frame_timeout)

    def s_device(s):
        rc, out, _e = dev.simctl("list", "devices", "-j")
        try:
            doc = json.loads(out)
            for runtime, devices in doc.get("devices", {}).items():
                for d in devices:
                    if d.get("udid") == args.udid:
                        run.meta.update({"device": d.get("name"), "runtime": runtime.rsplit(".", 1)[-1]})
        except ValueError:
            pass
        dev.simctl("ui", args.udid, "appearance", "light")
        dev.simctl("ui", args.udid, "content_size", "medium")
        dev.simctl("status_bar", args.udid, "override", "--time", "9:41", "--dataNetwork", "wifi", "--wifiMode", "active",
                   "--wifiBars", "3", "--cellularMode", "active", "--cellularBars", "4", "--batteryState", "charged", "--batteryLevel", "100")
        s.note(f"{run.meta.get('device')} / {run.meta.get('runtime')}")
        s.check("the simulator answers", rc == 0, run.meta.get("runtime", ""))

    def recover(s):
        """Before a step: appearance and text size back to normal, the app running (started again when an earlier
        step left it dead: that step has already said so)."""
        dev.simctl("ui", args.udid, "appearance", "light")
        dev.simctl("ui", args.udid, "content_size", "medium")
        if dev.alive():
            return
        s.note("recovered: the app was dead before this step; started it again")
        dev.launch(cold=True)
        t = dev.wait_for_front(timeout=args.first_frame_timeout)
        s.check("the app could be started again before this step", t is not None, f"first frame after {t:.1f} s" if t is not None else "no frame")
        dev.settle(2.0)
        state["portrait"] = dev.last_sig or state["portrait"]

    run.recover = recover

    def s_home(s):
        # the home screen as it is after boot and install (the app's icon on it): no launch of SpringBoard through simctl
        # (the first trial run lost 127 s to it and the install after it timed out)
        dev.settle(3.0)
        sig, w, h, fname = dev.capture("home-baseline")
        s.shots.append(fname)
        dev.home_sig = sig
        s.check("the home screen is a picture", not is_flat(sig), f"{flat_fraction(sig):.3f}")
        s.note(f"screen {w}x{h}")

    def s_install(s):
        if args.app:
            t0 = time.time()
            rc, text = dev.to_file("install.log", "xcrun", "simctl", "install", args.udid, args.app, timeout=600)
            s.check("simctl install succeeds", rc == 0, f"{text.strip()[-160:]} ({time.time() - t0:.0f} s)")
            plist = Path(args.app) / "Info.plist"
            if plist.exists():
                import plistlib
                info = plistlib.loads(plist.read_bytes())
                s.note(f"{info.get('CFBundleIdentifier')} {info.get('CFBundleShortVersionString')} min iOS {info.get('MinimumOSVersion')} orientations {info.get('UISupportedInterfaceOrientations')}")

    # the order of the demos pipeline's smoke test, which works: boot, install, then everything else
    run.step("00-install", "install the Simulator app", s_install)
    run.step("01-device", "the simulator is up; light appearance, medium text, a neutral status bar (9:41, full battery)", s_device)
    run.step("02-home-baseline", "the home screen before the app starts: the picture an app must differ from", s_home)

    def s_cold(s):
        l = dev.launch(cold=True)
        s.note(f"simctl launch: {l['out'] or l['err']}")
        s.check("simctl launch reports a process id", bool(l["pid"]), l["out"] or l["err"])
        t = dev.wait_for_front(timeout=args.first_frame_timeout)
        s.check("a first frame appears (alive, not flat, not the home screen)", t is not None,
                f"after {t:.1f} s" if t is not None else f"nothing after {args.first_frame_timeout} s")
        dev.settle(3.0)
        state["portrait"] = dev.look(s, "portrait")
        dev.alive_step(s)
        crashes = dev.crash_reports()
        s.check("the host wrote no crash report for the app", not crashes, ", ".join(crashes))
        err_txt = l["console"][1].read_text(errors="replace") if l["console"][1].exists() else ""
        out_txt = l["console"][0].read_text(errors="replace") if l["console"][0].exists() else ""
        panics = [ln for ln in (err_txt + out_txt).splitlines() if "panicked" in ln or "unexpected panic" in ln or "FATAL" in ln]
        s.check("the console shows no panic", not panics, panics[0][:200] if panics else "")
        s.note(f"console: {len(out_txt.splitlines())} stdout lines, {len(err_txt.splitlines())} stderr lines")

    run.step("03-cold-start", "start the app from nothing: first frame, portrait picture, no panic, no crash report", s_cold)

    def s_modes(s):
        """AZ_BACKEND=cpu / gpu / auto through SIMCTL_CHILD_, one cold start each. Per mode: which renderer RAN (the
        engine's `[render] backend=` line on the console), the picture, the time to the first frame as the poll saw it;
        then the cpu and gpu pictures compared pixel by pixel. The Simulator has no frame-time source outside the app: the frame
        times are the engine's own `[render] frames` lines during a scripted swipe through its debug server (`--debug-port`: a
        `mobile-e2e` app has one; a plain app has none and says so)."""
        results = {}
        for mode in ("cpu", "gpu", "auto"):
            extra = {"AZ_BACKEND": mode}
            if args.debug_port:
                extra["AZ_DEBUG"] = str(args.debug_port)
            dev.launch(extra_env=extra, cold=True)
            t = dev.wait_for_front(timeout=first_frame_timeout_for(mode, args.first_frame_timeout))
            dev.settle(2.5)
            sig = dev.look(s, f"AZ_BACKEND-{mode}")
            full = dev.last_rgba_full()
            ran, requested, device = backend_ran(dev.console_text())
            s.check(f"AZ_BACKEND={mode}: the app draws", t is not None and not is_flat(sig), f"first frame after {t:.1f} s" if t is not None else "no frame")
            frames = None
            if args.debug_port and t is not None:
                state_answer = dev.server_up(args.debug_port)
                if state_answer is None:
                    s.note(f"AZ_BACKEND={mode}: no debug server answered on port {args.debug_port}: no scripted swipe, no frame times")
                else:
                    frames, made, took = dev.swipe_frame_stats(args.debug_port, state_answer, SWIPE_SECONDS)
                    s.note(f"AZ_BACKEND={mode}: scripted swipe: {made} touch moves in {took:.1f} s ({made / max(took, 0.001):.1f} a second, "
                           f"each waits for a frame)" + ("" if frames else "; no frames line of a full second (fewer than 8 frames in a second)"))
                    if frames and frames.get("draw_median") is not None:
                        s.note(f"AZ_BACKEND={mode}: the draw alone takes median {frames['draw_median']:.1f} / p95 {frames['draw_p95']:.1f} ms")
            if t is not None:
                idle = dev.idle_windows(IDLE_SECONDS)
                s.check(f"AZ_BACKEND={mode}: an idle window draws no frames ({IDLE_SECONDS:.0f} s with nothing touching it)", not idle,
                        (f"{len(idle)} frames line(s) while idle, the last: n={idle[-1]['n']} median {idle[-1]['median']} ms: the window draws "
                         "continuously while it shows the same picture") if idle else "no frames line",
                        required=(mode == "gpu" and args.expect_gpu))
            rss = dev.rss_mb()
            if rss is not None:
                run.meta[f"rss_mb_{mode}"] = round(rss)
                s.note(f"AZ_BACKEND={mode}: resident memory {rss:.0f} MB" + (" (after the swipe)" if frames else ""))
            results[mode] = {"ran": ran, "requested": requested, "device": device, "rgba": full[0] if full else None,
                             "dims": (full[1], full[2]) if full else dev.dims, "first_frame_s": t, "displayed_ms": None, "frames": frames,
                             "rss_mb": rss}
        report_modes(run, s, results, args.expect_gpu, "ios", hardware=args.hardware)
        dev.launch(cold=True)       # the default start again: the picture the later steps compare with
        dev.wait_for_front(timeout=args.first_frame_timeout)
        dev.settle(2.0)
        state["portrait"] = dev.look(s, "default-again")

    run.step("04-render-modes", "AZ_BACKEND=cpu, gpu, auto: which renderer ran, first frame, cpu against gpu pixel by pixel", s_modes)

    # The GPU path's fallbacks, made to happen. AZ_GPU_FAIL=<stage> exists only in a libazul built with the cargo feature test-hooks
    # (the mobile-e2e variant): a shipped libazul ignores it and runs the GPU path to the end. The first stage is the probe of that:
    # a build that does not fail at it has no hooks, and the other ten would each run the whole GPU bring-up to say the same.
    GPU_FAIL_SPECS = ("library", "functions", "context", "make_current", "surface", "probe", "renderer", "document", "shaders",
                      "frame", "lost:2")

    def s_gpu_fallback(s):
        """One launch per stage of the GPU bring-up (and a frame that cannot go out, and a context lost twice): the engine says
        which stage failed, draws on the CPU, keeps the contract line, shows a picture, stays alive and crashes nowhere."""
        injected = 0
        reported_crashes = set(dev.crash_reports())
        for spec in GPU_FAIL_SPECS:
            want = gpu_fail_stage(spec)
            dev.launch(extra_env={"AZ_BACKEND": "gpu", "AZ_GPU_FAIL": spec}, cold=True)      # a new console file for every launch
            t = dev.wait_for_front(timeout=first_frame_timeout_for("gpu", args.first_frame_timeout))
            dev.settle(2.5)
            sig = dev.look(s, f"AZ_GPU_FAIL-{spec.replace(':', '-')}")
            text = dev.console_text()
            ran, requested, device = backend_ran(text)
            stages = [stage for stage, _cause in gpu_fallbacks(text)]
            if not stages and injected == 0:
                why = ("the engine ran the GPU path to the end" if ran == "gpu" else
                       f"the engine ran the {ran} renderer without a fallback line (a build with no GPU path)")
                s.note(f"AZ_GPU_FAIL={spec}: {why}: this libazul has no failure hook (hook not built in); the other stages are skipped")
                s.check("this build has the failure hook (AZ_GPU_FAIL)", False, "hook not built in", required=args.expect_hooks)
                break
            injected += 1
            label = f"AZ_GPU_FAIL={spec}"
            s.check(f"{label}: the app draws (a fallback is never a blank window)", t is not None and not is_flat(sig),
                    f"first frame after {t:.1f} s" if t is not None else "no frame")
            s.check(f"{label}: the fallback line names the stage {want}", want in stages, f"fallback lines say {stages}")
            s.check(f"{label}: the contract line says backend=cpu requested=gpu", (ran, requested) == ("cpu", "gpu"),
                    f"the last [render] line says backend={ran} requested={requested} device={device}")
            dev.alive_step(s, f" after {label}")
            crashes = [c for c in dev.crash_reports() if c not in reported_crashes]
            reported_crashes.update(crashes)
            s.check(f"{label}: the host wrote no crash report", not crashes, ", ".join(crashes))
        s.note(f"{injected} of {len(GPU_FAIL_SPECS)} failures were injected")
        dev.launch(cold=True)       # the default start again: the picture the later steps compare with
        dev.wait_for_front(timeout=args.first_frame_timeout)
        dev.settle(2.0)
        state["portrait"] = dev.look(s, "default-after-failures")

    run.step("04b-gpu-fallback", "AZ_BACKEND=gpu with AZ_GPU_FAIL=<stage> for every stage: the engine names the stage, the CPU draws, the app lives",
             s_gpu_fallback)

    def s_appearance(s):
        base = dev.look(s, "light")
        dev.simctl("ui", args.udid, "appearance", "dark")
        dev.settle(3.0)
        dark = dev.look(s, "dark", expect_changed_from=base)
        dev.alive_step(s)
        dev.simctl("ui", args.udid, "appearance", "light")
        dev.settle(2.5)
        dev.look(s, "light-again")

    run.step("05-dark-mode", "the system appearance switches to dark and back (a trait change under the running app)", s_appearance, needs_app=True)

    def s_content_size(s):
        base = dev.look(s, "text-medium")
        dev.simctl("ui", args.udid, "content_size", "accessibility-extra-extra-extra-large")
        dev.settle(3.0)
        big = dev.look(s, "text-accessibility-xxxl")
        s.check("the page reacts to the text size", sig_diff(big, base) > SAME_BELOW, f"differs {sig_diff(big, base):.3f}", required=False)
        dev.alive_step(s)
        dev.simctl("ui", args.udid, "content_size", "medium")
        dev.settle(2.5)

    run.step("06-text-size", "the system text size goes to the largest accessibility size and back", s_content_size, needs_app=True)

    def s_background(s):
        pid0 = dev.pid()
        console_before = len(dev.process_console().splitlines())
        before = dev.look(s, "before-background")
        dev.open_other_app()
        dev.settle(3.0)
        away, *_ = dev.capture("other-app-in-front")
        s.shots.append(f"{dev.shot_count:02d}-other-app-in-front.png")
        s.check("Settings is in front now", sig_diff(away, before) > SAME_BELOW, f"differs {sig_diff(away, before):.3f}")
        s.check("the app's process survives being backgrounded", dev.alive(), f"pid {dev.pid() or '-'}")
        dev.settle(7.0)  # in all 10 s
        s.check("the app's process survives 10 s in the background", dev.alive(), f"pid {dev.pid() or '-'}")
        dev.launch(cold=False)
        t = dev.wait_for_front(timeout=30)
        s.check("the app is back in front", t is not None, f"after {t:.1f} s" if t is not None else "not back after 30 s")
        dev.settle(2.0)
        after = dev.look(s, "after-background")
        s.check("same process as before", dev.pid() == pid0, f"{pid0} -> {dev.pid()}", required=False)
        s.check("the window shows the picture it had before (it was repainted after the return)", sig_diff(after, before) < 0.08, f"differs {sig_diff(after, before):.3f}")
        crashes = dev.crash_reports()
        s.check("the host wrote no crash report for the app", not crashes, ", ".join(crashes))
        if gpu_run:
            # Apple's rule: an app in the background issues no GL command. The Simulator does not enforce it (a phone kills the app);
            # the engine's own lines do prove it: it says when its GL commands stop and when they are allowed again
            g = gl_in_background(dev.process_console().splitlines()[console_before:])
            s.check("GL: the engine stopped its GL commands when the app went to the background", g["stopped"] >= 1,
                    f"{g['stopped']} `GL commands stopped` line(s) on the console")
            s.check("GL: no GL command between the background and the return (no frame, no failed GL action)", not g["inside"],
                    "; ".join(g["inside"][:3]) if g["inside"] else "none of the engine's lines shows one")
            s.check("GL: the engine allowed its GL commands again on the return", g["allowed"] >= 1,
                    f"{g['allowed']} `GL commands allowed again` line(s) on the console")

    run.step("07-background-10s", "another app comes to the front for 10 s, then the app again: same process, same picture", s_background, needs_app=True)

    def s_rotate(s):
        before = dev.look(s, "before-rotate")
        ok, msg = dev.gui_keystroke(28)    # Cmd-Left: rotate left
        s.note(f"Simulator rotate left via osascript: {'sent' if ok else 'FAILED: ' + msg}")
        s.check("the runner lets osascript press Cmd-Left in the Simulator", ok, msg, required=False)
        dev.settle(3.5)
        sig = dev.look(s, "landscape")
        w, h = dev.dims
        s.note(f"screenshot {w}x{h} after the rotation")
        changed = sig_diff(sig, before) > SAME_BELOW
        s.check("the picture changed with the rotation", changed, f"differs {sig_diff(sig, before):.3f}", required=False)
        dev.alive_step(s)
        if ok:
            dev.gui_keystroke(29)          # Cmd-Right: back
            dev.settle(3.5)
            again = dev.look(s, "portrait-again")
            s.check("back in portrait the picture is the first one again", sig_diff(again, before) < 0.10, f"differs {sig_diff(again, before):.3f}", required=False)
            dev.alive_step(s, " after rotating back")
        if not (ok and changed) and args.app:
            # the rotation could not be pressed or did nothing: the landscape LAYOUT through an app that allows only landscape
            s.note("falling back to a landscape-only copy of the app (Info.plist orientations)")
            try:
                copy = dev.landscape_copy()
                rc, _o, err = dev.simctl("install", args.udid, str(copy), timeout=180)
                s.check("the landscape-only copy installs", rc == 0, err.strip()[:160], required=False)
                dev.launch(cold=True)
                t = dev.wait_for_front(timeout=args.first_frame_timeout)
                dev.settle(3.0)
                lsig = dev.look(s, "landscape-only-app")
                s.check("the landscape-only copy draws", t is not None and not is_flat(lsig), f"first frame after {t:.1f} s" if t is not None else "no frame", required=False)
                s.note(f"landscape-only app screenshot {dev.dims[0]}x{dev.dims[1]}")
            finally:
                dev.simctl("install", args.udid, args.app, timeout=180)   # the original app again
                dev.launch(cold=True)
                dev.wait_for_front(timeout=args.first_frame_timeout)
                dev.settle(2.0)

    run.step("08-rotate", "rotate the device left and back while the app runs (Simulator menu through osascript)", s_rotate, needs_app=True)

    def s_memory(s):
        console_before = len(dev.process_console().splitlines())
        before = dev.look(s, "before-memory-warning")
        ok, msg = dev.gui_menu("Debug", "Simulate Memory Warning")
        s.note(f"Debug > Simulate Memory Warning: {'sent' if ok else 'FAILED: ' + msg}")
        s.check("the Simulator menu item was pressed", ok, msg, required=False)
        dev.settle(3.0)
        dev.look(s, "after-memory-warning")
        dev.alive_step(s)
        if gpu_run:
            released = [l for l in dev.process_console().splitlines()[console_before:] if MEMORY_RELEASED.search(l)]
            s.check("GL: the engine released the renderer's caches at the memory warning", bool(released), f"{len(released)} line(s) on the console",
                    required=ok)

    run.step("09-memory-warning", "Debug > Simulate Memory Warning while in the foreground", s_memory, needs_app=True)

    def s_cold2(s):
        dev.terminate()
        dev.settle(1.5)
        s.check("the process is gone after terminate", not dev.alive(), "")
        dev.launch(cold=True)
        t = dev.wait_for_front(timeout=args.first_frame_timeout)
        s.check("a second cold start shows a frame", t is not None, f"after {t:.1f} s" if t is not None else "no frame")
        dev.settle(2.0)
        dev.look(s, "cold-start-2")
        dev.alive_step(s)

    run.step("10-terminate-and-start-again", "kill the process, start again", s_cold2)

    def s_logs(s):
        # straight into a file with a bound (a pipe hung the first trial for an hour); the last 6 minutes, info level
        rc, out = dev.to_file("log-all.txt", "xcrun", "simctl", "spawn", args.udid, "log", "show", "--last", "6m", "--style", "compact", "--info", timeout=240)
        s.note(f"log show exit {rc}, {len(out.splitlines())} lines")
        mine = [l for l in out.splitlines() if exe in l or "launchd_sim" in l and dev.bundle in l]
        (run.logs_dir / "log-app.txt").write_text("\n".join(mine), encoding="utf-8")
        rep = dev.crash_reports()
        s.check("the host wrote no crash report for the app in the whole run", not rep, ", ".join(rep))
        d = Path.home() / "Library" / "Logs" / "DiagnosticReports"
        (run.logs_dir / "DiagnosticReports-ls.txt").write_text("\n".join(sorted(p.name for p in d.glob("*"))) if d.exists() else "no DiagnosticReports", encoding="utf-8")
        consoles = sorted(run.logs_dir.glob("console-*.txt"))
        text = "\n".join(c.read_text(errors="replace") for c in consoles)
        bad = [l for l in text.splitlines() if "panicked" in l or "unexpected panic" in l or "FATAL" in l]
        s.check("no panic on any console of the run", not bad, bad[0][:200] if bad else "")
        s.note(f"{len(text.splitlines())} console lines of the app; {len(mine)} unified-log lines of the app")

    run.step("11-logs", "the whole log: no crash report, no panic, what the engine said", s_logs)
    run.write()
    return run


# ---------------------------------------------------------------------------------------------
def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="platform", required=True)
    a = sub.add_parser("android")
    a.add_argument("--out", required=True)
    a.add_argument("--apk", default="")
    a.add_argument("--package", default="com.azul.AzWidgets")
    a.add_argument("--activity", default="")
    a.add_argument("--label", default="AzWidgets")
    a.add_argument("--gpu", default="swiftshader_indirect")
    a.add_argument("--serial", default="")
    a.add_argument("--points", default="", help="JSON {name: [fx, fy]} overriding the tap points (fractions of the screen)")
    a.add_argument("--first-frame-timeout", type=int, default=45)
    a.add_argument("--only", default="", help="comma list of step numbers to run besides 00-02, e.g. 03,08,09,12 (a first look at another app)")
    a.add_argument("--series", type=int, default=0, help="N cold starts to measure (0: none)")
    a.add_argument("--expect-gpu", action="store_true", help="the build has a GPU renderer: a gpu request that ran the CPU one FAILS")
    a.add_argument("--hardware", action="store_true", help="the device has a real GPU (a phone, not a CI emulator): `auto` must then run the GL renderer")
    a.add_argument("--floor-apk", default="", help="the floor APK (an empty Java Activity) measured the same way")
    a.add_argument("--tap-label", default="", help="press the button the accessibility tree names this (step 03e)")
    i = sub.add_parser("ios")
    i.add_argument("--out", required=True)
    i.add_argument("--udid", required=True)
    i.add_argument("--bundle", default="com.azul.AzWidgets")
    i.add_argument("--exe", default="")
    i.add_argument("--app", default="", help="the .app directory to install")
    i.add_argument("--label", default="AzWidgets")
    i.add_argument("--first-frame-timeout", type=int, default=45)
    i.add_argument("--expect-gpu", action="store_true", help="the build has a GPU renderer: a gpu request that ran the CPU one FAILS")
    i.add_argument("--hardware", action="store_true", help="the device has a real GPU (a phone, not a CI Simulator): `auto` must then run the GL renderer")
    i.add_argument("--only", default="", help="comma list of step numbers to run besides 00-02")
    i.add_argument("--backend", default="", choices=["", "cpu", "gpu", "auto"],
                   help="start the app with AZ_BACKEND=<this> in every step of the lifecycle (default: the app's own choice, the CPU renderer); "
                        "gpu also makes the checks that no GL command goes out in the background REQUIRED")
    i.add_argument("--expect-hooks", action="store_true",
                   help="the build has the failure hooks (cargo feature test-hooks: a mobile-e2e app): a build that ignores AZ_GPU_FAIL FAILS step 04b")
    i.add_argument("--debug-port", type=int, default=0,
                   help="the port of the app's debug server (a mobile-e2e app starts one with AZ_DEBUG=<port>): the render-mode step swipes through it "
                        "and reads the engine's frame lines (0: no swipe, no frame times)")
    args = ap.parse_args(argv)
    run = android_scenarios(args) if args.platform == "android" else ios_scenarios(args)
    failed = [s.name for s in run.steps if not s.ok]
    print(f"\nQA {'PASS' if not failed else 'FAIL'}: {len(run.steps) - len(failed)}/{len(run.steps)} steps"
          + (f"; failed: {', '.join(failed)}" if failed else ""))
    return 0   # the verdict is in qa.json; a failing app is a result, not a broken run


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
