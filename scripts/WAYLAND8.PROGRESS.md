# WAYLAND8 progress (wave 8, branch wt/wayland8, base 45c6bf98b)

Task: research whether our Wayland backend has the "faster wl_shm" KDE did
(https://www.phoronix.com/news/KDE-Plasma-Faster-WL-SHM); analyse our present path; implement a client-side win
if clear, with tests that run without a compositor. Report: scripts/WAYLAND8_2026_10_03.md.

## DONE
- branch wt/wayland8 created at 45c6bf98b
- step 1 (research) done. Facts (for the report):
  - Phoronix 2026-05-07 -> Xaver Hugl's blog https://zamundaaa.github.io/wayland/2026/05/06/making-wl-shm-fast.html
    -> KWin MR !9178 "opengl/egldisplay: allow importing shm buffers through udmabuf" (Plasma 6.7, bugs 506240, 513295)
    -> Qt change https://codereview.qt-project.org/c/qt/qtbase/+/733869 "platforms/wayland: allocate shm buffers to be
       usable as udmabufs" (MERGED 2026-05-06, dev, Pick-to 6.11; +18/-2 in qwaylandshmbackingstore.cpp).
  - OLD KWin path: per committed shm buffer, a BLOCKING main-thread CPU memcpy into a GPU-accessible buffer, then a
    GPU upload copy (system RAM -> system RAM again on iGPUs). NEW: KWin wraps the client's memfd range with the
    udmabuf driver into a dmabuf and imports it into EGL; if the stride suits the GPU it is sampled directly (zero
    copies), else falls back to the old upload. Compositor-side change.
  - CLIENT side (the Qt diff, verbatim logic): `stride = alignTo(width*4, 256)`; `alloc = alignTo(stride*height,
    getpagesize())` - pool size (and buffer offset 0, one memfd per buffer) page-aligned, stride a multiple of 256.
    No protocol change, no feedback of the alignment - 256 is "what all common GPUs can read from".
  - Cost: 3840x2160 -> ~1.6% more memory per buffer. Result: KWin 80-90% of one core -> ~20% scrolling KDevelop.

## IN PROGRESS
- step 2: read dll/src/desktop/shell2/linux/wayland/* present path (wl_shm pool, buffers, damage, memcpy)

## NEXT
- step 3: write findings into the report
- step 4: implement a client-side win (RED test first) if there is one

## Decisions
- (none yet)

## Open questions
- (none yet)
