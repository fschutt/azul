# VIDEO_PATH - AzMeet's video path: resample early, damage only the tile, NV12 end to end

Branch `wt/video-path` from `6ae909529`. Nothing was compiled (house rules); every file I wrote
parses (`rustfmt --check` / `node --check` / `py_compile`). The audit
`scratchpad/VIDEO_AUDIT_2026_09_30.md` (sections c/d) is folded in: its ranks 1, 2, 4, 5, 6
(partly), 7, 8, 10 (partly) and 11 are done here, rank 3 (encode off the UI thread, zero copy)
and 9 (decode off the UI thread) are designed below, not built.

## What changed, by the four asks

### 1. Resample as early as possible

- **Frames keep their format.** `VideoFrame` gains `format` (RGBA8 / BGRA8 / an NV12 variant).
  The capture seam gains `CaptureRequest.format` (the widget config's `output_format`) and
  `CaptureRead::FrameIn { width, height, format }`; the loop, `cut_frame`, the fan-out and
  `present_frame` carry the format end to end. Before, a macOS camera frame was BGRA ->
  swizzled to RGBA in the slot -> copied again for the reader -> scaled as RGBA -> labelled
  RGBA8 -> swizzled back to BGRA by `ImageRef::new_rawimage`. Now: copied once out of the
  locked CoreVideo buffer, in the format it came in.
- **NV12 capture.** AVFoundation asks the data output for `'420v'` / `'420f'` when the widget
  wants NV12 (AzMeet does), 32-BGRA otherwise; both are copied plane by plane / row by row, no
  conversion (`CaptureSlot::publish_nv12` / `publish_packed`). `CaptureSlot::take_newer` hands
  the frame to the worker by **swapping buffers** (no second full-frame copy).
- **One scaler, format-preserving, crop not squash.** `image_scale::resample_frame_rect` (the
  new `ResampleFn`, also the vImage backend's contract) scales BGRA/RGBA as they are (the sampler
  is channel-order agnostic) and NV12 plane by plane; `cut` takes the centre at the asked aspect
  (`cover_crop`, CSS `object-fit: cover`, even-aligned for NV12) instead of squashing 16:9 into
  3:2 (the audit's squashed preview); a crop already at the asked size is row copies.
- **Renditions are a cascade.** `fan_out` cuts the largest consumer first and every other one
  from the smallest frame already made that covers it (720 -> 360 -> 180), not every rendition
  from the full capture.
- **Decoded frames at the tile's size.** `VideoDecoder::set_output_size` (VideoToolbox scales in
  the decompression session: IOSurface-backed destination buffers at that size); AzMeet measures
  each remote tile in device pixels (`Remote.tile_px`) and asks its decoders for exactly that.
- **1:1 raster.** A frame that arrives at the tile's device size is a 1:1 blit in cpurender: only
  the visible source rows are converted, once each, and copied (byte-identical to `sample`).
- **A 540 rung** in the iroh ladder: a 200-logical-px tile on a 2x display asks for 960x540, not
  1280x720 (-44 % pixels in every pass; the camera has the 960x540 preset).
- The vImage scaler keeps BGRA as BGRA (no output swizzle), takes the crop, and uses the default
  kernel instead of Lanczos5. `stream.rs`'s own bilinear scaler (a twin) now calls
  `resample_frame`.

### 2. Damage only the tile

How a frame reached the screen before (the finding): `change_node_image` -> Paint tier ->
`apply_image_change` patched the display list in place (`patch_node_image`) **and**
`content_change_result` marked the display list dirty -> the next macOS frame ran
`regenerate_display_list_for_dom` (the whole list from the layout tree) -> on the GPU a full
`build_webrender_transaction`: the new `ImageRef` is a new hash -> a new `ImageKey` ->
`AddImage` + a new texture + the display list resent + a scene rebuild, the old key deleted two
epochs later. Headless answered the repaint with an incremental relayout.

Now:
- `content_change_result` marks the list dirty only for `RebuildDisplayList`. `Paint` sets
  `CommonWindowState::content_repaint_pending` and owes a repaint only: the CPU backends' DL diff
  damages exactly the patched item, headless paints from the layout as it stands
  (`repaint_only`; the three copies of its CPU paint tail are one `paint_cpu_frame`).
- **GPU: one key per tile.** The content overlay's raw frames get a stable key per
  `(dom, node)` (`RendererResources::node_image_slots`, minted from the `ImageRef` id counter by
  `ImageKey::unique_image_slot` so no image ever derives it): first frame `add_image`, every later
  one `update_image` of the same key, the previous frame forgotten (never `DeleteImage`d). The
  lightweight transaction (`build_image_only_transaction`, every backend) uploads them and asks
  for a frame with the same display list; only when a tile gets its key inside the lightweight
  transaction are the display lists resent once (`set_display_lists`, split out of the full
  transaction). The full transaction registers overlay frames first, so they never take a
  per-frame key there either.
- **The frame gate** is `ContentDirtyTier::PaintHidden` (between `Unchanged` and `Paint`, ->
  `DoNothing`): `apply_image_change` patches the item and returns it when
  `LayoutWindow::node_is_visible_in_window` says the tile shows no pixel - the window minimized,
  the box below/beside the window, or clipped away by a box of its `ScrollChain`. The next frame
  painted for another reason (the scroll that brings it back) shows the patched frame.
- macOS records WebRender's dirty rects in the frame report instead of collapsing them to
  `Full`. (CGL still composites the whole window on present: a real present-damage win on macOS
  needs a CALayer/IOSurface or Metal presenter.)

### 3. GPU / NV12

- `RawImageFormat` gains, at the END: `NV12Rec601Video`, `NV12Rec601Full`, `NV12Rec709Video`,
  `NV12Rec709Full` (matrix and range are part of the format, the enum stays a plain C enum).
  `Nv12Layout` (Y plane, then `ceil(w/2) x ceil(h/2)` Cb,Cr pairs), `YuvCoefficients` (THE
  fixed-point YCbCr table, with a row converter), `yuv_to_rgb`, `nv12_to_rgba`.
  `ImageRef::new_rawimage` keeps NV12 as it is (no conversion, opaque, no mipmaps).
- **GPU:** an NV12 image is two WebRender images, R8 luma + RG8 chroma, that are views into ONE
  buffer (`offset` / `stride`, no split copy - `nv12_plane_descriptors`); compositor2 pushes a
  `YuvImage` (`YuvData::NV12`, `ColorDepth::Color8`, Rec601/Rec709, Limited/Full). The chroma key
  lives in `RendererResources::nv12_chroma_keys`; the image GC deletes both planes.
- **CPU:** cpurender converts NV12 rows through the same table inside the existing blit (the
  fused convert + scale pass, only the rows the visible window touches; 1:1 is a straight row
  convert).
- **Encoder (VideoToolbox):** created strongest first: hardware REQUIRED + low-latency rate
  control (macOS 11.3+, High profile), then hardware required, then the OS default; realtime,
  no frame reordering, `MaxFrameDelayCount` 0, `ExpectedFrameRate` 30, Rec.709 colour tags,
  real-clock timestamps. Source buffers are `'420v'` + IOSurface from the session's own pool: an
  NV12 frame is copied plane by plane into it, VideoToolbox converts nothing. `settings()` and
  the open log line say what the session got (see "what each platform gets").
- **Decoder (VideoToolbox):** `set_output_format` (NV12 `'420v'`/`'420f'`, BGRA8, RGBA8 = the
  default so existing users do not change) and `set_output_size`; a change on a running stream
  waits for the next IDR. NV12 comes out plane by plane with its matrix from the buffer's YCbCr
  attachment. The Vulkan decoder hands its NV12 out as it is when asked (its own float YUV table,
  a twin, is gone: RGBA goes through `nv12_to_rgba`).
- **Screen share (ScreenCaptureKit):** the widget's pixel format (`'420v'` for AzMeet), 15 fps
  unless asked (screen text wants resolution more than frames), and only COMPLETE frames (the
  `SCStreamFrameInfoStatus` attachment) are copied; idle / blank / suspended buffers are skipped
  before a pixel is touched. SCK already scales to the requested size on the GPU.
- **JPEG / non-Apple fallback:** Linux / Windows / Android backends still deliver RGBA8 through
  `CaptureRead::Frame`; `encode_jpeg` (and PDF export) convert NV12 to RGB themselves.
- **AzMeet:** camera and screen opened with `output_format: NV12Rec709Video`; the consumer's
  frame is MOVED into the encoder (it was deep-copied per frame), copied only when JPEG peers need
  it too; decoders hand NV12 out at the tile's size; decoded frames go onto the tile in their own
  format, premultiplied (no swizzle, no per-pixel premultiply - audit rank 2); the mic meter moves
  at most 10 times a second (audit rank 11).

### 4. Measure

`examples/azul-meet/scripts/cpu_sample.py` (stdlib; `--pid NAME=PID ... --seconds 15 --json`)
and `two-clients.mjs --cpu [--cpu-seconds 15] [--windowed] [--camera]`: once both sides decode
each other's video it samples both clients' `ps -o %cpu=` once a second and prints mean and max
per client, then the run goes on. `--windowed` drops `AZ_BACKEND=headless` (real windows),
`--camera` the test pattern (real camera; macOS asks for access once) - that is the number a
user sees. Target: a two-person call well under 10 % per client on Apple Silicon.

## What each platform gets (encode)

- **macOS 11.3+ (Apple Silicon / T2 Intel):** hardware H.264, low-latency rate control, High
  profile, realtime, no reordering, frame delay 0. `hardware` in `settings()` is `Some(true)`
  (hardware was required; the query itself is refused while low latency is on).
- **macOS < 11.3, or a Mac whose encoder refuses low latency:** hardware required, Main profile,
  realtime, no reordering; `hardware` from `UsingHardwareAcceleratedVideoEncoder`.
- **A Mac without a hardware H.264 encoder (VMs):** the OS default session (software), Main.
- **iOS:** the encoder-specification keys are macOS-only in the SDK; they are loaded as optional
  symbols, so iOS gets the OS default (hardware on every iPhone/iPad).
- **Linux / Windows / Android:** no encoder engine (unchanged): AzMeet sends JPEG.

## Commits

| hash | what |
|---|---|
| 6dad43b2e | test(core): NV12 is a RawImageFormat that loads as-is (RED) |
| 8a0a01a7d | feat(core): RawImageFormat::NV12 variants, Nv12Layout, YuvCoefficients |
| 6eeed4d18 | test(layout): frames keep their format, crop not squash, cascade; NV12 blit (RED) |
| f788a02da | feat(layout): VideoFrame.format, resample_frame_rect, cover_crop, cascade, NV12 raster |
| 0622e1f66 | test(dll): a video frame damages only its tile; hidden tiles ask for no frame (RED) |
| 3698f9396 | test(dll): a video tile keeps one webrender key, NV12 is two planes (RED) |
| 8f5c3e2c7 | feat: image content update - no layout, no DL rebuild, one key, PaintHidden, YuvImage |
| cc468b11d | test: captures keep the pixel format the platform hands out (RED) |
| 104c9c499 | feat(capture): NV12 / BGRA from camera and screen, no conversion, no second copy |
| 871cee80f | test(videotoolbox): NV12 in/out, hardware-scaled decode, hardware realtime encode (RED) |
| 66003d063 | feat(video_codec): hardware low-latency encoder, pooled '420v', scaling NV12 decoder |
| 723a6e7e3 | test(iroh): a 400-device-pixel tile asks for 540, not 720 (RED) |
| 255205454 | feat(iroh): a 540 rung |
| cb795ba45 | feat(azmeet): NV12 end to end, decoded at the tile's size, no copies, meter 10 Hz |
| 4ef66e61f | feat(azmeet): two-clients --cpu (+ cpu_sample.py) |
| ad4da92d2 | docs(capture): the read contract names FrameIn |
| 0680cbced, 33996ae22, 4e2629f90 | progress checkpoints |

(`cb795ba45` was amended once, locally, to fix a message it had picked up from the commit
before it; nothing was pushed.)

## api.json changes (via `azul-doc autofix` / patch - none made by hand)

1. `image.RawImageFormat` - append `enum_fields`, in this order, after `RGBAF32`:
   `NV12Rec601Video`, `NV12Rec601Full`, `NV12Rec709Video`, `NV12Rec709Full`
   (docs: "NV12 (4:2:0 YCbCr, two planes): Y plane then interleaved Cb,Cr pairs; Rec.601
   matrix, video range" / "... Rec.601, full range" / "... Rec.709, video range" /
   "... Rec.709, full range").
2. `video.VideoFrame` - append struct field `format: RawImageFormat` AFTER `bytes` (field
   order width, height, bytes, format; no padding waste: 4+4+U8Vec+4 has the same tail padding
   as the "optimal" order). Doc: "The byte layout of `bytes`: RGBA8 / BGRA8 / an NV12 format."
   The C / Python / other bindings' struct literals of `VideoFrame` need the field.
3. `image.VideoDecoder` - two functions:
   - `set_output_format`: `fn_args: [{"self": "ref"}, {"format": "RawImageFormat"}]`,
     `fn_body: "object.set_output_format(format)"`, doc "Hand decoded frames out in `format`
     (NV12, BGRA8 or RGBA8, the default); a change on a running stream starts at its next
     keyframe."
   - `set_output_size`: `fn_args: [{"self": "ref"}, {"width": "u32"}, {"height": "u32"}]`,
     `fn_body: "object.set_output_size(width, height)"`, doc "Hand decoded frames out at
     `width` x `height` (0 x 0: the stream's own size), scaled by the decoder; a change on a
     running stream starts at its next keyframe."
   (The wasm stub in `dll/src/unified/video_codec.rs` has both, as no-ops.)
4. Doc-only: `image.VideoEncoder.encode` ("Submit one `VideoFrame` (NV12, BGRA8 or RGBA8)");
   `iroh.IrohTileRole.rendition_height` ("(90, 180, 360, 540 or 720)").

Not in api.json (Rust-only, no entry needed): `Nv12Layout`, `YuvCoefficients`, `yuv_to_rgb`,
`nv12_to_rgba`, `RawImageFormat::{is_nv12, is_full_range, is_rec709, nv12}`,
`VideoFrame::{with_format, expected_len}`, `ImageKey::unique_image_slot`, the two new
`RendererResources` fields, `ContentDirtyTier::PaintHidden`, `CaptureRequest.format`,
`CaptureRead::FrameIn`, the `image_scale` API (`SrcRect`, `cover_crop`, `resample_frame(_rect)`,
`frame_output_format`, new `ResampleFn` signature).

Behaviour changes callers can see: a `CameraWidget` / `ScreenCaptureWidget` with the default
config (`output_format: BGRA8`) now delivers BGRA8 frames to `on_frame` / `on_consumer_frame`
on macOS (it delivered RGBA8 labelled as nothing); `frame.format` says which. `RGBA8` in the
config keeps the old bytes. `VideoDecoder` still defaults to RGBA8.

## Least sure to compile

1. `dll/src/desktop/extra/camera/avfoundation.rs` `publish_pixel_buffer`: the objc2-core-video
   0.3.2 free functions (`CVPixelBufferGetBaseAddressOfPlane`, `...BytesPerRowOfPlane`,
   `...GetPixelFormatType`, deprecated `CVBufferGetAttachment` under `#[allow(deprecated)]`),
   the extern statics `kCVImageBufferYCbCrMatrixKey` / `..._ITU_R_601_4`, and the address
   compare `&*value as *const _ as *const u8`.
2. `dll/src/desktop/extra/screencap/macos.rs`: the hand-declared
   `extern "C" fn CMSampleBufferGetSampleAttachmentsArray` (resolved against CoreMedia, which
   objc2-core-media links), `msg_send!` on raw `*mut AnyObject` receivers (`count`,
   `objectAtIndex:`, `objectForKey:`, `integerValue`), and
   `lib.get::<*const *mut AnyObject>(b"SCStreamFrameInfoStatus\0")`.
3. `dll/src/desktop/extra/video_codec/videotoolbox.rs`: the new dlsym table entries and the
   `od!` optional-constant macro; `buffer_attributes`' closure calling unsafe fns inside the
   enclosing `unsafe` block.
4. `layout/src/image_scale.rs`: `sample_with<const C: usize, F>` called with `&|x, y| ..`
   closures (C inferred from the closure's return type).
5. `dll/src/desktop/wr_translate2.rs`: the split of `build_webrender_transaction` into
   `set_display_lists` + `build_webrender_transaction_tail`, and `upload_overlay_images`'
   destructuring of `LayoutWindow`.
6. `examples/azul-meet/src/lib.rs`: depends on the regenerated bindings (the NV12 variants,
   `VideoFrame { .., format }`, `CameraConfig { output_format, ..CameraConfig::default() }`,
   `VideoDecoder::set_output_format(&self, ..)` / `set_output_size`).
7. `layout/src/image.rs`: `nv12_as_rgba` is `#[allow(dead_code)]` when no encoder feature is on.

## Test commands (release only)

```sh
# core: the NV12 format
cargo test --release -p azul-core --lib nv12_tests
cargo test --release -p azul-core --features codegen --lib nv12_tests   # if the core suite needs it

# layout: scaler, raster, capture pipeline
cargo test --release -p azul-layout --lib image_scale
cargo test --release -p azul-layout --lib cpurender::raster          # nv12 / frame_at_the_tiles_size / fast_image_blit
cargo test --release -p azul-layout --lib widgets::capture_common
cargo test --release -p azul-layout --lib widgets::camera
cargo test --release -p azul-layout --lib widgets::screencap
cargo test --release -p azul-layout --lib widgets::video
cargo test --release -p azul-layout --lib image::
cargo test --release -p azul-layout --test all

# dll: tile damage (headless), stable keys, slot, VideoToolbox (Apple hardware), vImage, ladder
cargo test --release -p azul-dll --lib --features build-dll video_frame
cargo test --release -p azul-dll --lib --features build-dll tile
cargo test --release -p azul-dll --lib --features build-dll overlay_upload_tests
cargo test --release -p azul-dll --lib --features build-dll capture_slot
cargo test --release -p azul-dll --lib --features build-dll vt_tests -- --nocapture   # prints the encoder settings
cargo test --release -p azul-dll --lib --features build-dll resample::macos
cargo test --release -p azul-dll --lib --features build-dll loadbalancer
cargo test --release -p azul-dll --lib --features build-dll                           # full suite (headless damage tests)

# AzMeet (after the api.json changes + a rebuilt libazul in target/azul-lib)
AZ_LINK_PATH=$PWD/target/azul-lib cargo test --release -p AzMeet --lib

# CPU, before and after (build AzMeet + libazul with the debug server first)
node examples/azul-meet/scripts/two-clients.mjs --cpu                        # headless, test pattern
node examples/azul-meet/scripts/two-clients.mjs --cpu --windowed             # real windows, test pattern
node examples/azul-meet/scripts/two-clients.mjs --cpu --windowed --camera    # the real call
python3 examples/azul-meet/scripts/cpu_sample.py --pid Ada=<pid> --pid Ben=<pid> --seconds 15
```

The headless tests to watch: `a_video_frame_on_a_tile_runs_no_layout_and_rebuilds_no_display_list`,
`a_video_frame_damages_exactly_its_tile`, `two_tiles_updating_in_one_tick_damage_two_rects_not_the_window`,
`a_video_frame_on_a_tile_below_the_window_requests_no_frame`,
`a_video_frame_on_a_tile_scrolled_out_of_its_box_requests_no_frame`,
`a_video_frame_on_a_minimized_window_requests_no_frame`.
The VideoToolbox test `the_decoder_hands_frames_out_at_the_asked_size` is the one that proves
VideoToolbox scales in the decompression session; if it fails on hardware, frames still come out
at the stream's size (correct, just not scaled early) and a `VTPixelTransferSession` step is the
fix.

## Shared files (for the merge with `wt/idle-cpu`)

Edits in files the animation / frame-pacing work also touches, kept small:
- `dll/src/desktop/shell2/common/event.rs`: `content_change_result` (Paint no longer marks the
  display list dirty; sets `content_repaint_pending`), new field
  `CommonWindowState::content_repaint_pending` (+ its initializer).
- `dll/src/desktop/shell2/macos/mod.rs`: ONE hunk - the GPU frame report records
  `FrameDamage::Rects(gpu_damage_rects)` instead of `Full`.
- `dll/src/desktop/shell2/headless/mod.rs`: `service_frame` gains the content-repaint arm;
  `paint_cpu_frame` (the CPU paint tail, was copied three times) and `repaint_only`; tests
  appended at the end of `mod tests`.
- `dll/src/desktop/wr_translate2.rs`: `register_frame_resources` (overlay frames first, NV12
  split), `collect_stale_image_deletes` (chroma plane), `build_webrender_transaction` split,
  `build_image_only_transaction` (overlay upload, one-time display-list resend).
- `layout/src/window.rs`: `apply_image_change` (the PaintHidden gate), new
  `node_is_visible_in_window`, the pagination match arm.
- `layout/src/overlay.rs`: `ContentDirtyTier::PaintHidden`.
The "damage rect -> request frame" gate for content updates is `PaintHidden` -> `DoNothing`:
it goes through the same `ProcessEventResult` every frame request goes through, so the idle
agent's `request_frame()` sees no request for a hidden tile. Occlusion (another window over
this one) is the pacer's: it should not pace frames for an occluded window at all.

## Twins found (NO DUPLICATION)

- `decode_vulkan.rs` had its own float NV12 -> RGBA table: removed, it uses the core table.
- `video_codec/stream.rs` had its own bilinear frame scaler: now `image_scale::resample_frame`.
- `camera/android.rs` still has a float YUV -> RGB loop (full-range BT.601 on what is usually
  limited range, per the audit): not touched, should use `YuvCoefficients`.
- Latent: GL callback textures use `(dom << 32) | node` as their image key, which WebRender's
  `u32` narrowing (`translate_image_key`) turns into the node index - it can collide with the
  small `ImageRef`-id keys. `ImageKey::unique` (its own counter) and `image_ref_hash_to_image_key`
  (the `ImageRef` id) also share one key space. The new video-tile keys come from the `ImageRef`
  id counter, so they collide with neither of the id-based ones.

## Zero copy (designed, not built)

The seam that is left is a frame that is a native handle instead of bytes:
1. `DecodedImage::Native` (or `VideoFrame` with a handle arm) holding a retained
   `CVPixelBufferRef` (IOSurface-backed: the capture's, the decoder's). Retain on hand-over,
   release when the frame is replaced and WebRender has unlocked it.
2. GPU display: an `ExternalImageHandler` arm (`wr_translate2.rs` already has one for GL
   textures) that, on `lock`, binds the IOSurface to two `GL_TEXTURE_RECTANGLE` textures with
   `CGLTexImageIOSurface2D` (plane 0 `GL_R8`/`GL_RED`, plane 1 `GL_RG8`/`GL_RG`) and hands
   WebRender `ExternalImageType::TextureHandle(ImageBufferKind::TextureRect)` for the two keys
   of the `YuvImage` (the vendored WebRender compiles `brush_yuv_image` for TextureRect). No CPU
   pixel pass from the network to the screen.
3. Encode: the capture's `CVPixelBufferRef` goes straight into `VTCompressionSessionEncodeFrame`
   (smaller renditions through `VTPixelTransferSession` into buffers of each session's pool),
   ON THE CAPTURE WORKER, with the packets handed to the app (a `FrameConsumer` kind "H.264 at
   kbps"; api change). That also removes `CompleteFrames` from the UI thread (audit rank 3).
4. CPU windows keep the byte path (`nv12_to_rgba` rows at raster time) - lock the buffer
   read-only for the blit.

## What is left

- Encode and decode off the UI thread (audit ranks 3 and 9); `CompleteFrames` stays per frame
  because AzMeet's keyframe policy reads each frame's packets right after `encode`.
- The zero-copy path above.
- vImage `Planar8` + `CbCr8` for NV12 scaling (NV12 takes the portable scaler today); a SIMD
  scaler registered on Linux / Windows.
- `AVCaptureVideoDataOutput` output scaling (`videoSettings` width/height) to capture at exactly
  the covering size instead of the next preset; SCK `dirtyRects` for partial copies and
  `DirtyRect::Partial` texture updates.
- cpurender: `Arc::make_mut` still clones the display list per video frame when the CPU backend
  holds the previous list (audit rank 13); damage could come from the content journal instead.
- Non-Apple native NV12 capture and hardware encoders (MF, VA-API / Vulkan encode, MediaCodec).
- macOS present damage (CGL composites the whole window).
- The parent's before/after `two-clients.mjs --cpu` numbers.
