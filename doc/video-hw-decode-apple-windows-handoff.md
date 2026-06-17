# Handoff: native hardware H.264 decode (macOS/iOS + Windows)

Audience: two future agents.
- **macOS/iOS agent** — implement VideoToolbox H.264 decode.
- **Windows agent** — implement Media Foundation H.264 decode.

Goal for both: produce decoded **RGBA8 `VideoFrame`s** through the *exact same*
internal interface the existing Vulkan Video decoder uses, so the demux,
pipeline, FFI, C player, and present/render paths all keep working unchanged.
Vulkan Video stays as the cross-platform fallback on x86_64 Linux/Windows.

All paths below are relative to the repo root `/home/fs/Development/azul`. Every
`file:line` was verified against the tree at the time of writing (commit
`69caee664`, 2026-06-17); line numbers may drift slightly — grep the named symbol
if a number looks off.

---

## 1. Overview

The video decode stack is fully built out *except the codec itself on
non-x86-Linux/Windows*:

- The container demux (MP4 → H.264 Annex-B) is pure Rust and works everywhere.
- The batch pipeline, the streaming widget worker, the C-ABI surface, and the C
  player are all done and call into one small decoder type.
- On **x86_64 Linux/Windows** that decoder is real (Vulkan Video, `gpu-video`).
- On **everything else (incl. all macOS/iOS, all Windows-on-ARM)** the decoder
  field does not even exist — `VideoDecoder` compiles to a **stub that produces
  zero frames**. The demux still runs and reports geometry; no pixels come out.

So your job is narrowly scoped: write one module that opens a native decoder,
feeds it Annex-B chunks, and returns `Vec<VideoFrame>` (RGBA8). Then gate it in
by `cfg` next to the Vulkan one. **You should not need to touch the demuxer, the
pipeline, the FFI/api.json, the C player, or the present/render code.**

### Important gotcha: the capability probe already over-promises on Apple

`probe_hw_decode()` (`dll/src/desktop/extra/video_codec/provision.rs:60-68`)
unconditionally returns `available: true, backend: "VideoToolbox"` on
macOS/iOS, and `backend()` (`mod.rs:52-62`) returns `"VideoToolbox"` too. But
`VideoDecoder::open()` only constructs a real backend under
`cfg(all(feature="video-native", target_arch="x86_64", any(target_os="linux",
target_os="windows")))` (`mod.rs:226-233`). **Result today on a Mac:
`VideoStartupCheck::run()` / `AzCapability_video_codec()` say "READY /
VideoToolbox", yet `decode()` returns nothing.** Closing that gap *is* the
macOS agent's task. (The Windows agent's gap is narrower — x86_64 Windows
already decodes via Vulkan; the MF backend adds Windows-on-ARM and a
non-Vulkan-dependent path.)

---

## 2. Current architecture (with file:line)

### 2.1 The decoder handle and backend selection — `dll/src/desktop/extra/video_codec/mod.rs`

- `fn backend() -> &'static str` (`mod.rs:52-62`): the per-OS string.
  `ios`/`macos` → `"VideoToolbox"`, `android` → `"MediaCodec"`,
  `linux`/`windows` → `"gpu-video"`, else `"none"`. `VideoEncoder::open` /
  `VideoDecoder::open` early-return an invalid handle when this is `"none"`
  (`mod.rs:123`, `mod.rs:220`).

- `struct DecoderInner` (`mod.rs:78-91`). The relevant fields:
  ```rust
  #[cfg(all(feature = "video-native", target_arch = "x86_64",
            any(target_os = "linux", target_os = "windows")))]
  backend: Option<decode_vulkan::VulkanVideoDecoder>,   // mod.rs:84-85
  #[cfg(all(... same cfg ...))]
  pending: std::collections::VecDeque<VideoFrame>,       // mod.rs:89-90
  ```
  `backend = None` means "behave like the stub" (no frames). `pending` buffers
  frames a single `decode()` produced (decode is pipelined + B-frame reordered,
  so one fed chunk can yield 0..N frames) so they can be handed out one per
  `decode`/`next_frame` call.

- `mod decode_vulkan;` is declared under the same cfg (`mod.rs:39-40`).

- `VideoDecoder::open(h265: bool)` (`mod.rs:219-241`): builds `DecoderInner`; the
  `backend` field is initialised at `mod.rs:226-233` — H.265 → `None` (not wired
  yet), H.264 → `decode_vulkan::VulkanVideoDecoder::open_h264()`.

- `VideoDecoder::decode(&self, data: U8Vec) -> OptionVideoFrame`
  (`mod.rs:253-272`): pushes everything `backend.decode(annexb)` returns into
  `pending`, then pops the front (`mod.rs:261-268`).

- `VideoDecoder::next_frame(&self) -> OptionVideoFrame` (`mod.rs:277-285`): pops
  one already-decoded frame, no new input.

- `VideoDecoder::flush(&self) -> OptionVideoFrame` (`mod.rs:290-303`): drains the
  backend at end-of-stream into `pending`, pops the front.

- The handle itself is `#[repr(C)] VideoDecoder { ptr, run_destructor }`
  (`mod.rs:193-197`) — a C-ABI opaque box. You do **not** touch this; you only
  add to `DecoderInner` + `open`/`decode`/`flush` internals.

### 2.2 The real Vulkan backend (your template) — `decode_vulkan.rs`

`dll/src/desktop/extra/video_codec/decode_vulkan.rs` is the reference for the
interface you must replicate:

- `pub struct VulkanVideoDecoder { decoder: gpu_video::BytesDecoder }` (`:29-31`).
- `pub fn open_h264() -> Option<Self>` (`:41-77`): returns `None` on any failure
  so the caller can fall back. **This `Option` return is the fallback hook** —
  see §5.
- `pub fn decode(&mut self, annexb: &[u8]) -> Vec<VideoFrame>` (`:82-93`): feeds
  one Annex-B chunk, maps each decoded `OutputFrame` (NV12) through
  `output_frame_to_rgba`.
- `pub fn flush(&mut self) -> Vec<VideoFrame>` (`:96-104`).
- `fn output_frame_to_rgba(...)` (`:108-122`) and `fn nv12_to_rgba(...)`
  (`:128-185`): the CPU colour conversion. Note it selects the YCbCr matrix from
  the stream-signalled `ColorSpace`/`ColorRange` (BT.601/709 × limited/full) and
  writes tightly-packed RGBA8 with `a = 255`. **Your NV12→RGBA (Windows) /
  NV12-or-BGRA→RGBA (macOS) conversion should follow the same structure**; you
  may even lift `nv12_to_rgba` into a shared helper (see Open Questions).

### 2.3 The demux + pipeline — `demux.rs`, `pipeline.rs`

- `demux.rs` module docs (`:1-14`): gpu-video / VideoToolbox / MF all want the
  H.264 **elementary stream**, not an MP4 container. `demux_mp4_h264`
  (`:57-129`) pulls the AVC track, rewrites each AVCC sample
  (`[u32-BE length][NAL]…`) into **Annex-B** (`00 00 00 01` start codes) via
  `append_avcc_as_annexb` (`:134-146`), and **prepends SPS+PPS before every
  keyframe** (`:106-111`) so a decoder can start on any IDR. `DemuxedH264`
  (`:36-50`) also exposes raw `sps` / `pps` (no start code) and `width/height/fps`
  — useful if a backend wants avcC instead of Annex-B (see §4 macOS).
- `pipeline.rs::decode_mp4_h264_bytes` (`:63-92`): the batch driver. Opens one
  `VideoDecoder` (`:65`), loops the demuxed chunks feeding `decode` + draining
  `next_frame` (`:69-77`), then `flush` + drain (`:79-83`). Returns `DecodedVideo`
  (`:27-40`: `width,height,fps,frames: VideoFrameVec, access_units_fed`).
- The pipeline test `pipeline_demuxes_and_decodes_the_whole_stream`
  (`pipeline.rs:106-157`) is your integration check (see §6).

### 2.4 The frame contract — `core/src/video.rs`

- `#[repr(C)] struct VideoFrame { width: u32, height: u32, bytes: U8Vec }`
  (`core/src/video.rs:58-67`). `bytes` is documented as **tightly-packed RGBA8,
  `width * height * 4`** (`:65-66`). `VideoFrame::new(w,h,bytes)` (`:69-78`).
- FFI `OptionVideoFrame` + `VideoFrameVec` wrappers (`:81-90`).
- `VideoConfig` (`:18-51`) carries `output_format: RawImageFormat` (default
  `BGRA8`, with a comment that `Nv12` is the intended future zero-copy path,
  `:27-29`). This is for the streaming widget; the batch decoder always emits
  RGBA8.

### 2.5 The present / render paths (you do NOT change these)

Once you produce RGBA8 `VideoFrame`s, two consumers already handle them:

- **GPU present** — `layout/src/widgets/capture_common.rs::present_frame`
  (`:86-128`): first frame allocates a GL texture, `upload_rgba` (`:131-144`,
  `tex_image_2d(..., RGBA, UNSIGNED_BYTE, frame.bytes)`), wraps an external-texture
  `ImageRef`, installs it once; later frames re-upload into the same id and
  recomposite. Used by the streaming `VideoWidget` worker.
- **CPU render** — `layout/src/cpurender.rs::render_image` (`:4692`+). It blits a
  `DecodedImage::Raw`. Heads-up: the format match (`:4724-4756`) has explicit
  arms only for `BGRA8` (swizzle) and `R8` (expand); the `_` arm draws a **gray
  placeholder**. RGBA8 frames reach CPU render as `ImageRef::new_rawimage(...,
  RGBA8)`; confirm RGBA8 actually rasterises (see Open Questions) — but this is a
  pre-existing concern shared with today's Vulkan frames, not something your
  backend changes.

### 2.6 The streaming widget — `layout/src/widgets/video.rs`

`VideoWidget` is a "dumb widget": an `<img>` fed by a background thread →
`video_writeback` → `present_frame`. Today the live worker is a **test pattern**
(`video_test_worker`, `:181-217`); a **replay worker** (`video_replay_worker`,
`:226-247`) cycles a caller-supplied `Vec<VideoFrame>` (e.g. from
`decode_mp4_h264_bytes`). Wiring a real *streaming* decode worker that pulls from
your backend frame-by-frame is a possible follow-up, but **not required** for
this task — the batch `decode_mp4_h264_bytes` path is the deliverable and it
already feeds the widget via `with_frames`.

---

## 3. The contract a new backend must satisfy

Implement a struct with **exactly** these three methods (mirror
`VulkanVideoDecoder`):

```rust
pub struct <Platform>VideoDecoder { /* session/transform state */ }

impl <Platform>VideoDecoder {
    /// Try to open a decode-only H.264 decoder. Return None on ANY failure so
    /// the caller can fall back. (This is the fallback hook — see §5.)
    pub fn open_h264() -> Option<Self>;

    /// Feed one Annex-B chunk (one access unit; may carry SPS/PPS on keyframes).
    /// Return frames that became ready, each already converted to tightly-packed
    /// RGBA8 (width*height*4, alpha = 255). May be 0..N (pipelining/reorder).
    pub fn decode(&mut self, annexb: &[u8]) -> Vec<VideoFrame>;

    /// Drain frames still buffered for reordering at end-of-stream.
    pub fn flush(&mut self) -> Vec<VideoFrame>;
}
```

Hard requirements:
1. **Output is RGBA8 `VideoFrame`** (`azul_core::video::VideoFrame::new(w, h,
   U8Vec::from_vec(rgba))`), tightly packed, no row padding, alpha 255. If the
   native buffer has a stride > `width*4` you must de-stride (the AVFoundation
   camera backend does exactly this — see §4).
2. **`open_h264()` returns `None`, never panics**, on missing API / unsupported
   device / decoder-create failure.
3. **Input is Annex-B** (start-code framed, SPS/PPS prepended on keyframes — see
   §2.3). If your API wants avcC/`CMVideoFormatDescription` instead, convert (see
   §4 macOS).
4. No new public/FFI types. `VideoFrame` already crosses the ABI.

---

## 4. Per-platform specifics

### 4.1 macOS / iOS — VideoToolbox

**API shape.** Create a `VTDecompressionSession` from a
`CMVideoFormatDescription`, then for each access unit wrap the compressed bytes
in a `CMBlockBuffer` → `CMSampleBuffer` and call
`VTDecompressionSessionDecodeFrame`. Decoded frames arrive as `CVPixelBuffer`s on
an output callback (push API) — park them in a shared slot and drain in
`decode()`/`flush()`, exactly like the AVFoundation camera delegate does.

**The format problem (must handle).** VideoToolbox does **not** take Annex-B. It
wants:
- a `CMVideoFormatDescription` built from the **avcC** parameter sets, via
  `CMVideoFormatDescriptionCreateFromH264ParameterSets` (pass the raw SPS+PPS —
  the demuxer already exposes them as `DemuxedH264.sps` / `.pps`, raw NAL bytes
  with no start code, `demux.rs:44-47`), and
- sample data in **AVCC** (length-prefixed) form, not start-code form.

So either:
- (a) **Reconstruct avcC from Annex-B inside `decode()`**: split the chunk on
  start codes, build the format description once from the first SPS/PPS you see,
  and rewrite each remaining NAL as `[u32-BE length][NAL]` for the
  `CMBlockBuffer`. (`demux.rs::append_avcc_as_annexb` is the inverse transform —
  read it to see the exact framing.) This keeps the `decode(annexb: &[u8])`
  signature intact, which is the contract. **Recommended.**
- (b) Add an optional avcC-carrying open path. More invasive; avoid unless (a)
  proves painful.

**CVPixelBuffer → RGBA8.** VideoToolbox can be asked (via the destination pixel
buffer attributes / `kCVPixelBufferPixelFormatTypeKey`) to output either
`kCVPixelFormatType_32BGRA` or NV12
(`kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange`). Easiest: request **32BGRA**
and do the cheap BGRA→RGBA channel swap — this is *already implemented* for
camera frames and you can copy it almost verbatim:
`dll/src/desktop/extra/camera/avfoundation.rs:62-100` (lock base address, read
`CVPixelBufferGetWidth/Height/BytesPerRow/BaseAddress`, swizzle BGRA→RGBA
de-striding per row, unlock). If you instead take NV12, port `nv12_to_rgba` from
`decode_vulkan.rs:128-185`.

**Crates.** Already in the tree (confirmed in `dll/Cargo.toml`):
- `objc2` (`:338`, and apple section), `objc2-core-media` (`:407-413`),
  `objc2-core-video` (`:414-419`), `objc2-foundation`, `dispatch2`. All are in
  `_internal_deps` (`Cargo.toml:631-639`) so they link in `build-dll`.
- **Missing — you must add:** `objc2-video-toolbox` (the VideoToolbox bindings;
  NOT present — verified absent from `Cargo.toml` and `Cargo.lock`). Add it as an
  `optional` dep in **both** the `cfg(target_os="ios")` block (`:183`+) and the
  `cfg(target_os="macos")` block (`:335`+), with features for
  `VTDecompressionSession` + `VTDecompressionProperties` + `VTErrors`.
- **Extend `objc2-core-media` features:** it currently enables `CMSampleBuffer`,
  `CMBlockBuffer`, `CMFormatDescription` (`:232-238` ios / `:407-413` macos). Add
  whatever gates `CMVideoFormatDescriptionCreateFromH264ParameterSets`
  (typically still under `CMFormatDescription`/`CMVideoFormatDescription` — check
  the crate) plus `CMTime` if needed for sample timing.
- Add the new crate to the `_internal_deps` list (`Cargo.toml:631-639`, alongside
  `objc2-core-media`) so it's pulled for `build-dll`.

**Reference pattern for objc2 in this codebase:**
`dll/src/desktop/extra/camera/avfoundation.rs` (whole file) — `define_class!`
push-delegate, `Retained<...>`, `CVPixelBuffer` locking, `Arc<Mutex<FrameSlot>>`
hand-off, and the per-OS module gating in `camera/mod.rs:14-15,52-65`. Mirror
this structure.

**cfg gate to use** (note: **no `target_arch` restriction** — VideoToolbox runs
on aarch64 Macs and iOS):
```rust
#[cfg(all(feature = "video-native", any(target_os = "macos", target_os = "ios")))]
mod decode_videotoolbox;
```

### 4.2 Windows — Media Foundation

**API shape.** Use the Media Foundation H.264 decoder MFT
(`CLSID_CMSAACDecMFT`'s video sibling — the H.264 Video Decoder MFT, an
`IMFTransform`). Set input type to `MFVideoFormat_H264`, output type to
`MFVideoFormat_NV12`, then `ProcessInput`(sample wrapping the Annex-B/AVCC bytes)
/ `ProcessOutput`(NV12 sample) in a loop. The decoder is async-capable but the
synchronous `ProcessInput`/`ProcessOutput` drain loop maps cleanly onto
`decode() -> Vec<VideoFrame>`.

**Input format.** The H.264 MFT accepts Annex-B byte-stream directly when the
input subtype is `MFVideoFormat_H264` (it has a built-in start-code parser), so
you can feed the demuxer's chunks as-is — no avcC conversion needed (simpler than
macOS). Verify against your MFT; if it insists on length-prefixed, reuse the same
AVCC-rewrite trick described for macOS.

**NV12 → RGBA8.** `ProcessOutput` hands back NV12 (`IMFMediaBuffer`, lock with
`IMF2DBuffer`/`Lock2D` to get the stride, or `Lock` for contiguous). Port
`nv12_to_rgba` from `decode_vulkan.rs:128-185` (it already exists and is
correct); de-stride if `Lock2D` reports a pitch > width.

**Crates.** The `windows` crate is already a dep (`dll/Cargo.toml:314`, version
`0.62`) and in `_internal_deps` (`Cargo.toml:643`). It currently enables
`Devices_Sensors`, `Foundation`, `Security_Credentials_UI`, `Win32_Foundation`,
`Win32_System_WinRT`, `Win32_System_Com` (`:314-321`) — **no Media Foundation
features yet** (verified). Add the MF feature gates to the `windows` dep:
`Win32_Media_MediaFoundation`, `Win32_Media_KernelStreaming` (for the MF*
GUIDs/structs), and likely `Win32_System_Com` (already on) for COM init.
COM/`IMFTransform` come from `windows` directly — no extra crate. (Do **not** use
`nokhwa` — that's camera-only and pulls a C toolchain; MF decode is pure
`windows`-crate COM.)

**Reference pattern for Windows COM in this codebase:** the camera backend
`dll/src/desktop/extra/camera/windows.rs` (uses `nokhwa`, but shows the
open/read/close handle shape and the feature-gated-stub convention). For raw COM
via the `windows` crate, the motion-sensors backend under
`cfg(target_os="windows")` is another example in-tree.

**cfg gate to use** (no `target_arch` restriction — MF runs on x86_64 and
aarch64 Windows):
```rust
#[cfg(all(feature = "video-native", target_os = "windows"))]
mod decode_mediafoundation;
```
Note this is **broader** than the Vulkan gate (which is x86_64-only). On x86_64
Windows you now have *two* candidates (MF and Vulkan) — see fallback (§5).

---

## 5. Fallback logic

The rule the user specified: **try the OS-native decoder first; if it fails to
open AND the platform is x86 Linux/Windows, fall back to Vulkan Video; else
error (no frames).**

`open_h264()` already returns `Option`, so wiring is just nested `or_else`. The
cleanest change keeps `decode()`/`next_frame()`/`flush()` bodies untouched by
hiding the choice behind one internal enum. Recommended shape inside `mod.rs`:

```rust
// One internal backend the rest of DecoderInner talks to.
#[cfg(all(feature = "video-native", any(
    all(target_arch = "x86_64", any(target_os="linux", target_os="windows")),
    any(target_os="macos", target_os="ios"),
    target_os="windows")))]
enum DecodeBackend {
    #[cfg(all(feature="video-native", target_arch="x86_64",
              any(target_os="linux", target_os="windows")))]
    Vulkan(decode_vulkan::VulkanVideoDecoder),
    #[cfg(all(feature="video-native", any(target_os="macos", target_os="ios")))]
    VideoToolbox(decode_videotoolbox::VideoToolboxDecoder),
    #[cfg(all(feature="video-native", target_os="windows"))]
    MediaFoundation(decode_mediafoundation::MediaFoundationDecoder),
}

impl DecodeBackend {
    fn open_h264() -> Option<Self> {
        // 1. OS-native first.
        #[cfg(all(feature="video-native", any(target_os="macos", target_os="ios")))]
        if let Some(d) = decode_videotoolbox::VideoToolboxDecoder::open_h264() {
            return Some(DecodeBackend::VideoToolbox(d));
        }
        #[cfg(all(feature="video-native", target_os="windows"))]
        if let Some(d) = decode_mediafoundation::MediaFoundationDecoder::open_h264() {
            return Some(DecodeBackend::MediaFoundation(d));
        }
        // 2. Vulkan fallback (x86_64 Linux/Windows only).
        #[cfg(all(feature="video-native", target_arch="x86_64",
                  any(target_os="linux", target_os="windows")))]
        if let Some(d) = decode_vulkan::VulkanVideoDecoder::open_h264() {
            return Some(DecodeBackend::Vulkan(d));
        }
        None
    }
    fn decode(&mut self, annexb: &[u8]) -> Vec<VideoFrame> { /* match self */ }
    fn flush(&mut self) -> Vec<VideoFrame> { /* match self */ }
}
```

Then change `DecoderInner.backend` from
`Option<decode_vulkan::VulkanVideoDecoder>` to `Option<DecodeBackend>`, broaden
its `cfg` (and `pending`'s) from the x86_64-Linux/Windows-only gate to the union
above, and swap the `open` init (`mod.rs:226-233`) to `DecodeBackend::open_h264()`.
`decode`/`next_frame`/`flush` (`mod.rs:253-303`) keep working unchanged because
they only call `.decode()` / `.flush()` on the field.

Notes:
- On **x86_64 Windows**, MF is tried before Vulkan (native first). If you'd
  rather keep Vulkan primary there to avoid churn, reverse those two blocks — but
  the user's rule is "native first", so MF-first is correct.
- On **aarch64 Linux** there's still no backend (neither MF nor VT nor x86
  Vulkan) → `None` → stub. That's expected and out of scope.
- Keep the per-arm `cfg` on the enum variants so each platform compiles only its
  own backend (matches how `decode_vulkan` is gated today).

---

## 6. How to test

### 6.1 Build (after editing `Cargo.toml` + the new module)

No public API changed, so **no api.json / autofix / normalize step is needed**.
Just the codegen + dll rebuild (from MEMORY `azul-codegen-pipeline`):
```
cargo build -r -p azul-doc
target/release/azul-doc codegen all          # writes target/codegen/ (azul.h, dll_api_*.rs, ...)
cargo build -r -p azul-dll --features build-dll   # -> target/release/libazul.so|dylib|dll
```
`build-dll` enables `video-native` (`Cargo.toml:513`). If you add a sub-feature
instead of extending `video-native`, add it to the `build-dll` list too.

Cross-compile sanity (the demux + stub must still build everywhere):
```
cargo check -p azul-dll --features link-static --target aarch64-apple-ios
cargo check -p azul-dll --features link-static --target x86_64-pc-windows-gnu
```

### 6.2 Unit / integration tests

- Demux test (no GPU): `cargo test -p azul-dll demux_big_buck_bunny` — needs
  `/tmp/video-media-samples/big-buck-bunny-480p-30sec.mp4` (soft-skips if absent),
  `demux.rs:182-209`.
- End-to-end decode: the pipeline test
  `pipeline_demuxes_and_decodes_the_whole_stream` (`pipeline.rs:106-157`) runs
  against `/tmp/video-media-samples/big-buck-bunny-360p.mp4`. By default it only
  asserts geometry (frame *production* is hardware-gated). On a real
  decode-capable runner set **`AZ_REQUIRE_HW_DECODE=1`** to force it to assert
  that most access units produced frames (`pipeline.rs:145-156`). Run this on a
  Mac / Windows box once your backend is in.
- Always wrap test runs: `timeout 600 cargo test ...` (MEMORY `cargo-test-timeout`
  — unbounded parallel builds have hard-locked this machine).

### 6.3 The demo apps (real pixels)

- Rust: `examples/azul-video/src/main.rs` — calls `VideoStartupCheck::run()` then
  `decode_mp4_h264_bytes` then plays frames through a timer.
- C: `examples/c/video.c` — same flow over the FFI
  (`AzVideoStartupCheck_run`, `AzDecodedVideo_decodeMp4H264`). Build per the
  header comment (`video.c:18-21`):
  `cc -o video.bin video.c -I../../target/codegen -L../../target/release -lazul`
  then `LD_LIBRARY_PATH=../../target/release ./video.bin`.
  Both prefer the local sample `/tmp/video-media-samples/big-buck-bunny-360p.mp4`
  and fall back to the BBB URL (`video.c:32-33`).

### 6.4 Headless golden snapshot (no display server)

The CPU renderer can dump the first frame to PNG and exit 0
(`dll/src/desktop/shell2/headless/mod.rs:978-1001`):
```
AZ_BACKEND=headless AZ_HEADLESS_SNAPSHOT_PATH=/tmp/x.png ./azul-video-app
```
Useful for CI/golden-image diffing. **Caveat:** this goes through
`cpurender::render_image` — verify RGBA8 actually rasterises there (see §2.5 /
Open Questions); if the snapshot is gray, that's the cpurender format gap, not
your decoder. The on-window GPU path (`present_frame`) is the source of truth for
"do real pixels show".

Other runtime knobs (MEMORY `azul-runtime-debug-knobs`): `AZ_BACKEND`
(`auto`/`gpu`/`cpu`/`headless`), `AZ_LOG`, `AZ_PROFILE`.

---

## 7. Open questions / watch-outs

1. **cpurender RGBA8 arm.** `render_image` (`cpurender.rs:4724-4756`) has explicit
   `BGRA8` and `R8` arms and a gray-placeholder `_` fallback — no visible `RGBA8`
   arm, yet the demos wrap frames as `RGBA8`. Confirm RGBA8 raw images render on
   the CPU path (it may be normalised earlier, or it may be a latent bug). This is
   pre-existing (shared with today's Vulkan frames) and **not** caused by your
   backend, but it affects the headless-snapshot test. If broken, the GPU present
   path still works; flag it separately rather than working around it in the
   decoder.
2. **Make the probe honest.** Once VideoToolbox decode works, `probe_hw_decode`
   on Apple (`provision.rs:60-68`) is finally truthful. If you ship the MF backend
   on Windows-on-ARM, consider whether `provision.rs:78-108` (which only probes
   Vulkan) should also report MF availability there — currently it'd say
   "Vulkan loader not found" on ARM Windows even though MF decodes fine. Low
   priority; coordinate before changing the probe (it drives the driver-install
   UX).
3. **Share `nv12_to_rgba`.** Both the Vulkan backend and the Windows MF backend
   (and the NV12 variant of VideoToolbox) need NV12→RGBA. Consider lifting
   `nv12_to_rgba` (`decode_vulkan.rs:128-185`) into a small shared
   `video_codec/yuv.rs` rather than copying it. Keep the `ColorSpace`/`ColorRange`
   matrix selection — don't hardcode BT.601.
4. **Colour space / range signalling.** The Vulkan path reads colour metadata per
   frame from `gpu-video`. VideoToolbox/MF expose this differently (attachments /
   MT_* attributes). If you can't get it cheaply, default to **BT.601 limited**
   (what `nv12_to_rgba` does for `Unspecified`) — correct for typical SD content,
   acceptable starting point.
5. **H.265.** `VideoDecoder::open(h265=true)` returns `None` backend today
   (`mod.rs:227-230`); the demuxer is H.264-only (`demux.rs` keys on
   `MediaType::H264`). Stay H.264-only unless explicitly asked.
6. **Streaming worker.** This task is the batch decoder. A true frame-by-frame
   streaming `VideoWidget` worker (replacing `video_test_worker`,
   `widgets/video.rs:181-217`) that pulls from your backend live is a separate,
   optional follow-up.
7. **Don't touch the FFI.** `dll/src/unified/video_codec.rs` has a repr-C stub
   `pipeline::DecodedVideo` (`:18-45`) that must stay layout-identical to the real
   one — but you're not adding types, so leave it alone. `api.json` needs no edit.
