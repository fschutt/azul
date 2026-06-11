//! PlatformCapability probes — "can I use this platform feature here, and which backend
//! serves it?" (Phase 2, item c).
//!
//! Every device subsystem (camera / microphone / audio / udp / sensors /
//! gamepad / geolocation / keyring / biometric / video) can be *probed*
//! up-front for a typed [`PlatformCapability`] instead of attempting an operation and
//! getting an ambiguous `0` / null / `None`. The probes are **non-destructive
//! and never panic** — a desktop with no motion sensor returns
//! `available = false` with a reason, it does not crash (the contract the bug
//! reports asked for). Exposed to C as `AzCapability_camera()` etc.
//!
//! `available` answers "is this feature usable on this target?" Where device
//! presence is cheaply + safely checkable (e.g. `/dev/video*` on Linux) it is
//! reflected; where it is only knowable by initialising hardware (which could be
//! slow or, for gamepads, hit the gilrs/libudev issue C5) the probe reports the
//! backend as present and notes in `reason` that the device is confirmed at
//! open/poll. Pair with the actual open() for ground truth — the self-test does.

use azul_css::AzString;

/// Result of a capability probe: whether the feature is usable on this
/// target/device, the backend that serves it, and a human-readable reason when
/// it is not (or a note about when device presence is confirmed).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformCapability {
    /// Whether the feature is usable on this target (see the module note on how
    /// device-presence is determined per subsystem).
    pub available: bool,
    /// The backend that serves this feature here (e.g. `"v4l2 (libv4l2)"`,
    /// `"AVFoundation"`, `"std::net::UdpSocket"`), or `"none"` if unsupported.
    pub backend: AzString,
    /// Empty when available; otherwise why not (or a note like "device confirmed
    /// at open()").
    pub reason: AzString,
}

#[inline]
fn cap(available: bool, backend: &'static str, reason: &'static str) -> PlatformCapability {
    PlatformCapability {
        available,
        backend: AzString::from_const_str(backend),
        reason: AzString::from_const_str(reason),
    }
}

impl PlatformCapability {
    /// Probe UDP networking (`AzUdp`). Always available — it is plain
    /// `std::net::UdpSocket`, no device or platform feature required.
    pub fn udp() -> PlatformCapability {
        cap(true, "std::net::UdpSocket", "")
    }

    /// Probe camera capture. On Linux a real check for a `/dev/video*` node;
    /// elsewhere reports the backend (device presence confirmed at `open()`).
    pub fn camera() -> PlatformCapability {
        if cfg!(target_os = "linux") {
            let have = (0..8).any(|i| {
                std::path::Path::new(&format!("/dev/video{}", i)).exists()
            });
            if have {
                cap(true, "v4l2 (libv4l2)", "")
            } else {
                cap(false, "v4l2 (libv4l2)", "no /dev/video* device node")
            }
        } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
            cap(true, "AVFoundation", "device presence confirmed at open()")
        } else if cfg!(target_os = "windows") {
            cap(true, "Media Foundation (nokhwa)", "device presence confirmed at open()")
        } else if cfg!(target_os = "android") {
            cap(true, "Camera2 (NDK)", "needs runtime CAMERA permission")
        } else {
            cap(false, "none", "no camera backend on this target")
        }
    }

    /// Probe microphone capture.
    pub fn microphone() -> PlatformCapability {
        if cfg!(target_os = "linux") {
            let have = std::path::Path::new("/dev/snd").exists();
            if have {
                cap(true, "ALSA (libasound)", "")
            } else {
                cap(false, "ALSA (libasound)", "no /dev/snd — no audio device")
            }
        } else if cfg!(target_os = "windows") {
            cap(true, "cpal", "default input device confirmed at open()")
        } else if cfg!(target_os = "android") {
            cap(true, "AAudio", "needs runtime RECORD_AUDIO permission")
        } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
            cap(true, "AVAudioEngine", "needs NSMicrophoneUsageDescription")
        } else {
            cap(false, "none", "no microphone backend on this target")
        }
    }

    /// Probe audio output (`AudioSink`).
    pub fn audio_output() -> PlatformCapability {
        if cfg!(target_os = "linux") {
            let have = std::path::Path::new("/dev/snd").exists();
            if have {
                cap(true, "ALSA (libasound)", "")
            } else {
                cap(false, "ALSA (libasound)", "no /dev/snd — no audio device")
            }
        } else if cfg!(target_os = "windows") {
            cap(true, "cpal", "default output device confirmed at open()")
        } else if cfg!(target_os = "android") {
            cap(true, "AAudio", "")
        } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
            cap(true, "AVAudioEngine", "")
        } else {
            cap(false, "none", "no audio-output backend on this target")
        }
    }

    /// Probe motion sensors. Linux checks the iio sysfs tree; desktops usually
    /// have no accelerometer (reported, not crashed); phones do.
    pub fn sensors() -> PlatformCapability {
        if cfg!(target_os = "linux") {
            let have = std::fs::read_dir("/sys/bus/iio/devices")
                .map(|mut d| d.next().is_some())
                .unwrap_or(false);
            if have {
                cap(true, "iio (sysfs)", "")
            } else {
                cap(false, "iio (sysfs)", "no iio motion-sensor device present")
            }
        } else if cfg!(target_os = "ios") {
            cap(true, "CoreMotion", "")
        } else if cfg!(target_os = "android") {
            cap(false, "SensorManager (JNI)", "Rust path ready; AzulSensors.java helper pending")
        } else if cfg!(target_os = "macos") {
            cap(false, "CoreMotion", "most Macs have no accelerometer; reading stays None")
        } else if cfg!(target_os = "windows") {
            cap(false, "WinRT Sensors", "most desktops have no accelerometer; reading stays None")
        } else {
            cap(false, "none", "no motion-sensor backend on this target")
        }
    }

    /// Probe gamepad input. Reports the backend without initialising gilrs
    /// (whose libudev/evdev enumeration is the suspect for the Linux double-free
    /// C5); a connected pad is detected when `poll()` runs.
    pub fn gamepad() -> PlatformCapability {
        if cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows")) {
            cap(true, "gilrs", "a controller is detected when polled (none may be connected)")
        } else if cfg!(target_os = "ios") {
            cap(true, "GCController", "pending backend")
        } else if cfg!(target_os = "android") {
            cap(true, "InputDevice (JNI)", "pending backend")
        } else {
            cap(false, "none", "no gamepad backend on this target")
        }
    }

    /// Probe geolocation. Real backends: geoclue D-Bus loop (Linux),
    /// CLLocationManager (macOS/iOS). Android's Rust/JNI path is wired but the
    /// `AzulGeolocation.java` helper hasn't shipped; Windows is still a stub.
    pub fn geolocation() -> PlatformCapability {
        if cfg!(target_os = "linux") {
            cap(true, "geoclue (D-Bus)", "needs the GeoClue2 service; fix delivered async")
        } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
            cap(true, "CoreLocation", "needs location permission; fix delivered async")
        } else if cfg!(target_os = "windows") {
            cap(false, "WinRT Geolocation", "not yet implemented (stub)")
        } else if cfg!(target_os = "android") {
            cap(false, "FusedLocationProvider (JNI)", "Rust path ready; AzulGeolocation.java helper pending")
        } else {
            cap(false, "none", "no geolocation backend on this target")
        }
    }

    /// Probe the secret keyring. Backend presence; the actual store may still be
    /// locked/absent (delivered async as `KeyringResult::Unavailable`).
    pub fn keyring() -> PlatformCapability {
        if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
            cap(true, "Keychain", "result delivered async via get_keyring_result")
        } else if cfg!(target_os = "linux") {
            cap(true, "libsecret (Secret Service)", "needs a running secret service")
        } else if cfg!(target_os = "windows") {
            cap(true, "Credential Manager", "")
        } else if cfg!(target_os = "android") {
            cap(true, "Keystore", "")
        } else {
            cap(false, "none", "no keyring backend on this target")
        }
    }

    /// Probe biometric auth — uses the real sync availability probe
    /// (`probe_availability`), so this reflects actual device/enrolment state.
    pub fn biometric() -> PlatformCapability {
        use azul_core::biometric::BiometricKind;
        match super::biometric::probe_availability() {
            BiometricKind::NotAvailable => {
                cap(false, "platform", "no usable biometric sensor (absent/unenrolled)")
            }
            BiometricKind::Fingerprint => cap(true, "fingerprint", ""),
            BiometricKind::Face => cap(true, "face", ""),
            BiometricKind::Iris => cap(true, "iris", ""),
        }
    }

    /// Probe hardware video decode for real (see
    /// [`crate::desktop::extra::video_codec::provision`]): on Apple/Android the
    /// built-in system codec, on Linux/Windows a live Vulkan
    /// `VK_KHR_video_decode_h264` device-extension probe. When unavailable, the
    /// reason notes whether a driver install could enable it (the full command
    /// list lives in `ProvisionPlan`).
    pub fn video_codec() -> PlatformCapability {
        let p = crate::desktop::extra::video_codec::provision::probe_hw_decode();
        let reason = if p.available {
            AzString::from_const_str("")
        } else if p.can_remediate {
            AzString::from(format!(
                "{} — a driver install can enable it (ProvisionPlan::detect)",
                p.detail
            ))
        } else {
            AzString::from(p.detail)
        };
        PlatformCapability {
            available: p.available,
            backend: AzString::from_const_str(p.backend),
            reason,
        }
    }
}
