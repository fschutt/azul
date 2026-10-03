# WAYLAND8 progress (wave 8, branch wt/wayland8, base 45c6bf98b)

Task: research whether our Wayland backend has the "faster wl_shm" KDE did
(https://www.phoronix.com/news/KDE-Plasma-Faster-WL-SHM); analyse our present path; implement a client-side win
if clear, with tests that run without a compositor. Report: scripts/WAYLAND8_2026_10_03.md.

## DONE
- branch wt/wayland8 created at 45c6bf98b

## IN PROGRESS
- step 1: read the Phoronix article + linked KWin MR / blog

## NEXT
- step 2: read dll/src/desktop/shell2/linux/wayland/* present path (wl_shm pool, buffers, damage, memcpy)
- step 3: write findings into the report
- step 4: implement a client-side win (RED test first) if there is one

## Decisions
- (none yet)

## Open questions
- (none yet)
