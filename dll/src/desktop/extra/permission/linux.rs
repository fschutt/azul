//! Linux permission backend: xdg-desktop-portal (MWA-C-permission).
//!
//! ```text
//! service:   org.freedesktop.portal.Desktop
//! object:    /org/freedesktop/portal/desktop
//! interface: org.freedesktop.portal.Camera   (AccessCamera)
//!            org.freedesktop.portal.Device   (AccessDevice: "microphone",
//!                                             "speakers", "camera")
//! ```
//!
//! Outside Flatpak / Snap most kernel surfaces are auto-granted to any
//! process that can open the device file (`/dev/video0` for cameras,
//! `/dev/input/*` for sensors). When no portal is reachable we therefore
//! report `Granted{Full}` with a log line so the user knows the platform
//! is not enforcing anything (this matches the module's long-standing
//! documented policy).
//!
//! Concurrency: a portal Access* call pops an OS dialog and blocks until
//! the user answers, so `handle_event` (called from the layout diff on the
//! UI thread) hands the round-trip to a one-shot worker thread — the same
//! pattern as the ScreenCast handshake in `extra/screencap/linux.rs` — and
//! the worker parks the outcome via `push_async_result`, where the
//! capability pump (MWA-A1b) folds it into the `PermissionManager` and
//! fires `PermissionChanged`. This is the designed "OS-async completion on
//! an OS/worker thread → channel → pump" seam, not a framework poll thread
//! (rule 11 targets pump/poll threads; portal prompts are user-interactive
//! and cannot complete synchronously).
//!
//! Geolocation permission is NOT requested here: the geoclue loop in
//! `extra/geolocation/linux.rs` owns that session and pushes the
//! `Capability::Geolocation` outcome from its `Start` result.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use azul_layout::managers::permission::{
    push_async_result, Capability, PermissionDiffEvent, PermissionQuality, PermissionState,
};

/// Portal Response codes (org.freedesktop.portal.Request::Response).
const RESPONSE_SUCCESS: u32 = 0;
const RESPONSE_CANCELLED: u32 = 1;

/// How long to wait for the user to answer the portal dialog before the
/// one-shot worker gives up (the dialog stays up; a later answer is simply
/// dropped — the next Subscribe re-asks).
const PORTAL_DIALOG_TIMEOUT: Duration = Duration::from_secs(120);

static REQUEST_COUNTER: AtomicU32 = AtomicU32::new(0);

pub fn handle_event(event: &PermissionDiffEvent) {
    match event {
        PermissionDiffEvent::Subscribe { capability, .. } => match capability {
            Capability::Camera => spawn_portal_request(Capability::Camera),
            Capability::Microphone => spawn_portal_request(Capability::Microphone),
            // Geolocation → geoclue (see module doc). ScreenCapture → the
            // ScreenCast portal handshake in extra/screencap/linux.rs (which
            // is session-based, not a plain Access call). Everything else
            // has no Linux permission surface → auto-granted policy applies
            // once a real consumer runs.
            _ => {}
        },
        // Portal grants are per-app and persistent; nothing to release.
        PermissionDiffEvent::Release { .. } | PermissionDiffEvent::Reconfigure { .. } => {}
    }
}

pub fn probe_status(capability: Capability) -> PermissionState {
    // The portals expose no non-interactive read-back of a pending grant;
    // outside sandboxes the kernel enforces nothing. Report NotDetermined so
    // the async Subscribe path (or the no-portal fallback) fills the manager
    // cache instead of guessing here.
    let _ = capability;
    PermissionState::NotDetermined
}

/// One-shot worker: portal round-trip → `push_async_result`.
fn spawn_portal_request(capability: Capability) {
    push_async_result(capability, PermissionState::Requested);
    std::thread::spawn(move || {
        let state = portal_access_blocking(capability);
        push_async_result(capability, state);
    });
}

/// Blocking portal Access request (worker thread only).
fn portal_access_blocking(capability: Capability) -> PermissionState {
    let Ok(conn) = zbus::blocking::Connection::session() else {
        return no_portal_fallback(capability, "no session bus");
    };
    let (interface, method) = match capability {
        Capability::Camera => ("org.freedesktop.portal.Camera", "AccessCamera"),
        Capability::Microphone => ("org.freedesktop.portal.Device", "AccessDevice"),
        _ => return PermissionState::NotDetermined,
    };
    let Ok(proxy) = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        interface,
    ) else {
        return no_portal_fallback(capability, "portal proxy unavailable");
    };

    // Request/Response pattern (same as extra/screencap/linux.rs): predict
    // the Request object path from our unique name + handle_token, subscribe
    // BEFORE calling (the response can be immediate), then wait.
    let Some(unique) = conn.unique_name().map(|n| n.to_string()) else {
        return no_portal_fallback(capability, "no unique bus name");
    };
    let sender_token = unique.trim_start_matches(':').replace('.', "_");
    let token = format!(
        "azulperm{}",
        REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    );
    let request_path = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        sender_token, token
    );
    let Ok(req_proxy) = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    ) else {
        return no_portal_fallback(capability, "request proxy unavailable");
    };
    let Ok(mut responses) = req_proxy.receive_signal("Response") else {
        return no_portal_fallback(capability, "cannot subscribe to Response");
    };

    let mut options: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    options.insert("handle_token", zbus::zvariant::Value::from(token.as_str()));

    // AccessCamera(options) vs AccessDevice(pid, devices, options) — the
    // Device portal wants the caller pid + a device-class list.
    let call_result: Result<zbus::zvariant::OwnedObjectPath, zbus::Error> = match capability {
        Capability::Camera => proxy.call(method, &(options,)),
        Capability::Microphone => {
            let pid = std::process::id();
            proxy.call(method, &(pid, vec!["microphone"], options))
        }
        _ => return PermissionState::NotDetermined,
    };
    if call_result.is_err() {
        return no_portal_fallback(capability, "portal call failed");
    }

    // Bounded wait for the user's answer (helper channel like screencap —
    // `responses.next()` alone blocks forever on a never-answered dialog).
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Some(msg) = responses.next() {
            let _ = tx.send(msg);
        }
    });
    let Ok(msg) = rx.recv_timeout(PORTAL_DIALOG_TIMEOUT) else {
        crate::plog_warn!(
            "[permission] linux: portal dialog for {:?} unanswered after {:?}",
            capability,
            PORTAL_DIALOG_TIMEOUT
        );
        return PermissionState::NotDetermined;
    };
    let response_code: u32 = match msg
        .body()
        .deserialize::<(u32, HashMap<String, zbus::zvariant::OwnedValue>)>()
    {
        Ok((code, _results)) => code,
        Err(_) => return PermissionState::NotDetermined,
    };
    match response_code {
        RESPONSE_SUCCESS => PermissionState::Granted {
            quality: PermissionQuality::Full,
        },
        RESPONSE_CANCELLED => PermissionState::Denied,
        _ => PermissionState::Denied,
    }
}

/// Documented policy when no portal is reachable: the kernel auto-grants to
/// any process that can open the device file, so report Granted{Full} and
/// log that the platform is not enforcing.
fn no_portal_fallback(capability: Capability, why: &str) -> PermissionState {
    crate::plog_warn!(
        "[permission] linux: {} for {:?} — no portal enforcement, reporting \
         Granted (kernel auto-grants to processes with device-file access)",
        why,
        capability
    );
    PermissionState::Granted {
        quality: PermissionQuality::Full,
    }
}
