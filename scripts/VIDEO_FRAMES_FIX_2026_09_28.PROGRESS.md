# VIDEO_FRAMES_FIX progress (checkpoint, delete in the last commit)

Branch `wt/video-frames` from `5414bfa6b` (PR #476). No compilation allowed.
Final report goes to `scripts/VIDEO_FRAMES_FIX_2026_09_28.md`.

## DONE

- `ec9aa540e` RED: two headless CPU tests in `dll/src/desktop/shell2/headless/mod.rs`
  (`a_virtual_view_scrolled_into_sight_repaints_where_it_is_shown`,
  `a_video_scrolled_into_sight_shows_its_later_frames_too`).
- `e30aedacb` FIX: `cpurender::compute_virtual_view_damage` takes the paint
  scroll offsets and damages each VirtualView at its painted position; the
  shell's `render_frame` and the e2e twin (`layout/src/e2e/cpu_backend.rs`)
  pass `next_scroll_baseline`; new unit test
  `a_virtual_view_inside_a_scrolled_frame_is_damaged_where_it_is_painted`.

## Root cause (found)

Frames reach the widget (`video_writeback` stores them, re-renders the
VirtualView in place). The CPU backend (the desktop default) damaged a view
that re-rendered in place at its CONTENT box; the Video card sits far down
AzWidgets' scrolled page, so every frame repainted off-screen. The poster
showed only because scrolling the card into sight repaints it.

Verified by reading (all fine): worker transport (Resume via
`merge_video_state` -> `seek_sender`), `VideoPlayback`, thread drain
(`run_all_threads`), macOS thread poll timer, `UpdateAllVirtualViews` ->
queue -> `drain_virtual_view_updates`, `repoint_orphaned_refanys`,
`ImageRef` identity (unique `id`, so image items differ per frame).

## IN PROGRESS

- Nothing half-done.

## NEXT (in order)

1. Write `scripts/VIDEO_FRAMES_FIX_2026_09_28.md` (root cause, commits +
   expected REDs, API changes = none, least-sure spots, history, open items).
2. Delete this file in the same commit.

## Open questions / items for the report

- `video_writeback` calls `trigger_all_virtual_view_rerender`: every frame
  re-invokes EVERY VirtualView of the window (perf, not correctness).
- Worker: `paused: config.paused || !config.autoplay` - a worker spawned
  for a widget with `autoplay: false, paused: false` still holds the poster.
- GL/WebRender path not affected (WR scrolls nested pipelines itself).

## History findings

- Linux Vulkan Video decode: `1d0dbc5cf` (2026-06-17) "real H.264 Vulkan
  Video decode (vendored+patched gpu-video)", verified on a GTX 960: 300/300
  BBB frames. Streaming widget `77d18a88e` (2026-06-17), then
  `57d53217d`/`52eeda29b`/`aa9052a5c`. gpu-video fix = smelter PR #2109
  (`2dd2cdf7b`, `271718a7b` -> crate `gpu-video-azul` 0.4.0). All on master
  and on this branch; nothing was lost - the June demo sized the widget
  in an unscrolled page (no overflow), which is why it played.
- GPU driver checker: provisioning API `6d42301d3`, `8440caea2`,
  `25bfc0891`, `468917044`, `badd911b4`, `668c83c6f`, `55c0c641b` (June
  11-16, issue #21); dialog `f321131d7` (2026-08-18)
  `SysDialogType::GpuCheck` (`layout/src/dialogs/gpu_check.rs`). On master
  and this branch. Duplicates `a71cf0c06` only on `remotes/pr/421`,
  `30d378adb` on no branch.
