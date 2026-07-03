//! Shared AVFoundation / TCC authorization gate for the capture backends.
//!
//! Nothing in a bare (unsigned, non-bundled) demo binary ever calls
//! `requestAccessForMediaType:` — so `startRunning` silently vends black
//! frames / silent samples when the app isn't authorized. This module is the
//! missing gate: camera `open` + mic `mic_open` call [`ensure_av_access`]
//! before building their `AVCaptureSession`, and the permission manager's
//! macOS request path fires [`request_av_access_nonblocking`].
//!
//! On macOS TCC attributes the grant to the *responsible process* — for a
//! binary launched from a terminal that's the terminal app itself, which CAN
//! be granted camera/mic access (the prompt names the terminal). So even
//! unsigned demo binaries get a working prompt as long as somebody actually
//! asks.

use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_av_foundation::{AVCaptureDevice, AVMediaType, AVMediaTypeAudio, AVMediaTypeVideo};

/// `AVAuthorizationStatus` raw values (`AVCaptureDevice.h`).
/// NotDetermined (0) is the catch-all `_` arm in the matches below.
const STATUS_RESTRICTED: isize = 1;
const STATUS_DENIED: isize = 2;
const STATUS_AUTHORIZED: isize = 3;

/// Human label for a media type ("camera" / "microphone") for log lines.
fn media_label(media: &AVMediaType) -> &'static str {
    // AVMediaType is an NSString four-cc: "vide" / "soun".
    if media.to_string() == "soun" {
        "microphone"
    } else {
        "camera"
    }
}

/// Check (and, if the user was never asked, request) TCC access for `media`
/// (`AVMediaTypeVideo` / `AVMediaTypeAudio`). Returns `true` when capture may
/// proceed.
///
/// - `Authorized` → `true`.
/// - `Denied` / `Restricted` → logs how to fix it and returns `false` (a
///   session started anyway would only vend black frames / silence).
/// - `NotDetermined` → calls `requestAccessForMediaType:completionHandler:`
///   and **blocks up to 60 s** for the user's answer. That's fine — the only
///   callers are the dedicated capture worker threads (camera / mic), never
///   the frame loop. A timeout (or an unresolvable request path) returns
///   `true` and lets the session try anyway, so this gate can never make
///   things worse than the old "just call `startRunning`" behavior.
pub fn ensure_av_access(media: &AVMediaType) -> bool {
    let label = media_label(media);
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) };
    match status.0 {
        STATUS_AUTHORIZED => true,
        STATUS_DENIED | STATUS_RESTRICTED => {
            let state = if status.0 == STATUS_DENIED {
                "denied"
            } else {
                "restricted (MDM / parental controls)"
            };
            crate::plog_warn!(
                "[avf_auth] {} access is {} — capture would only produce black/silent \
                 samples. On macOS the grant is per responsible process (the terminal \
                 app when launched from a terminal); enable it under System Settings → \
                 Privacy & Security → {}.",
                label,
                state,
                if label == "camera" { "Camera" } else { "Microphone" }
            );
            false
        }
        // STATUS_NOT_DETERMINED (or an unknown future value).
        _ => {
            crate::plog_info!(
                "[avf_auth] {} access not determined — requesting (OS prompt)…",
                label
            );
            let (tx, rx) = mpsc::channel::<bool>();
            let handler = RcBlock::new(move |granted: Bool| {
                let _ = tx.send(granted.as_bool());
            });
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler);
            }
            match rx.recv_timeout(Duration::from_secs(60)) {
                Ok(true) => {
                    crate::plog_info!("[avf_auth] {} access granted by user", label);
                    true
                }
                Ok(false) => {
                    crate::plog_warn!(
                        "[avf_auth] {} access denied by user — capture backend will \
                         report failure (widget keeps its test pattern)",
                        label
                    );
                    false
                }
                Err(_) => {
                    // No answer within 60 s (prompt still up, or the request
                    // path didn't fire at all). Let the session try anyway —
                    // never make things worse.
                    crate::plog_warn!(
                        "[avf_auth] {} access request unanswered after 60s — \
                         proceeding optimistically",
                        label
                    );
                    true
                }
            }
        }
    }
}

/// [`ensure_av_access`] for the camera. `AVMediaTypeVideo` missing at runtime
/// (never on a real macOS/iOS) → optimistic `true`.
pub fn ensure_camera_access() -> bool {
    match unsafe { AVMediaTypeVideo } {
        Some(m) => ensure_av_access(m),
        None => true,
    }
}

/// [`ensure_av_access`] for the microphone. `AVMediaTypeAudio` missing at
/// runtime (never on a real macOS/iOS) → optimistic `true`.
pub fn ensure_mic_access() -> bool {
    match unsafe { AVMediaTypeAudio } {
        Some(m) => ensure_av_access(m),
        None => true,
    }
}

/// Fire-and-forget access request for the permission manager's diff path
/// (`extra/permission/macos.rs`). Never blocks — safe to call from the frame
/// loop. If the status is already determined it just logs it; otherwise it
/// fires `requestAccessForMediaType:completionHandler:` with a completion
/// block that logs the outcome (the next `probe_status` poll picks up the
/// new state).
pub fn request_av_access_nonblocking(video: bool) {
    let media = match unsafe { if video { AVMediaTypeVideo } else { AVMediaTypeAudio } } {
        Some(m) => m,
        None => return,
    };
    let label = media_label(media);
    // MWA-C-permission: every arm now parks its outcome in the process-global
    // channel — the capability pump folds it into the PermissionManager on
    // the next event pass and fires PermissionChanged. Previously outcomes
    // were only logged, so manager state stayed NotDetermined forever.
    use azul_layout::managers::permission::{
        push_async_result, Capability, PermissionQuality, PermissionState,
    };
    let capability = if video {
        Capability::Camera
    } else {
        Capability::Microphone
    };
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) };
    match status.0 {
        STATUS_AUTHORIZED => {
            crate::plog_info!("[avf_auth] {} access already granted", label);
            push_async_result(
                capability,
                PermissionState::Granted {
                    quality: PermissionQuality::Full,
                },
            );
        }
        STATUS_DENIED | STATUS_RESTRICTED => {
            crate::plog_warn!(
                "[avf_auth] {} access denied/restricted — re-prompting is not \
                 possible; grant it under System Settings → Privacy & Security \
                 (the grant is keyed to the responsible process, e.g. the \
                 terminal app when launched from a terminal)",
                label
            );
            push_async_result(
                capability,
                if status.0 == STATUS_RESTRICTED {
                    PermissionState::Restricted
                } else {
                    PermissionState::Denied
                },
            );
        }
        _ => {
            crate::plog_info!("[avf_auth] requesting {} access (async, OS prompt)…", label);
            push_async_result(capability, PermissionState::Requested);
            let handler = RcBlock::new(move |granted: Bool| {
                // Runs on a TCC dispatch-queue thread; the channel is the
                // designed OS-thread → pump handoff (MWA-A1b).
                let state = if granted.as_bool() {
                    crate::plog_info!("[avf_auth] async request: access granted");
                    PermissionState::Granted {
                        quality: PermissionQuality::Full,
                    }
                } else {
                    crate::plog_warn!("[avf_auth] async request: access denied by user");
                    PermissionState::Denied
                };
                push_async_result(capability, state);
            });
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler);
            }
        }
    }
}
