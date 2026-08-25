# Platform-integration audit (P2-P8) — desktop + mobile

Status of every device/OS-facing integration, per platform. The mobile (iOS /
Android) backends got the focus; this audit extends the lens to desktop
(macOS / Linux / Windows) and flags every stub + TODO.

Legend: ✅ real native backend · 🔶 stub (test-pattern / no real backend) ·
📝 TODO (file exists, not implemented) · ❌ missing (no-op fallthrough).

> **Progress (this session):** landed Linux **sensors** (iio sysfs), Linux
> **geolocation** (GeoClue2/zbus), Linux **audio** (ALSA playback + capture,
> dlopen), the **capture seam** (camera+screencap pull frames from a registered
> platform backend, test-pattern fallback), and **safe-area/notches** (core +
> accessor + macOS `NSView` + iOS `UIView`) — plus the libudev-sys dlopen fork
> (cross-compile unblock), the `video_codec` API, and the LayoutWindow dedup.
> The matrix reflects these.

| System (P#) | macOS | Linux | Windows | iOS | Android | Backend / notes |
|---|---|---|---|---|---|---|
| **UDP** (P8) | ✅ | ✅ | ✅ | ✅ | ✅ | `std::net::UdpSocket` — portable, no gaps |
| **SQLite `Db`** (P4) | ✅ | ✅ | ✅ | ✅ | ✅ | rusqlite, behind `db-sqlite` feature |
| **PDF** (P5) | ✅ | ✅ | ✅ | ✅ | ✅ | printpdf, behind `pdf` feature |
| **Gamepad** (P6) | ✅ gilrs | ✅ gilrs | ✅ gilrs | 🔶 | 🔶 | desktop real (linux now cross-compiles via the libudev-sys dlopen fork); apple/android `GCController`/`InputDevice` are no-op stubs |
| **Geolocation** (P3) | ✅ CoreLocation | ✅ GeoClue2 (zbus) | 📝 WinRT Geolocator | ✅ | ✅ | windows still a TODO stub (returns no fix) |
| **Sensors** (P6) | ✅ CoreMotion | ✅ iio sysfs | ❌ no-op | ✅ CoreMotion | ✅ SensorManager | windows still has no backend (`poll` falls through) |
| **Biometric** (P6) | ✅ LocalAuthentication | ❌ no-op | ❌ no-op | ✅ | ✅ | linux/windows missing (no PAM / Windows Hello) |
| **Permission** (P1) | 📝 TODO | 📝 TODO | 📝 TODO | 📝 TODO | ~partial | the *request* side is TODO on every platform; status read-back works |
| **Camera** (P6) | 🔶→seam | 🔶→seam | 🔶→seam | 🔶→seam | 🔶→seam | **capture seam landed** (worker pulls from a registered `CaptureVTable`, test-pattern fallback); per-OS backends (AVFoundation / Camera2 / v4l2 / MediaFoundation) plug in — not yet written |
| **Screen capture** (P6) | 🔶→seam | 🔶→seam | 🔶→seam | 🔶→seam | 🔶→seam | **capture seam landed**; per-OS backends (ScreenCaptureKit / X11 / DXGI; **Wayland dummy** per the user) plug in — not yet written |
| **Video playback** (P6) | 🔶 | 🔶 | 🔶 | 🔶 | 🔶 | `video.rs` SMPTE-bars test pattern; real vk-video/native decode not written |
| **Mic capture** (P7) | 🔶 | ✅ ALSA | 🔶 | 🔶 | 🔶 | **linux ALSA capture real** (dlopen `libasound`, via the mic seam); macOS/Windows/mobile still the 440 Hz test tone |
| **Audio playback `AudioSink`** (P7) | 🔶 | ✅ ALSA | 🔶 | 🔶 | 🔶 | **linux ALSA playback real** (dlopen `libasound`); macOS/Windows/mobile still the frame-counting stub |
| **Video codec** (P7/P8) | 🔶 VideoToolbox-sel | 🔶 gpu-video-sel | 🔶 gpu-video-sel | 🔶 | 🔶 MediaCodec-sel | `video_codec` selects the native backend per platform (`backend_name()`) but the FFI is a stub |

## What's actually solid cross-platform
UDP, SQLite, PDF (pure-Rust / portable engines) and **gamepad** (gilrs, all
desktop). Geolocation + sensors + biometric are real on Apple + Android.

## Desktop gaps, prioritized for "extend to desktop"

**Tractable now (pure Rust / file-based, cross-compiles, no heavy system dep):**
1. **Linux sensors via industrial-I/O** — read `/sys/bus/iio/devices/iio:deviceN/in_{accel,anglvel,magn}_*_raw` (+ `*_scale`). Pure file reads, graceful when no iio device exists (most desktops). Closes the biggest "sensors on linux" gap the user named.
2. **Clean no-op confirmations** — sensors/biometric already fall through to nothing on linux/windows (compiles), but should be explicit (a documented no-op fn) so it's intentional, not accidental.

**Real but heavier (native APIs via dlopen per the desktop-dlopen rule):**
3. **Camera**: v4l2 (`/dev/video*`, ioctls — pure syscalls) on Linux; AVFoundation (macOS); Media Foundation (Windows). Linux v4l2 is the most tractable (no dlopen even — ioctls).
4. **Screen capture**: X11 `XShmGetImage` (dlopen libX11/libXext) on Linux-X11; **Wayland = dummy** (real needs xdg-desktop-portal + PipeWire — acceptable to stub per the user); ScreenCaptureKit (macOS); DXGI Desktop Duplication (Windows).
5. **Audio**: rodio/cpal (cross-platform desktop) for both mic capture + `AudioSink` — one crate covers macOS/Linux/Windows; gate behind a feature (+ dlopen ALSA on Linux per the desktop rule).
6. **Geolocation linux/windows**: zbus → GeoClue2 (Linux); WinRT Geolocator (Windows).
7. **Video codec**: gpu-video (Vulkan Video) on desktop Linux/Windows; VideoToolbox (Apple); behind a `video` feature; on-device.

**Mobile gaps (separate from this desktop pass):** gamepad apple/android stubs; permission request-side TODOs everywhere.

## Cross-compile rule (established)
Any desktop system lib must be **dlopen'd** (libloading) not link-bound, so the
dll cross-compiles to any host (see `forks/libudev-sys`, the gilrs fix). v4l2 +
iio are file/ioctl-based (no lib, no dlopen needed); X11/ALSA/etc. dlopen.

## First extension this pass
Linux sensors (iio) — real, pure-Rust, cross-compiles, the user's named example.

## Windowing & input — SUPER_PLAN_1 non-mobile review (2026-05-21)

SUPER_PLAN_1 (`SUPER_PLAN.md`) added touch/pen/gestures/orientation for iOS+Android, with safe-area-insets as an iOS stretch goal (Sprint L, line 163). Reviewing whether those extend to **desktop**, plus the desktop-only multi-monitor / multi-window:

| System | macOS | Linux | Windows | Status |
|---|---|---|---|---|
| **Multi-monitor** | ✅ CoreGraphics | ✅ XRandR / wayland | ✅ EnumDisplay | real per-platform |
| **DPI / scale** | ✅ | ✅ | ✅ | real |
| **Multi-window** | ✅ registry | ✅ | ✅ | window registry (run.rs + per-OS) |
| **Desktop touch (multi-touch)** | ❌ | ❌ | ❌ | mouse-only; no X11 XInput2 / WM_TOUCH multitouch (mobile has `TouchPointVec`) |
| **Desktop pen / tablet (Wacom)** | ❌ | ❌ | ❌ | `PenState` exists (mobile) but **not populated on desktop** — no XInput2 valuators / Windows Ink / NSEvent tablet |
| **Orientation** | n/a | (iio-derivable) | n/a | desktop auto-rotate could derive from the new iio sensor backend |
| **Notches / safe-area** | ✅ NSView | n/a | ❌ | **DONE this session**: core css `SafeAreaInsets` + `get_safe_area_insets` (codegen-exposed), populated on macOS (`NSView.safeAreaInsets`) **and iOS** (`UIView`); Android cutout (JNI) pending |

**Solid:** multi-monitor, DPI, multi-window. **Gaps:** desktop multi-touch, desktop pen/Wacom, safe-area/notches.

Fix plan (tractable-first, per the established patterns):
1. ~~safe-area-insets~~ **DONE (2026-05-21)** — used the existing css `SafeAreaInsets` (not a new type); `CallbackInfo::get_safe_area_insets` codegen-exposed; populated on macOS (`NSView.safeAreaInsets`) + iOS (`UIView`). Android cutout (JNI from scratch) pending.
2. **Linux Wacom** — XInput2 tablet valuators (pressure/tilt) via dlopen `libXi` -> `PenState` (the user's named ask). Device-tested.
3. **Desktop multi-touch** — X11 XInput2 touch / Windows WM_TOUCH -> `TouchPointVec`. Device-tested.
