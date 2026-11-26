#!/usr/bin/env python3
"""mobile_gallery.py: the contact sheets of the phone QA runs (LOCAL tool: needs ImageMagick's `magick`).

    mobile_gallery.py --out DIR --column "Android / CPU renderer / emulator GL: swiftshader_indirect"=android/swiftshader/shots \
                                --column "Android / CPU renderer / emulator GL: guest"=android/guest/shots \
                                [--column "iOS Simulator / CPU renderer"=ios/shots ...]

Each --column is a title and the `shots/` directory of one scripts/ci/mobile_qa.py run (`NN-<stem>.png`). The sheets
have one ROW per column (so the platform and mode read down the left) and one CELL per scenario; a scenario a run did
not reach (or that the platform has no equivalent of) is a grey cell that says so. Five sheets, grouped:
  1-first-frame-and-touch     the launcher, the first frame, scrolled, scrolled back, a tap, the keyboard
  2-rotation-and-leaving      landscape, a tap in landscape, portrait again, home, back in front, 10 s away
  3-system-changes            memory pressure, dark mode, text size / font scale, display density
  4-dying-and-coming-back     BACK and start again, force-stop, killed in the background, 300 random touches
  5-cpu-against-gpu           AZ_BACKEND cpu / gpu / auto and the pixel diff of cpu against gpu
A cell is the picture at 216 x 480; the full pictures stay in the runs' shots/ directories.
"""
import argparse
import re
import subprocess
import sys
from pathlib import Path

CELL_W, CELL_H = 216, 480

# sheet -> [(caption, [stems of the picture on Android], [stems on iOS])]
SHEETS = {
    "1-first-frame-and-touch": [
        ("launcher before", ["home-baseline"], ["home-baseline"]),
        ("first frame", ["portrait"], ["portrait", "AZ_BACKEND-cpu"]),
        ("AZ_BACKEND=gpu", ["AZ_BACKEND-gpu"], ["AZ_BACKEND-gpu"]),
        ("AZ_BACKEND=auto", ["AZ_BACKEND-auto"], ["AZ_BACKEND-auto"]),
        ("scrolled by a swipe", ["scrolled-down"], []),
        ("scrolled back up", ["scrolled-back-up"], []),
        ("tap on Dark", ["after-tap-dark"], []),
        ("text field + keyboard", ["keyboard-and-text"], []),
    ],
    "2-rotation-and-leaving": [
        ("landscape", ["landscape"], ["landscape-only-app", "landscape"]),
        ("landscape after a tap", ["landscape-after-tap"], []),
        ("portrait again", ["portrait-again"], ["portrait-again"]),
        ("home pressed", ["home-pressed"], ["other-app-in-front"]),
        ("back in front", ["resumed"], []),
        ("after 10 s away", ["after-10s-background"], ["after-background"]),
    ],
    "3-system-changes": [
        ("memory pressure", ["after-trim"], ["after-memory-warning"]),
        ("system dark mode", ["system-dark"], ["dark"]),
        ("back to light", ["system-light-again"], ["light-again"]),
        ("font / text size big", ["font-1.5"], ["text-accessibility-xxxl"]),
        ("display density 480", ["density-480"], []),
        ("density reset", ["density-reset"], []),
    ],
    "5-cpu-against-gpu": [
        ("AZ_BACKEND=cpu", ["AZ_BACKEND-cpu"], ["AZ_BACKEND-cpu"]),
        ("AZ_BACKEND=gpu", ["AZ_BACKEND-gpu"], ["AZ_BACKEND-gpu"]),
        ("AZ_BACKEND=auto", ["AZ_BACKEND-auto"], ["AZ_BACKEND-auto"]),
        ("diff cpu / gpu (red = differs)", ["diff-cpu-vs-gpu"], ["diff-cpu-vs-gpu"]),
    ],
    "4-dying-and-coming-back": [
        ("after BACK", ["after-back"], []),
        ("start again after BACK", ["relaunch-after-back"], []),
        ("cold start 2", ["cold-start-2"], ["cold-start-2"]),
        ("after the system killed it", ["after-system-kill"], []),
        ("after 300 random touches", ["after-monkey"], []),
    ],
}


FONT_CANDIDATES = ("/System/Library/Fonts/Supplemental/Arial.ttf", "/System/Library/Fonts/Helvetica.ttc",
                   "/Library/Fonts/Arial.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
FONT = next((f for f in FONT_CANDIDATES if Path(f).exists()), "")


def magick(*args):
    args = list(args)
    if FONT and "-annotate" in args:
        args = ["-font", FONT] + args       # ImageMagick has no default font of its own on a Mac
    p = subprocess.run(["magick", *map(str, args)], capture_output=True)
    if p.returncode != 0:
        raise RuntimeError(p.stderr.decode("utf-8", "replace")[:300])


def find_shot(shots_dir, stems):
    for stem in stems:
        hits = sorted(Path(shots_dir).glob(f"[0-9]*-{stem}.png"))
        if hits:
            return hits[0]
    return None


def cell(shot, caption, work, name, missing_text):
    out = work / f"{name}.png"
    if shot is None:
        magick("-size", f"{CELL_W}x{CELL_H}", "xc:#383838", "-fill", "#aaaaaa", "-gravity", "center", "-pointsize", "14",
               "-annotate", "+0+0", missing_text, out)
    else:
        magick(shot, "-resize", f"{CELL_W}x{CELL_H}", "-background", "#202020", "-gravity", "center", "-extent", f"{CELL_W}x{CELL_H}", out)
    labelled = work / f"{name}-l.png"
    magick(out, "-background", "#202020", "-gravity", "south", "-splice", "0x26", "-fill", "white", "-pointsize", "13",
           "-annotate", "+0+4", caption, labelled)
    return labelled


def build(out_dir, columns):
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    work = out_dir / ".work"
    work.mkdir(exist_ok=True)
    made = []
    for sheet, cells in SHEETS.items():
        rows = []
        for ri, (title, shots_dir) in enumerate(columns):
            ios = title.lower().startswith("ios")
            parts = []
            for ci, (caption, a_stems, i_stems) in enumerate(cells):
                stems = i_stems if ios else a_stems
                shot = find_shot(shots_dir, stems) if stems else None
                why = "no such step on this platform" if not stems else "not reached (the app was gone)"
                parts.append(cell(shot, caption, work, f"{sheet}-{ri}-{ci}", why))
            strip = work / f"{sheet}-{ri}-strip.png"
            magick(*parts, "+append", strip)
            label = work / f"{sheet}-{ri}-label.png"
            magick("-size", "190x" + str(CELL_H + 26), "xc:#101010", "-fill", "white", "-gravity", "center", "-pointsize", "15",
                   "-annotate", "+0+0", "\n".join(wrap(title, 24)), label)
            row = work / f"{sheet}-{ri}-row.png"
            magick(label, strip, "+append", row)
            rows.append(row)
        sheet_png = out_dir / f"contact-sheet-{sheet}.png"
        magick(*rows, "-append", sheet_png)
        made.append(sheet_png)
        print("wrote", sheet_png)
    for p in work.glob("*.png"):
        p.unlink()
    work.rmdir()
    return made


def wrap(text, width):
    words, lines, cur = text.replace("/", "/ ").split(), [], ""
    for w in words:
        if len(cur) + len(w) + 1 > width and cur:
            lines.append(cur.strip())
            cur = ""
        cur += w + " "
    if cur.strip():
        lines.append(cur.strip())
    return lines


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--column", action="append", required=True, help='"title=path/to/shots"')
    args = ap.parse_args(argv)
    cols = []
    for c in args.column:
        title, _, path = c.rpartition("=")
        cols.append((title, path))
    build(args.out, cols)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
