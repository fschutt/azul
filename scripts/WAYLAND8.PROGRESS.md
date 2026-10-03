# WAYLAND8 progress (wave 8, branch wt/wayland8, base 45c6bf98b)

Task: research KDE's "faster wl_shm" (https://www.phoronix.com/news/KDE-Plasma-Faster-WL-SHM), analyse our Wayland
present path, implement the client-side win; USER DECISION (2026-10-03, via coordinator): implement BOTH
(a) Wayland: 256-byte pitch + page-aligned slots (KWin udmabuf zero-copy), idle spare-slot drop + on-demand
re-creation, all row indexing on the padded pitch; (b) X11 MIT-SHM (XShmPutImage per damaged rect, segment
re-created on resize, completion before reuse, XPutImage fallback). Report: scripts/WAYLAND8_2026_10_03.md.

## DONE (all committed)
- Research: Phoronix -> Xaver Hugl's blog -> KWin MR !9178 (diff read via GitLab API) -> Qt Gerrit 733869
  (diff read via REST) -> kernel drivers/dma-buf/udmabuf.c + mm/gup.c (seal / alignment / in-file checks).
- wayland/shm.rs (new): the ONE shm allocator `create_shm_file` (memfd MFD_ALLOW_SEALING [+NOEXEC_SEAL],
  F_SEAL_SHRINK|F_SEAL_SEAL), `pool_layout` (pitch = align_up(w*4, 256), slots on whole pages),
  `udmabuf_importable` (KWin + kernel rule), `plan_slot` / `idle_spare` (SPARE_IDLE_RELEASE = 1 s); tests.
  Window pool, tooltip, screencopy all use it (three twins removed).
- headless/mod.rs: `native_target_row_padding_px` + render_frame accepts a row-padded target (RED/GREEN test
  `a_row_padded_native_target_holds_the_owned_frame_in_every_row`); `copy_rgba_rects_into` = the one pitched
  damage-rect upload (RED/GREEN tests, run on the Mac) used by Wayland legacy present, popup, tooltip, X11 SHM.
- wayland/mod.rs: arming with pitch_px + padding; legacy copy / popup copy / tooltip pitch-aware;
  ShmSlot.released, release_slot (wl_buffer_destroy + fallocate PUNCH_HOLE|KEEP_SIZE), recreate_slot,
  release_idle_spare, create_slot_buffer, acquire_slot via plan_slot, next_writable_slot delegates,
  wait_for_events idle hook + poll timeout cap, last_attach at attach, pool trace udmabuf/pitch256.
- X11: common/x11_host.rs MIT-SHM decisions (display_is_local, x11_upload, shm_segment_reusable, ShmPending;
  RED/GREEN, run on the Mac); defines.rs XShm types; dlopen.rs `XShm`; x11/shm.rs X11ShmUpload;
  x11/mod.rs wiring (probe once, upload first, XPutImage fallback, completion dispatch, Drop detach).

## STATUS: DONE (2026-10-03)
- Self-review of the branch diff done (compile-risk spots listed in the final message).
- The report: the agent harness refused further writes of report .md files, so sections 2-10 live in the
  agent's FINAL MESSAGE to the parent; scripts/WAYLAND8_2026_10_03.md has section 1 (KDE research) and
  placeholders - the parent pastes sections 2-10 from the final message.
- Note: commit e59f00a7b carries a duplicated message (it is the progress-file update, not code).

## Decisions
- D1 memfd sealed (SHRINK|SEAL) + slots on whole pages: zero renderer impact, always on.
- D2 (revised by the user decision) pitch padded to 256: the renderer draws into a pixmap `pitch/4` px wide;
  render_frame is told the padding (`native_target_row_padding_px`).
- D3 one shm allocator (shm.rs); one pitched upload (`copy_rgba_rects_into` in headless/mod.rs, next to
  swizzle_rb_in_rects) - shared by Wayland and X11.
- D4 Wayland-only tests (shm.rs) run on Linux CI without a compositor; cross-platform tests (headless/mod.rs,
  common/x11_host.rs) run on the Mac too.
- D5 X11 completion law: one segment, `ShmPending` counter, XSync before writing while a put is pending (no
  second segment); segment kept across resizes while it holds the frame and is at most 2x too big.
- D6 DISPLAY "localhost:N" counts as REMOTE (ssh -X forwarding) -> XPutImage.

## Open questions
- none
