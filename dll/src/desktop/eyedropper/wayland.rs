//! Wayland screen read for the eyedropper: the desktop portal's
//! `org.freedesktop.portal.Screenshot.Screenshot` call.
//!
//! A Wayland client cannot read the screen: the compositor shows nothing
//! but the client's own surfaces. The portal is the sanctioned way - the
//! compositor (GNOME, KDE, sway + xdg-desktop-portal-wlr, ...) shows ITS
//! permission dialog ("Allow <app> to take a screenshot?"), and on consent
//! hands back a PNG file. That dialog is the opt-in the eyedropper wants on
//! a platform without free screen access; a refusal cancels the pick.
//!
//! The call blocks the calling thread for the user's answer (bounded - see
//! [`PORTAL_DIALOG_TIMEOUT`]); the dialog is the compositor's, so the
//! blocked event loop is not what the user is looking at.

use std::{collections::HashMap, time::Duration};

use azul_core::geom::LogicalPosition;

use super::Screenshot;

/// How long the portal dialog may stay unanswered before the pick counts
/// as cancelled.
const PORTAL_DIALOG_TIMEOUT: Duration = Duration::from_secs(120);

/// Take a screenshot through the portal. `scale` is the physical-per-logical
/// factor of the window that asked (the portal's PNG is in physical pixels).
#[must_use]
pub fn capture(scale: f32) -> Option<Screenshot> {
    let png = portal_screenshot_png()?;
    let decoded = match azul_layout::image::decode::decode_raw_image_from_any_bytes(&png) {
        azul_layout::image::decode::ResultRawImageDecodeImageError::Ok(img) => img,
        azul_layout::image::decode::ResultRawImageDecodeImageError::Err(e) => {
            crate::plog_warn!(
                "[eyedropper] wayland: the portal screenshot did not decode: {:?}",
                e
            );
            return None;
        }
    };
    let rgba = super::raw_image_to_rgba(&decoded)?;
    #[allow(clippy::cast_possible_truncation)] // image dimensions
    Some(Screenshot {
        width: decoded.width as u32,
        height: decoded.height as u32,
        rgba,
        origin: LogicalPosition::zero(),
        scale: scale.max(0.01),
    })
}

/// The portal round trip: request -> (dialog) -> Response(uri) -> file bytes.
fn portal_screenshot_png() -> Option<Vec<u8>> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static REQUEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    let conn = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            crate::plog_warn!("[eyedropper] wayland: no session bus ({e}) - no screenshot portal");
            return None;
        }
    };
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
    )
    .ok()?;

    // Request/Response: predict the Request object path from our unique
    // name + handle token, subscribe BEFORE calling (the answer can be
    // immediate), then wait for it.
    let unique = conn.unique_name().map(|n| n.to_string())?;
    let sender_token = unique.trim_start_matches(':').replace('.', "_");
    let token = format!(
        "azuleyedropper{}",
        REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    );
    let request_path = format!("/org/freedesktop/portal/desktop/request/{sender_token}/{token}");
    let req_proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    )
    .ok()?;
    let mut responses = req_proxy.receive_signal("Response").ok()?;

    let mut options: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    options.insert("handle_token", zbus::zvariant::Value::from(token.as_str()));
    // Not "interactive": we want the frame, not the portal's own editor.
    options.insert("interactive", zbus::zvariant::Value::from(false));
    // An empty parent window handle: the dialog is not attached to us.
    let call: Result<zbus::zvariant::OwnedObjectPath, zbus::Error> =
        proxy.call("Screenshot", &("", options));
    if let Err(e) = call {
        crate::plog_warn!("[eyedropper] wayland: Screenshot portal call failed: {e}");
        return None;
    }

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Some(msg) = responses.next() {
            let _ = tx.send(msg);
        }
    });
    let Ok(msg) = rx.recv_timeout(PORTAL_DIALOG_TIMEOUT) else {
        crate::plog_warn!("[eyedropper] wayland: screenshot dialog unanswered - pick cancelled");
        return None;
    };
    let (code, results) = msg
        .body()
        .deserialize::<(u32, HashMap<String, zbus::zvariant::OwnedValue>)>()
        .ok()?;
    if code != 0 {
        crate::plog_info!(
            "[eyedropper] wayland: screenshot declined (code {code}) - pick cancelled"
        );
        return None;
    }
    let uri: String = results
        .get("uri")
        .and_then(|v| String::try_from(v.clone()).ok())?;
    let path = uri.strip_prefix("file://").unwrap_or(uri.as_str());
    let bytes = std::fs::read(path).ok()?;
    // The portal's file is ours to clean up.
    let _ = std::fs::remove_file(path);
    Some(bytes)
}
