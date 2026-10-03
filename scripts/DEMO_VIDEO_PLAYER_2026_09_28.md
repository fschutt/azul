# AzWidgets video player (Big Buck Bunny in `<video>`) - 2026-09-28

Branch `wt/demo-video-player`, based on `7e020dc49` (PR #476,
`fix/input-bugs-2026-09-19`). Nothing here was compiled or type-checked;
the parent compiles once at the end. Read "Least sure to compile" first.

## What was built

A **Video** card in AzWidgets (after "Display"), in its own module
`examples/azul-widgets/src/video.rs`. The hooks in `lib.rs` are `mod video;`,
one `Showcase` field (`video: RefAny`), one `let video_card = video::card(..)`
line, one `.with_child(video_card)` and one `video: video::new_state()` line.

- It fetches Big Buck Bunny, the same URL as `examples/c/video.c`:
  `https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_2MB.mp4`.
  It shows the first frame as a poster with a round play button centred on
  it. The config is `autoplay: false, paused: true`, so nothing autoplays.
- Play and pause work from the button under the video and from a click
  anywhere on the video. The overlay over the video is itself a `<button>`.
  While the video plays the overlay hides; when paused it shows the play
  button again, and at the end a replay button.
- A time display (`m:ss / m:ss`, `-:--` until the length is known) and a
  seek bar. A click on the bar seeks to that point. Left and Right on the
  focused bar step one second.
- Loading shows a note ("Loading Big Buck Bunny…"). A failure (network, no
  decoder in the build, a bad file, a decoder that yields nothing) shows a
  note with the widget's own message plus "Click to try again". Pressing
  play after a failure retries.
- Accessibility: the overlay and the small toggle are `NodeType::Button`s
  with accessible names ("Play video" / "Pause video" / "Play video again" /
  "Try to play the video again", and "Play" / "Pause"). Space and Enter work
  through the engine's activation behaviour (Enter or Space on a focused
  button sends a synthetic `Click`). The seek bar has `TabIndex::Auto` and
  the accessible name "Seek".
- Every colour is a `system:` colour: `control-background`, `accent`,
  `accent-text`, `window-background`, `text`, `secondary-text`,
  `button-face`, `button-text`, `separator`. The stage is 480x270, the
  clip's 16:9, so the decoder scales frames without stretching them.

The widget library did not have what the card needs, so it was extended
with RED tests first:

1. **Poster, pause and resume, seek, end, status**
   (`layout/src/widgets/video.rs`, `core/src/video.rs`).
   - `VideoConfig::paused` is a live transport flag. When it flips across a
     relayout, `merge_video_state` sends `VideoTransport::{Pause, Resume}` to
     the worker, next to the existing seek (`f32`) and source messages.
   - `VideoWidget::with_on_status` hooks every `VideoStatus` the decoder
     reports (`phase`, `position_s`, `duration_s`, `message`), through
     `video_status_writeback`.
   - `VideoPlayback` is the worker's transport with the time handed in, so
     it can be tested without a decoder:
     - frames are kept in presentation order;
     - the clock starts at the first frame on screen;
     - the poster is held until the app plays;
     - a video that outruns the decoder waits for it;
     - a video that does not loop ends on its last frame; with `looping` it
       wraps;
     - status reports are throttled to one per 250 ms of playback, the
       media-player manager's `TIME_UPDATE_INTERVAL_S`.
2. **The streaming worker** (`dll/src/desktop/extra/video_codec/stream.rs`).
   - **macOS was compiled out.** The decode was gated to x86_64
     Linux/Windows, although `VideoDecoder` already has a `VideoToolbox`
     engine on Apple. Before this change, the `<video>` widget on the
     user's Mac showed its grey no-signal tile forever. The gate is now
     `feature = "video-native"`; the engine is chosen inside `VideoDecoder`.
   - It honours `autoplay`, `paused`, `looping` and the start `timestamp`
     through `VideoPlayback`. Before, it ignored all four and looped
     forever.
   - Failures report `VideoStatus::failed(message)` instead of returning
     silently. After a failure the worker waits for Resume (a retry), a new
     source, or `TerminateThread`.
3. **B-frames** (`demux.rs`). `H264Chunk::pts_ms` was the DECODE time: the
   demuxer dropped the `ctts` composition offset. The BBB clip has a
   194-entry `ctts` table, checked by downloading it and walking its boxes.
   `VideoToolbox` hands frames back in decode order, one per access unit.
   The worker now takes each frame's presentation time from the access unit
   it came from on Apple (`DECODER_EMITS_DECODE_ORDER`). On gpu-video, which
   reorders to display order itself, it uses the sorted times.

## Commits

| # | Commit | Kind | Expected RED |
|---|--------|------|--------------|
| 1 | `8662fbf31` test(video): a video holds its poster, pauses on request and says what it is doing | RED | See the list below |
| 2 | `bb1b7b02c` feat(video): the widget holds a poster, pauses and resumes, and reports its status | fix | - |
| 3 | `86cadf9e1` test(video): the streaming decoder says why a video cannot play, and shows B-frames in order | RED | See the list below |
| 4 | `cf336102b` fix(video): the streaming decoder plays on macOS, holds a poster, pauses, and says why it fails | fix | - |
| 5 | `3c8d4a8f3` feat(examples): a Video card in AzWidgets plays Big Buck Bunny in the `<video>` widget | demo | - |
| 6 | `d63161964` test(examples): the theme check reads the Video card's styles too | test | Passes. It extends the check to `video.rs`. |
| 7 | this report | docs | - |

**Commit 1** adds the API with the behaviour stubbed. Run
`cargo test -p azul-layout --lib widgets::video`; these tests fail:

- `merge_pauses_and_resumes_the_worker_when_paused_flips`
- `merge_adopts_the_fresh_status_hook_and_keeps_the_reported_status`
- `a_reported_status_is_stored_and_handed_to_the_hook`
- `a_status_without_a_hook_is_still_stored`
- `a_video_that_does_not_autoplay_holds_its_first_frame_as_a_poster`
- `the_clock_starts_with_the_first_frame_not_with_the_download`
- `resume_plays_from_the_poster_and_pause_freezes_the_position`
- `pausing_before_the_first_frame_keeps_the_video_held_when_it_arrives`
- `frames_are_shown_in_presentation_order_whatever_order_they_decode_in`
- `a_frame_decoded_after_the_one_on_screen_does_not_move_the_picture`
- `a_video_that_ends_holds_its_last_frame_and_says_so_once`
- `playing_after_the_end_starts_over`
- `a_looping_video_wraps_to_the_start`
- `a_seek_moves_the_picture_and_is_reported_at_once`
- `a_seek_is_clamped_into_the_video`
- `position_reports_come_about_four_times_a_second`
- `playback_that_outruns_the_decoder_waits_for_it`

These already pass at commit 1 (data and builder pins):

- `merge_stays_quiet_while_paused_is_unchanged`
- `a_fresh_widget_is_loading_and_carries_its_status_hook`
- `a_status_payload_of_the_wrong_type_is_ignored`
- core `video_status_contract`

**Commit 3**, two RED tests:

- `cargo test -p azul-dll --lib stream_tests` fails with `panicked: no Failed
  status, the worker reported only []`. That holds on every target and
  feature set.
- `cargo test -p azul-dll --features video-native --lib demux_tests` fails
  `a_presentation_time_includes_the_composition_offset` with `left: 0.0,
  right: 66.66666666666667`.

## Public API changes for `azul-doc autofix`

`api.json` was not edited. Everything below must be synced **before**
codegen. `VideoConfig` and `VideoWidget` are passed by value across the
C ABI, so a stale `AzVideoConfig` or `AzVideoWidget` corrupts memory.

**`azul_core::video`** (module `video`):
- `VideoConfig`: a **new field `paused: bool`**, between `looping` and
  `output_format`. `Default` sets it to `false`.
- **New** `#[repr(C)] enum VideoPhase { Loading, Paused, Playing, Ended, Failed }`.
  - derive: `Debug, Default, Copy, Clone, PartialEq, Eq, Hash`.
  - `#[default] Loading`.
- **New** `#[repr(C)] struct VideoStatus { message: AzString, position_s: f32, duration_s: f32, phase: VideoPhase }`.
  - derive: `Debug, Clone, PartialEq`.
  - custom `Default`, which is `loading()`.
- **New** fns:
  - `VideoStatus::loading() -> VideoStatus` (const)
  - `VideoStatus::new(phase: VideoPhase, position_s: f32, duration_s: f32) -> VideoStatus` (const)
  - `VideoStatus::failed(message: AzString) -> VideoStatus` (const)
  - `VideoStatus::progress(&self) -> f32`

**`azul_layout::widgets::video`** (module `widgets`; the callback family
lands wherever autofix puts `VideoMount*` / `OnVideoFrame*`):
- `VideoWidget`: a **new field `on_status: OptionOnVideoStatus`**, last.
- **New** fns:
  - `VideoWidget::set_on_status(&mut self, data: RefAny, callback: OnVideoStatusCallback)`
  - `VideoWidget::with_on_status(self, data: RefAny, callback: OnVideoStatusCallback) -> VideoWidget`

  The Rust source has `C: Into<OnVideoStatusCallback>`, like
  `with_on_frame`.
- **New callback family** from `impl_widget_callback!` +
  `impl_managed_callback!`, the same shape as `VideoMount`:
  - `OnVideoStatusCallbackType = extern "C" fn(RefAny, CallbackInfo, VideoStatus) -> Update`
  - `OnVideoStatusCallback { cb, ctx }`. api.json spells `ctx` as
    `callable`, like every widget callback.
  - `OnVideoStatus { refany, callback }`. api.json spells `refany` as
    `data`, like `VideoMount`.
  - `OptionOnVideoStatus`
  - C exports `AzApp_setOnVideoStatusCallbackInvoker` and
    `AzOnVideoStatusCallback_createFromHostHandle`, plus the invoker type
    `AzOnVideoStatusCallbackInvoker`.
- **Keep** `VideoWidget.dom`'s `fn_body` as
  `crate::unified::video_codec::video_widget_dom(video_widget)`. The
  `preflight_contracts.py` widget-wiring check fails otherwise.
- New `pub` items that are **not** FFI (not reachable from the API, and must
  not go into api.json):
  - `STATUS_INTERVAL_S`
  - `VideoTransport`
  - `VideoPlayback`
  - `VideoTick`
  - `video_status_writeback`
  - the fields `VideoWidgetState::{on_status, status}`

**`azul-dll`**: no public API change. The new items in `stream.rs` and
`demux.rs` are private (`send_status`, `Control`, `next_control`,
`Requested`, `wait_for_retry`, `load_stream`, `presentation_ms`,
`DECODER_EMITS_DECODE_ORDER`). The `video_decode_worker` signature is
unchanged.

## Build features the demo needs

AzWidgets links `link-dynamic` against a `libazul` built with
`--features build-dll`. That already contains everything:

| Feature | What it gives the card |
|---------|------------------------|
| `video-native` | The `mp4` demuxer, the streaming worker, and gpu-video on x86_64 Linux/Windows (glibc). |
| `libloading` | Through `_internal_deps`: the dlopen'd `VideoToolbox` engine on macOS and iOS. |
| `http` | The download (`HttpRequestConfig::download_bytes_blocking`, Range request). |
| `icons` | The Material glyphs `play_arrow`, `pause`, `replay`. |

The feature sets were not changed. What was missing on macOS was the worker's
`#[cfg]`, which is fixed in code. Other builds degrade to a `Failed` note:

- `link-static` builds (Android, iOS) have no `video-native`, and say so.
- Linux aarch64 or musl has `video-native` but no decode engine, and
  `decode_engine_missing_reason()` says so.

## Manual check (macOS)

The recipe is the one in `MACOS_POLISH_STATUS_2026_09_26.md`:

1. Run `azul-doc autofix` for the API changes above, then
   `target/release/azul-doc codegen all`.
2. Build and install the dylib:
   ```
   cargo build --release -j 6 -p azul-dll --features build-dll
   ```
   Copy the dylib into `target/azul-lib`.
3. Build and run AzWidgets:
   ```
   AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzWidgets
   DYLD_LIBRARY_PATH=$PWD/target/azul-lib ./target/release/AzWidgets
   ```
   Add `AZ_VIDEO_FRAMELOG=1` for the worker's log: fetch, demux, and every
   status as `[vstream] Paused 0.00/10.00s decoded=…`.
4. Scroll to the **Video** card. It should show:
   - "Loading Big Buck Bunny…" briefly, then the bunny's first frame under a
     round accent-coloured play button;
   - `0:00 / 0:10` under it;
   - nothing playing.
5. Click the play button. It should:
   - hide the overlay and play the bunny smoothly, with no back-and-forth
     jitter (that jitter was the B-frame order bug);
   - move the time and the bar about four times a second;
   - make the small button show "pause".
6. Click anywhere on the video. It pauses, and the play button comes back.
   Click the small button: it plays again.
7. Click the middle of the seek bar. The video jumps to about 0:05. Tab to
   the bar and press Left or Right: it steps one second.
8. Let it end. It holds the last frame and shows the replay button. Press
   play: it starts over.
9. Tab to the overlay or the small button and press Space, then Enter. Each
   toggles playback. VoiceOver reads "Play video" or "Pause video".
10. Switch the system appearance to dark and back. The card follows it: no
    fixed colours.
11. Error path: turn Wi-Fi off and restart. The note reads "Could not play
    the video / Could not download the video: … Click to try again." Turn
    Wi-Fi on and click: it loads.

## Least sure to compile

1. **Generated-API names in the demo** (`video.rs`). They depend on where
   autofix places the new types:
   - `azul::dom::OnVideoStatusCallback`. This is a guess, based on
     `VideoMountCallback` and `OnVideoFrameCallback` both being in `dom`.
   - `azul::video::{VideoPhase, VideoStatus}`: path `azul_core::video::`
     maps to `video`.
   - the struct literal `OnVideoStatusCallback { cb, callable }`;
   - `with_on_status` taking the struct and not the fn pointer;
   - `AzVideoStatus: Clone`, and its field names.

   Adjust the imports if autofix chooses otherwise.
2. **Clippy extreme lints** (`-D warnings` on core and layout). I made these
   `const fn` because they are const-eligible on 1.91:
   - `VideoPlayback::new`, `set_duration`, `duration`, `finish`,
     `invalidate`, `position`, `rebase`, `clamp_to_end`;
   - `sanitize_position` (uses `f32::is_finite`, const since 1.83);
   - `VideoStatus::loading`, `new`, `failed`.

   `VideoStatus::progress` and the fns that drop an `Option<VideoStatus>`
   stay non-const. `missing_const_for_fn` and `doc_markdown` are the likely
   complainers if I misjudged one.
3. `layout`: `impl_managed_callback!` with `extra_args: [ status: VideoStatus ]`.
   It is the same shape as `VideoSetup`, but a second exported
   `#[no_mangle]` pair.
4. `dll/stream.rs`: the `if let … else if let` chain over one `RefAny` in
   `next_control`. It copies the existing chain, which compiled, adding one
   branch for `VideoTransport`.
5. `dll/stream.rs`: `display_pts.sort_by(f32::total_cmp)`, and the
   `display_order_pts` closure borrowing `display_pts` across the decode
   loop.
6. `dll` test `stream_tests`: the hand-built `ThreadSender` and
   `ThreadReceiver` (`ThreadSenderInner { ptr: Box<Sender<..>>, … }`). They
   mirror the layout test harness, but from the dll crate.

## Open

- **RSS**: the worker keeps every decoded frame, as it did before. For BBB
  360p that is 640x360x4 bytes x 300 frames, about 276 MB, while the card
  is mounted. A sliding window that re-decodes from the previous keyframe on
  a seek would fix it. The clip has 2 keyframes, and `VideoToolbox` decodes
  360p at hundreds of fps.
- **The `VideoToolbox` ordering assumption.** The code assumes one frame per
  access unit, handed back synchronously (`VtDecoder::decode` says
  "typically before this returns"). If VT ever delays a frame, the times
  shift by one access unit. Watch for jitter in step 5 of the manual check.
  The batch pipeline (`pipeline.rs`, used by `examples/c/video.c`) still
  collects frames in decode order on macOS.
- **The widget draws no status of its own.** An app without `on_status`
  still sees only the grey no-signal tile on a failure. The test-pattern
  and replay workers never report a status (they stay `Loading`).
- **The seek bar is not a slider.** It is a plain track and fill with a
  name and a tab stop: no ARIA slider role and no value text. Dragging to
  scrub is not supported, only click and arrow keys. A seek is a *change*
  of `timestamp`, so a repeat seek to the same spot is nudged by 1 ms.
- **Rebuild frequency.** The card redraws the whole page (`RefreshDom`) on
  each status, about 4 per second while playing. That is fine for the
  demo. A large app would rather update the time and bar nodes in place.
- **The media-player manager is not connected.**
  `CallbackInfo::media_*`, `get_media_state` and the
  `Play`/`Pause`/`TimeUpdate` events are separate from the widget's
  status; bridging them is future work.
- Headless or E2E: an unmocked download under `AZ_E2E` fails with
  "unmocked http request under e2e". The card shows the failure note, with
  no panic. No E2E scenario drives the card yet.
