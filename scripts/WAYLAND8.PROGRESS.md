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

  - KWin MR diff (fetched via GitLab API): GpuManager opens /dev/udmabuf; createUdmabuf(shm) refuses
    `offset % pagesize != 0`, ioctl UDMABUF_CREATE {memfd = pool fd, offset, size = align(h*stride, page)};
    pitch = stride, LINEAR. Import cached per buffer; failure -> old loadShmTexture/updateShmTexture copy path.
    UDmabufReleasePoint keeps the client buffer referenced until the GPU fence of the frame that sampled it
    -> with udmabuf the CURRENT buffer stays busy until the next commit + GPU done (no early release).
  - KERNEL (drivers/dma-buf/udmabuf.c, fetched): check_memfd_seals: file must be shmem/hugetlb AND have
    F_SEAL_SHRINK AND must NOT have F_SEAL_WRITE/F_SEAL_FUTURE_WRITE; offset and size PAGE_ALIGNED;
    memfd_pin_folios: end >= i_size -> EINVAL (range must lie inside the file).
- step 2 (our backend) read: CpuFallbackState::new (wayland/mod.rs ~8940): memfd_create("azul-fb", MFD_CLOEXEC)
  -> NO MFD_ALLOW_SEALING, NO F_SEAL_SHRINK => the kernel REFUSES udmabuf for every buffer we make.
  Pool = 2 slots, stride = w*4 (tight), slot 1 offset = stride*h (page-aligned only if w*h % 1024 == 0).
  Present: native path renders straight into the slot (AzulPixmap::from_external, TIGHT stride required:
  render_frame checks target dims == frame dims), catch_up_slot copies the other slot's stale rects,
  ARGB-only compositors get an in-place R<->B swizzle of the damage; per-rect wl_surface.damage_buffer (v4+);
  attach only when damaged. Resize = new pool each configure. Duplicate allocators: tooltip.rs
  allocate_shm_buffer, screencopy.rs memfd() - same memfd-then-shm_open code three times.

## Decisions
- D1: the clear client win = memfd with MFD_ALLOW_SEALING + F_SEAL_SHRINK|F_SEAL_SEAL, every slot's offset and
  size page-aligned (slot_bytes = align_up(stride*h, page), pool = 2*slot_bytes). Zero renderer impact.
- D2: stride stays TIGHT (w*4): the renderer draws directly into the slot and AzulPixmap has no row pitch.
  Widths with w*4 % 256 == 0 (w % 64 == 0: 1280/1920/2560/3840...) then get KWin's zero-copy path; others get a
  udmabuf whose EGL import may refuse the pitch (driver rule) -> KWin's old copy, no regression. Padding the
  pitch = plan item (needs a row-pitch in AzulPixmap) - not done here.
- D3: ONE helper for the shm file (new wayland/shm.rs), used by the window pool, the tooltip and screencopy
  (NO DUPLICATION rule) - the three copies are twins today.
- D4: tests live in wayland/shm.rs (#[cfg(test)]): pure layout math + a real memfd seal check; Linux-only
  module => they run on Linux CI without a compositor; the Mac build does not compile them.

- step 4 commits: RED shm.rs + tests (`test(wayland8): RED ...`), GREEN pool_layout page-aligned,
  GREEN create_shm_file sealed memfd (safe fn). `mod shm;` registered in wayland/mod.rs.

- step 4b DONE: CpuFallbackState::new uses shm::create_shm_file + shm::pool_layout (+ udmabuf/pitch256 trace,
  legacy copy dst_stride = cpu_state.stride). tooltip.rs uses the helper (CString import dropped).

## COORDINATOR ASKS (2026-10-03, mid-task) - in force
1. stride padded to a multiple of 256 bytes, every slot page-aligned; EVERYTHING that indexes rows uses the
   padded stride (renderer draws into the slot! AzulPixmap::from_external is tight -> needs a row pitch:
   layout/src/cpurender/pixmap.rs AzulPixmap + every `row * width * 4` site, or render into a pitch-aware view).
2. drop the spare slot when idle (~1 s without frames: destroy its wl_buffer, punch the memory out
   (fallocate PUNCH_HOLE / madvise REMOVE) or one memfd per slot) and re-create it on demand when the next frame
   finds the remaining slot still held -> an idle window holds ONE buffer. Test-first (pure state machine).
3. note the ARGB8888-only swizzle path's extra pass in the report.
4. X11 in the report: CPU path = plain XPutImage (pixels through the socket + server copy) -> MIT-SHM plan
   (XShmPutImage, shared segment, fallback when the extension is missing / remote display), bytes per frame
   before/after. Implementing MIT-SHM optional (X11 files are mine for this).

## USER DECISION (via coordinator, 2026-10-03) - IMPLEMENT BOTH
(a) Wayland: 256-byte stride + page-aligned slots; idle spare-slot drop + on-demand re-creation; everything that
    indexes rows uses the padded stride. (b) X11 MIT-SHM is NOT optional: XShmQueryExtension / XShmCreateImage /
    XShmAttach / XShmPutImage per damaged rect, segment sized to the window, re-created on resize, wait for
    XShmCompletionEvent before reusing (or double-buffer), fall back to XPutImage when the extension is missing,
    the display is remote, or attach fails. libXext dlopen'd like the other X libs. RED tests first for:
    stride/offset math, slot lifecycle state machine, the X11 fallback decision, the damage-rect copy.

## DONE (since)
- screencopy.rs memfd twin removed (uses shm::create_shm_file); report skeleton committed (section 1 filled).

## DESIGN NOTES for the padded pitch (read before coding)
- render_frame (dll/src/desktop/shell2/headless/mod.rs ~1101) refuses a native target whose dims != frame dims.
  Plan: new CpuBackend field `native_target_row_padding_px: u32` (set at EVERY arming, 0 on macOS/Windows);
  render_frame accepts `ext.width() == pixel_w + padding`. AzulPixmap's width IS its pitch, so a pixmap of
  stride/4 px wide over the slot draws the frame in columns [0, pixel_w) and the padding is never presented.
  Full renders `fill` the padding too (harmless). Wayland arming: from_external(ptr, stride/4, h).
- Fix every linear copy that assumed tight rows: popup render_if_ready (copies pixmap linearly into the slot),
  tooltip blit_pixmap (stride = width*4).

## DONE (A1, padded pitch) - all committed
- shm.rs RED/GREEN pitch = align_up(w*4, 256) (+ pitch_px / padding_px); headless/mod.rs: field
  `native_target_row_padding_px` + RED test `a_row_padded_native_target_holds_the_owned_frame_in_every_row` +
  GREEN render_frame accepts target_w = pixel_w + padding; Wayland arming uses pitch_px + padding;
  `copy_rgba_rects_into` (headless/mod.rs, RED tests + GREEN) = the one pitched damage-rect upload, used by the
  Wayland legacy present, the popup copy (was linear!), the tooltip blit (new `stride` field).

## IN PROGRESS
- A2 spare-slot lifecycle: pure state machine in shm.rs (RED), then CpuFallbackState wiring

## NEXT (exact order)
- A2 RED: shm.rs `SpareSlotPolicy` tests (idle >= 1 s with the spare free -> release; a frame that finds the
  kept slot busy and the spare released -> recreate; never release a busy slot / the slot holding the frame)
- A2 GREEN + wiring: CpuFallbackState: slot `released` state (wl_buffer destroyed, range punched with
  fallocate(PUNCH_HOLE|KEEP_SIZE)), acquire_slot recreates via wl_shm_pool_create_buffer at the same offset;
  idle hook: find where the Wayland loop parks (wait_for_events) and call cpu_state.release_idle_spare(now)
- B X11 MIT-SHM (see USER DECISION above)
- report sections 2-10

## Decisions
- D1: the clear client win = memfd with MFD_ALLOW_SEALING + F_SEAL_SHRINK|F_SEAL_SEAL, every slot's offset and
  size page-aligned (slot_bytes = align_up(stride*h, page), pool = 2*slot_bytes). Zero renderer impact.
- D2: stride stays TIGHT (w*4): the renderer draws directly into the slot and AzulPixmap has no row pitch.
  Widths with w*4 % 256 == 0 (w % 64 == 0: 1280/1920/2560/3840...) then get KWin's zero-copy path; others get a
  udmabuf whose EGL import may refuse the pitch (driver rule) -> KWin's old copy, no regression. Padding the
  pitch = plan item (needs a row-pitch in AzulPixmap) - not done here.
- D3: ONE helper for the shm file (new wayland/shm.rs), used by the window pool, the tooltip and screencopy
  (NO DUPLICATION rule) - the three copies are twins today.
- D4: tests live in wayland/shm.rs (#[cfg(test)]): pure layout math + a real memfd seal check; Linux-only
  module => they run on Linux CI without a compositor; the Mac build does not compile them.

- step 4 commits: RED shm.rs + tests (`test(wayland8): RED ...`), GREEN pool_layout page-aligned,
  GREEN create_shm_file sealed memfd (safe fn). `mod shm;` registered in wayland/mod.rs.

- step 4b DONE: CpuFallbackState::new uses shm::create_shm_file + shm::pool_layout (+ udmabuf/pitch256 trace,
  legacy copy dst_stride = cpu_state.stride). tooltip.rs uses the helper (CString import dropped).

## COORDINATOR ASKS (2026-10-03, mid-task) - in force
1. stride padded to a multiple of 256 bytes, every slot page-aligned; EVERYTHING that indexes rows uses the
   padded stride (renderer draws into the slot! AzulPixmap::from_external is tight -> needs a row pitch:
   layout/src/cpurender/pixmap.rs AzulPixmap + every `row * width * 4` site, or render into a pitch-aware view).
2. drop the spare slot when idle (~1 s without frames: destroy its wl_buffer, punch the memory out
   (fallocate PUNCH_HOLE / madvise REMOVE) or one memfd per slot) and re-create it on demand when the next frame
   finds the remaining slot still held -> an idle window holds ONE buffer. Test-first (pure state machine).
3. note the ARGB8888-only swizzle path's extra pass in the report.
4. X11 in the report: CPU path = plain XPutImage (pixels through the socket + server copy) -> MIT-SHM plan
   (XShmPutImage, shared segment, fallback when the extension is missing / remote display), bytes per frame
   before/after. Implementing MIT-SHM optional (X11 files are mine for this).

## USER DECISION (via coordinator, 2026-10-03) - IMPLEMENT BOTH
(a) Wayland: 256-byte stride + page-aligned slots; idle spare-slot drop + on-demand re-creation; everything that
    indexes rows uses the padded stride. (b) X11 MIT-SHM is NOT optional: XShmQueryExtension / XShmCreateImage /
    XShmAttach / XShmPutImage per damaged rect, segment sized to the window, re-created on resize, wait for
    XShmCompletionEvent before reusing (or double-buffer), fall back to XPutImage when the extension is missing,
    the display is remote, or attach fails. libXext dlopen'd like the other X libs. RED tests first for:
    stride/offset math, slot lifecycle state machine, the X11 fallback decision, the damage-rect copy.

## DONE (since)
- screencopy.rs memfd twin removed (uses shm::create_shm_file); report skeleton committed (section 1 filled).

## DESIGN NOTES for the padded pitch (read before coding)
- render_frame (dll/src/desktop/shell2/headless/mod.rs ~1101) refuses a native target whose dims != frame dims.
  Plan: new CpuBackend field `native_target_row_padding_px: u32` (set at EVERY arming, 0 on macOS/Windows);
  render_frame accepts `ext.width() == pixel_w + padding`. AzulPixmap's width IS its pitch, so a pixmap of
  stride/4 px wide over the slot draws the frame in columns [0, pixel_w) and the padding is never presented.
  Full renders `fill` the padding too (harmless). Wayland arming: from_external(ptr, stride/4, h).
- Fix every linear copy that assumed tight rows: popup render_if_ready (copies pixmap linearly into the slot),
  tooltip blit_pixmap (stride = width*4).

## IN PROGRESS
- A1 RED: shm.rs test "the row pitch is a multiple of 256 bytes" (replaces "stays tight")

## NEXT (exact order)
- A1 GREEN pool_layout pads to 256; wire render_frame field + wayland arming + popup + tooltip (commit each)
- A2 spare-slot lifecycle: pure state machine in shm.rs (RED tests) -> CpuFallbackState (destroy wl_buffer +
  fallocate PUNCH_HOLE|KEEP_SIZE on the slot range; recreate on demand); find the idle hook in the event loop
- B  X11 MIT-SHM: read dll/src/desktop/shell2/linux/x11 CPU present + dlopen; RED tests (DISPLAY local/remote,
     fallback decision, damage-rect copy into the segment); dlopen libXext; wire; commit per unit
- report sections 2-10

## Decisions
- D1: the clear client win = memfd with MFD_ALLOW_SEALING + F_SEAL_SHRINK|F_SEAL_SEAL, every slot's offset and
  size page-aligned (slot_bytes = align_up(stride*h, page), pool = 2*slot_bytes). Zero renderer impact.
- D2: stride stays TIGHT (w*4): the renderer draws directly into the slot and AzulPixmap has no row pitch.
  Widths with w*4 % 256 == 0 (w % 64 == 0: 1280/1920/2560/3840...) then get KWin's zero-copy path; others get a
  udmabuf whose EGL import may refuse the pitch (driver rule) -> KWin's old copy, no regression. Padding the
  pitch = plan item (needs a row-pitch in AzulPixmap) - not done here.
- D3: ONE helper for the shm file (new wayland/shm.rs), used by the window pool, the tooltip and screencopy
  (NO DUPLICATION rule) - the three copies are twins today.
- D4: tests live in wayland/shm.rs (#[cfg(test)]): pure layout math + a real memfd seal check; Linux-only
  module => they run on Linux CI without a compositor; the Mac build does not compile them.

- step 4 commits: RED shm.rs + tests (`test(wayland8): RED ...`), GREEN pool_layout page-aligned,
  GREEN create_shm_file sealed memfd (safe fn). `mod shm;` registered in wayland/mod.rs.

- step 4b DONE: CpuFallbackState::new uses shm::create_shm_file + shm::pool_layout (+ udmabuf/pitch256 trace,
  legacy copy dst_stride = cpu_state.stride). tooltip.rs uses the helper (CString import dropped).

## COORDINATOR ASKS (2026-10-03, mid-task) - in force
1. stride padded to a multiple of 256 bytes, every slot page-aligned; EVERYTHING that indexes rows uses the
   padded stride (renderer draws into the slot! AzulPixmap::from_external is tight -> needs a row pitch:
   layout/src/cpurender/pixmap.rs AzulPixmap + every `row * width * 4` site, or render into a pitch-aware view).
2. drop the spare slot when idle (~1 s without frames: destroy its wl_buffer, punch the memory out
   (fallocate PUNCH_HOLE / madvise REMOVE) or one memfd per slot) and re-create it on demand when the next frame
   finds the remaining slot still held -> an idle window holds ONE buffer. Test-first (pure state machine).
3. note the ARGB8888-only swizzle path's extra pass in the report.
4. X11 in the report: CPU path = plain XPutImage (pixels through the socket + server copy) -> MIT-SHM plan
   (XShmPutImage, shared segment, fallback when the extension is missing / remote display), bytes per frame
   before/after. Implementing MIT-SHM optional (X11 files are mine for this).

## IN PROGRESS
- screencopy.rs memfd() -> shm::create_shm_file (dedupe, last twin)

## NEXT (exact order)
- a) screencopy dedupe commit
- b) report skeleton scripts/WAYLAND8_2026_10_03.md (research + per-frame analysis) - commit
- c) ask 1: design the pitch. Read layout/src/cpurender/pixmap.rs AzulPixmap; decide: add `stride` to
     AzulPixmap (from_external_with_stride) vs. one-memfd-per-slot only. RED test of pool_layout pitch 256.
- d) ask 2: spare-slot release state machine (pure fn + tests in shm.rs), wire into CpuFallbackState
- e) ask 4: read dll/src/desktop/shell2/linux/x11 CPU present; report section; maybe MIT-SHM
- f) finish report, commit

## Decisions
- (none yet)

## Open questions
- (none yet)
