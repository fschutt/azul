# VIDEO-AUDIT 2026-09-30 — how efficient is azul's video pipeline?

Read-only audit of `fix/input-bugs-2026-09-19` at `6ae909529`. No file in the repository was changed and nothing was compiled.
Every claim about the code gives `file:line`. Every claim about an OS API gives a source in §R (web research, with dates).

**The scenario the numbers describe.** A two-person AzMeet call on an Apple Silicon Mac. Each side runs its own
process, and its window uses the GPU (WebRender over CGL) — `WindowCreateOptions::create` leaves `hw_accel` at its
default. The view is the gallery, with 300×200 logical tiles (`examples/azul-meet/src/lib.rs:532`), which is
600×400 device px on Retina. A 600-px tile asks for the 720 rung (`dll/src/desktop/extra/iroh/loadbalancer.rs:14`,
`LADDER = [(90,…),(180,…),(360,…),(720,…)]`), so each side captures and sends **1280×720** and shows the peer's
1280×720 in a 600×400 box. The camera runs at the device's default rate, normally 30 fps: `CameraConfig::default().fps = 0`,
and `avfoundation.rs:151` then leaves the rate alone.

Frame sizes used below: 720p RGBA = **3.69 MB**, 720p NV12 = **1.38 MB**, 600×400 RGBA preview = **0.96 MB**,
1080p RGBA = 8.29 MB.

**About the ms figures.** They are estimates, not measurements. A memcpy or a vectorised swizzle of 3.69 MB costs
about 0.1–0.3 ms on an M-series P-core. The scalar, indexed per-pixel loops (`videotoolbox.rs:669`, `:792`) run at
roughly one pixel per ns, which is about 1 ms per 720p frame. A zeroed 3.69 MB `vec!` costs about 225 16-KB page
faults, roughly 0.3–0.5 ms. Section (e) says how to replace every estimate with a measured number. The byte counts are exact.

---

## (a) Per-frame cost today

### A1. Camera → local preview tile (macOS)

| # | file:line | thread | what happens per frame | bytes | est. |
|---|---|---|---|---|---|
| C1 | `dll/src/desktop/extra/camera/avfoundation.rs:34-35, 324-330` | AVF | `videoSettings = {PixelFormat: 32BGRA}`. Cameras normally deliver 4:2:0 (`420v`/`420f`; Chrome's capturer defaults to `420v`, §R1.2), so asking for BGRA very likely makes AVFoundation convert every frame, and the buffer grows from 1.5 to 4 B/px. Apple does not document the conversion (UNVERIFIED): check the first `availableVideoPixelFormatTypes` entry, or use `videoSettings = @{}`, which gives the device-native format. | w 3.69 MB | inside AVF |
| C2 | `avfoundation.rs:72` → `dll/src/desktop/extra/capture_slot.rs:66-73, 125-132` | AVF dispatch queue | lock the pixel buffer, then swizzle BGRA→RGBA row by row into the slot's reused Vec, under the slot mutex. This runs for **every** camera frame, even when the worker will drop it. | r 3.69 + w 3.69 | 0.3–1 ms |
| C3 | `capture_slot.rs:96-97` (`out.extend_from_slice(&slot.rgba)`) | capture worker | copy the slot into the worker's `buf` | r+w 3.69 | 0.2–0.4 ms |
| C4 | `layout/src/widgets/capture_common.rs:838-848` | worker | the frame is read (C3) **before** the in-flight latch is checked, so a frame that gets dropped has already paid C2 and C3 | — | waste |
| C5 | `capture_common.rs:895-898` → `layout/src/image_scale.rs:242-252` → `dll/src/desktop/extra/resample/macos.rs:64, 86-92` | worker | preview cut: a new `vec![0; 0.96 MB]`, then `vImageScale_ARGB8888` with `kvImageHighQualityResampling` (Lanczos5, `macos.rs:30`) and a NULL temp buffer, so vImage allocates its scratch on every call. The scale goes from 1280×720 (16:9) to 600×400 (3:2), which is **non-uniform: the preview is squashed.** | r 3.69, w 0.96 | 1–2 ms |
| C6 | `capture_common.rs:899` → `image_scale.rs:249` (`src.bytes.to_vec()`) | worker | the 720p consumer is the same size as the capture, so it is a fresh allocation plus a memcpy. **Each rendition is cut from the full frame** (`image_scale.rs:260-275`): one downscale per rendition, no cascade. | 3.69 | 0.3–0.6 ms |
| C7 | `capture_common.rs:695` → `:196` → `core/src/resources.rs:1907, 2750, 2921-2940` | **main** | `ImageRef::new_rawimage`, then `load_rgba8`: an RGBA→BGRA swizzle in place (the pixels were BGRA at C1) plus an opacity scan | r+w 0.96 | 0.1–0.3 ms |
| C8 | `layout/src/window.rs:8472` (`Arc::make_mut(&mut lr.display_list).patch_node_image`) | main | The layout cache still holds the same `Arc<DisplayList>` (`layout/src/solver3/mod.rs:1894-1901`, confirmed by the comment at `window.rs:13418`). C9's regeneration hands the cache the same new `Arc` again (`window.rs:20595-20600`), and on CPU windows `previous_display_list` holds one too (`dll/src/desktop/shell2/headless/mod.rs:1021, 1477`). `make_mut` therefore **deep-clones the whole display list on every frame**, and then `patch_node_image` scans every item (`layout/src/solver3/display_list.rs:1141-1175`). | O(items) alloc | 0.1–1 ms |
| C9 | `dll/src/desktop/shell2/common/event.rs:4735-4739` → `dll/src/desktop/shell2/macos/mod.rs:8205-8217` | main | Paint tier → `mark_display_list_dirty` → on the next frame, `regenerate_display_list_for_dom(0)`: a **full display-list regeneration** from the layout tree (`window.rs:20418`). The in-place patch from C8 is thrown away. | O(nodes) | 0.5–3 ms |
| C10 | `macos/mod.rs:8521` → `dll/src/desktop/wr_translate2.rs:1302-1405, 781-857` | main | **full WebRender transaction.** Every DL item is scanned for images. The new frame is a new `ImageRef` hash and so a **new ImageKey → `AddImage`**; raw frames never get `UpdateImage`. The only `update_image` call is for GL callback textures (`wr_translate2.rs:2780`). The old key gets `DeleteImage` after 2 epochs (`wr_translate2.rs:1408`). `translate_displaylist_to_wr` runs for every DOM, then `set_display_list` → **scene rebuild** on the scene-builder thread. | O(items) | 1–4 ms |
| C11 | `webrender/core/src/texture_cache.rs:1427-1446` + `webrender/core/src/renderer/init.rs:237` | render | Each new key allocates a new texture: anything wider or taller than 512 px becomes a **standalone** texture (atlas 1024, `wr_translate2.rs:269-283`), and 600×400 does. The upload is a memcpy into a PBO (`UploadMethod::PixelBuffer`), then `glTexSubImage`. | 0.96 (preview) / 3.69 (remote) | GPU alloc + copy |
| C12 | `wr_translate2.rs:334-345`, `macos/mod.rs:8680-8690` | GPU | CGL has no partial present, so **the whole window is composited every frame**. The frame report records `FrameDamage::Full` whenever WebRender reports any dirty rect, so GPU "tile-only damage" cannot be measured today. | window | GPU |

### A2. Camera → H.264 → iroh (macOS)

C1–C6 are shared with A1: one capture feeds the preview and every rendition. From the writeback on:

| # | file:line | thread | what happens per frame | bytes | est. |
|---|---|---|---|---|---|
| E1 | `capture_common.rs:689-691` → `lib.rs:1291-1307` → `send_video` `lib.rs:2005-2051` → `send_h264` `lib.rs:2057` | **main** | The encoder runs inside the widget's writeback, on the UI thread. | — | — |
| E2 | `lib.rs:2104` (`bytes: rgba.clone()`) | main | `U8Vec::clone` is a **deep copy** (`css/src/macros.rs:919-936`), made only to hand `VideoEncoder::encode` a frame by value. `encode` then just takes `frame.bytes.as_ref()` (`dll/src/desktop/extra/video_codec/mod.rs:375-388`). | 3.69 | 0.3–0.6 ms |
| E3 | `videotoolbox.rs:646-656` | main | `CVPixelBufferCreate(…, BGRA, attrs = NULL)` **on every frame**: no `VTCompressionSessionGetPixelBufferPool` and no IOSurface-backing key | alloc 3.69 | 0.1–0.5 ms |
| E4 | `videotoolbox.rs:667-675` | main | scalar, indexed per-pixel RGBA→BGRA swizzle into the CVPixelBuffer. This undoes C2. | r+w 3.69 | ~1 ms |
| E5 | inside VT (`videotoolbox.rs:21-23` says so) | VT | converts BGRA → 4:2:0. Apple: when the source attributes "cannot be reconciled … VideoToolbox converts each CVImageBuffer internally", and "pixel buffers not allocated by VideoToolbox increase the chance that you'll have to copy image data" (§R1.1). E3's buffer is neither pooled nor IOSurface-backed. | 3.69 → 1.38 | unmeasured |
| E6 | `videotoolbox.rs:678` | main | `pts = frame_idx / 30`, a synthetic 30 fps clock. Dropped frames and real timing are invisible to rate control, and no `ExpectedFrameRate` is set. | — | quality |
| E7 | `videotoolbox.rs:717` (`VTCompressionSessionCompleteFrames(session, pts)`) | **main, blocking** | forces every frame out synchronously, so **the UI thread waits for the hardware encode** and the encoder cannot pipeline | — | 2–6 ms stall (unmeasured) |
| E8 | `videotoolbox.rs:482-532`, `mod.rs:384`, `lib.rs:2121, 2165` | VT / main | AVCC→Annex-B rewrite, `U8Vec::from_vec`, `encode_packet` copy, `packet.clone()` per peer | 10–40 KB | negligible |

**Encoder configuration** (`videotoolbox.rs:546-630`):

- `VTCompressionSessionCreate(…, encoderSpecification = NULL, sourceImageBufferAttributes = NULL, …)` (`:554-565`).
  - Hardware is **neither required nor verified**. `kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder` is never read.
  - There is **no `EnableLowLatencyRateControl`**, which can only be set in the encoder specification at creation time.
  - There is **no source-buffer pool** to allocate from.
- Set: `RealTime = true` (`:571-575`), `AllowFrameReordering = false` (`:576-580`, so no B-frames), `ProfileLevel = H264_Main_AutoLevel` (`:581-585`), `AverageBitRate` (`:587-600`) and `MaxKeyFrameInterval = 60` (`:601-613`).
- Not set:
  - `MaxFrameDelayCount`
  - `ExpectedFrameRate`
  - `DataRateLimits`
  - `ConstrainedBaseline`/`ConstrainedHigh` (Main without B-frames works, but constrained profiles are what WebRTC sends)
  - `PrioritizeEncodingSpeedOverQuality`
  - colour tags (`ColorPrimaries` / `TransferFunction` / `YCbCrMatrix`)

**Renditions.** `routes::encode_set` (`examples/azul-meet/src/routes.rs:269-276`) encodes the smallest and the largest height that viewers asked for. Each one is a separate `FrameConsumer` (`lib.rs:1016-1019`, `feed_consumer` `lib.rs:659-665`). Each is cut **from the full captured frame** by vImage on the capture worker (C6), then copied (E2), swizzled (E4) and encoded synchronously on the main thread (E7), with one `VtEncoder` per rendition (`lib.rs:2084-2093`). A size change reopens the encoder (`lib.rs:2079-2083`).

**Non-Apple platforms have no H.264 encoder at all.** `encode_engine()` returns `Err` on Linux, Windows and Android (`video_codec/mod.rs:98-126`). AzMeet then sends **JPEG**: `send_jpeg` `lib.rs:2173-2186` does a `rgba.clone()`, `premultiplied_alpha: false`, and `encode_jpeg` on the UI thread, for every frame and every rendition. The receiver JPEG-decodes on its UI thread (`lib.rs:2309-2312`).

### A3. Receive → H.264 decode → remote tile (macOS)

| # | file:line | thread | what happens per frame | bytes | est. |
|---|---|---|---|---|---|
| R1 | `lib.rs:4156-4159` (`PUMP_MS = 15`, `lib.rs:143`) | main | The 15 ms `pump_link` timer drains iroh events on the UI thread. | — | — |
| R2 | `lib.rs:2319` (`U8Vec::from(payload.to_vec())`) | main | copies the payload | 10–40 KB | small |
| R3 | `videotoolbox.rs:909-925, 940-955` | main | builds the AU, then copies it into a CMBlockBuffer (`CMBlockBufferReplaceDataBytes`) | small | small |
| R4 | `videotoolbox.rs:981-987` (flags = 0) | **main, blocking** | a **synchronous** `VTDecompressionSessionDecodeFrame` on the UI thread, with no decoder specification. `kVTDecompressionPropertyKey_RealTime` is not set, but it defaults to true (§R1.5), so the problem is the thread, not the setting. | — | 1–4 ms stall |
| R5 | `videotoolbox.rs:855-868` | VT | `destinationImageBufferAttributes = {PixelFormat: BGRA}` only. The decoder's native output is 4:2:0, so VT runs a pixel-transfer to BGRA. Neither the IOSurface nor the GL/Metal-compatibility key is requested. | w 3.69 | unmeasured (§R1.5) |
| R6 | `videotoolbox.rs:788-797` | main (the callback runs inline) | `vec![0u8; w*h*4]`, a new 3.69 MB zeroed allocation per frame, then a scalar BGRA→RGBA swizzle | r+w 3.69 | ~1–1.5 ms |
| R7 | `lib.rs:2326` → `rgba_image` `lib.rs:1876-1884` (**`premultiplied_alpha: false`**) → `resources.rs:2944-2955` | main | `load_rgba8` swizzles RGBA→BGRA again, undoing R6, and **premultiplies every pixel** (three multiply-divides, `resources.rs:2692-2701`) although alpha is always 255 | r+w 3.69 | 1–2 ms |
| R8 | `lib.rs:1409-1429` → `change_node_image` | main | the same display path as C8–C12: a DL clone, a full DL regeneration, a full WebRender transaction, **a new 1280×720 standalone texture every frame**, a 3.69 MB PBO memcpy + upload, and a full-window composite | 3.69 | see C8–C12 |
| R9 | `lib.rs:1462-1492` | main | every 130 ticks (about 2 s), `continue_and_refresh_dom()` triggers a full DOM rebuild and relayout. This is low frequency and acceptable. | — | — |

On Linux and Windows (x86_64 + `video-native`), decode runs through Vulkan Video. It downloads NV12 to CPU memory and converts it to RGBA with a **scalar float loop** (`dll/src/desktop/extra/video_codec/decode_vulkan.rs:129-190`), then continues through R7–R8.

### A4. Screen share (macOS: ScreenCaptureKit)

| # | file:line | what happens | assessment |
|---|---|---|---|
| S1 | `dll/src/desktop/extra/screencap/macos.rs:264-286` | `SCStreamConfiguration` sets `width/height` to the covering size, so SCK scales on the GPU and the backend does **not** capture full Retina and downscale on the CPU. It also sets `pixelFormat = BGRA`, `showsCursor`, `queueDepth = 5` and `minimumFrameInterval = 1/fps`, where fps defaults to **30**. | The size is good. The format is wrong for encode (§R2 recommends `420v` for encode). 30 fps is high for screen content. |
| S2 | `macos.rs:171-203` | The output callback ignores `SCStreamFrameInfo`: no status (`.idle` / `.blank` / `.complete`), no `dirtyRects`, no `contentRect` / `scaleFactor`. It only skips buffers without an image. | The damage information is thrown away. WebRTC's SCK capturer at least processes only `Complete`/`Started` frames. It too marks the whole frame on any dirty rect, but WWDC22 recommends using dirty rects (§R2). |
| S3 | `macos.rs:369-380, 272-273` | For a display, the output size is the requested 16:9 rendition size. The display is usually 16:10 or 3:2. `scalesToFit` / `preservesAspectRatio` are not set explicitly. | The picture is letterboxed or stretched, depending on the OS default. |
| S4 | `macos.rs:195` → C2 … C12, E1 … E8 | the same CPU chain as the camera | S2 + C3–C12 + E2–E7 per changed frame |
| S5 | `layout/src/widgets/screencap.rs:242-256` | `exclude_self: true` stops the feedback loop | good |
| S6 | local tile | Your own shared screen is shown at up to 30 fps, through the full display rebuild. | pure local overhead |

**Other operating systems:**

- **Windows**:
  - DXGI desktop duplication does a **full-monitor** `CopyResource` into a staging texture and a synchronous `Map` (`dll/src/desktop/extra/screencap/windows.rs:297-347`).
  - It never downscales on the GPU, ignores the requested size and never calls `GetFrameDirtyRects` / `GetFrameMoveRects`.
  - It then swizzles on the CPU (`windows.rs:350-361`) and scales with the **portable scalar scaler**, up to 16 taps per output pixel (`image_scale.rs:127-170`). Only macOS registers vImage (`dll/src/desktop/extra/resample/mod.rs:21-31`), so 4K→720p here is tens of ms per frame.
- **Linux**:
  - PipeWire + DMA-BUF import (good), but the EGL blit renders at **source size** (`dll/src/desktop/extra/screencap/dmabuf.rs:527-545`), then `glReadPixels` reads back the full-resolution RGBA.
  - There is no `SPA_META_VideoDamage`, and the portable scaler follows.

### A5. Camera on other operating systems

- **Windows**:
  - The MF source reader runs with `MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING = 1` and asks for `RGB32` (`dll/src/desktop/extra/camera/windows.rs:332-363`). With no D3D manager, that is a software colour conversion.
  - After that come BGRX→RGBA in Rust (`windows.rs:446-460`) and the portable scaler.
  - The `camera-native` build uses nokhwa's MJPEG decode to RGBA instead (`windows.rs:33-75`).
- **Linux**:
  - `V4L2_PIX_FMT_RGB24` is requested through libv4l2, which converts YUYV/MJPEG in software (`dll/src/desktop/extra/camera/v4l2.rs:9-11, 378-393`).
  - Then a bounds-checked RGB24→RGBA loop (`v4l2.rs:589-605`) and polling with 5 ms sleeps (`v4l2.rs:526`).
- **Android**:
  - `AImageReader` YUV_420_888 goes through a **scalar f32 YUV→RGB** loop (`dll/src/desktop/extra/camera/android.rs:196-212`), with full-range BT.601 constants applied to what is usually limited-range data.
  - It polls with 5 ms sleeps. There is no MediaCodec encoder, so frames go out as JPEG.

### A6. Per-second totals for the scenario (one side)

- **Sender**, camera 720p at 30 fps:
  - 6 CPU full-frame passes over 3.69 MB: C2, C3, C5 read, C6, E2, E4. That is about 180 passes/s, **≈ 1.3 GB/s of memory traffic**.
  - 3 zeroed allocations of 0.96–3.69 MB each: C5, C6, E3.
  - A synchronous hardware encode on the UI thread (E7).
- **Receiver**, 720p at 30 fps: 2 CPU passes (R6, R7 with premultiply) + 1 PBO copy + 1 allocation per frame, and a synchronous decode on the UI thread (R4).
- **Display**, both tiles:
  - Up to 60 pictures/s: preview 30 + remote 30. The display link coalesces requests within one vsync (`macos/mod.rs:9024-9037`), but the two 30 fps streams arrive out of phase, so they rarely share one.
  - Each picture costs a DL clone + a full DL regeneration + a full WebRender scene rebuild + a new texture + a full-window composite.
  - The mic level meter adds up to 50 more VirtualView rebuilds/s while you speak (`lib.rs:1757-1770` → `layout/src/widgets/progressbar.rs:415` → `macos/mod.rs:8361`).
- **Estimate**: 5–8 ms of CPU per sent frame and 3–5 ms per received frame, plus 2–6 ms per display rebuild. That is **roughly 25–40 % of one core**, which is consistent with the ~30 % the user sees.

---

## (b) The ideal pipeline, per path and OS

The guiding rules, taken from WebRTC/Chrome (§R7):

- Capture once, at the size the largest consumer needs, in the camera's or compositor's native 4:2:0.
- Keep the frame in a GPU/IOSurface buffer and hand the same buffer to the hardware encoder and to the compositor.
- Scale on the GPU, once per rendition.
- Decode into a native 4:2:0 surface and composite it with a YUV shader.
- Update the one texture in place with damage that is only that tile.
- Do not touch the display list or the scene for a new video frame.

### macOS / iOS

**Camera.**

- `AVCaptureVideoDataOutput.videoSettings = {kCVPixelBufferPixelFormatTypeKey: '420v' or '420f', whichever the active format delivers}`. On macOS, also add `kCVPixelBufferWidthKey/HeightKey` for the largest rendition, so AVFoundation scales to exactly that size. The macOS output header allows these keys, but this was not re-verified in §R. Otherwise pick an `activeFormat` / preset ≥ the largest rendition.
- Choose the preset or `activeFormat` for the largest rendition; 960×540 exists (`avfoundation.rs:104-119`).
- The delegate **retains** the IOSurface-backed `CVPixelBuffer` and never locks or copies it.

**Encode, on the capture or encode queue, never the UI thread.**

- Create the session with:
  - `encoderSpecification = {EnableLowLatencyRateControl: true}` on macOS 11.3+. This gives infinite GOP, no B-frames, no lookahead and "one in, one out" (§R1.3). The profile rules conflict between sources: the current docs say "only High profiles", WWDC21 says Constrained Baseline / Constrained High (both constants need macOS 12.0). Chrome turns it on only for hardware + low-delay + an SVC-capable profile.
  - `{RequireHardwareAcceleratedVideoEncoder: true}` where the fallback should be JPEG
  - `sourceImageBufferAttributes = {'420v', w, h, IOSurface: {}}`, which is exactly what libwebrtc passes
- Properties: `RealTime`, `AllowFrameReordering = false`, a constrained profile, `ExpectedFrameRate`, `MaxKeyFrameInterval(Duration)`, `AverageBitRate` / `DataRateLimits`, and colour tags.
- Largest rendition: `VTCompressionSessionEncodeFrame(pb, real PTS)`, with **zero copies**.
- Every smaller rendition: `VTPixelTransferSessionTransferImage(pb → buffer from that session's pool)`. Apple does not document whether this runs in hardware (UNVERIFIED), so measure it. libwebrtc instead scales NV12 on the CPU with libyuv `NV12Scale` (§R1.4), which costs about 0.1–0.3 ms at 720p. libwebrtc's `SimulcastEncoderAdapter` also scales **every layer from the original frame, not as a cascade** (§R7), so azul's "one cut per rendition from the capture" matches it. The waste is in how each cut is made (RGBA, Lanczos, copies), not in the structure.
- Packets leave through the output callback into a queue that the network side drains. **No `CompleteFrames` per frame.**

**Preview, with zero copies.**

- Bind the same IOSurface to GL: `CGLTexImageIOSurface2D` on `GL_TEXTURE_RECTANGLE`, plane 0 as `GL_R8`/`GL_RED`, plane 1 as `GL_RG8`/`GL_RG` (§R1.6, which is Firefox's exact code).
- Hand it to WebRender as an external image, `ImageBufferKind::TextureRect`, drawn by `brush_yuv_image`. The vendored copy compiles its TextureRect variant on desktop GL (`webrender/core/src/renderer/shade.rs:50-56, 994-1037`).
- Keep the `CVPixelBuffer` alive until WebRender unlocks it.
- The GPU scales and letterboxes to the tile, and the preview never influences the capture size beyond the largest rendition.

**Decode.**

- `VTDecompressionSession` with `destinationImageBufferAttributes = {'420v'/'420f', IOSurface: {}, OpenGLCompatibility or MetalCompatibility: true}`, `kVTDecompressionPropertyKey_RealTime`, and `kVTDecodeFrame_EnableAsynchronousDecompression`, fed from a receive thread.
- The output `CVPixelBuffer` goes through the same NV12 external-image path: **zero CPU pixel passes** from the network to the screen.

**Screen share.**

- `SCStreamConfiguration`:
  - `pixelFormat = '420v'` (WWDC22: "420v for encoding", BGRA for on-screen display) and `width/height` = the rendition size, with the display's aspect preserved. Chrome sets `scalesToFit = NO` and computes the size itself.
  - `minimumFrameInterval` 1/15, or lower for static content
  - `queueDepth` 3–5 (the allowed range is 3–8), and `colorMatrix` to match the encoder tags. Return each surface faster than `minimumFrameInterval × (queueDepth − 1)`, or SCK drops frames (§R2).
- Read `SCStreamFrameInfo.status` and skip everything but `.complete`. Read `dirtyRects`: skip the encode when it is empty, and send `DirtyRect::Partial` for the local tile.
- SCK buffers are IOSurface-backed (Chrome forwards `CVPixelBufferGetIOSurface` without a copy, §R2), so they go straight to VT.
- Show the local own-screen tile at 1–5 fps, or as a thumbnail.
- Screen text wants resolution more than frames: send, for example, 1080p at 5–15 fps instead of 720p at 30.

**Display in azul, on every OS.** A video tile gets a **stable image key per (dom, node)**. A frame of the same size and format becomes `txn.update_image(key, …, DirtyRect::All)` + `skip_scene_builder()` + `generate_frame()`, with no DL work at all. That is exactly what `process_image_callback_updates` already does for GL callback textures (`wr_translate2.rs:2747-2786`: "Stable (dom, node) external id … per-frame texture updates are update_image (pixels only), never a scene rebuild"). Only a size or format change goes through AddImage + DL rebuild.

### Windows

**Camera.**

- The MF source reader gets a D3D manager (`MF_SOURCE_READER_D3D_MANAGER`) and **native NV12** output; drop the RGB32 advanced processing.
- Frames are `IMFDXGIBuffer` → `ID3D11Texture2D` NV12.

**Encode.**

- A hardware H.264 MFT (`MFTEnumEx` with `MFT_ENUM_FLAG_HARDWARE`, async MFT + `IMFDXGIDeviceManager`), with `CODECAPI_AVLowLatencyMode`, CBR, and a GOP set.
- Renditions and BGRA→NV12 go through `ID3D11VideoProcessor` / `VideoProcessorBlt` on the GPU (§R3).
- This fixes today's JPEG fallback, the largest non-Mac cost.

**Screen.**

- Windows.Graphics.Capture: `Direct3D11CaptureFramePool` with its size set to the rendition, and dirty regions on Windows 11 24H2+ (§R3).
- Or DXGI duplication, with `GetFrameDirtyRects`/`GetFrameMoveRects`, GPU downscale + NV12 before any staging readback, and a readback only when the CPU renderer needs pixels.

**Decode and display.** Vulkan Video (today) or D3D11VA/MF decode, then the NV12 planes go to WebRender as R8 + RG8 `update_image` on stable keys. The zero-copy step later is a shared texture (WGL_NV_DX_interop2) instead of CPU memory.

### Linux

**Camera.** V4L2 native YUYV/NV12, or MJPEG without libv4l2 conversion. `VIDIOC_EXPBUF` gives a DMA-BUF that goes to EGL / VA-API.

**Encode.**

- VA-API H.264 (`VAProfileH264ConstrainedBaseline`/High, `VAEntrypointEncSliceLP` where available), with surfaces imported from DMA-BUF.
- Or Vulkan Video encode (`VK_KHR_video_encode_h264`) through gpu-video.

**Screen.**

- Portal ScreenCast → PipeWire DMA-BUF with modifiers (already there), plus `SPA_META_VideoDamage` and `SPA_META_Cursor`.
- Make the existing EGL blit render at the **requested size** and into NV12 (Y and UV passes) before `glReadPixels` (`dmabuf.rs:527-545` already has `dst_w/dst_h`).

**Decode and display.** The Vulkan Video NV12 goes straight to WebRender YUV (R8 + RG8), not through `nv12_to_rgba`.

### Android

**Camera.** CameraX/Camera2 streams straight into `MediaCodec.createInputSurface()`, `COLOR_FormatSurface`, with no CPU access. Configure the encoder with `KEY_LATENCY` (API 26, encoders only, in frames), `KEY_PRIORITY = 0` (realtime, API 23), and a Surface-sized rendition. `KEY_LOW_LATENCY` (API 30) is for **decoders** (§R5). A second Camera2 output target feeds the preview, a `SurfaceTexture` → `GL_TEXTURE_EXTERNAL_OES`. WebRender supports TextureExternal on GLES ESSL3 (`shade.rs:55, 1018-1020`).

**Decode.** MediaCodec renders to a Surface, or an `ImageReader(PRIVATE)` + `HardwareBuffer` → `EGLImage` → external texture.

**Screen.** MediaProjection → a `VirtualDisplay` sized to the rendition, onto the encoder's input Surface.

---

## (c) Ranked changes, largest win first

Saving = CPU of the scenario in A6. Risk: L / M / H. api = whether `api.json` changes. Those changes go through `azul-doc autofix` only.

| rank | change | file:line | expected saving | risk | api |
|---|---|---|---|---|---|
| 1 | **Stable-key video images: same-size frames become `update_image` with no DL and no scene work.** When a Paint-tier image change keeps the same size and format, record the pixels for the backend and skip `mark_display_list_dirty` and `regenerate_display_list_for_dom`. Use the existing stable-key path for raw (and later NV12) data, `txn.update_image(key, desc, Raw, DirtyRect::All)` + `skip_scene_builder()`. Keep the overlay write, so the CPU backend (which reads the overlay/DL) still sees the frame. Also stop `Arc::make_mut` deep-cloning the DL per frame: patch only when the DL `Arc` is unique, or let the CPU backend take image damage from the content journal. | `event.rs:4735-4739`, `macos/mod.rs:8205-8217`, `window.rs:8466-8474`, `wr_translate2.rs:1302-1405` (new-key AddImage) vs `:2701-2810` (template) | removes a DL regeneration, a full WR translation, a scene rebuild, a texture alloc and GC churn for **every** picture, about 60/s here. **≈5–12 % of a core.** Also cuts GPU memory churn (3 live 720p textures per tile today, because of the 2-epoch GC). | M | no |
| 2 | **Receive path: stop the triple conversion (quick win, no NV12 needed).** Keep VT's BGRA and present it as `RawImageFormat::BGRA8` with `premultiplied_alpha: true`. `load_bgra8` then neither copies nor converts (`resources.rs:3113-3135`, "DO NOT CLONE"). `dec_output` becomes a row memcpy, or better, keep the `CVPixelBuffer`. At minimum, set `premultiplied_alpha: true` in `rgba_image`. | `videotoolbox.rs:788-797`, `lib.rs:1876-1884`, `lib.rs:2326` | per received frame: −1 swizzle, −1 swizzle + premultiply, −1 zeroed alloc. **≈2–3 ms/frame, ≈6–9 % of a core at 30 fps.** | L | no (VideoFrame docs say RGBA; keep that for `recv_frame`, add an internal BGRA fast path, or do it together with #4) |
| 3 | **Encode off the UI thread and zero-copy.** Encode on the capture worker (or a per-rendition encode queue) with the camera's `CVPixelBuffer`. Request `420v` (not BGRA) from AVFoundation, create the VT session with `sourceImageBufferAttributes` + the pool, use `VTPixelTransferSession` for smaller renditions, and hand **packets** (not RGBA frames) to the app. | `avfoundation.rs:324-330`, `capture_slot.rs`, `capture_common.rs:881-910`, `videotoolbox.rs:633-727`, `lib.rs:2057-2168`; new `FrameConsumer` kind | removes C1 (the 2.7× bytes), C2, C6, E2, E3, E4 and E7 (the main-thread stall). **≈4–7 ms/frame, ≈10–20 % of a core**, plus UI latency. | M-H | **yes**: `FrameConsumer` gains an output kind (RGBA8 / NV12 / H.264 {kbps, fps, profile}) and `ConsumerFrame` gains packets, or a new `EncodedConsumerFrame`; `VideoEncoder::encode_native` |
| 4 | **NV12 end to end for display** (the `wt/video-path` scope). Add `RawImageFormat::NV12`, with Y/UV strides, range and matrix. WebRender lowers it to `push_yuv_image(YuvData::NV12(y_key, uv_key), ColorDepth::Color8, YuvColorSpace::Rec709/601, ColorRange::Limited/Full, ImageRendering::Auto)`, with Y as `ImageFormat::R8` and UV as `RG8` on **stable keys** (#1). The VT decoder outputs `420v` + IOSurface. The zero-copy step after that is IOSurface → GL rectangle textures through `ExternalImageHandler` (`wr_translate2.rs:360-405`). | `core/src/resources.rs:1565-1578`, `wr_translate2.rs:1049-1071, 1251-1287`, `dll/src/desktop/compositor2.rs` (emit YuvImage), `videotoolbox.rs:855-868`, cpurender (NV12 raster) | upload 1.38 MB instead of 3.69 MB (−62 %). No swizzle and no premultiply. The GPU does the colour conversion. **≈1–2 ms/frame more on top of #2**, and zero with IOSurface. | M (colour range/matrix, odd sizes, CPU backend twin) | **yes**: `RawImageFormat::NV12`, the `VideoFrame` format and strides, `ScreenCaptureConfig.output_format` (already reserved: `core/src/screencap.rs:40-43`) |
| 5 | **Preview without a CPU cut on GPU windows.** Show the captured frame, or the NV12 surface, and let WebRender scale it. Cut on the CPU only for CPU-rendered windows. Keep the aspect ratio (object-fit: cover) instead of the non-uniform `cut`. | `capture_common.rs:648-656, 895-898`, `image_scale.rs:242-252` | −1 vImage Lanczos pass (1–2 ms) and −1 allocation per frame. **Fixes the squashed preview.** | L | no |
| 6 | **Encoder settings.** Add `kVTVideoEncoderSpecification_EnableLowLatencyRateControl` (11.3+) and `Enable/RequireHardwareAcceleratedVideoEncoder`, and log `UsingHardwareAcceleratedVideoEncoder`. That query returns -12900 once low-latency mode is on (§R1.3), so check hardware in a session without it, or rely on `Require…`. Use a constrained profile (12.0+), `ExpectedFrameRate`, real capture PTS instead of `frame_idx/30`, and `MaxFrameDelayCount` (Chrome uses 3; 0/1 for strictly one-in-one-out). Drop the per-frame `CompleteFrames` (use the async output). Add colour tags. | `videotoolbox.rs:554-615, 678, 717` | UI stall −2–6 ms/frame (E7), better rate control on dropped frames, and a guarantee of hardware encode | L-M (low-latency RC constrains the profile) | no |
| 7 | **Ladder: add a 540 rung (960×540).** A 600×400 device-px tile needs 540, not 720, and AVF has the preset. | `loadbalancer.rs:14` (+ kbps), `routes.rs:263` | −44 % pixels in **every** pass and every encode for the common gallery tile | L | no (the ladder is internal) |
| 8 | **Screen share settings.** `420v` for the encode path, 5–15 fps, `SCStreamFrameInfo` status + `dirtyRects` (skip idle, partial local update), aspect-preserving size, and the local own-screen tile at a low rate. | `screencap/macos.rs:264-286, 171-203` | screen share goes from 30 fps × the full chain to changed frames only, at ≤15 fps. **≈50–80 % of the share cost.** | L-M | only the NV12 variant (#4) |
| 9 | **Decode off the UI thread.** `kVTDecodeFrame_EnableAsynchronousDecompression` on a receive thread; the UI thread only swaps the image. | `lib.rs:2230-2330`, `videotoolbox.rs:903-996` | UI stall −1–4 ms per received frame | M | maybe (a worker in the app, or a decoder thread inside `VideoDecoder`) |
| 10 | **Capture-loop hygiene.** Check `in_flight` **before** `read` (`capture_common.rs:838-848`). Have `publish_bgra` store the raw frame (or the retained buffer) and convert lazily. Stop `mem::take(buf)`, which forces a new 3.69 MB allocation when the source travels (`:901`). Pass a vImage temp buffer. Use `kvImageNoFlags` (bilinear-ish) instead of Lanczos5 for live frames. | `capture_common.rs:838-901`, `capture_slot.rs:56-80`, `resample/macos.rs:30, 86-92` | 0.5–2 ms/frame | L | no |
| 11 | **Mic meter at 50 Hz → full scene rebuild.** Throttle it to ~10 Hz, or make it a GPU transform/opacity value. | `lib.rs:1757-1770`, `progressbar.rs:415`, `macos/mod.rs:8361` | up to 50 full rebuilds/s while speaking | L | no |
| 12 | **Non-Mac: a real encoder and GPU capture scaling.** MF H.264 HW MFT (Windows), VA-API or Vulkan Video encode (Linux), MediaCodec Surface (Android). GPU downscale in DXGI/PipeWire before readback, and dirty rects. Register a SIMD resampler (e.g. `fast_image_resize`) on non-Mac until then. | `video_codec/mod.rs:98-126`, `screencap/windows.rs:297-361`, `dmabuf.rs:527-545`, `resample/mod.rs:21-31` | replaces JPEG encode + decode (the largest CPU item on Windows/Linux) and 4K scalar scaling (tens of ms/frame) | H (new backends) | `VideoEncoder` backend names only |
| 13 | **CPU renderer: no DL clone + diff for an image-only change.** The content journal already knows the node, so damage = the node's bounds. | `window.rs:8472`, `headless/mod.rs:777-800` | O(items) clone + diff per frame on CPU windows | M | no |

**Order to land them.**

1. #2, #5, #6, #7 and #10 first. They are low risk and have no api change, so they are measurable at once.
2. Then #1, the structural fix for "minimal updates".
3. Then #4 + #3 (NV12 + encode on the worker, with api.json).
4. #8, #9 and #11 alongside.
5. #12 per platform.

---

## (d) For the `wt/video-path` agent (NV12, the GPU path, early downscale, tile-only damage)

**Include:**

1. **NV12 with explicit plane layout.** Y plane w×h. UV plane `ceil(w/2)×ceil(h/2)` interleaved Cb,Cr, with `y_stride`, `uv_stride` and `uv_offset`. `CVPixelBufferGetBytesPerRowOfPlane` pads rows (often to 64 B), so never assume tight packing. Also carry the **range** (video/limited vs full, `420v` vs `420f`) and the **matrix** (BT.601 / 709). Read them from the buffer attachments (`kCVImageBufferYCbCrMatrixKey`) or the stream VUI; do not hard-code them. `decode_vulkan.rs` already reads the signalled colour space, and `android.rs:207-209` hard-codes the wrong one.
2. **WebRender lowering.** One `push_yuv_image(YuvData::NV12(y, uv), ColorDepth::Color8, color_space, color_range, ImageRendering::Auto)` per video item (`webrender/api/src/display_list.rs:1424-1446`). It goes where `DisplayListItem::Image` becomes `push_image` today (`dll/src/desktop/compositor2.rs:1289-1360`); that branch stretches to `bounds` with no object-fit. Y = `ImageFormat::R8` and UV = `ImageFormat::RG8`. `translate_image_format` already maps both (`wr_translate2.rs:1056-1058`). The shaders are there: `webrender/core/res/brush_yuv_image.glsl` and `yuv.glsl`, built for Texture2D and TextureRect (`webrender/build/src/shader_features.rs:186-199`). The CPU compositor also has SWGL YUV shaders, but azul's cpurender is not SWGL, so it needs its own NV12 path.
3. **Stable keys and UpdateImage** for both planes, per (dom, node), exactly like `process_image_callback_updates` (`wr_translate2.rs:2747-2786`). Use `DeleteImage` + `AddImage` only on a size or format change. Otherwise a new `ImageRef` identity per frame keeps today's AddImage churn and forces a scene rebuild (C10).
4. **Do not mark the display list dirty** for a same-size frame (`event.rs:4735`). Frame generation must take `build_image_only_transaction` (`macos/mod.rs:8536-8543`).
5. **Tile-only damage must be visible to tests.** Report WebRender's `results.dirty_rects` in the FrameReport instead of collapsing them to `FrameDamage::Full` (`macos/mod.rs:8680-8690`). Note that CGL still composites the whole window (`wr_translate2.rs:334-345`, no partial present). A real present-damage win on macOS needs a CALayer/IOSurface-backed or Metal path; say so in the report instead of claiming it.
6. **Early downscale at the source**:
   - AVF preset + `videoSettings` width/height (macOS)
   - SCK `width/height` (already done)
   - `VTPixelTransferSession` for renditions
   - `ID3D11VideoProcessor` (Windows)
   - the EGL blit size (Linux)

   Never on the CPU after a full-size readback. One scale per rendition from the capture is fine on a GPU.
7. **A zero-copy seam, even if the first commit copies.** Model a frame as either CPU planes or a native handle: CVPixelBufferRef / IOSurfaceRef, `ID3D11Texture2D`, `AHardwareBuffer`, DMA-BUF fd + modifier. The handle needs retain and release that span WebRender's `lock`/`unlock` (`wr_translate2.rs:360-405`). An NV12 `Vec` is only the portable fallback.
8. **The CPU backend twin.** `capture_common.rs:149-160` documents the shipped bug where a GL-only path froze CPU windows. Every NV12 frame must also render in cpurender: convert only the damaged rect at raster time, or once per frame into a cached BGRA with vImage `vImageConvert_420Yp8_CbCr8ToARGB8888` / a SIMD loop. Test both backends.
9. **api.json through autofix.** `RawImageFormat::NV12`, `VideoFrame { format, strides }` (its docs promise RGBA8 today, `core/src/video.rs:198-205`), and `ScreenCaptureConfig.output_format` already reserves NV12. Keep RGBA the default, so `VideoDecoder::recv_frame` users do not break.

**Avoid:**

- **Any steady-state NV12→RGBA on the CPU.** `decode_vulkan.rs:129-190` and `android.rs:196-212` are the anti-pattern; per-pixel f32 math is especially slow.
- **Scalar indexed per-pixel loops.** If the CPU must convert, use vImage or chunked SIMD.
- **Deep `U8Vec` clones per frame** (`lib.rs:2104`, `lib.rs:2184`, `capture_common.rs:173`). Pass slices or move.
- **Encode or decode on the UI thread**, and `VTCompressionSessionCompleteFrames` per frame.
- **Requesting BGRA from AVFoundation, SCK or the VT decoder** once NV12 exists. It forces a 4:2:0→BGRA conversion and 2.7× the bytes, only to convert back (E5). For reference, even a SIMD `ARGBToNV12` costs ~0.23 ms per 720p frame (libyuv, §R7), and azul's scalar loops are several times that.
- **Letting the Retina preview size drive the camera above the largest rendition** (`required_capture_size`, `capture_common.rs:607-617`). The GPU upscales the preview for free.
- **Aspect distortion in cuts**: `cut` resamples to exactly w×h (`image_scale.rs:242-252`). Crop or letterbox instead.

---

## (e) Proving each win

1. **Per-frame pipeline counters, exposed for tests.** Add a `VideoPipelineStats` (atomics) with these counters:
   - `cpu_full_frame_copies`, `cpu_swizzles`, `cpu_color_conversions`, `cpu_scales`, `frame_allocs`, `bytes_touched`
   - `encode_calls_on_ui_thread`, `decode_calls_on_ui_thread`
   - `wr_add_image`, `wr_update_image`, `scene_rebuilds`

   Serve it through a debug-server op (`get_video_stats`) and the frame report, next to the existing `dl_rebuilds` (`layout/src/window.rs:724`, `layout/src/e2e/full.rs:420-423`). The tests, RED first, are named as sentences:
   - "a same-size remote video frame neither rebuilds the display list nor the scene" (`dl_rebuilds == 0`, `scene_rebuilds == 0` and `wr_update_image == frames` over 30 frames) → #1
   - "a decoded H.264 frame reaches the screen with at most one CPU copy" → #2 (≤1), #4 (0 with IOSurface)
   - "a camera frame reaches the H.264 encoder without a CPU pixel pass" → #3
   - "the preview of a GPU window is not cut on the CPU" (`cpu_scales == 0`) → #5
   - "the encoder and decoder never run on the UI thread" → #3 / #9
   - "an idle shared screen encodes nothing" (SCK `.idle` → 0 encodes) → #8
2. **Hardware checks without TCC**:
   - Extend `videotoolbox_roundtrip` (`videotoolbox.rs:1045`) to assert that `kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder` is true on Apple Silicon. Query it on a session without low-latency mode, which makes it return -12900.
   - Assert that the decoder's output format is `420v`/`420f` and that `CVPixelBufferGetIOSurface != NULL`.
   - Assert that encode never blocks longer than one frame interval.
3. **Pure unit tests (Linux CI)**:
   - The fake-backend loop tests in `capture_common.rs:2110-2350` can count resampler calls. "Two renditions of a 720p capture cost one scale, not two, when one equals the capture size."
   - "A frame arriving while one is in flight is not read."
4. **CPU% in `examples/azul-meet/scripts/two-clients.mjs`.**
   - Today it runs headless test patterns, with no camera, no VT capture and no GPU window (`two-clients.mjs:6-17`), so it cannot see most of this.
   - Add `--measure-cpu <secs>`. Once video flows, sample `ps -o %cpu=,rss= -p <pid>` every second for 20 s for both processes. Better, add a debug-server op returning `getrusage(RUSAGE_SELF)` utime/stime and per-thread times (`task_threads`/`thread_info` on macOS), so the UI thread's share shows.
   - Add a `--gpu-window` mode (not headless, test-pattern source, real VT encode/decode) and log a baseline at `6ae909529`. Every change above reports the before/after mean CPU and UI-thread CPU. Assert a budget once #1–#3 land.
5. **Attribution.** Build with the `probe` feature and add spans around C2/C5/C6/E4/E7/R4/R6/R7/C9/C10 (`layout/src/probe.rs`). `AZ_PROFILE=cpu` prints per-phase averages and p99s. For a flame graph, run `xcrun xctrace record --template 'Time Profiler' --attach <pid> --time-limit 20s`. For energy, `sudo powermetrics --samplers cpu_power,gpu_power -i 1000`.

---

## §R Web research

All sources were fetched on **2026-09-30**. Anything that could not be confirmed from a primary source is marked
UNVERIFIED.

### R1. macOS / iOS: capture → VideoToolbox → display

**R1.1 Encoder input pool**
- `VTCompressionSessionGetPixelBufferPool` (macOS 10.8 / iOS 8) "returns a pool that provides ideal source pixel buffers".
- If the attributes cannot be reconciled, "VideoToolbox converts each CVImageBuffer internally". The pool can change when session properties change, so fetch it again.
- https://developer.apple.com/documentation/videotoolbox/vtcompressionsessiongetpixelbufferpool(_:)
- `VTCompressionSessionCreate`: "Using pixel buffers not allocated by VideoToolbox increases the chance that you'll have to copy image data." https://developer.apple.com/documentation/videotoolbox/vtcompressionsessioncreate(allocator:width:height:codectype:encoderspecification:imagebufferattributes:compresseddataallocator:outputcallback:refcon:compressionsessionout:)
- `kCVPixelBufferIOSurfacePropertiesKey` (macOS 10.6): an empty dictionary gives default IOSurface backing. https://developer.apple.com/documentation/corevideo/kcvpixelbufferiosurfacepropertieskey
- libwebrtc's `RTCVideoEncoderH264` passes `{PixelFormatType: 420f/420v, IOSurfaceProperties: {}, OpenGLCompatibility: YES}` as source attributes, and hands an `RTCCVPixelBuffer` straight to VT unless it must be cropped. https://webrtc.googlesource.com/src/+/refs/heads/main/sdk/objc/components/video_codec/RTCVideoEncoderH264.mm

**R1.2 Camera format**
- `AVCaptureVideoDataOutput.videoSettings = @{}` delivers the device-native format; `nil` delivers a default uncompressed format. https://developer.apple.com/documentation/avfoundation/avcapturevideodataoutput/videosettings
- Chrome's AVFoundation capturer defaults to `kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange` and forwards IOSurface-backed NV12 without a copy (`ReceiveExternalGpuMemoryBufferFrame`). https://github.com/chromium/chromium/blob/main/media/capture/video/apple/video_capture_device_avfoundation.mm
- UNVERIFIED: that built-in Apple Silicon cameras are natively 420v/420f, and that BGRA forces an AVF conversion. It is inferred. One third-party report observed 420v from UVC webcams and Continuity Camera (2026-09-26): https://github.com/kortexa-ai/aicamera/issues/71

**R1.3 Encoder specification and properties**

| Key | Availability |
|---|---|
| `kVTVideoEncoderSpecification_EnableLowLatencyRateControl` | macOS 11.3 / iOS 14.5 |
| `kVTCompressionPropertyKey_BaseLayerFrameRateFraction` | macOS 11.3 / iOS 14.5 |
| `kVTProfileLevel_H264_ConstrainedBaseline_AutoLevel` / `_ConstrainedHigh_AutoLevel` | **macOS 12.0 / iOS 15.0** |
| `kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality` | macOS 11.0 / iOS 14.0 |
| `kVTVideoEncoderSpecification_Enable/RequireHardwareAcceleratedVideoEncoder`, `kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder` | macOS 10.9 (iOS 17.4) |
| `kVTCompressionPropertyKey_RealTime` | macOS 10.9; default NULL = unknown |
| `kVTCompressionPropertyKey_MaxFrameDelayCount` | macOS 10.8; default unlimited |

- Sources: https://developer.apple.com/documentation/videotoolbox/kvtvideoencoderspecification_enablelowlatencyratecontrol, …/kvtprofilelevel_h264_constrainedbaseline_autolevel, …/kvtcompressionpropertykey_prioritizeencodingspeedoverquality, …/kvtvideoencoderspecification_requirehardwareacceleratedvideoencoder, …/kvtcompressionpropertykey_realtime, …/kvtcompressionpropertykey_maxframedelaycount
- **Low-latency mode:**
  - The current Apple docs say: infinite GOP, no B-frames, no lookahead, temporal layers, "Only High profiles".
  - WWDC21 session 10158 says: H.264, Constrained Baseline and Constrained High, "one in, one out", up to 100 ms less latency at 720p30, plus long-term references. https://developer.apple.com/videos/play/wwdc2021/10158/
  - The two sources disagree on profiles; test on the target OS.
- **Gotcha:** with low-latency mode on, querying `UsingHardwareAcceleratedVideoEncoder` returns -12900 (Apple forums, May 2024). https://developer.apple.com/forums/thread/751291
- **libwebrtc** sets RealTime, ProfileLevel, AllowFrameReordering=false, MaxKeyFrameInterval(+Duration), AverageBitRate + DataRateLimits and ForceKeyFrame. It does **not** use low-latency mode (RTCVideoEncoderH264.mm, above).
- **Chrome** enables low-latency mode only for hardware + low delay + an SVC-capable profile, sets `MaxFrameDelayCount = 3` and `BaseLayerFrameRateFraction = 0.5` for two temporal layers, and uses `RequireHardwareAcceleratedVideoEncoder`. https://github.com/chromium/chromium/blob/main/media/gpu/mac/vt_video_encode_accelerator_mac.mm

**R1.4 Scaling**
- `VTPixelTransferSessionTransferImage` (macOS 10.8, iOS 16) scales and converts pixel formats. It has `kVTPixelTransferPropertyKey_ScalingMode` (Normal / CropSourceToCleanAperture / Letterbox / Trim) and `_RealTime` (10.15). https://developer.apple.com/documentation/videotoolbox/vtpixeltransfersessiontransferimage(_:from:to:)
- UNVERIFIED: that it runs on the GPU or media engine.
- libwebrtc's `RTCCVPixelBuffer` scales NV12 on the CPU with libyuv `NV12Scale`. https://webrtc.googlesource.com/src/+/refs/heads/main/sdk/objc/components/video_frame_buffer/RTCCVPixelBuffer.mm

**R1.5 Decode**
- `kVTDecompressionPropertyKey_RealTime` (macOS 10.10) defaults to true. https://developer.apple.com/documentation/videotoolbox/kvtdecompressionpropertykey_realtime
- `kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder` is macOS 10.9. https://developer.apple.com/documentation/videotoolbox/kvtvideodecoderspecification_enablehardwareacceleratedvideodecoder
- `kCVPixelBufferMetalCompatibilityKey` is macOS 10.11. https://developer.apple.com/documentation/corevideo/kcvpixelbuffermetalcompatibilitykey

**R1.6 Displaying NV12**
- **GL:** Firefox `MacIOSurface::BindTexImage` calls `CGLTexImageIOSurface2D` on `GL_TEXTURE_RECTANGLE_ARB`, with plane 0 as `GL_RED` and plane 1 as `GL_RG` (LUMINANCE / LUMINANCE_ALPHA on a compatibility profile), `GL_UNSIGNED_BYTE` (`UNSIGNED_SHORT` for P010). https://github.com/mozilla-firefox/firefox/blob/main/gfx/2d/MacIOSurface.cpp
- **Metal:** `CVMetalTextureCacheCreateTextureFromImage` (macOS 10.11), with plane 0 as `MTLPixelFormatR8Unorm` and plane 1 as `RG8Unorm`. https://developer.apple.com/documentation/corevideo/cvmetaltexturecachecreatetexturefromimage(_:_:_:_:_:_:_:_:_:)
- **Core Animation:** `AVSampleBufferDisplayLayer` (macOS 10.8). Firefox's NativeLayerCA uses `layer.contents = IOSurface`, or, for video, an `AVSampleBufferDisplayLayer` fed with `CMSampleBufferCreateReadyWithImageBuffer`. https://github.com/mozilla-firefox/firefox/blob/main/gfx/layers/NativeLayerCA.mm
- UNVERIFIED: IOSurface as `CALayer.contents` is not documented by Apple.

### R2. ScreenCaptureKit (macOS 12.3+)
- **SCStreamConfiguration:** width, height, scalesToFit, sourceRect, destinationRect, pixelFormat (`'BGRA'`, `'l10r'`, `'420v'`, `'420f'`), colorMatrix, colorSpaceName, showsCursor, queueDepth, minimumFrameInterval. The class dates from 12.3; `capturesAudio` is 13.0 and `captureResolution` is 14.0.
  - queueDepth: default and minimum 3, maximum 8. minimumFrameInterval: 0 means the maximum rate.
  - https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration (and …/pixelformat, …/queuedepth, …/minimumframeinterval)
- **SCStreamFrameInfo keys:** status, displayTime, scaleFactor, contentScale, contentRect, boundingRect, screenRect, dirtyRects.
- **SCFrameStatus:** complete, idle ("the display didn't change"), blank, suspended, started, stopped. https://developer.apple.com/documentation/screencapturekit/scstreamframeinfo, …/scframestatus
- **WWDC22 "Take ScreenCaptureKit to the next level"** https://developer.apple.com/videos/play/wwdc2022/10155/
  - `420v` for encoding, BGRA for display.
  - queueDepth 3–8, with 5 for 4K60.
  - Return surfaces within `minimumFrameInterval × (queueDepth − 1)`, or frames are lost.
  - "use dirty rects to only encode and transmit the regions with new updates".
  - Crop by contentRect and correct with contentScale / scaleFactor.
- **Chrome** (`ScreenCaptureKitDeviceMac`): `420v`, `scalesToFit = NO`, `showsCursor = YES`, and the IOSurface is forwarded via `CVPixelBufferGetIOSurface` without a copy. https://github.com/chromium/chromium/blob/main/content/browser/media/capture/screen_capture_kit_device_mac.mm
- **WebRTC `ScreenCapturerSck`** (macOS 14+):
  - Uses BGRA and processes only Complete/Started frames.
  - When any dirty rect is non-empty it marks the **whole** frame updated (TODO crbug 327458809).
  - Wraps the IOSurface (`DesktopFrameIOSurface`) without copying.
  - https://webrtc.googlesource.com/src/+/refs/heads/main/modules/desktop_capture/mac/screen_capturer_sck.mm
- **Deprecations:**
  - `CGWindowListCreateImage` is obsoleted in the macOS 15 SDK. https://trac.macports.org/ticket/71136
  - `CGDisplayStream*` is unavailable in the macOS 15 SDK (FreeRDP build log, 2024-09-04). https://github.com/FreeRDP/FreeRDP/issues/10558
  - azul already uses SCK, which is correct.

### R3. Windows
- **Hardware H.264 MFTs** are asynchronous (`MF_TRANSFORM_ASYNC`) and carry `MFT_ENUM_HARDWARE_URL_Attribute`. Enumerate them with `MFTEnumEx(MFT_ENUM_FLAG_HARDWARE)`. https://learn.microsoft.com/en-us/windows/win32/medfound/hardware-mfts
- **D3D11 input:** `MF_SA_D3D11_AWARE` (Windows 8), then `MFT_MESSAGE_SET_D3D_MANAGER` with an `IMFDXGIDeviceManager`. https://learn.microsoft.com/en-us/windows/win32/medfound/mf-sa-d3d11-aware
- **Low latency:** `CODECAPI_AVLowLatencyMode` / `MF_LOW_LATENCY` (Windows 8): one input gives one output, with no reorder delay. https://learn.microsoft.com/en-us/windows/win32/medfound/codecapi-avlowlatencymode
- **Inbox H.264 encoder:** input NV12 / I420 / YUY2; `AVEncVideoForceKeyFrame`, `AVEncMPVGOPSize`, `AVEncMPVDefaultBPictureCount` = 0, CBR. https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-encoder
- **Windows.Graphics.Capture:**
  - `Direct3D11CaptureFramePool.CreateFreeThreaded` is Windows 10 1809.
  - `GraphicsCaptureSession.DirtyRegionMode`, `Direct3D11CaptureFrame.DirtyRegions` and `MinUpdateInterval` need SDK 26100 (Windows 11 24H2).
  - https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturedirtyregionmode
- **DXGI duplication:** `IDXGIOutputDuplication::GetFrameDirtyRects` / `GetFrameMoveRects`; apply the move rects first. https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutputduplication-getframedirtyrects
- **GPU colour conversion and scaling:** `ID3D11VideoContext::VideoProcessorBlt`. https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11videocontext-videoprocessorblt
- **Camera:** `MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING` inserts a video processor for YUV→RGB32 and resizing; "some conversions can be performed in hardware". https://learn.microsoft.com/en-us/windows/win32/medfound/mf-source-reader-enable-advanced-video-processing

### R4. Linux
- **xdg-desktop-portal ScreenCast v6:** `cursor_mode` Metadata=4, `persist_mode` / `restore_token`. https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html
- **PipeWire DMA-BUF:** negotiate `SPA_FORMAT_VIDEO_modifier` (MANDATORY | DONT_FIXATE); the producer fixates the modifier; set `dataType = 1<<SPA_DATA_DmaBuf`. https://github.com/PipeWire/pipewire/blob/master/doc/dox/internals/dma-buf.dox
- **Metadata:** `SPA_META_VideoDamage` is an array of `spa_meta_region`; there is also `SPA_META_Cursor`. https://github.com/PipeWire/pipewire/blob/master/spa/include/spa/buffer/meta.h
  - UNVERIFIED: which compositors fill in the damage (xdg-desktop-portal-wlr does).
- **VA-API:**
  - Import DMA-BUF with `vaCreateSurfaces` + `VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2` + `VADRMPRIMESurfaceDescriptor`. https://github.com/intel/libva/blob/master/va/va_drmcommon.h
  - Encode with `VAEntrypointEncSliceLP` and `VAProfileH264ConstrainedBaseline`. https://lists.freedesktop.org/archives/libva/2016-May/003935.html
- **V4L2:** stateful M2M encoders exist (https://docs.kernel.org/userspace-api/media/v4l/dev-encoder.html). The stateless encoder uAPI was still RFC in 2025.

### R5. Android
- `MediaCodec.createInputSurface()`: call after configure and before start. "Surface uses native video buffers without mapping or copying them to ByteBuffers; thus, it is much more efficient." https://developer.android.com/reference/android/media/MediaCodec
- **Format keys:**
  - `KEY_LATENCY` (API 26, video encoders only, in frames)
  - `KEY_PRIORITY` (API 23, 0 = realtime)
  - `KEY_LOW_LATENCY` (API 30, **decoders**): https://source.android.com/docs/core/media/low-latency-media
- **MediaProjection:** `createVirtualDisplay(..., surface)` onto a MediaCodec, ImageReader or SurfaceTexture surface. Android 14 allows one `createVirtualDisplay` per projection token. https://developer.android.com/media/grow/media-projection
- **Display:** `AHardwareBuffer` (API 26) → `eglGetNativeClientBufferANDROID` → `eglCreateImageKHR` → `glEGLImageTargetTexture2DOES`. https://developer.android.com/ndk/reference/group/a-hardware-buffer

### R6. WebRender
- **Upstream (servo/webrender, crate 0.62):**
  - `push_yuv_image(common, bounds, YuvData, ColorDepth, YuvColorSpace, ColorRange, ImageRendering)`
  - `YuvData::{NV12, P010, NV16, PlanarYCbCr, InterleavedYCbCr}`
  - `ImageBufferKind::{Texture2D, TextureRect, TextureExternal, TextureExternalBT709}`
  - `ExternalImageHandler::{lock, unlock}` with `channel_index` per plane
  - `Transaction::update_image(key, descriptor, data, &DirtyRect::{All, Partial})`: "lets WebRender optimize the amount of data to transfer to the GPU. The data provided must still represent the entire image."
  - https://github.com/servo/webrender/blob/main/webrender_api/src/display_list.rs, …/display_item.rs, …/image.rs, https://github.com/servo/webrender/blob/main/webrender/src/render_api.rs
- **The vendored copy has the same API:**
  - `webrender/api/src/display_item.rs:2011-2105` (`YuvImageDisplayItem`, `YuvData::NV12`, `YuvRangedColorSpace`)
  - `webrender/api/src/display_list.rs:1424-1446` (`push_yuv_image`)
  - `webrender/core/res/brush_yuv_image.glsl` + `yuv.glsl`
  - optimized-shader features `webrender/build/src/shader_features.rs:186-199`
  - runtime support: Texture2D always; TextureRect on desktop GL; TextureExternal on GLES; ESSL3 for YUV (`webrender/core/src/renderer/shade.rs:50-62, 994-1037`)
  - the native-compositor YUV surfaces are in `webrender/core/src/composite.rs`
- **Firefox's IOSurface NV12 path:**
  - plane 0 = `R8` and plane 1 = `RG8` external images (`TextureHandle(mIOSurfaceImageKind)`), then `PushNV12Image(…, ColorDepth::Color8, colorspace, range)`. https://github.com/mozilla-firefox/firefox/blob/main/gfx/layers/opengl/MacIOSurfaceTextureHostOGL.cpp
  - `RenderMacIOSurfaceTextureHost::Lock` binds each plane via `BindTexImage` and returns pixel-space UVs, as rectangle textures need. https://github.com/mozilla-firefox/firefox/blob/main/gfx/webrender_bindings/RenderMacIOSurfaceTextureHost.cpp
  - This is the template for azul's `ExternalImageHandler` (`dll/src/desktop/wr_translate2.rs:360-405`), which today only serves Texture2D GL ids.

### R7. How WebRTC and Chrome structure it, with numbers
- **Frame buffers:** `VideoFrameBuffer::Type::{kNative, kI420, …, kNV12}`. The default `CropAndScale` "works by converting to I420", which native buffers override. https://webrtc.googlesource.com/src/+/refs/heads/main/api/video/video_frame_buffer.h
- **Simulcast:** `SimulcastEncoderAdapter` scales **every layer from the original input** (not a cascade), and skips scaling for kNative buffers when the encoder `supports_native_handle`. https://webrtc.googlesource.com/src/+/refs/heads/main/media/engine/simulcast_encoder_adapter.cc
- **Capture at the needed size:** `VideoAdapter::AdaptFrameResolution` applies sink wants at the source. https://chromium.googlesource.com/external/webrtc/+/master/media/base/video_adapter.h
- **Zoom** documents only user toggles ("Use hardware acceleration for …"); the pipeline is not public. https://support.zoom.com/hc/en/article?id=zm_kb&sysparm_article=KB0063824
- **libyuv on x64 at 1280×720** (F. Barchard, 2020-04-16): ARGBToNV12 0.23 ms, I420ToNV12 0.11 ms, YUY2ToNV12 0.14 ms per frame. https://groups.google.com/g/discuss-libyuv/c/SCQSQYQf5tw/m/sSwNIFssBAAJ
  - So a well-vectorised conversion is ~0.2 ms. azul's scalar loops are several times that, and the right answer is not to convert at all.
