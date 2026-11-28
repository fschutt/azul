#!/usr/bin/env python3
"""startup_strip_pair.py: the BEFORE and the AFTER of a cold start in one picture (standard library only; ffmpeg for the videos).

    startup_strip_pair.py BEFORE.mp4 AFTER.mp4 OUT.png [--night]

Two recordings of scripts/ci/startup_lab.py (`<row>/start1/cold.mp4`), each cut into frames at 30 fps and labelled like the lab labels them; the picture
is the BEFORE rows above the AFTER rows, every second frame from two frames before the app phase begins to six frames after the settled picture, 14 to a
row (so a row is 0.93 s). The report line says the run lengths of both. The captions (which is which) go in the caption file next to the picture.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import flicker_audit as fa  # noqa: E402
import startup_lab as lab  # noqa: E402


def pick_frames(labels, step=2, lead=2, tail=6, cap=84):
    """The frame indices of a cold start worth a picture: from `lead` frames before the first frame of the app phase (the first label that is not `L`) to
    `tail` frames after the first settled picture (`2`; the end of the recording when there is none), every `step`-th, at most `cap`."""
    first = next((i for i, c in enumerate(labels) if c != "L"), 0)
    settled = next((i for i, c in enumerate(labels) if c == "2"), len(labels) - 1)
    return list(range(max(0, first - lead), min(len(labels), settled + tail + 1), step))[:cap]


def rows_of(picks, per_row=14, offset=0):
    return [[i + offset for i in picks[k:k + per_row]] for k in range(0, len(picks), per_row)]


def main(argv):
    night = "--night" in argv
    args = [a for a in argv if not a.startswith("--")]
    if len(args) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    before, after, out = args
    frames_b, frames_a = fa.cut_frames(before), fa.cut_frames(after)
    if not frames_b or not frames_a:
        print("a recording could not be cut into frames (ffmpeg?)", file=sys.stderr)
        return 1
    labels_b = lab.label_frames([fa.signature_of(f) for f in frames_b], night=night)
    labels_a = lab.label_frames([fa.signature_of(f) for f in frames_a], night=night)
    rows = rows_of(pick_frames(labels_b)) + rows_of(pick_frames(labels_a), offset=len(frames_b))
    fa.write_strip(out, frames_b + frames_a, rows)
    print(f"BEFORE {lab.runs_text(labels_b)}")
    print(f"AFTER  {lab.runs_text(labels_a)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
