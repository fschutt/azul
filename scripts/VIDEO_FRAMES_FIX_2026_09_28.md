# Video frames after the poster - 2026-09-28

Branch `wt/video-frames`, based on `5414bfa6b` (PR #476,
`fix/input-bugs-2026-09-19`). Nothing here was compiled or type-checked;
the parent compiles once per wave.

## The bug

User test on macOS AzWidgets: the Video card fetched Big Buck Bunny and showed
its first frame (the poster). After a press on play no later frame appeared.

## Root cause

The frames were fine all the way to the widget. What never came was the
**repaint**, and it was the CPU renderer's damage tracking.

The Video widget's body is a `VirtualView`. Each decoded frame reaches the UI
thread as a writeback (`video_writeback`). The writeback stores the frame as
the widget's `current_frame` and re-renders the view **in place**
(`trigger_all_virtual_view_rerender`). An in-place re-render changes only the
view's **child** display list; the parent list stays byte-identical. The one
source of damage for it is therefore `cpurender::compute_virtual_view_damage`.

That function took the view's box straight from the parent's `VirtualView`
item, which is a **content-space** rect. The rasteriser paints the view at
that box minus the accumulated offset of the scroll frames around it. Inside a
scrolled box, every re-render was damaged a scroll offset below where the view
is shown.

- The Video card is the fifth section of AzWidgets' scrolled page. With the
  card in sight, the damage for every frame landed hundreds of pixels further
  down, mostly off-screen, and the card never repainted.
- The poster showed only because **scrolling** the card into sight repaints it
  (scroll strips are projected correctly), and the poster was already decoded
  by then.
- The CPU backend is the desktop default (`AzBackend::Cpu`), and macOS, X11,
  Wayland, Windows and headless all share `headless::CpuBackend::render_frame`.
  So the same bug froze **any** in-place `VirtualView` content (map tiles, a
  document) in a scrolled box on every CPU platform.
- The parent-list diff has had this projection since
  `damage_change_inside_scrolled_frame_repaints_at_viewport_position`. The
  child-list damage never got it.
- The June 17 Linux demo (`examples/azul-video`, deleted on 2026-09-09) played
  because its video sat in an unscrolled body.

### What was checked along the path and is correct

These were read end to end; none of them stops a frame:

1. **Transport.** "Play" flips `VideoConfig::paused`. The rebuild runs
   `merge_video_state`, which sends `VideoTransport::Resume` through
   `seek_sender`, on both the full reconcile (`transfer_states`) and the
   pre-cascade skip (`merge_fresh_dataset`). The worker's `next_control`
   decodes it. `VideoPlayback::resume` starts the clock from the poster (unit
   tests in `layout/src/widgets/video.rs`).
2. **The worker keeps decoding** while paused. It only holds the
   presentation, and paces itself at 30 ms per loop once everything is
   decoded.
3. **Delivery.** `run_all_threads` drains every queued writeback per poll. The
   macOS thread poll timer (`tickTimers:` then `process_timers_and_threads`)
   and `drawRect` both poll. `UpdateAllVirtualViews` queues every view, and
   `drain_virtual_view_updates` re-invokes it before the paint.
4. **Dataset identity.** The worker writes into the allocation the view
   renders from. `merge_video_state` returns the old allocation, and
   `repoint_orphaned_refanys` re-points the fresh `VirtualView` refany at it.
5. **Image identity.** Every frame is a new `ImageRef` with a never-reused
   `id`. `is_visually_equal` compares that id, so the child-list diff sees
   every frame as a change. The WebRender path uploads new images from every
   `layout_results` display list.

## Commits

| # | Commit | Kind | Expected RED |
|---|--------|------|--------------|
| 1 | `ec9aa540e` test(video): a view scrolled into sight repaints where it is shown, and a video shows more than its first frame | RED | see below |
| 2 | `e30aedacb` fix(cpurender): a VirtualView in a scrolled box is damaged where it is painted | fix | - |
| 3 | `0dcdccd48` progress checkpoint (removed again by the report commit) | chore | - |
| 4 | this report | docs | - |

Run `cargo test -p azul-dll --lib headless::tests::a_` (the headless tests
need `cpurender`, which `azul-dll` has by default). Both tests put the view in
a 200x100 box that scrolls a 600px column. The column is scrolled by 300, so
the view's box (content y 308..368) is on screen at y 8..68.

**Commit 1** fails these two tests:

- `a_virtual_view_scrolled_into_sight_repaints_where_it_is_shown`. A view
  paints one colour. The colour changes in place, the view is queued for a
  re-render, the queue is drained, and one frame is painted with no relayout.
  - Assert: `sample_px(50, 30) == Some([30, 220, 30, 255])`.
  - Today: `Some([220, 30, 30, 255])`, the old colour.
- `a_video_scrolled_into_sight_shows_its_later_frames_too`. A real
  `VideoWidget` replays four solid-colour frames through `with_frames`: a real
  worker thread, the same `video_writeback`. The test runs the frame loop
  (`process_timers_and_threads`, drain, paint) for up to 5 s.
  - Assert: at least 3 of the 4 frame colours are seen at (50, 30).
  - Today: 0 (the grey no-signal tile stays).

**Commit 2** makes both pass and adds the unit test
`cpurender::…::a_virtual_view_inside_a_scrolled_frame_is_damaged_where_it_is_painted`
(`cargo test -p azul-layout --lib a_virtual_view_inside`). That test also pins
that a view after the scroll frame keeps its own position.

## The fix

- `compute_virtual_view_damage(parent, current, previous, scroll_offsets)`:
  - It gains a `&ScrollOffsetMap` argument.
  - It walks `PushScrollFrame`/`PopScrollFrame` exactly like the parent diff.
  - It damages each view at its **painted** box, on both paths (precise and
    whole-view) and in the clip.
- `headless::CpuBackend::render_frame` (every CPU platform) and its e2e twin
  `layout/src/e2e/cpu_backend.rs` compute the view damage after
  `next_scroll_baseline` and pass that map. Incremental frames rasterise at the
  baseline, so the damage lands where the pixels go. The full-repaint path
  ignores the rects, as before.
- The five existing `compute_virtual_view_damage` unit tests pass an empty
  (unscrolled) map. Their expectations are unchanged.

## API changes

None for `api.json`. `azul_layout::cpurender::compute_virtual_view_damage` is a
Rust-only `pub fn` with a **new fourth parameter**. It is not in api.json.
Every caller is in this diff (the dll shell, the e2e twin and the unit tests).

## Least sure to compile

1. The headless test helper `paint_frame` borrows
   `window.common.layout_window` and `window.common.renderer_resources`
   immutably while it borrows `window.cpu_backend` mutably. These are disjoint
   fields, the same shape `regenerate_layout_inner` uses.
2. `layout_cache.scroll_id_to_node_id.values().copied().min()` in
   `scroll_outermost_frame_to`. `NodeId` is `Ord`.
3. `palette_index`: `a.abs_diff(*b)` on `&u8` (auto-deref).
4. The harness line `assert_eq!(…, Some(SWATCH_RED), "harness: …")` after the
   300px scroll relies on the scroll-shift path repainting the whole clip for
   a delta larger than the clip. If it fails, it fails as a **harness**
   message, not as the bug.
5. `rustfmt` was not run. All added lines are under 100 columns, but the
   parent's format pass may still rewrap a chain or two.

## History (the next stage)

Nothing of the Linux GPU video work is lost from the engine. All of it is on
`master` and on this branch.

- **Vulkan Video decode (Linux/Windows x86_64)**:
  - `1d0dbc5cf` (2026-06-17) "real H.264 Vulkan Video decode (vendored+patched
    gpu-video)". Verified on a GTX 960 with driver 580: 300/300 BBB frames.
    The code is in `dll/src/desktop/extra/video_codec/decode_vulkan.rs`, gated
    by `cfg(az_gpu_video)` (`dll/build.rs`: `video-native` and x86_64
    glibc-Linux or Windows).
  - The streaming widget is `77d18a88e` (2026-06-17), then resize
    `57d53217d`, seek `52eeda29b` and source change `aa9052a5c`.
  - `38d4749ca` is the C player (`examples/c/video.c`).
  - The gpu-video Maxwell queue fix is smelter PR #2109: `2dd2cdf7b`
    (2026-07-17), then `271718a7b` switched to the published crate
    `gpu-video-azul` 0.4.0 (`dll/Cargo.toml:266`).
- **What was "overwritten"**: `63efaa2e8` (2026-09-09, "fix(temp): rework
  docs, remove useless scripts / docs") deleted three things:
  - the **AzVideo demo** `examples/azul-video/` (the full-window BBB player
    with a scrubbing timeline, last version at `63efaa2e8^`);
  - `doc/video-hw-decode-apple-windows-handoff.md`;
  - the camera/screenshare/gamepad/self-test demos.

  Restore any of them with `git show 63efaa2e8^:<path>`.
- **GPU driver checker**:
  - The provisioning API is `VideoStartupCheck::run` / `remediate` /
    `remediate_with_progress` and `ProvisionPlan`, in
    `dll/src/desktop/extra/video_codec/provision.rs` (issue #21). Its commits:
    - `6d42301d3` probe + planner (06-11);
    - `8440caea2` example `dll/examples/video_codec_provision.rs` (06-11);
    - `25bfc0891` reboot signal (06-11);
    - `468917044` apt progress (06-12);
    - `badd911b4` reboot-safety gate + kernel autofix (06-12);
    - `668c83c6f` DLL surface (06-12);
    - `55c0c641b` NVIDIA can't-boot-black hardening (06-16).
  - The **popup dialog** is `f321131d7` (2026-08-18)
    "SysDialogType::GpuCheck - a system dialog over the EXISTING provisioning
    API", in `layout/src/dialogs/gpu_check.rs`. It shows the GL
    vendor/renderer, runs `VideoStartupCheck::run()` on a worker, and remediates
    only on a button press, with a progress bar.
  - Stray duplicates of the provisioning commits: `a71cf0c06` is only on
    `remotes/pr/421`; `30d378adb`, `9b9761ee4` and `823359291` are on no
    branch.

## Open items

- **Every frame re-renders every view.** `video_writeback` calls
  `trigger_all_virtual_view_rerender`, so each frame re-invokes every
  `VirtualView` in the window (other widgets' views, icon views). This costs
  performance, not correctness. It should re-render only its own view: the
  writeback knows no node id today, so the widget would record its view's
  `DomNodeId` on mount or resize.
- **A fresh worker still holds the poster.** The worker holds when
  `config.paused || !config.autoplay`. A worker spawned for a widget that is
  already `paused: false` (for example after the node failed to reconcile and
  re-mounted) holds the poster until `paused` flips again.
- **Nothing drives the real decoder headless.** No test runs the streaming
  worker, because the repo has no tiny H.264 MP4. The transport has unit tests
  (`VideoPlayback`); the new test drives the replay worker through the same
  writeback.
- **The GPU backend is unaffected.** WebRender scrolls nested pipelines
  itself. The fix was not checked against `AZ_BACKEND=gpu` on hardware.
- **Manual check.** Use the recipe in `DEMO_VIDEO_PLAYER_2026_09_28.md`.
  After play, the bunny should move with the card anywhere on the scrolled
  page. `AZ_BLIT_DEBUG` / `AZ_PATCH_DEBUG` print the damage rects.
