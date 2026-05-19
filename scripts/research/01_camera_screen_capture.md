# 01 — Camera capture + screen sharing across 5 platforms

**Scope:** research-only inventory for SUPER_PLAN_2 §1 items #2 (camera) and #3 (screen sharing). Produces the implementation brief for the next session. Web/W3C primitives are noted so a future WASM backend has a target.

**Architecture anchors (from §0 of SUPER_PLAN_2):**
- `NodeType::Image(ImageRef)` — `core/src/dom.rs` line ~629. The proposed `Video`/`CameraPreview` node reuses the `ImageRef` shallow-pointer machinery; every captured frame writes into the *same* `DecodedImage::Raw((descriptor, data))` so WebRender treats it as an `ImageKey` update, not a new resource.
- Manager pattern — `layout/src/managers/` (gesture, focus, scroll, …). New: `camera.rs`, `screen_capture.rs`.
- Injection seam — `dll/src/desktop/shell2/<platform>/mod.rs`. Pattern: native callback → `manager.inject_native_<event>` → `detect_*` accessor reads override slot, clears it. See `GestureAndDragManager::inject_native_gesture` at `layout/src/managers/gesture.rs:501`.
- CallbackInfo — `layout/src/callbacks.rs` line ~741. New accessors: `get_camera_frame(stream_id)`, `get_screen_capture_state(stream_id)`.
- `RawImageFormat` — `core/src/resources.rs:693`. Already has `BGRA8`/`RGBA8`/`RGB8`. NV12 / YUV420 is **not** present and needs to be added (or converted at capture time).

---

## A. CAMERA — per platform

### A.1 iOS — `AVFoundation` / `AVCaptureSession`

- **Framework:** `AVFoundation.framework`.
- **Entry points:**
  - `AVCaptureSession` (the session owns inputs + outputs). https://developer.apple.com/documentation/avfoundation/avcapturesession
  - `AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .front | .back)` to pick a camera. https://developer.apple.com/documentation/avfoundation/avcapturedevice
  - `AVCaptureDeviceInput(device: …)` wraps the device into a session input.
  - `AVCaptureVideoDataOutput` for raw frames, delegate `AVCaptureVideoDataOutputSampleBufferDelegate.captureOutput(_:didOutput:from:)`. https://developer.apple.com/documentation/avfoundation/avcapturevideodataoutput
  - Each delegate call hands a `CMSampleBuffer`; extract `CVPixelBuffer` via `CMSampleBufferGetImageBuffer`. https://developer.apple.com/documentation/coremedia/cmsamplebuffer
- **Permission strings (Info.plist):**
  - `NSCameraUsageDescription` — required, must be human-readable. https://developer.apple.com/documentation/bundleresources/information_property_list/nscamerausagedescription
  - Trigger prompt via `AVCaptureDevice.requestAccess(for: .video) { granted in … }`. https://developer.apple.com/documentation/avfoundation/avcapturedevice/1624584-requestaccess
- **Frame format:** `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` (NV12) by default; opt-in `kCVPixelFormatType_32BGRA` via `videoSettings`. NV12 is the cheap path — BGRA forces a hardware colour-convert on capture. https://developer.apple.com/documentation/corevideo/kcvpixelformattype_32bgra
- **Delivery shape:** synchronous delegate callback on a user-supplied `DispatchQueue`. Set via `setSampleBufferDelegate(_:queue:)`.
- **Existing Rust crates:**
  - [`nokhwa`](https://crates.io/crates/nokhwa) — supports iOS via `objc2` bindings; v0.10+ has an `input-avfoundation` feature. TODO: verify nokhwa iOS support is current (last check 0.10.x; may still be desktop-AVFoundation only). https://github.com/l1npengtul/nokhwa
  - [`objc2-av-foundation`](https://crates.io/crates/objc2-av-foundation) — auto-generated bindings (preferred, low-level, no opinion). https://docs.rs/objc2-av-foundation
  - [`cidre`](https://github.com/yury/cidre) — modern Rust Apple framework bindings with `AVCaptureSession` coverage.

### A.2 Android — `CameraX` (preferred) / `Camera2`

- **Framework:** `androidx.camera:camera-core`, `camera-camera2`, `camera-lifecycle`, `camera-view`. https://developer.android.com/training/camerax
- **Entry points:**
  - `ProcessCameraProvider.getInstance(context)` returns a `ListenableFuture<ProcessCameraProvider>`. https://developer.android.com/reference/androidx/camera/lifecycle/ProcessCameraProvider
  - `ImageAnalysis.Builder().setOutputImageFormat(OUTPUT_IMAGE_FORMAT_YUV_420_888 | RGBA_8888).build()`. https://developer.android.com/reference/androidx/camera/core/ImageAnalysis
  - `ImageAnalysis.Analyzer.analyze(ImageProxy)` — invoked on a user-supplied `Executor`. `ImageProxy.getPlanes()` returns three `Plane`s for YUV. https://developer.android.com/reference/androidx/camera/core/ImageAnalysis.Analyzer
  - For preview-only: `Preview.SurfaceProvider` writes directly to a `Surface`; if Azul renders the preview itself (we do), prefer `ImageAnalysis` so we own the bytes.
  - Lower-level alternative: `android.hardware.camera2.CameraManager.openCamera(...)`. https://developer.android.com/reference/android/hardware/camera2/CameraManager
- **Permission strings (AndroidManifest.xml):**
  - `<uses-permission android:name="android.permission.CAMERA" />` — https://developer.android.com/reference/android/Manifest.permission#CAMERA
  - `<uses-feature android:name="android.hardware.camera" android:required="false" />` to avoid forcing the feature.
  - Runtime prompt: `ActivityCompat.requestPermissions(activity, new String[]{Manifest.permission.CAMERA}, REQ_CODE)` on API 23+. https://developer.android.com/training/permissions/requesting
- **Frame format:** `ImageFormat.YUV_420_888` (default; three-plane, NV12-equivalent layout but with `pixelStride` you must check). `RGBA_8888` available on `ImageAnalysis` since CameraX 1.1. https://developer.android.com/reference/androidx/camera/core/ImageAnalysis#OUTPUT_IMAGE_FORMAT_RGBA_8888
- **Delivery shape:** JVM callback on an `Executor` chosen at registration time. Bridge: Azul's JNI shim (the project already has one for IME) pushes the bytes through `inject_camera_frame(stream_id, bytes, w, h, format, ts)`.
- **Existing Rust crates:**
  - [`ndk-camera`](https://docs.rs/ndk/latest/ndk/camera/index.html) — wraps NDK Camera2 (`<camera/NdkCameraManager.h>`); avoids JNI for the hot path. https://developer.android.com/ndk/reference/group/camera
  - `nokhwa` has no Android backend yet.

### A.3 macOS — `AVFoundation` / `AVCaptureSession`

- **Framework:** same as iOS (`AVFoundation.framework`); some entry points differ.
- **Entry points:** `AVCaptureSession` + `AVCaptureDeviceInput` + `AVCaptureVideoDataOutput` (identical names). https://developer.apple.com/documentation/avfoundation/avcapturesession
  - Device discovery: `AVCaptureDevice.DiscoverySession(deviceTypes: [.builtInWideAngleCamera, .externalUnknown], mediaType: .video, position: .unspecified)`. https://developer.apple.com/documentation/avfoundation/avcapturedevice/discoverysession
  - On Apple Silicon, Continuity Camera makes an iPhone show up as an `AVCaptureDevice` for free. TODO: verify it's transparent or whether we need to opt-in.
- **Permission strings:**
  - Info.plist: `NSCameraUsageDescription` (same string as iOS). https://developer.apple.com/documentation/bundleresources/information_property_list/nscamerausagedescription
  - Entitlement (sandbox apps only): `com.apple.security.device.camera` — https://developer.apple.com/documentation/bundleresources/entitlements/com_apple_security_device_camera
  - Hardened-runtime apps: `NSCameraUsageDescription` is required even outside the sandbox or capture is denied.
  - Trigger via `AVCaptureDevice.requestAccess(for: .video) { granted in … }` (same as iOS).
- **Frame format:** `kCVPixelFormatType_422YpCbCr8` (YUYV) is common from older USB cams; NV12 + BGRA both supported. Set explicitly via `videoSettings`. Same `CVPixelBuffer` → `CMSampleBuffer` pipeline.
- **Delivery shape:** delegate callback on a dispatch queue.
- **Existing Rust crates:**
  - `nokhwa` (input-avfoundation feature) is mature on macOS. https://docs.rs/nokhwa
  - `objc2-av-foundation` for hand-rolled control.
  - `cidre` for high-level wrappers.

### A.4 Linux — PipeWire (preferred) / V4L2 direct

- **PipeWire (preferred for sandboxed apps, Wayland-friendly):**
  - Wire protocol via the `xdg-desktop-portal` `org.freedesktop.portal.Camera` interface. https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Camera.html
  - Flow: app calls `Camera.AccessCamera()` over D-Bus → portal asks user → on grant, app receives a PipeWire fd via `Camera.OpenPipeWireRemote()` → app connects to the PipeWire daemon and consumes a stream node.
  - Library: [`pipewire-rs`](https://gitlab.freedesktop.org/pipewire/pipewire-rs) (still pre-1.0; canonical Rust binding). https://crates.io/crates/pipewire
- **V4L2 direct (no portal, non-sandboxed; legacy):**
  - `/dev/video*` device nodes; `VIDIOC_QUERYCAP`, `VIDIOC_S_FMT`, `VIDIOC_REQBUFS`, `VIDIOC_QBUF`, `VIDIOC_DQBUF`, `VIDIOC_STREAMON`. https://www.kernel.org/doc/html/latest/userspace-api/media/v4l/v4l2.html
  - Rust crate: [`v4l`](https://crates.io/crates/v4l). Synchronous frame dequeue or `mmap`-based zero-copy.
  - `nokhwa` uses V4L2 on Linux out of the box.
- **Permission strings:**
  - Plain V4L2: user must be in the `video` group (no API-level prompt).
  - Portal: D-Bus name `org.freedesktop.portal.Desktop`, object `/org/freedesktop/portal/desktop`, interface `org.freedesktop.portal.Camera`. The portal renders its own prompt; no manifest declaration needed but the app should advertise `--talk-name=org.freedesktop.portal.Desktop` in Flatpak's `manifest.json`. https://docs.flatpak.org/en/latest/sandbox-permissions.html
- **Frame format:** V4L2 driver-dependent. Most webcams expose `V4L2_PIX_FMT_YUYV` (4:2:2 packed), some MJPEG. PipeWire negotiates via SPA POD: typical `SPA_VIDEO_FORMAT_NV12` / `RGBA`. PipeWire camera nodes generally publish multiple formats; choose at stream-connect time.
- **Delivery shape:**
  - V4L2: blocking `select()`/`poll()` on the fd; we'd run a worker thread per stream.
  - PipeWire: callback into the PipeWire main loop thread. Either pin one PW loop per Linux backend or marshal frames to the Azul event loop via the existing crossbeam channels.
- **Existing Rust crates:**
  - `pipewire-rs` (https://docs.rs/pipewire) — bindings, manual loop.
  - `nokhwa` (V4L2 only).
  - `gstreamer-rs` (https://crates.io/crates/gstreamer) — heavy but covers both V4L2 and PipeWire via `pipewiresrc`.
  - [`ashpd`](https://crates.io/crates/ashpd) — high-level XDG portal binding (Camera, ScreenCast, FileChooser). Strongly recommended for the portal flow. https://docs.rs/ashpd

### A.5 Windows — `Windows.Media.Capture.MediaCapture` (WinRT)

- **Framework:** WinRT `Windows.Media.Capture` namespace. Win32 fallback is Media Foundation (`IMFSourceReader`).
- **Entry points:**
  - `MediaCapture` class — `InitializeAsync(MediaCaptureInitializationSettings)` selects device + media-stream profile. https://learn.microsoft.com/en-us/uwp/api/windows.media.capture.mediacapture
  - `MediaFrameReader` — `CreateFrameReaderAsync(sourceInfo)` + `FrameArrived` event for raw frames. https://learn.microsoft.com/en-us/uwp/api/windows.media.capture.frames.mediaframereader
  - Source enumeration: `MediaFrameSourceGroup.FindAllAsync()`. https://learn.microsoft.com/en-us/uwp/api/windows.media.capture.frames.mediaframesourcegroup
  - Media Foundation alternative (Win32): `MFCreateSourceReaderFromMediaSource`, `IMFSourceReader::ReadSample`. https://learn.microsoft.com/en-us/windows/win32/medfound/source-reader
- **Permission strings:**
  - Packaged apps (MSIX / store): declare `<DeviceCapability Name="webcam"/>` in `Package.appxmanifest`. https://learn.microsoft.com/en-us/windows/uwp/packaging/app-capability-declarations
  - Unpackaged Win32 apps (our default): respect the Settings → Privacy → Camera toggle. As of Win10 2004, even desktop apps are subject to it; the OS returns access-denied at `InitializeAsync`. No manifest entry, but we must surface a clear error code. https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/capture-photos-and-video-with-mediacapture
  - WinRT trigger: `AppCapability.RequestAccessForCapabilitiesAsync(...)` is *only* for packaged apps. Otherwise pop the OS settings deep link `ms-settings:privacy-webcam`.
- **Frame format:** `MediaEncodingSubtypes` includes `Nv12`, `Yuy2`, `Rgb32`, `Argb32`. The selected profile is enumerated via `MediaFrameSourceInfo.VideoProfileMediaDescription`. Sample default: NV12 for built-in webcams.
- **Delivery shape:** `MediaFrameReader.FrameArrived` is a WinRT event handler invoked on a thread-pool thread; backing frame is a `SoftwareBitmap` (CPU) or `Direct3DSurface` (GPU). https://learn.microsoft.com/en-us/uwp/api/windows.media.capture.frames.mediaframereader.framearrived
- **Existing Rust crates:**
  - [`windows`](https://crates.io/crates/windows) crate from MS — official WinRT bindings, covers `Windows::Media::Capture::*`. https://microsoft.github.io/windows-docs-rs/doc/windows/Media/Capture/
  - `nokhwa` uses Media Foundation on Windows.
  - `escapi` (legacy DirectShow) — avoid.

---

## B. SCREEN SHARING — per platform

### B.1 iOS — `ReplayKit` (severely constrained)

- **Framework:** `ReplayKit.framework`. https://developer.apple.com/documentation/replaykit
- **Entry points:**
  - In-app capture: `RPScreenRecorder.shared().startCapture(handler: { sampleBuffer, bufferType, error in … })`. Delivers `CMSampleBuffer`s of types `.video`, `.audioApp`, `.audioMic`. https://developer.apple.com/documentation/replaykit/rpscreenrecorder/1620843-startcapture
  - System-wide capture (cross-app, requires a Broadcast Upload Extension target): `RPBroadcastSampleHandler.processSampleBuffer(_:with:)`. https://developer.apple.com/documentation/replaykit/rpbroadcastsamplehandler
- **iOS limitation (the big one):**
  - `startCapture` only sees the *calling app's* surface. To capture other apps the user must trigger a system-level broadcast via Control Center → Screen Recording → pick our extension. The extension runs in a separate process with a 50 MB memory cap. https://developer.apple.com/documentation/replaykit/creating-a-broadcast-upload-extension
  - This means Azul cannot offer a generic "share my whole device screen" feature inside a single-process app. Mark as **in-app-only** in the API.
- **Permission flow:**
  - For in-app capture: `RPScreenRecorder.isAvailable` + automatic system alert at `startCapture` time. No Info.plist key strictly required, but `NSMicrophoneUsageDescription` if `microphoneEnabled = true`.
  - For broadcast extension: separate target with `NSExtensionPointIdentifier = com.apple.broadcast-services-upload`.
- **Frame format:** `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` from the sample buffer.
- **Delivery shape:** delegate-style callback on an Apple-managed queue.
- **Existing Rust crates:** none worth using; needs `objc2` + `objc2-replay-kit` (https://docs.rs/objc2-replay-kit).

### B.2 Android — `MediaProjection`

- **Framework:** `android.media.projection.MediaProjectionManager`. https://developer.android.com/reference/android/media/projection/MediaProjectionManager
- **Entry points:**
  - `MediaProjectionManager mpm = context.getSystemService(MediaProjectionManager.class);`
  - `startActivityForResult(mpm.createScreenCaptureIntent(), REQ_CODE)` → user picks "Start now" or "Cancel" in the system dialog. https://developer.android.com/reference/android/media/projection/MediaProjectionManager#createScreenCaptureIntent()
  - `MediaProjection mp = mpm.getMediaProjection(resultCode, resultIntent);`
  - `mp.createVirtualDisplay(name, w, h, dpi, flags, surface, callback, handler)` writes the screen into a `Surface`. The `Surface` may come from an `ImageReader` (CPU access) or `MediaCodec` (encode). https://developer.android.com/reference/android/media/projection/MediaProjection#createVirtualDisplay(java.lang.String,%20int,%20int,%20int,%20int,%20android.view.Surface,%20android.hardware.display.VirtualDisplay.Callback,%20android.os.Handler)
- **Android 14 change:** apps must register a foreground service of type `mediaProjection` (`<service android:foregroundServiceType="mediaProjection"/>`) before calling `getMediaProjection`, else `SecurityException`. https://developer.android.com/about/versions/14/changes/fgs-types-required#media-projection
- **Permission strings:**
  - `<uses-permission android:name="android.permission.FOREGROUND_SERVICE"/>` (14+).
  - `<uses-permission android:name="android.permission.FOREGROUND_SERVICE_MEDIA_PROJECTION"/>` (14+).
  - The screen-capture user prompt is forced by `createScreenCaptureIntent`; no separate manifest opt-in.
- **Frame format:** whatever the `Surface` sink accepts. `ImageReader.newInstance(w, h, PixelFormat.RGBA_8888, maxImages)` gives BGRA-equivalent CPU bytes (`Image.getPlanes()[0].getBuffer()`). https://developer.android.com/reference/android/media/ImageReader
- **Delivery shape:** `ImageReader.OnImageAvailableListener.onImageAvailable(reader)` on a user-supplied `Handler`.
- **Existing Rust crates:** none direct; JNI bridge from the Android shell.

### B.3 macOS — `ScreenCaptureKit` (12.3+) — preferred; `CGDisplayStream` deprecated

- **Framework:** `ScreenCaptureKit.framework` (macOS 12.3+). https://developer.apple.com/documentation/screencapturekit
- **Entry points:**
  - `SCShareableContent.current(excludingDesktopWindows:onScreenWindowsOnly:completionHandler:)` enumerates displays + windows + apps. https://developer.apple.com/documentation/screencapturekit/scshareablecontent
  - `SCContentFilter(display: SCDisplay, excludingWindows: [SCWindow])` or `SCContentFilter(desktopIndependentWindow: SCWindow)` — the user-selected target. https://developer.apple.com/documentation/screencapturekit/sccontentfilter
  - `SCStreamConfiguration` — frame size, pixel format (`kCVPixelFormatType_32BGRA`), `minimumFrameInterval`, `capturesAudio` (13+).
  - `SCStream(filter:configuration:delegate:)` — start with `startCapture()`, deliver via `SCStreamOutput.stream(_:didOutputSampleBuffer:of:)`. https://developer.apple.com/documentation/screencapturekit/scstream
- **macOS 15 picker:** `SCContentSharingPicker` — system-level shareable-content picker replacing custom UI. https://developer.apple.com/documentation/screencapturekit/sccontentsharingpicker
- **Permission flow:**
  - System Settings → Privacy & Security → Screen Recording. First call to `SCShareableContent.current` triggers the consent prompt and a TCC entry. https://developer.apple.com/documentation/screencapturekit/about-screen-capture-permissions
  - No Info.plist string strictly required (TCC handles it), but bundle must be signed.
  - To re-check programmatically: `CGPreflightScreenCaptureAccess()` / `CGRequestScreenCaptureAccess()` (these predate ScreenCaptureKit but still drive the same TCC entry). https://developer.apple.com/documentation/coregraphics/3946366-cgpreflightscreencaptureaccess
- **Frame format:** BGRA (`kCVPixelFormatType_32BGRA`) most common; 4:2:0 supported too. Frame metadata via `SCStreamFrameInfo` (status, dirty rects, scale factor).
- **Delivery shape:** delegate on a dispatch queue.
- **Existing Rust crates:**
  - [`screencapturekit-rs`](https://crates.io/crates/screencapturekit) — community wrapper. https://github.com/svtlabs/screencapturekit-rs
  - `cidre` has SCK bindings.
  - Generated bindings: `objc2-screen-capture-kit`.

### B.4 Linux — PipeWire / `org.freedesktop.portal.ScreenCast`

- **Framework:** XDG Desktop Portal ScreenCast interface (D-Bus) + PipeWire for the frame transport. https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html
- **Entry points (sequence):**
  1. `CreateSession(options)` → returns a session handle.
  2. `SelectSources(session_handle, options)` with `types` (1=monitor, 2=window, 4=virtual) and `cursor_mode` — user sees the portal's chooser dialog.
  3. `Start(session_handle, parent_window, options)` → on accept, response includes `streams: a(ua{sv})` (PipeWire node IDs + metadata).
  4. `OpenPipeWireRemote(session_handle, options)` → returns a unix fd.
  5. Connect to PipeWire on that fd, attach to the stream nodes by ID, configure SPA POD (preferred format), start streaming.
- **Permission flow:** entirely handled by the portal's dialog. The app declares no a-priori permission; the user picks per-session. Cancel returns `Response::Cancelled`.
- **Frame format:** SPA video formats — `BGRA`, `RGBA`, `NV12`, `YUY2`, `RGBx`. Preferred: `BGRA` for CPU rendering, `NV12` for the GPU-decode path. Sample timestamps via PipeWire buffer metadata. Buffer can be `MemFd` (CPU mmap) or `DMABUF` (GPU import).
- **Delivery shape:** PipeWire `on_process` callback on the PW thread.
- **Existing Rust crates:**
  - `ashpd` — high-level `ScreenCast` portal flow. https://docs.rs/ashpd/latest/ashpd/desktop/screencast/index.html
  - `pipewire-rs` — low-level stream consumer.
  - [`libwayshot`](https://crates.io/crates/libwayshot) — wlr-screencopy fallback for compositors that don't speak the portal (wlroots-based: sway, hyprland). Not portable to GNOME/KDE without the portal.
  - [`xcap`](https://crates.io/crates/xcap) — abstraction across X11 / Win / macOS. TODO: verify it covers PipeWire portal (last check: only X11/XCB on Linux).

### B.5 Windows — `Windows.Graphics.Capture.GraphicsCaptureItem`

- **Framework:** WinRT `Windows.Graphics.Capture` (Win10 1903+). https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture
- **Entry points:**
  - User picker: `GraphicsCapturePicker.PickSingleItemAsync()` returns a `GraphicsCaptureItem` (window or monitor). https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturepicker
  - Programmatic: `GraphicsCaptureItem.CreateForMonitor(HMONITOR)` / `CreateForWindow(HWND)` via `IGraphicsCaptureItemInterop` (Win32 interop). https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/
  - `Direct3D11CaptureFramePool.Create(device, pixelFormat, numberOfBuffers, size)`. https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframepool
  - `framePool.CreateCaptureSession(item)` + `session.StartCapture()`.
  - Frame delivery: `framePool.FrameArrived` event → `framePool.TryGetNextFrame()` returns `Direct3D11CaptureFrame` containing a `Direct3DSurface` (D3D11 texture). https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframepool.framearrived
- **Permission flow:**
  - Picker form: zero prompts — the user's act of picking *is* the consent.
  - Programmatic form: on Win11 24H2+ the system shows a yellow recording border around the captured area; no programmatic gate prior to that.
  - Optional capability declaration for packaged apps: `<DeviceCapability Name="graphicsCapture"/>` enables programmatic capture without the picker. https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture#packaging
- **Frame format:** `DirectXPixelFormat.B8G8R8A8UIntNormalized` (BGRA) — frame lives in a `IDirect3DSurface` (GPU texture). Map for CPU read via `ID3D11Texture2D` + `Map`. Use `MinUpdateInterval` (Win11) to throttle.
- **Delivery shape:** WinRT event on a thread-pool thread; you must call `TryGetNextFrame` immediately or the buffer recycles.
- **Existing Rust crates:**
  - `windows` crate — `Windows::Graphics::Capture::*`. https://microsoft.github.io/windows-docs-rs/doc/windows/Graphics/Capture/index.html
  - [`windows-capture`](https://crates.io/crates/windows-capture) — high-level wrapper, BGRA buffers in callback form. https://github.com/NiiightmareXD/windows-capture
  - `xcap` (BitBlt fallback — slow, captures GDI; avoid for production).

---

## C. INTEGRATION SKETCH FOR AZUL

### C.1 NodeType extension

Add to `core/src/dom.rs:239` (the `NodeType` enum, alongside `Image(BoxOrStatic<ImageRef>)`). Two strategies — pick one:

```rust,ignore
// Option 1: Re-use Image. Camera/screen-capture frames update the underlying
// DecodedImage::Raw in place. NodeType stays unchanged; existing renderer
// already handles ImageRef. CSS uses `image()` URL or a CSS class.
//
// Pros: zero renderer work, zero new variants, zero hashing-contract churn.
// Cons: no semantic distinction in the DOM; `<video>` analog feels off.

// Option 2: New variants (semantic clarity, matches HTML <video>):
NodeType::Video(VideoRef),
NodeType::CameraPreview(CameraRef),
```

`VideoRef` and `CameraRef` would each wrap an `ImageRef` internally so the renderer path stays identical:

```rust,ignore
#[repr(C)]
pub struct CameraRef {
    pub image: ImageRef,            // shared texture — written to by capture thread
    pub stream_id: CaptureStreamId, // identifies which manager-side stream
    pub orientation: CaptureOrientation, // portrait/landscape rotation (0/90/180/270)
}

#[repr(C, u8)]
pub enum CaptureOrientation { Up, Down, Left, Right, Mirror }
```

The capture thread calls `image_ref.write_pixels(...)` — `ImageRef` already has shallow-copy semantics so all clones see the new bytes (subject to the `Arc` ref counting at `core/src/resources.rs:794`). Recommendation: **Option 2** — adds two variants, easy to special-case for orientation + mirroring in the GPU shader, leaves room for stream-state events on the node itself.

**Hashing-contract updates (per `doc/guide/en/internals/dom.md` §"Adding a new field to NodeData"):**
- `Hash for NodeData` — hash `stream_id` + `orientation`, *not* the underlying pixels (those change every frame; the node's logical identity must be stable).
- `compute_node_changes` — new flag `CAMERA_STREAM_CHANGED 0x2000`; map to `RelayoutScope::None` (texture-only update, no layout).
- `NodeDataFingerprint` — fold into `attrs_hash` if cheap.

### C.2 New managers (`layout/src/managers/`)

`layout/src/managers/camera.rs`:

```rust,ignore
pub struct CameraManager {
    pub streams: BTreeMap<CaptureStreamId, CameraStream>,
    pub permission_state: PermissionState,
    pub native_camera_event: Option<NativeCameraEvent>, // injection slot
}

pub struct CameraStream {
    pub id: CaptureStreamId,
    pub config: CameraConfig,
    pub target: ImageRef,           // the shared texture
    pub last_frame_ts: u64,
    pub state: StreamState,         // Starting / Running / Paused / Error
    pub stats: CaptureStats,        // fps, dropped, last_error
}

pub struct CameraConfig {
    pub facing: CameraFacing,       // Front | Back | External
    pub preferred_resolution: (u32, u32),
    pub preferred_fps: u32,
    pub output_format: RawImageFormat, // BGRA8 / NV12 (NV12 needs new variant)
}

#[repr(C, u8)]
pub enum NativeCameraEvent {
    FrameArrived { stream_id: u64, ts: u64, w: u32, h: u32, format: RawImageFormat },
    PermissionGranted,
    PermissionDenied,
    DeviceLost { stream_id: u64 },
    Error { stream_id: u64, code: CaptureErrorCode },
}
```

`layout/src/managers/screen_capture.rs`: symmetric, with `ScreenCaptureSource { Display(u32) | Window(WindowHandle) | Region(Rect) }`.

Both follow the gesture-manager pattern (`layout/src/managers/gesture.rs:435`): a `native_*_event: Option<...>` slot, an `inject_native_*` setter, and a `take_native_*` consumer.

### C.3 CallbackInfo accessors (`layout/src/callbacks.rs`)

```rust,ignore
impl CallbackInfo {
    pub fn get_camera_manager(&self) -> &CameraManager;
    pub fn get_camera_manager_mut(&mut self) -> &mut CameraManager;
    pub fn get_camera_frame(&self, stream_id: CaptureStreamId) -> Option<&CameraStream>;
    pub fn get_screen_capture_state(&self, stream_id: CaptureStreamId) -> Option<&ScreenCaptureStream>;

    // High-level helpers
    pub fn start_camera(&mut self, config: CameraConfig) -> Result<CaptureStreamId, CaptureError>;
    pub fn stop_camera(&mut self, stream_id: CaptureStreamId);
    pub fn pause_camera(&mut self, stream_id: CaptureStreamId);
    pub fn flip_camera(&mut self, stream_id: CaptureStreamId); // front ↔ back
}
```

### C.4 EventFilter variants (`core/src/events.rs:1512`)

Add to `HoverEventFilter` (so a `<video>` node can listen to its own stream):

```text
CameraFrame                  // a new frame arrived on this node's stream
CameraStreamStart            // stream transitioned to Running
CameraStreamStop             // stream stopped (user / device-lost / error)
CameraPermissionGranted
CameraPermissionDenied
ScreenCaptureFrame
ScreenCaptureStart
ScreenCaptureStop
ScreenCapturePermissionGranted
ScreenCapturePermissionDenied
```

`CameraFrame` etc. are *node-targeted* (the listening node is the `<video>` node). Permission events bubble up the tree the same way `DroppedFile` does.

Update `HoverEventFilter::to_focus_event_filter()` (currently at `core/src/events.rs:1647`) to return `None` for these — capture events don't have a focus analogue.

### C.5 Permission API (`dll/src/desktop/app.rs`)

```rust,ignore
impl App {
    pub fn request_camera_permission(
        &mut self,
        prompt: AzString,
    ) -> Result<CameraPermissionResponse, PermissionError>;

    pub fn request_screen_capture_permission(
        &mut self,
        prompt: AzString,
        sources: ScreenCaptureSourceFilter, // Displays | Windows | Both
    ) -> Result<ScreenCapturePermissionResponse, PermissionError>;
}

#[repr(C, u8)]
pub enum PermissionError {
    UserDenied,
    OsRestricted,        // parental controls, MDM
    Unsupported,         // no camera, headless, iOS broadcast-only
    Pending,             // platform prompt is async; deliver via event
    InternalError(AzString),
}

#[repr(C)]
pub struct CameraPermissionResponse {
    pub granted: bool,
    pub devices: CameraDeviceInfoVec, // device IDs, friendly names, facing, sensors
}
```

On iOS / macOS / Android the prompt is async, so the *real* flow is: caller invokes `request_camera_permission` → returns `Pending` → background thread eventually injects `NativeCameraEvent::PermissionGranted` → the `CameraPermissionGranted` event fires next frame.

Platform mapping:

| Platform | Trigger fn | Async? | UI shown |
|---|---|---|---|
| iOS | `AVCaptureDevice.requestAccess(for: .video, …)` | yes | system alert |
| Android | `ActivityCompat.requestPermissions(...)` | yes | system dialog |
| macOS | `AVCaptureDevice.requestAccess(for: .video, …)` | yes | TCC sheet |
| Linux | XDG `Camera.AccessCamera()` (portal) / no prompt for V4L2 | yes / no | portal dialog |
| Windows | `MediaCapture.InitializeAsync(...)` returns access-denied if user blocked it in Settings | no prompt — settings deep-link | none / `ms-settings:privacy-webcam` |

For screen capture:

| Platform | Trigger | UI shown |
|---|---|---|
| iOS | `RPScreenRecorder.startCapture(...)` | system sheet (in-app only) |
| Android | `MediaProjectionManager.createScreenCaptureIntent()` | "Start now / Cancel" |
| macOS | first `SCShareableContent.current(...)` call | TCC sheet; user must add app to Screen Recording in System Settings |
| Linux | portal `SelectSources` + `Start` | portal source-picker (per-session) |
| Windows | `GraphicsCapturePicker.PickSingleItemAsync()` | system picker (or programmatic with optional yellow border in 24H2+) |

### C.6 Platform injection points

`dll/src/desktop/shell2/<platform>/mod.rs` — each backend owns its capture session and calls `app.layout_window.camera_manager.inject_native_event(...)` from the platform callback. Pattern mirrors `inject_native_gesture` in `dll/src/desktop/shell2/ios/mod.rs:370`.

| Platform | File to extend | What it owns |
|---|---|---|
| iOS | `dll/src/desktop/shell2/ios/mod.rs` | `AVCaptureSession` + delegate; `RPScreenRecorder` |
| Android | `dll/src/desktop/shell2/android/mod.rs` | JNI calls into a Kotlin `CameraXBridge` + `MediaProjectionBridge` shipped in the app's APK template |
| macOS | `dll/src/desktop/shell2/macos/mod.rs` | `AVCaptureSession`; `SCStream` (new submodule `shell2/macos/capture.rs`) |
| Linux | `dll/src/desktop/shell2/linux/mod.rs` (covers X11 and Wayland) | `pipewire-rs` stream on a worker thread; `ashpd` for the portal handshake |
| Windows | `dll/src/desktop/shell2/windows/mod.rs` | `MediaCapture` + `Direct3D11CaptureFramePool` |

### C.7 W3C-equivalent primitives (for the future WASM backend)

| Azul | W3C |
|---|---|
| `NodeType::Video(VideoRef)` / `NodeType::CameraPreview(...)` | `<video>` https://html.spec.whatwg.org/multipage/media.html#the-video-element |
| `App::request_camera_permission(...)` | `navigator.mediaDevices.getUserMedia({ video: true })` https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia |
| `App::request_screen_capture_permission(...)` | `navigator.mediaDevices.getDisplayMedia({ video: true })` https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getDisplayMedia |
| `CameraStream` | `MediaStream` + `MediaStreamTrack` https://developer.mozilla.org/en-US/docs/Web/API/MediaStream |
| `On::CameraFrame` | `requestVideoFrameCallback` https://wicg.github.io/video-rvfc/ |
| `PermissionError::UserDenied` | `NotAllowedError` DOMException |
| `PermissionError::Unsupported` | `NotFoundError` / `NotSupportedError` |
| Device enumeration | `navigator.mediaDevices.enumerateDevices()` |

The web backend would lower `CameraRef` to a hidden `<video>` element + `MediaStream` and render it into a WebGL texture, then back through the existing `ImageRef` path.

### C.8 Risks / gotchas

1. **Pixel format mismatch.** Most cameras deliver YUV / NV12 natively. WebRender consumes BGRA/RGBA. Two options:
   - CPU-convert at frame arrival (cheap on modern x86 + SIMD, painful on low-end ARM Android). Crate: `yuvutils-rs` or in-tree SIMD.
   - GPU-convert in a shader (preferred for high-res / 60fps). Needs a new `RawImageFormat::NV12 { y_plane, uv_plane }` variant + a sampling shader. WebRender doesn't currently have YUV samplers; would need a `ImageDescriptor::external_image` route. **TODO: verify** whether WebRender's external-image plumbing already supports YUV (Mozilla used to ship a YUV path for video; ours may have been stripped).
2. **GPU texture upload performance.** At 1080p30, we're uploading ~248 MB/s in BGRA. Mitigation: zero-copy via `IOSurface` (macOS), `AHardwareBuffer` (Android), `IDXGISurface` (Windows), `DMABUF` (Linux PipeWire). Each requires extending `DecodedImage` with a platform-specific external-handle variant.
3. **Threading model.** All five platforms deliver frames on a non-Azul thread. The pattern: platform thread writes into a triple-buffered `ImageRef` (lock-free swap), then signals the Azul event loop to redraw. **Never** call into `LayoutWindow` from the capture thread — only the slot mutation is permitted; the event-loop wakeup happens via the existing `EventLoopProxy` channel.
4. **Energy cost.** A persistent 30 fps camera stream + capture-driven redraw is a battery killer. Manager should expose `CameraConfig::throttle_when_node_invisible: bool` (default true) — when the `<video>` node leaves the viewport, downgrade to 1 fps or stop, like `IntersectionObserver` would. Hook into `IFrameManager`'s visibility tracking.
5. **iOS broadcast-extension architecture.** A separate Xcode target with its own bundle ID is needed; the Azul build system does not currently emit those. Marking iOS screen-capture as "in-app only" for v1 sidesteps this entirely. Cross-process screen sharing is a v2 deliverable.
6. **macOS Screen Recording TCC reset.** If the user revokes Screen Recording permission in System Settings, the app must be relaunched to recover (TCC quirk). Document the failure mode; the manager surfaces `PermissionError::OsRestricted` until the app restarts.
7. **Wayland-without-portal.** GNOME and KDE both ship the portal. wlroots (sway, hyprland) implements it via `xdg-desktop-portal-wlr`. Fully bare compositors (e.g. niri before 0.1.7) may lack it — detect via `org.freedesktop.portal.Desktop` presence and surface `PermissionError::Unsupported`. **TODO: verify niri portal status.**
8. **Android 14 foreground-service requirement.** If we forget the `<service android:foregroundServiceType="mediaProjection"/>` declaration, the screen-capture call dies with a `SecurityException` on first frame. The Android codegen template needs an opt-in mechanism — perhaps a feature flag `azul-android-screen-capture` that injects the manifest snippet.
9. **Windows webcam-privacy hardware kill switch.** On many laptops the F-key hardware mute physically disables the camera before the driver sees it. Surface that as `CaptureErrorCode::HardwareDisabled` rather than a generic error so apps can give a useful message.
10. **NV12 → BGRA SIMD path.** Need to verify what's in-tree. If nothing, recommend `yuv` crate (https://crates.io/crates/yuv) which has AArch64 NEON + AVX2 paths. **TODO: verify.**
11. **macOS `SCShareableContent` deprecation churn.** The `current` static method is being replaced with `excludingDesktopWindows:onScreenWindowsOnly:` in newer SDKs. Pin to the deprecated form for compatibility with 12.3–13.x or runtime-dispatch. **TODO: verify** which is current in Xcode 15.4.

### C.9 api.json + codegen impact

New types reach the 35 binding languages via the existing codegen path (`azul-doc autofix add` + `codegen all`, per SUPER_PLAN_2 §2). Surfaces to add:

- `CameraRef`, `CameraConfig`, `CameraStream`, `CameraDeviceInfo`, `CameraFacing`, `CaptureOrientation`, `CaptureStreamId`, `CaptureErrorCode`
- `ScreenCaptureRef`, `ScreenCaptureSource`, `ScreenCaptureSourceFilter`, `ScreenCaptureStream`
- `PermissionError`, `CameraPermissionResponse`, `ScreenCapturePermissionResponse`
- `HoverEventFilter::CameraFrame` etc. (enum variant additions are FFI-stable as long as the discriminant is non-overlapping; append, don't reorder).
- `CallbackInfo` methods (each becomes a C-ABI function: `AzCallbackInfo_getCameraFrame(...)`).
- `App` methods: `AzApp_requestCameraPermission`, `AzApp_startCamera`, etc.

### C.10 Per-platform implementation cost (rough sizing, no commitment)

| Platform | Camera | Screen capture | Combined LoC est. |
|---|---|---|---|
| iOS | 1 day | 1 day (in-app only) | ~600 |
| macOS | 1 day | 2 days (SCK + TCC) | ~900 |
| Android | 2 days (JNI bridge) | 2 days (JNI + FGS plumbing) | ~1400 |
| Linux | 1 day (V4L2) + 2 days (portal) | 2 days (portal + PipeWire) | ~1200 |
| Windows | 1 day | 1 day | ~600 |
| Shared (managers, NodeType, callbacks, events, codegen) | ~1500 |
| **Total** | | | **~6200 LoC**, ~12 working days |

These are blind estimates; actual delivery depends on YUV/BGRA conversion path (in-tree vs external), zero-copy GPU plumbing (deferred), and codegen coverage iterations.

---

## D. SUMMARY — recommended implementation order

1. **macOS camera** via `nokhwa` (input-avfoundation) — simplest, no permission complexity, validates the manager + NodeType + ImageRef-update pipeline end-to-end.
2. **Windows camera** via `windows` crate `MediaCapture` — proves the WinRT delivery shape works.
3. **Linux camera (V4L2 direct)** via `v4l` crate — proves the non-portal path; provides an early signal on the YUV → BGRA conversion path.
4. **iOS + Android camera** — JNI/Objective-C bridge work; permission flow lands here.
5. **Linux camera (PipeWire portal)** via `ashpd` + `pipewire-rs` — replaces V4L2 for sandboxed flows.
6. **macOS screen capture** via `ScreenCaptureKit` — most polished demo target.
7. **Windows screen capture** via `Graphics.Capture` — picker UX is great, low friction.
8. **Android screen capture** via `MediaProjection` — Android 14 FGS plumbing is the gotcha.
9. **Linux screen capture** via portal — reuses the `ashpd` plumbing from step 5.
10. **iOS screen capture (in-app only)** via `RPScreenRecorder` — defer broadcast-extension to v2.

Cross-platform invariants land first (NodeType + managers + EventFilter + CallbackInfo + permission API). Per-platform backends fill in the injection points incrementally; partial coverage is acceptable — `PermissionError::Unsupported` is a first-class outcome.
