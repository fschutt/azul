# VIDEO_REVIEW - progress

Branch `wt/video-review` from `253266d13`. Task: re-analyze the day's NV12 / video work
(20 commits `6ae909529..253266d13`) for implementation bugs. Nothing compiled, nothing run.

## DONE
- c844ccaee test(image_scale): a same-size cut of an odd-sized NV12 frame is a copy (RED)
- 7f662e86e fix(image_scale): an NV12 crop evens only the sides it cuts
- ef2052247 fix(azmeet): a rendition no longer shown closes its decoder; a fresh decoder is
  told the tile's size
- 224bf67d2 fix(capture): the camera and screen delegates lock the pixel buffer read-only
- report `scripts/VIDEO_REVIEW_2026_09_30.md`

## IN PROGRESS
- nothing

## NEXT
- parent: compile, run the test commands in the report, apply the stream.rs one-liner
  (write-up 4) if wanted (file off limits for this task)

## Open questions
- the 67 threads: not pinned (see report, "not pinned")
