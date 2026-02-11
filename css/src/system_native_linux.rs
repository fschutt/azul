//! Native Linux system style discovery.
//!
//! Strategy (in order of preference):
//!
//! 1. **XDG Desktop Portal** via raw D-Bus (no external crates needed).
//!    The portal method `org.freedesktop.portal.Settings.Read` is available
//!    on GNOME 42+, KDE Plasma 6, Sway, Hyprland (via xdg-desktop-portal-gtk
//!    or -wlr).  This gives us the colour-scheme, accent colour, and more.
//!
//! 2. **GSettings CLI** — fallback to spawning `gsettings` for GNOME or
//!    `kreadconfig5` for KDE (reuses the existing `io`-feature discovery).
//!
//! 3. **Hardcoded defaults** — `defaults::gnome_adwaita_light()`.
//!
//! No external crates are linked.  All D-Bus communication is done via a raw
//! Unix socket connection to the session bus using a minimal inline
//! implementation of the D-Bus wire protocol.  This avoids pulling in `zbus`
//! or `dbus` as a dependency.

use alloc::string::String;
use super::{defaults, LinuxCustomization, ScrollbarPreferences, ScrollbarVisibility};
use crate::corety::{AzString, OptionString};

// ── D-Bus wire-protocol helpers (minimal, read-only) ─────────────────────

/// Read the XDG Desktop Portal `org.freedesktop.appearance` settings.
///
/// Returns `(color_scheme, accent_color_rgb)` where color_scheme is:
///   0 = no preference, 1 = dark, 2 = light.
/// Returns `None` if the portal is unavailable.
#[cfg(feature = "io")]
fn query_xdg_portal() -> Option<(u32, Option<(f64, f64, f64)>)> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    // Connect to session D-Bus
    let bus_addr = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok()?;
    // Parse "unix:path=/run/user/1000/bus" or similar
    let path = bus_addr
        .strip_prefix("unix:path=")?;
    // Handle additional parameters after comma
    let path = path.split(',').next()?;

    let mut stream = UnixStream::connect(path).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok()?;

    // D-Bus authentication: simplest method is EXTERNAL with uid
    let uid = unsafe { libc_getuid() };
    let auth_msg = alloc::format!("\0AUTH EXTERNAL {}\r\nBEGIN\r\n", hex_encode_uid(uid));
    stream.write_all(auth_msg.as_bytes()).ok()?;

    // Read auth response (we just need "OK <guid>")
    let mut buf = [0u8; 256];
    let n = stream.read(&mut buf).ok()?;
    let resp = core::str::from_utf8(&buf[..n]).ok()?;
    if !resp.contains("OK") { return None; }

    // Send Hello message to get our unique name (required before any method call)
    let hello_msg = build_dbus_method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "Hello",
        &[],
        1,
    );
    stream.write_all(&hello_msg).ok()?;
    // Read Hello response (we ignore it, just need to consume it)
    let mut resp_buf = vec![0u8; 4096];
    let _ = stream.read(&mut resp_buf);

    // Now call org.freedesktop.portal.Settings.Read for color-scheme
    let read_msg = build_dbus_method_call(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
        "Read",
        &[
            DValue::String("org.freedesktop.appearance"),
            DValue::String("color-scheme"),
        ],
        2,
    );
    stream.write_all(&read_msg).ok()?;

    let mut resp_buf = vec![0u8; 4096];
    let n = stream.read(&mut resp_buf).ok()?;

    // Parse the response to extract the uint32 color-scheme value
    // The response is a D-Bus message containing a variant(variant(uint32))
    let color_scheme = parse_uint32_from_variant_response(&resp_buf[..n]).unwrap_or(0);

    // Try to read accent-color (may not be available on all portals)
    let accent_msg = build_dbus_method_call(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
        "Read",
        &[
            DValue::String("org.freedesktop.appearance"),
            DValue::String("accent-color"),
        ],
        3,
    );
    stream.write_all(&accent_msg).ok()?;

    let mut resp_buf2 = vec![0u8; 4096];
    let n2 = stream.read(&mut resp_buf2).unwrap_or(0);
    let accent = parse_rgb_from_variant_response(&resp_buf2[..n2]);

    Some((color_scheme, accent))
}

// ── Minimal D-Bus message builder ────────────────────────────────────────

enum DValue<'a> {
    String(&'a str),
}

fn build_dbus_method_call(
    destination: &str,
    path: &str,
    interface: &str,
    method: &str,
    args: &[DValue<'_>],
    serial: u32,
) -> alloc::vec::Vec<u8> {
    // This is a simplified D-Bus message builder for method calls.
    // It only supports string arguments (sufficient for portal queries).
    let mut body = alloc::vec::Vec::new();
    let mut sig = String::new();
    for arg in args {
        match arg {
            DValue::String(s) => {
                sig.push('s');
                let bytes = s.as_bytes();
                // String: uint32 length + bytes + NUL + padding to 4-byte boundary
                body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                body.extend_from_slice(bytes);
                body.push(0); // NUL terminator
                // Pad to 4-byte alignment
                while body.len() % 4 != 0 { body.push(0); }
            }
        }
    }

    let mut header_fields = alloc::vec::Vec::new();
    // PATH (1)
    append_header_field(&mut header_fields, 1, 'o', path);
    // INTERFACE (2)
    append_header_field(&mut header_fields, 2, 's', interface);
    // MEMBER (3)
    append_header_field(&mut header_fields, 3, 's', method);
    // DESTINATION (6)
    append_header_field(&mut header_fields, 6, 's', destination);
    // SIGNATURE (8) — if we have arguments
    if !sig.is_empty() {
        // Signature header: code=8, variant sig='g', then sig bytes
        while header_fields.len() % 8 != 0 { header_fields.push(0); }
        header_fields.push(8); // field code
        header_fields.push(1); // variant signature: 1 byte 'g'
        header_fields.push(b'g');
        header_fields.push(0); // padding
        let sig_bytes = sig.as_bytes();
        header_fields.push(sig_bytes.len() as u8);
        header_fields.extend_from_slice(sig_bytes);
        header_fields.push(0);
    }
    // Pad header fields to 8-byte alignment
    while header_fields.len() % 8 != 0 { header_fields.push(0); }

    let mut msg = alloc::vec::Vec::new();
    // Fixed header: endianness(1) + type(1) + flags(1) + version(1)
    msg.push(b'l'); // little-endian
    msg.push(1);    // METHOD_CALL
    msg.push(0);    // flags
    msg.push(1);    // protocol version
    // body length (uint32)
    msg.extend_from_slice(&(body.len() as u32).to_le_bytes());
    // serial (uint32)
    msg.extend_from_slice(&serial.to_le_bytes());
    // header fields array length (uint32)
    msg.extend_from_slice(&(header_fields.len() as u32).to_le_bytes());
    // header fields
    msg.extend_from_slice(&header_fields);
    // Pad to 8-byte alignment before body
    while msg.len() % 8 != 0 { msg.push(0); }
    // body
    msg.extend_from_slice(&body);

    msg
}

fn append_header_field(buf: &mut alloc::vec::Vec<u8>, code: u8, sig: char, value: &str) {
    // Align to 8 bytes (start of struct)
    while buf.len() % 8 != 0 { buf.push(0); }
    buf.push(code);
    // variant signature
    buf.push(1); // sig length
    buf.push(sig as u8);
    buf.push(0); // NUL
    // Pad to 4 bytes for the string/object-path value
    while buf.len() % 4 != 0 { buf.push(0); }
    let bytes = value.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
    buf.push(0);
}

fn parse_uint32_from_variant_response(data: &[u8]) -> Option<u32> {
    // Very simplified: scan backwards for a plausible uint32 value (0, 1, or 2)
    // in the response body.  A full parser is overkill for this single value.
    if data.len() < 16 { return None; }
    // Skip the 12-byte fixed header + header fields to find the body
    let body_len = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    let header_fields_len = u32::from_le_bytes(data[12..16].try_into().ok()?) as usize;
    let body_start = 16 + header_fields_len;
    // Align to 8
    let body_start = (body_start + 7) & !7;
    if body_start + body_len > data.len() { return None; }
    let body = &data[body_start..body_start + body_len];
    // The body is variant(variant(uint32)).  The uint32 is at the end.
    if body.len() >= 4 {
        let val = u32::from_le_bytes(body[body.len()-4..].try_into().ok()?);
        if val <= 2 { return Some(val); }
    }
    None
}

fn parse_rgb_from_variant_response(_data: &[u8]) -> Option<(f64, f64, f64)> {
    // accent-color is a (ddd) struct — complex to parse from raw bytes.
    // For now, return None and let the caller fall back to the GTK accent.
    None
}

extern "C" { fn getuid() -> u32; }
unsafe fn libc_getuid() -> u32 { getuid() }

fn hex_encode_uid(uid: u32) -> String {
    let uid_str = alloc::format!("{}", uid);
    let mut hex = String::new();
    for b in uid_str.bytes() {
        hex.push_str(&alloc::format!("{:02x}", b));
    }
    hex
}

// ── GSettings / CLI fallback helpers ─────────────────────────────────────

#[cfg(feature = "io")]
fn gsettings_get(schema: &str, key: &str) -> Option<String> {
    use std::process::{Command, Stdio};
    let out = Command::new("gsettings")
        .args(["get", schema, key])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().trim_matches('\'').to_string())
    } else {
        None
    }
}

#[cfg(feature = "io")]
fn discover_linux_extras(style: &mut super::SystemStyle) {
    // Icon theme
    if let Some(icon) = gsettings_get("org.gnome.desktop.interface", "icon-theme") {
        style.linux.icon_theme = OptionString::Some(icon.into());
    }
    // Cursor theme + size
    if let Some(cursor) = gsettings_get("org.gnome.desktop.interface", "cursor-theme") {
        style.linux.cursor_theme = OptionString::Some(cursor.into());
    }
    if let Some(size_s) = gsettings_get("org.gnome.desktop.interface", "cursor-size") {
        if let Ok(sz) = size_s.parse::<u32>() {
            style.linux.cursor_size = sz;
        }
    }
    // GTK theme
    if let Some(gtk) = gsettings_get("org.gnome.desktop.interface", "gtk-theme") {
        style.linux.gtk_theme = OptionString::Some(gtk.into());
    }
    // Button layout (determines button side for CSD)
    if let Some(layout) = gsettings_get("org.gnome.desktop.wm.preferences", "button-layout") {
        style.linux.titlebar_button_layout = OptionString::Some(layout.clone().into());
        // Parse button side from layout: "close,minimize,maximize:" → Left
        //                                ":close,minimize,maximize" → Right
        if layout.starts_with(':') {
            style.metrics.titlebar.button_side = super::TitlebarButtonSide::Right;
        } else {
            style.metrics.titlebar.button_side = super::TitlebarButtonSide::Left;
        }
    }
    // Env-var fallbacks (work on ALL Linux WMs)
    if style.linux.cursor_theme.is_none() {
        if let Ok(t) = std::env::var("XCURSOR_THEME") {
            style.linux.cursor_theme = OptionString::Some(t.into());
        }
    }
    if style.linux.cursor_size == 0 {
        if let Ok(s) = std::env::var("XCURSOR_SIZE") {
            if let Ok(sz) = s.parse::<u32>() {
                style.linux.cursor_size = sz;
            }
        }
    }
}

// ── Public entry point ───────────────────────────────────────────────────

/// Discover the Linux system style.
///
/// Tries XDG Desktop Portal first (raw D-Bus), then falls back to
/// `gsettings` CLI, and finally to hardcoded GNOME Adwaita defaults.
pub(super) fn discover() -> super::SystemStyle {
    let mut style = defaults::gnome_adwaita_light();

    // 1. Try XDG Portal for theme detection
    #[cfg(feature = "io")]
    {
        if let Some((color_scheme, accent_rgb)) = query_xdg_portal() {
            match color_scheme {
                1 => { style = defaults::gnome_adwaita_dark(); } // prefer-dark
                2 => { style = defaults::gnome_adwaita_light(); } // prefer-light
                _ => {} // no preference, keep default
            }
            if let Some((r, g, b)) = accent_rgb {
                style.colors.accent = OptionColorU::Some(crate::props::basic::color::ColorU::new_rgb(
                    (r.clamp(0.0, 1.0) * 255.0) as u8,
                    (g.clamp(0.0, 1.0) * 255.0) as u8,
                    (b.clamp(0.0, 1.0) * 255.0) as u8,
                ));
            }
        }
    }

    // 2. Query additional Linux-specific settings via gsettings
    #[cfg(feature = "io")]
    {
        discover_linux_extras(&mut style);
    }

    // 3. Detect desktop environment
    #[cfg(feature = "io")]
    {
        style.platform = super::Platform::Linux(super::detect_linux_desktop_env());
        style.language = super::detect_system_language();
    }
    #[cfg(not(feature = "io"))]
    {
        style.platform = super::Platform::Linux(
            super::DesktopEnvironment::Other(AzString::from_const_str("unknown")),
        );
    }

    style
}
