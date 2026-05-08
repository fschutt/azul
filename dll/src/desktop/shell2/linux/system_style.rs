//! Native Linux system style discovery.
//!
//! Strategy (in order of preference):
//!
//! 1. **XDG Desktop Portal** via raw D-Bus (no external crates needed).
//!    The portal method `org.freedesktop.portal.Settings.Read` is available
//!    on GNOME 42+, KDE Plasma 6, Sway, Hyprland (via xdg-desktop-portal-gtk
//!    or -wlr).  This gives us the colour-scheme, accent colour, and more.
//!
//! 2. **CLI discovery** — spawning `kreadconfig5` for KDE, `gsettings` for
//!    GNOME, or parsing Hyprland/Sway/i3/pywal configs for riced desktops.
//!
//! 3. **Hardcoded defaults** — `defaults::gnome_adwaita_light()`.
//!
//! No external crates are linked.  All D-Bus communication is done via a raw
//! Unix socket connection to the session bus using a minimal inline
//! implementation of the D-Bus wire protocol.  This avoids pulling in `zbus`
//! or `dbus` as a dependency.

use alloc::string::String;
use alloc::boxed::Box;
use azul_css::system::{
    defaults, SystemStyle, Platform, DesktopEnvironment, Theme,
    TitlebarButtonSide, ToolbarStyle,
};
use azul_css::dynamic_selector::{OsVersion, OsFamily, BoolCondition};
use azul_css::corety::{AzString, OptionString, OptionF32};
use azul_css::props::basic::color::{ColorU, OptionColorU, parse_css_color};
use azul_css::props::basic::pixel::{PixelValue, OptionPixelValue};
use azul_css::css::Css;
use azul_css::parser2::new_from_str;

// ── D-Bus wire-protocol helpers (minimal, read-only) ─────────────────────

/// Read the XDG Desktop Portal `org.freedesktop.appearance` settings.
///
/// Returns `(color_scheme, accent_color_rgb)` where color_scheme is:
///   0 = no preference, 1 = dark, 2 = light.
/// Returns `None` if the portal is unavailable.
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

/// Argument types supported by [`build_dbus_method_call`].
enum DValue<'a> {
    String(&'a str),
}

/// Build a little-endian D-Bus `METHOD_CALL` message with string arguments.
///
/// Encodes the 12-byte fixed header, header fields (PATH, INTERFACE, MEMBER,
/// DESTINATION, and optionally SIGNATURE), and a body of NUL-terminated,
/// 4-byte-aligned strings per the D-Bus wire protocol specification.
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
        // Signature header field (code 8).  The value is a VARIANT whose
        // contained type is SIGNATURE ('g').  Layout:
        //   [8-byte aligned struct start]
        //   u8  field code (8)
        //   u8  variant-sig length (1)
        //   u8  'g'              — the variant carries a SIGNATURE value
        //   u8  NUL terminator for the variant signature
        //   u8  body-sig length
        //   ... body-sig bytes
        //   u8  NUL terminator for the body signature
        while header_fields.len() % 8 != 0 { header_fields.push(0); }
        header_fields.push(8); // field code
        header_fields.push(1); // variant signature length: 1 byte
        header_fields.push(b'g'); // variant signature: SIGNATURE type
        header_fields.push(0); // NUL terminator for variant signature
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

/// Append a single D-Bus header field (struct aligned to 8 bytes) whose
/// value is a VARIANT containing a string or object-path.
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

/// Extract a `uint32` from a D-Bus method-return whose body is
/// `variant(variant(uint32))`.  Uses a heuristic: reads the last 4 bytes
/// of the body and accepts values 0–2 (the defined colour-scheme range).
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

/// Parse an `(f64, f64, f64)` accent colour from a D-Bus variant response.
///
/// Currently a stub — the `(ddd)` D-Bus struct is non-trivial to decode
/// from raw bytes.  Returns `None` so the caller falls back to GTK accent.
fn parse_rgb_from_variant_response(_data: &[u8]) -> Option<(f64, f64, f64)> {
    // accent-color is a (ddd) struct — complex to parse from raw bytes.
    // For now, return None and let the caller fall back to the GTK accent.
    None
}

extern "C" { fn getuid() -> u32; }
unsafe fn libc_getuid() -> u32 { getuid() }

/// Hex-encode a UID for the D-Bus `AUTH EXTERNAL` handshake.
///
/// Each ASCII digit of the decimal UID is converted to its two-char hex
/// representation (e.g. UID 1000 → "31303030").
fn hex_encode_uid(uid: u32) -> String {
    let uid_str = alloc::format!("{}", uid);
    let mut hex = String::new();
    for b in uid_str.bytes() {
        hex.push_str(&alloc::format!("{:02x}", b));
    }
    hex
}

// ── GSettings / CLI fallback helpers ─────────────────────────────────────

/// Run `gsettings get <schema> <key>` and return the trimmed, unquoted value.
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

/// Populate additional Linux-specific fields in `style` via `gsettings` CLI
/// queries and environment-variable fallbacks.
fn discover_linux_extras(style: &mut SystemStyle) {
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
        // Parse button side from layout: "close,minimize,maximize:" → Left
        //                                ":close,minimize,maximize" → Right
        let is_right = layout.starts_with(':');
        style.linux.titlebar_button_layout = OptionString::Some(layout.into());
        style.metrics.titlebar.button_side = if is_right {
            TitlebarButtonSide::Right
        } else {
            TitlebarButtonSide::Left
        };
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

    // ── Animation metrics ────────────────────────────────────────────
    if let Some(anim_s) = gsettings_get("org.gnome.desktop.interface", "enable-animations") {
        let enabled = anim_s.trim() != "false";
        style.animation.animations_enabled = enabled;
        if !enabled {
            style.prefers_reduced_motion = BoolCondition::True;
            style.accessibility.prefers_reduced_motion = true;
        }
    }

    // ── Audio metrics ────────────────────────────────────────────────
    if let Some(ev) = gsettings_get("org.gnome.desktop.sound", "event-sounds") {
        style.audio.event_sounds_enabled = ev.trim() != "false";
    }
    if let Some(inp) = gsettings_get("org.gnome.desktop.sound", "input-feedback-sounds") {
        style.audio.input_feedback_sounds_enabled = inp.trim() != "false";
    }

    // ── Visual hints ─────────────────────────────────────────────────
    // Note: these keys are deprecated in newer GNOME (3.28+) but still
    // respected by many GTK apps.  Safe to query; returns None if absent.
    if let Some(v) = gsettings_get("org.gnome.desktop.interface", "menus-have-icons") {
        style.visual_hints.show_menu_images = v.trim() != "false";
    }
    if let Some(v) = gsettings_get("org.gnome.desktop.interface", "buttons-have-icons") {
        style.visual_hints.show_button_images = v.trim() != "false";
    }
    if let Some(v) = gsettings_get("org.gnome.desktop.interface", "toolbar-style") {
        style.visual_hints.toolbar_style = match v.trim() {
            "text" => ToolbarStyle::TextOnly,
            "both" => ToolbarStyle::TextBelowIcon,
            "both-horiz" => ToolbarStyle::TextBesideIcon,
            _ => ToolbarStyle::IconsOnly,
        };
    }

    // ── Input extras (caret blink) ───────────────────────────────────
    if let Some(blink) = gsettings_get("org.gnome.desktop.interface", "cursor-blink") {
        if blink.trim() == "false" {
            style.input.caret_blink_rate_ms = 0;
        }
    }
    if let Some(blink_time) = gsettings_get("org.gnome.desktop.interface", "cursor-blink-time") {
        if let Ok(ms) = blink_time.trim().parse::<u32>() {
            style.input.caret_blink_rate_ms = ms;
        }
    }
}

// ── CLI subprocess helper ───────────────────────────────────────────────

/// Spawn a subprocess with a timeout and return its stdout as a trimmed string.
///
/// Returns `Err(())` if the process fails to spawn, exits non-zero, or the
/// timeout expires.
fn run_command_with_timeout(program: &str, args: &[&str], timeout_ms: u64) -> Result<String, ()> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(());
                }
                let output = child.wait_with_output().map_err(|_| ())?;
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                return Ok(s);
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err(());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return Err(()),
        }
    }
}

// ── CLI-based desktop environment discovery ─────────────────────────────

/// Discover system style from GNOME via `gsettings` CLI.
///
/// Queries the GTK theme name (dark vs light), font name, font size,
/// monospace font, and color-scheme preference.
fn discover_gnome_style() -> Result<SystemStyle, ()> {
    // Check color-scheme first (GNOME 42+)
    let color_scheme = gsettings_get("org.gnome.desktop.interface", "color-scheme")
        .unwrap_or_default();

    let is_dark = color_scheme.contains("prefer-dark")
        || gsettings_get("org.gnome.desktop.interface", "gtk-theme")
            .map(|t| t.to_lowercase().contains("dark"))
            .unwrap_or(false);

    let mut style = if is_dark {
        defaults::gnome_adwaita_dark()
    } else {
        defaults::gnome_adwaita_light()
    };

    // Font discovery
    if let Some(font_str) = gsettings_get("org.gnome.desktop.interface", "font-name") {
        // Format is typically "Cantarell 11" or "Ubuntu Regular 11"
        if let Some((name, size)) = parse_font_name_and_size(&font_str) {
            style.fonts.ui_font = OptionString::Some(name.into());
            style.fonts.ui_font_size = OptionF32::Some(size);
        } else {
            style.fonts.ui_font = OptionString::Some(font_str.into());
        }
    }

    if let Some(mono_str) = gsettings_get("org.gnome.desktop.interface", "monospace-font-name") {
        if let Some((name, size)) = parse_font_name_and_size(&mono_str) {
            style.fonts.monospace_font = OptionString::Some(name.into());
            style.fonts.monospace_font_size = OptionF32::Some(size);
        } else {
            style.fonts.monospace_font = OptionString::Some(mono_str.into());
        }
    }

    if let Some(title_str) = gsettings_get("org.gnome.desktop.wm.preferences", "titlebar-font") {
        if let Some((name, size)) = parse_font_name_and_size(&title_str) {
            style.fonts.title_font = OptionString::Some(name.into());
            style.fonts.title_font_size = OptionF32::Some(size);
        } else {
            style.fonts.title_font = OptionString::Some(title_str.into());
        }
    }

    // Accent color (GNOME 47+)
    if let Some(accent) = gsettings_get("org.gnome.desktop.interface", "accent-color") {
        // GNOME accent-color is a named color like "blue", "teal", "green", etc.
        let color = match accent.trim() {
            "blue"    => Some(ColorU::new_rgb( 53, 132, 228)),
            "teal"    => Some(ColorU::new_rgb( 38, 162, 105)),
            "green"   => Some(ColorU::new_rgb( 46, 194,  82)),
            "yellow"  => Some(ColorU::new_rgb(246, 211,  45)),
            "orange"  => Some(ColorU::new_rgb(255, 120,   0)),
            "red"     => Some(ColorU::new_rgb(237,  51,  59)),
            "pink"    => Some(ColorU::new_rgb(220,  79, 133)),
            "purple"  => Some(ColorU::new_rgb(145,  65, 172)),
            "slate"   => Some(ColorU::new_rgb(111, 131, 150)),
            _ => None,
        };
        if let Some(c) = color {
            style.colors.accent = OptionColorU::Some(c);
        }
    }

    Ok(style)
}

/// Discover system style from KDE Plasma via `kreadconfig5` / `kreadconfig6`.
///
/// Queries kdeglobals for theme, fonts, and color scheme.
fn discover_kde_style() -> Result<SystemStyle, ()> {
    // Try kreadconfig6 first (Plasma 6), fall back to kreadconfig5
    let kread = if run_command_with_timeout("kreadconfig6", &["--help"], 500).is_ok() {
        "kreadconfig6"
    } else if run_command_with_timeout("kreadconfig5", &["--help"], 500).is_ok() {
        "kreadconfig5"
    } else {
        return Err(());
    };

    // Detect dark/light from color scheme name
    let color_scheme_name = run_command_with_timeout(
        kread,
        &["--group", "General", "--key", "ColorScheme"],
        1000,
    ).unwrap_or_default();

    let is_dark = color_scheme_name.to_lowercase().contains("dark");

    let mut style = if is_dark {
        // Use GNOME dark as base, then override with KDE specifics
        let mut s = defaults::gnome_adwaita_dark();
        s.platform = Platform::Linux(DesktopEnvironment::Kde);
        s
    } else {
        let mut s = defaults::gnome_adwaita_light();
        s.platform = Platform::Linux(DesktopEnvironment::Kde);
        s
    };

    style.theme = if is_dark { Theme::Dark } else { Theme::Light };

    // Font discovery
    if let Ok(font_str) = run_command_with_timeout(
        kread,
        &["--group", "General", "--key", "font"],
        1000,
    ) {
        // KDE font format: "Noto Sans,10,-1,5,50,0,0,0,0,0"
        let parts: Vec<&str> = font_str.split(',').collect();
        if parts.len() >= 2 {
            style.fonts.ui_font = OptionString::Some(parts[0].trim().into());
            if let Ok(size) = parts[1].trim().parse::<f32>() {
                style.fonts.ui_font_size = OptionF32::Some(size);
            }
        }
    }

    if let Ok(fixed_font) = run_command_with_timeout(
        kread,
        &["--group", "General", "--key", "fixed"],
        1000,
    ) {
        let parts: Vec<&str> = fixed_font.split(',').collect();
        if parts.len() >= 2 {
            style.fonts.monospace_font = OptionString::Some(parts[0].trim().into());
            if let Ok(size) = parts[1].trim().parse::<f32>() {
                style.fonts.monospace_font_size = OptionF32::Some(size);
            }
        }
    }

    // Accent / highlight color
    if let Ok(highlight) = run_command_with_timeout(
        kread,
        &["--group", "Colors:Selection", "--key", "BackgroundNormal"],
        1000,
    ) {
        // Format: "r,g,b" e.g. "61,174,233"
        let parts: Vec<&str> = highlight.split(',').collect();
        if parts.len() >= 3 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                parts[0].trim().parse::<u8>(),
                parts[1].trim().parse::<u8>(),
                parts[2].trim().parse::<u8>(),
            ) {
                style.colors.accent = OptionColorU::Some(ColorU::new_rgb(r, g, b));
            }
        }
    }

    // Window decoration button layout
    if let Ok(layout) = run_command_with_timeout(
        kread,
        &["--group", "org.kde.kdecoration2", "--key", "ButtonsOnLeft"],
        1000,
    ) {
        if !layout.is_empty() {
            style.metrics.titlebar.button_side = TitlebarButtonSide::Left;
        }
    }

    // GTK theme used under KDE (for consistency)
    if let Some(gtk) = gsettings_get("org.gnome.desktop.interface", "gtk-theme") {
        style.linux.gtk_theme = OptionString::Some(gtk.into());
    }

    Ok(style)
}

/// Discover system style from "riced" desktops: Hyprland, Sway, i3, pywal.
///
/// Checks for pywal `colors.json`, parses Hyprland/Sway/i3 configs for
/// rounding, borders, and accent colors.  Falls back to gsettings for the
/// GTK font if available.
fn discover_riced_style() -> Result<SystemStyle, ()> {
    let home = std::env::var("HOME").map_err(|_| ())?;

    let is_hyprland = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok();
    let is_sway = std::env::var("SWAYSOCK").is_ok();
    let is_i3 = std::env::var("I3SOCK").is_ok();

    if !is_hyprland && !is_sway && !is_i3 {
        // Not a known riced WM
        return Err(());
    }

    let mut style = defaults::gnome_adwaita_dark();

    let de_name = if is_hyprland {
        "Hyprland"
    } else if is_sway {
        "Sway"
    } else {
        "i3"
    };
    style.platform = Platform::Linux(DesktopEnvironment::Other(AzString::from(de_name)));

    // ── pywal colors ────────────────────────────────────────────────
    let pywal_path = alloc::format!("{}/.cache/wal/colors.json", home);
    if let Ok(json_str) = std::fs::read_to_string(&pywal_path) {
        parse_pywal_colors(&json_str, &mut style);
    }

    // ── Hyprland config ─────────────────────────────────────────────
    if is_hyprland {
        let hypr_conf = alloc::format!("{}/.config/hypr/hyprland.conf", home);
        if let Ok(conf) = std::fs::read_to_string(&hypr_conf) {
            parse_hyprland_config(&conf, &mut style);
        }
    }

    // ── Sway config ─────────────────────────────────────────────────
    if is_sway {
        let sway_conf = alloc::format!("{}/.config/sway/config", home);
        if let Ok(conf) = std::fs::read_to_string(&sway_conf) {
            parse_sway_config(&conf, &mut style);
        }
    }

    // ── i3 config ───────────────────────────────────────────────────
    if is_i3 {
        let i3_conf = alloc::format!("{}/.config/i3/config", home);
        if let Ok(conf) = std::fs::read_to_string(&i3_conf) {
            parse_sway_config(&conf, &mut style); // i3 and sway share similar config syntax
        }
    }

    // ── GTK font fallback via gsettings ─────────────────────────────
    if style.fonts.ui_font.is_none() {
        if let Some(font_str) = gsettings_get("org.gnome.desktop.interface", "font-name") {
            if let Some((name, size)) = parse_font_name_and_size(&font_str) {
                style.fonts.ui_font = OptionString::Some(name.into());
                style.fonts.ui_font_size = OptionF32::Some(size);
            }
        }
    }

    Ok(style)
}

/// Parse pywal `colors.json` and apply to the style.
///
/// Expected format (simplified):
/// ```json
/// {
///   "special": { "background": "#1a1b26", "foreground": "#c0caf5", "cursor": "#c0caf5" },
///   "colors": { "color0": "#1a1b26", "color1": "#f7768e", ... "color15": "#c0caf5" }
/// }
/// ```
fn parse_pywal_colors(json_str: &str, style: &mut SystemStyle) {
    // Minimal JSON extraction — no serde needed for this flat structure
    fn extract_json_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
        let pattern = alloc::format!("\"{}\"", key);
        let idx = json.find(&pattern)?;
        let after_key = &json[idx + pattern.len()..];
        // Skip whitespace and colon
        let after_colon = after_key.find(':').map(|i| &after_key[i + 1..])?;
        let trimmed = after_colon.trim_start();
        if trimmed.starts_with('"') {
            let start = 1;
            let end = trimmed[start..].find('"')?;
            Some(&trimmed[start..start + end])
        } else {
            None
        }
    }

    if let Some(bg) = extract_json_value(json_str, "background") {
        if let Ok(c) = parse_css_color(bg) {
            style.colors.window_background = OptionColorU::Some(c);
            style.theme = Theme::Dark; // pywal usually means dark
        }
    }

    if let Some(fg) = extract_json_value(json_str, "foreground") {
        if let Ok(c) = parse_css_color(fg) {
            style.colors.text = OptionColorU::Some(c);
        }
    }

    if let Some(cursor) = extract_json_value(json_str, "cursor") {
        if let Ok(c) = parse_css_color(cursor) {
            style.colors.accent = OptionColorU::Some(c);
        }
    }

    // Try color1 as an accent alternative if cursor wasn't useful
    if style.colors.accent.is_none() {
        if let Some(color1) = extract_json_value(json_str, "color1") {
            if let Ok(c) = parse_css_color(color1) {
                style.colors.accent = OptionColorU::Some(c);
            }
        }
    }
}

/// Parse Hyprland config for rounding, border_size, and `col.active_border`.
fn parse_hyprland_config(conf: &str, style: &mut SystemStyle) {
    for line in conf.lines() {
        let line = line.trim();
        // Skip comments
        if line.starts_with('#') { continue; }

        if let Some(val) = extract_config_value(line, "rounding") {
            if let Ok(px) = val.parse::<f32>() {
                style.metrics.corner_radius = OptionPixelValue::Some(
                    PixelValue::from_metric(azul_css::props::basic::length::SizeMetric::Px, px)
                );
            }
        }

        if let Some(val) = extract_config_value(line, "border_size") {
            if let Ok(px) = val.parse::<f32>() {
                style.focus_visuals.focus_border_width = OptionPixelValue::Some(
                    PixelValue::from_metric(azul_css::props::basic::length::SizeMetric::Px, px)
                );
            }
        }

        if let Some(val) = extract_config_value(line, "col.active_border") {
            // Hyprland colors: "rgba(33ccffee)" or "rgb(33ccff)"
            let color_str = val.trim();
            if let Some(hex) = color_str.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
                if let Ok(c) = parse_css_color(&alloc::format!("#{}", hex)) {
                    style.colors.accent = OptionColorU::Some(c);
                }
            } else if let Some(hex) = color_str.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
                if let Ok(c) = parse_css_color(&alloc::format!("#{}", hex)) {
                    style.colors.accent = OptionColorU::Some(c);
                }
            }
        }
    }
}

/// Parse Sway/i3 config for border-related settings and accent colors.
fn parse_sway_config(conf: &str, style: &mut SystemStyle) {
    for line in conf.lines() {
        let line = line.trim();
        if line.starts_with('#') { continue; }

        // "default_border pixel 2"
        if line.starts_with("default_border") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[1] == "pixel" {
                if let Ok(px) = parts[2].parse::<f32>() {
                    style.focus_visuals.focus_border_width = OptionPixelValue::Some(
                        PixelValue::from_metric(azul_css::props::basic::length::SizeMetric::Px, px)
                    );
                }
            }
        }

        // "client.focused #4c7899 #285577 #ffffff #2e9ef4 #285577"
        if line.starts_with("client.focused ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // parts[1] = border, parts[2] = background, parts[3] = text, parts[4] = indicator
            if parts.len() >= 3 {
                if let Ok(c) = parse_css_color(parts[2]) {
                    style.colors.accent = OptionColorU::Some(c);
                }
            }
        }

        // "font pango:DejaVu Sans Mono 10"
        if line.starts_with("font ") {
            let rest = line.strip_prefix("font ").unwrap_or("");
            let rest = rest.strip_prefix("pango:").unwrap_or(rest);
            if let Some((name, size)) = parse_font_name_and_size(rest) {
                style.fonts.ui_font = OptionString::Some(name.into());
                style.fonts.ui_font_size = OptionF32::Some(size);
            }
        }
    }
}

/// Extract a value from a "key = value" or "key value" config line.
fn extract_config_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let line = line.trim();
    if !line.starts_with(key) {
        return None;
    }
    let after_key = &line[key.len()..];
    if after_key.is_empty() {
        return None;
    }
    // The character immediately after the key must be whitespace or '='
    // to avoid matching a longer keyword (e.g. "rounding_power" for "rounding").
    let first = after_key.as_bytes()[0];
    if first != b'=' && !first.is_ascii_whitespace() {
        return None;
    }
    let rest = after_key.trim_start();
    if rest.starts_with('=') {
        Some(rest[1..].trim())
    } else {
        // "key value" form (whitespace separator)
        Some(rest)
    }
}

// ── OS version detection ────────────────────────────────────────────────

/// Detect the Linux kernel version by running `uname -r`.
///
/// Returns `OsVersion::unknown()` if detection fails.
fn detect_linux_version() -> OsVersion {
    let release = match run_command_with_timeout("uname", &["-r"], 1000) {
        Ok(s) => s,
        Err(_) => return OsVersion::unknown(),
    };

    // "6.5.0-44-generic" → major=6, minor=5
    let parts: Vec<&str> = release.split('.').collect();
    if parts.len() >= 2 {
        if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
            // Encode as major * 1000 + minor to allow ordering
            let version_id = major * 1000 + minor;
            return OsVersion::new(OsFamily::Linux, version_id);
        }
    }

    OsVersion::unknown()
}

// ── Accessibility queries ───────────────────────────────────────────────

/// Detect GNOME reduced-motion preference via `gsettings`.
fn detect_gnome_reduced_motion() -> BoolCondition {
    match gsettings_get("org.gnome.desktop.interface", "enable-animations") {
        Some(val) => {
            if val.trim() == "false" {
                BoolCondition::True // reduced motion IS preferred
            } else {
                BoolCondition::False
            }
        }
        None => BoolCondition::False,
    }
}

/// Detect GNOME high-contrast theme via `gsettings`.
fn detect_gnome_high_contrast() -> BoolCondition {
    match gsettings_get("org.gnome.desktop.interface", "high-contrast") {
        Some(val) => {
            if val.trim() == "true" {
                BoolCondition::True
            } else {
                BoolCondition::False
            }
        }
        None => {
            // Also check if the GTK theme name contains "HighContrast"
            match gsettings_get("org.gnome.desktop.interface", "gtk-theme") {
                Some(theme) if theme.contains("HighContrast") => BoolCondition::True,
                _ => BoolCondition::False,
            }
        }
    }
}

/// Detect KDE reduced-motion preference via `kreadconfig5`/`kreadconfig6`.
fn detect_kde_reduced_motion() -> BoolCondition {
    // Try kreadconfig6 first, then kreadconfig5
    let kread = if run_command_with_timeout("kreadconfig6", &["--help"], 500).is_ok() {
        "kreadconfig6"
    } else {
        "kreadconfig5"
    };

    match run_command_with_timeout(
        kread,
        &["--group", "KDE", "--key", "AnimationDurationFactor"],
        1000,
    ) {
        Ok(val) => {
            // A factor of 0 means animations are disabled
            match val.trim().parse::<f32>() {
                Ok(factor) if factor <= 0.0 => BoolCondition::True,
                _ => BoolCondition::False,
            }
        }
        Err(_) => BoolCondition::False,
    }
}

// ── Language detection ──────────────────────────────────────────────────

/// Detect the user's language from environment variables.
///
/// Priority: `LANGUAGE` > `LANG` > `LC_ALL`.  Returns a BCP 47-style tag
/// (e.g. "en-US").  Falls back to "en-US" if nothing is set.
fn detect_language_linux() -> AzString {
    // LANGUAGE can contain a colon-separated list; take the first entry
    if let Ok(lang) = std::env::var("LANGUAGE") {
        let first = lang.split(':').next().unwrap_or("en_US");
        let first = first.split('.').next().unwrap_or("en_US");
        if !first.is_empty() {
            return AzString::from(first.replace('_', "-"));
        }
    }
    if let Ok(lang) = std::env::var("LANG") {
        let lang = lang.split('.').next().unwrap_or("en_US");
        if !lang.is_empty() && lang != "C" && lang != "POSIX" {
            return AzString::from(lang.replace('_', "-"));
        }
    }
    if let Ok(lang) = std::env::var("LC_ALL") {
        let lang = lang.split('.').next().unwrap_or("en_US");
        if !lang.is_empty() && lang != "C" && lang != "POSIX" {
            return AzString::from(lang.replace('_', "-"));
        }
    }
    AzString::from_const_str("en-US")
}

// ── App-specific stylesheet loading ─────────────────────────────────────

/// Load an application-specific stylesheet from the user's config directory.
///
/// Path: `<config_dir>/azul/styles/<exe_name>.css`
///
/// Config directory is determined by:
/// - Linux:   `$XDG_CONFIG_HOME` or `~/.config`
/// - macOS:   `~/Library/Application Support`
/// - Windows: `%APPDATA%`
///
/// Returns `None` if the file does not exist or cannot be parsed.
fn load_app_specific_stylesheet() -> Option<Css> {
    // Bail out if ricing is disabled
    if std::env::var("AZUL_DISABLE_RICING").is_ok() {
        return None;
    }

    let exe_name = std::env::current_exe().ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))?;

    let config_dir = get_config_dir()?;

    let css_path = alloc::format!("{}/azul/styles/{}.css", config_dir, exe_name);
    let css_str = std::fs::read_to_string(&css_path).ok()?;
    let (css, _warnings) = new_from_str(&css_str);
    if css.is_empty() { None } else { Some(css) }
}

/// Get the platform-appropriate user config directory.
fn get_config_dir() -> Option<String> {
    // On Linux, prefer XDG_CONFIG_HOME, fall back to ~/.config
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(xdg);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(alloc::format!("{}/.config", home));
    }
    None
}

// ── Font parsing helper ─────────────────────────────────────────────────

/// Parse a font string like "Cantarell 11" or "Ubuntu Bold 12" into
/// (font_name, size).  The size is the last whitespace-separated token
/// that can be parsed as a float.
fn parse_font_name_and_size(s: &str) -> Option<(String, f32)> {
    let s = s.trim();
    if let Some(last_space) = s.rfind(' ') {
        let (name_part, size_part) = s.split_at(last_space);
        if let Ok(size) = size_part.trim().parse::<f32>() {
            return Some((name_part.trim().to_string(), size));
        }
    }
    None
}

// ── Public entry point ───────────────────────────────────────────────────

/// Discover the Linux system style.
///
/// Tries XDG Desktop Portal first (raw D-Bus), then CLI-based discovery
/// (KDE, GNOME, riced desktops), and finally hardcoded GNOME Adwaita defaults.
pub(crate) fn discover() -> SystemStyle {

    // ── 1. Try XDG Desktop Portal (D-Bus) ───────────────────────────
    let portal_result = query_xdg_portal();

    if let Some((color_scheme, accent_rgb)) = portal_result {
        let mut style = match color_scheme {
            1 => defaults::gnome_adwaita_dark(),   // prefer-dark
            2 => defaults::gnome_adwaita_light(),   // prefer-light
            _ => defaults::gnome_adwaita_light(),   // no preference
        };

        if let Some((r, g, b)) = accent_rgb {
            style.colors.accent = OptionColorU::Some(ColorU::new_rgb(
                (r.clamp(0.0, 1.0) * 255.0) as u8,
                (g.clamp(0.0, 1.0) * 255.0) as u8,
                (b.clamp(0.0, 1.0) * 255.0) as u8,
            ));
        }

        // Even with portal success, fill in extras from gsettings
        discover_linux_extras(&mut style);
        style.platform = Platform::Linux(azul_css::system::detect_linux_desktop_env());
        style.language = detect_language_linux();
        style.os_version = detect_linux_version();
        style.prefers_reduced_motion = detect_gnome_reduced_motion();
        style.prefers_high_contrast = detect_gnome_high_contrast();
        style.app_specific_stylesheet = load_app_specific_stylesheet().map(Box::new);
        return style;
    }

    // ── 2. CLI-based discovery ──────────────────────────────────────
    // Check for the "smoke and mirrors" easter egg — skip standard DE
    // detection and go straight to riced desktop parsing.
    let smoke = std::env::var("AZUL_SMOKE_AND_MIRRORS").is_ok();

    let mut style = if smoke {
        discover_riced_style()
            .or_else(|_| discover_kde_style())
            .or_else(|_| discover_gnome_style())
            .unwrap_or_else(|_| defaults::gnome_adwaita_light())
    } else {
        // Normal priority: KDE > GNOME > riced > defaults
        let desktop_env = azul_css::system::detect_linux_desktop_env();
        match &desktop_env {
            DesktopEnvironment::Kde => {
                discover_kde_style()
                    .or_else(|_| discover_gnome_style())
                    .unwrap_or_else(|_| defaults::gnome_adwaita_light())
            }
            DesktopEnvironment::Gnome => {
                discover_gnome_style()
                    .or_else(|_| discover_kde_style())
                    .unwrap_or_else(|_| defaults::gnome_adwaita_light())
            }
            DesktopEnvironment::Other(_) => {
                discover_riced_style()
                    .or_else(|_| discover_gnome_style())
                    .or_else(|_| discover_kde_style())
                    .unwrap_or_else(|_| defaults::gnome_adwaita_light())
            }
        }
    };

    // ── 3. Fill in extras and metadata ──────────────────────────────
    discover_linux_extras(&mut style);
    style.platform = Platform::Linux(azul_css::system::detect_linux_desktop_env());
    style.language = detect_language_linux();
    style.os_version = detect_linux_version();

    // Accessibility — try GNOME first, then KDE
    if style.prefers_reduced_motion == BoolCondition::False {
        style.prefers_reduced_motion = detect_gnome_reduced_motion();
    }
    if style.prefers_reduced_motion == BoolCondition::False {
        style.prefers_reduced_motion = detect_kde_reduced_motion();
    }
    style.prefers_high_contrast = detect_gnome_high_contrast();

    // App-specific ricing stylesheet
    style.app_specific_stylesheet = load_app_specific_stylesheet().map(Box::new);

    style
}
