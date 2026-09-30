# VIDEO_REVIEW - the NV12 / video path of 2026-09-30, re-read for implementation bugs

Branch `wt/video-review` from `253266d13`. Scope: the 20 commits `6ae909529..253266d13`
(NV12 as a `RawImageFormat`, NV12 capture, NV12 in/out of VideoToolbox, the WebRender YUV
path, stable image keys, early downscaling, tile-only damage, AzMeet) plus the two bugs the
parent already found (RG8 in the shared texture cache; the `<video>` worker decoding the
whole clip). Nothing was compiled or run (house rules); every edited file parses
(`rustfmt --check`: no errors, no new hunks).

Every line number below is in this branch's tree (`253266d13` + the four commits).

## Findings, by severity

### Crash

None new. The RG8 shared-cache panic is the parent's (`webrender/core/src/texture_cache.rs`
`is_allowed_in_shared_cache`; the RED test `253266d13` is in this tree, the fix is not - the
parent has it uncommitted). Everything else an RG8 / NV12 plane flows through was checked:

- `wr_translate2::nv12_plane_descriptors` (1543): Y plane `R8`, `stride = w`, `offset 0`;
  chroma `RG8`, `size = (ceil(w/2), ceil(h/2))`, `stride = ceil(w/2) * 2`, `offset = w*h`.
  Odd sizes: the chroma row is `ceil(w/2)*2` bytes, which is exactly what `Nv12Layout`
  packs and what `CaptureSlot::publish_nv12` / `copy_decoded` copy per row. WebRender's
  length assert (`texture_cache.rs` 1663: `offset + w*bpp + (h-1)*stride <= len`) equals the
  packed total for both planes. An odd luma stride goes through WebRender's staging copy
  (`required_upload_size_and_stride` rounds the destination stride; `UNPACK_ALIGNMENT 1`).
- `update_image` with a changed size / format on the stable key: `texture_cache.rs` 981-982
  reallocates when `entry.size != descriptor.size || entry.input_format != descriptor.format`,
  so a tile that grows, or turns BGRA -> NV12 (its key goes BGRA8 -> R8), is fine. NV12 ->
  BGRA drops the chroma key AND resends the display lists (`upload_overlay_images` 1621:
  `drop_chroma` sets `new_slot`), so no `YuvImage` names a deleted key.
- The CPU rasterizer's NV12 rows (`cpurender/raster.rs` `source_row_to_rgba` ->
  `YuvCoefficients::row_to_rgba`, core 1741): `uv_row` indexed at `(x/2)*2 + 1 < ceil(w/2)*2`
  for every `x < w`, so the odd last column reads its own pair; the 1:1 path converts only
  `py_lo..py_hi`. `Nv12Layout` and `checked_total_len` saturate / check. `nv12_to_rgba`
  (1789) guards `width == 0` before `chunks_exact_mut`.
- The fixed-point table `YuvCoefficients::of`: every coefficient re-derived (Rec.601 /
  Rec.709, video 255/219 and 255/224, full range) - all eight rows match to the unit.
- `VideoFrame.format` defaults to `RGBA8` in `VideoFrame::new`; every capture / decoder
  constructor sets it; the api.json list in `VIDEO_PATH_2026_09_30.md` covers the bindings.
- Every `match` over `RawImageFormat` in the tree has NV12 arms or a wildcard (the core
  test files only hold format LISTS), so no exhaustiveness error is waiting for the parent.

### Leak

1. FIXED `ef2052247` - AzMeet kept a hardware decoder per rendition ever shown.
   `examples/azul-meet/src/lib.rs` `take_video` (2267): `received` only grew, and a
   `VideoIn` whose rendition this side stopped showing (the tile moved to another rung - the
   new 540 rung makes that common) kept its `VideoDecoder` (one `VTDecompressionSession`, its
   IOSurface output pool at the tile's size, one `VTDecoderXPCService` connection) for the
   rest of the call; when the sender stopped that stream no packet ever came to close it.
   `stop_culled` (2713, every plan change / 2 s) now closes the decoders of streams `shows`
   says this side no longer shows, next to the encoders it already closed. Bounded (rungs x
   tracks x peers) but each one is an XPC session.

2. FIXED `ef2052247` - a fresh AzMeet decoder was never told the tile's size.
   `decode_picture` (2349) compared `input.output_size` (what the PREVIOUS decoder was told)
   with `tile_px`; after every `input.decoder = None` (a restart, a stream shown again, an
   inert decoder) the new decoder handed frames out at the stream's size and the renderer
   scaled every frame - the early downscale was lost from the first restart on. Now
   `output_size` resets with the decoder.

3. Not a leak, verified: `DecShared.frames` (`videotoolbox.rs`) is drained inside every
   `decode()` (synchronous decode, flags 0, the callback runs inline) and `flush()`, so it
   holds at most one access unit's frames; `EncShared.chunks` the same after
   `CompleteFrames(pts)` (1139). `DecoderInner.ready` / `EncoderInner.packets` are drained by
   AzMeet after every call; the `<video>` worker (`stream.rs`) drains `next_frame` per chunk.
   Every session is invalidated + released: `VtEncoder::drop` (CompleteFrames(invalid) ->
   Invalidate -> CFRelease), `VtDecoder::drop` -> `reset_session` (session + format
   description), also on every SPS/PPS change and output change; the `refcon` Arcs outlive
   the sessions (fields drop after `Drop::drop`). The pooled '420v' source buffer is
   `CFRelease`d right after `EncodeFrame`; `VTCompressionSessionGetPixelBufferPool` is a
   borrowed reference. `CVBufferGetAttachment` is a Get (no release). Plane reads use
   `CVPixelBufferGetBaseAddressOfPlane` for '420v'/'420f' and `GetBaseAddress` for BGRA,
   always inside the lock.

4. NOT PINNED (pre-existing, engine): a capture / video widget REMOVED from the DOM keeps its
   worker. `layout/src/window.rs` `run_all_threads` (16437) retires a thread only when
   `is_finished()`; nothing terminates a worker whose widget node is gone (camera.rs /
   screencap.rs have no unmount hook; the loop ends only when `sender.send` fails, i.e. the
   `Thread` was dropped - at window close). A camera widget dropped by a `RefreshDom` keeps
   the camera on and `camera_writeback` running per frame (the marker lookup then finds no
   node). Root-cause fix (engine, not done here - no compile): after the DOM diff, terminate
   the threads whose `writeback_data` RefAny is held by no node's dataset any more (strong
   count == the thread's own), or give the capture widgets an unmount that sends
   `TerminateThread`. This is the most likely source of "67 threads" in AzWidgets (its page
   rebuilds on every status / theme change; each `<video>` rebuild is merged, but a camera /
   screen widget is not), together with iroh's tokio pool in AzMeet.

### Wrong picture

5. FIXED `7f662e86e` (RED `c844ccaee`) - `image_scale::cover_crop(.., even = true)` evened the
   crop's size unconditionally. A same-size NV12 cut of an odd-sized frame (a 641x361 screen
   capture in a 641x361 tile: SCK delivers the covering size, which is the tile's device
   size, which is often odd) became a 640x360 crop: every frame was RESAMPLED (a one-pixel
   upscale) instead of row-copied, and the last column and row of the picture were lost. Now
   only a side the crop cuts is evened; a side spanning the whole source keeps its odd
   extent, whose chroma pair the source's own layout holds (`resample_nv12` reads it through
   the same `ceil(w/2)` rule, so the 1:1 row-copy path is taken). Test:
   `a_same_size_cut_of_an_odd_sized_nv12_frame_is_a_copy_not_a_resample`.

6. WRITE-UP (low today, wrong once a 601 stream shows as NV12) - the YCbCr matrix of a
   decoded / captured buffer is detected by POINTER equality with the CoreVideo constant:
   `videotoolbox.rs` 1269 (`matrix == lib.kCVImageBufferYCbCrMatrix_ITU_R_601_4`) and
   `camera/avfoundation.rs` 96 (`core::ptr::eq(..)`). VideoToolbox sets the attachment from
   the format description's extension (a CoreMedia constant,
   `kCMFormatDescriptionYCbCrMatrix_ITU_R_601_4`, documented as "the same string" - not the
   same pointer), so a 601-tagged stream decoded as NV12 can read as Rec.709. Today every
   NV12 consumer is AzMeet, whose encoder tags Rec.709 (line 900), so nothing is visibly
   wrong. Fix: dlsym `CFEqual` into `VtLib` and compare with it
   (`(lib.CFEqual)(matrix, k601) != 0`); in avfoundation use `objc2_core_foundation::CFEqual`
   (or `NSString::isEqual` through the toll-free bridge).

7. WRITE-UP (wrong picture on non-709 / full-range sources) - the encoder is fixed to
   '420v' + Rec.709: `videotoolbox.rs` 828 creates the source pool as '420v' and 900-914 tag
   the session Rec.709 whatever the frames say. An `NV12*Full` frame ('420f', a camera or a
   screen share opened with a full-range config) is copied into a video-range buffer: the
   encoder reads 0..255 as 16..235 (crushed blacks / clipped whites at the receiver); a
   camera whose '420v' attachment says Rec.601 (the SD presets, many external cameras) is
   tagged 709 and decodes with a slightly wrong hue. AzMeet asks for `NV12Rec709Video` and
   FaceTime cameras deliver 709, so the call is right; the API is not. Fix: `VtEncoder::open`
   takes the frame format (pool '420f' for a full-range format, matrix tag 601 for a Rec.601
   one), or the pool is created lazily from the first frame's format.

8. WRITE-UP - `VtDecoder`'s pending output (`set_output_format` / `set_output_size` on a
   running stream) is applied only on an IDR NAL (1475/1484). A stream whose random-access
   points are non-IDR I-slices with recovery-point SEI (some encoders) never switches; a
   paused stream never does either (no new keyframe). AzMeet's encoder emits IDRs every 60
   frames, so today a tile resize takes effect within 2 s.

9. Checked, correct: `premultiplied_alpha: true` on BGRA8 / RGBA8 decoded and captured
   frames (`load_bgra8` 3387 / `load_rgba8` keep the bytes, alpha 255 => straight ==
   premultiplied); NV12 is loaded as-is and opaque (`load_nv12` 3112, exact length - every
   producer packs exactly: capture, VT `copy_decoded`, `resample_nv12`, Vulkan
   `planes.truncate(total)` at `decode_vulkan.rs` 156); JPEG frames keep the old path.
   Byte order: BGRA stays BGRA from capture to tile (`publish_packed`, `cut`, vImage without
   the output swizzle, `load_bgra8`), RGBA8 swizzles once in `load_rgba8`; VT `bgra_buffer`
   swizzles only RGBA8. AzMeet's tiles are 16:9 boxes (STAGE 568x320, THUMB 142x80), so
   `set_output_size(tile_px)` keeps the stream's aspect; `measure_tiles` (2895) uses the
   window's `dpi / 96` like `rendition_height` always did. `SCFrameStatus` values
   (complete 0, started 4) and `CMSampleBufferGetSampleAttachmentsArray`'s toll-free
   `NSArray` / `NSDictionary` reads are right.

### Perf

10. FIXED `224bf67d2` - both capture delegates locked the CoreVideo buffer READ-WRITE
    (`CVPixelBufferLockFlags(0)`) to read it: CoreVideo then invalidates the buffer's GPU /
    IOSurface caches on unlock, per camera and screen frame. Now `kCVPixelBufferLock_ReadOnly`
    (1). No test possible without a device; the change is the flag.

11. WRITE-UP, file off limits (`stream.rs`) - the `<video>` worker never applies
    `VideoConfig::output_format`. `stream.rs` 321 opens the decoder and leaves it at the
    RGBA8 default, so on macOS every frame is swizzled BGRA -> RGBA in `copy_decoded` and
    back RGBA -> BGRA in `load_rgba8`; the AzWidgets card's `output_format: BGRA8` is dead.
    One line after 321: `decoder.set_output_format(config.output_format);` (BGRA8 => zero
    swizzles; an NV12 config => the YUV shader). Not the reason the video does not show: the
    RGBA path is complete (VT BGRA out -> swizzle -> `video_writeback` -> `new_rawimage`
    RGBA8 -> the VirtualView `<img>` -> a new image key per frame, as before).

12. WRITE-UP - `capture_common::cut_frame` (953) moves the frame out of `buf`
    (`core::mem::take`) whenever the source frame travels (an `on_frame` hook, or no preview
    cut), so the next `take_newer` swaps an EMPTY Vec into the slot and the slot allocates a
    full frame again: with a hook set, the "no second copy" design costs an allocation per
    frame. Fix: hand the slot a Vec with the old capacity (`core::mem::replace(buf,
    Vec::with_capacity(buf.capacity()))`), or keep two buffers in the loop.

13. Checked: the hidden-tile gate (`node_is_visible_in_window` 5279, `apply_image_change`
    8534 -> `PaintHidden` -> `DoNothing`) and the tile coming back: the next lightweight or
    full transaction runs `upload_overlay_images` first (`build_image_only_transaction` 2788
    step 1.5; `register_frame_resources` before the image scan), which uploads the newest
    overlay frame of every node, so the scroll that brings the tile back shows the newest
    frame on both backends; the CPU backends paint from the patched list. The overlay image
    of a rebuilt DOM is dropped per DOM (`overlay.rs` 591), so a stale key is not re-added
    and GC'd in a loop. The 540 rung (`loadbalancer.rs`) and its bitrate are consistent
    with the ladder; the encoder clock (`started`, 949) gives real-time PTS and
    `CompleteFrames(pts)` makes `encode` synchronous as AzMeet's keyframe policy needs.
    macOS consumes `Paint` through `ShouldReRenderCurrentWindow` -> the lightweight
    transaction (`macos/mod.rs` 8636); only headless reads `content_repaint_pending`.

## What is left / for the parent

- Apply write-up 11 (one line in `stream.rs`) if wanted; 6 and 7 are API-level and belong
  to the next VideoToolbox pass; 4 is an engine change (thread termination on widget
  removal) worth its own RED headless test; 12 is a one-liner in `cut_frame`.
- The AzWidgets "video does not play" report: with the RG8 fix and the decode gate in
  place, nothing else in the codec / format path blocks a frame (the `<video>` path is
  RGBA8 end to end and never touches NV12). If it still shows nothing, the next suspects
  are the gate itself (`VideoPlayback::wants_frame` while `paused` - the card starts held)
  and the VirtualView re-render, both the parent's files.

## Commits

| hash | what |
|---|---|
| c844ccaee | test(image_scale): a same-size cut of an odd-sized NV12 frame is a copy, not a resample (RED) |
| 7f662e86e | fix(image_scale): an NV12 crop evens only the sides it cuts; the whole frame keeps its odd extent |
| ef2052247 | fix(azmeet): a rendition no longer shown closes its decoder; a fresh decoder is told the tile's size |
| 224bf67d2 | fix(capture): the camera and screen delegates lock the pixel buffer read-only |

## api.json

No public API change. (`cover_crop`, `stop_culled`, `decode_picture` and the lock flags are
Rust-internal.)

## Least sure to compile

1. `examples/azul-meet/src/lib.rs` `stop_culled`: the `flat_map` closure borrows `s`
   (shared) while `s.remotes.iter()` holds another shared borrow of `*s` - fine for a
   `&mut MeetState` reborrowed twice immutably, but if the borrow checker objects, collect
   `(remote index, key)` in a plain nested `for` over `s.remotes.iter().enumerate()` first.
   Item patterns: `.iter()` on `BTreeMap<(u32, u16), VideoIn>` yields `(&(u32, u16),
   &VideoIn)`; the filters destructure through the reference.
2. `CVPixelBufferLockFlags(1)` - the same tuple-struct constructor the code already used
   with 0.
3. `layout/src/image_scale.rs`: the test uses `azul_core::resources::Nv12Layout` by path
   (also imported at the top of the file).

## Test commands (release only)

```sh
cargo test --release -p azul-layout --lib image_scale      # RED before 7f662e86e: the new test
cargo test --release -p azul-layout --lib cpurender::raster # nv12 blit still byte-identical
cargo test --release -p azul-layout --lib widgets::capture_common
cargo test --release -p azul-dll --lib --features build-dll capture_slot
cargo test --release -p azul-dll --lib --features build-dll vt_tests -- --nocapture   # Apple hardware
cargo test --release -p azul-dll --lib --features build-dll overlay_upload_tests
AZ_LINK_PATH=$PWD/target/azul-lib cargo test --release -p AzMeet --lib
```
