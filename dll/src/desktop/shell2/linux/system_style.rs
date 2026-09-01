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

use alloc::boxed::Box;
use alloc::string::String;
use azul_css::corety::{AzString, OptionF32, OptionString};
use azul_css::css::Css;
use azul_css::dynamic_selector::{BoolCondition, OsFamily, OsVersion};
use azul_css::parser2::new_from_str;
use azul_css::props::basic::color::{parse_css_color, ColorU, OptionColorU};
use azul_css::props::basic::pixel::{OptionPixelValue, PixelValue};
use azul_css::system::{
    defaults, DesktopEnvironment, Platform, ScrollbarVisibility, SubpixelType, SystemStyle, Theme,
    TitlebarButtonSide, ToolbarStyle,
};

// ── D-Bus wire-protocol helpers (minimal, read-only) ─────────────────────

/// Write every byte of `buf` to a Unix-domain socket with `MSG_NOSIGNAL`, so a
/// peer that has closed the connection yields `EPIPE` instead of raising
/// `SIGPIPE`.
///
/// Rust installs `SIG_IGN` for `SIGPIPE` in its own runtime startup, but azul is
/// frequently loaded into a *host* process — most notably the Python extension
/// (`import azul`), where `main()` is CPython's and Rust's startup never ran, so
/// `SIGPIPE` keeps its default *terminate* disposition. A plain
/// `UnixStream::write_all` to the session bus would then kill the whole host
/// process the moment the D-Bus daemon or an absent xdg-desktop-portal drops the
/// socket mid-handshake (observed: SIGPIPE at the portal `Settings.Read` write).
/// Using `MSG_NOSIGNAL` keeps the failure local — the caller's `.ok()?` turns it
/// into a clean `None` and we fall back to CLI/defaults. This never changes the
/// process-wide `SIGPIPE` disposition, which a library must not do behind the
/// host's back.
fn send_all_nosignal(
    stream: &std::os::unix::net::UnixStream,
    mut buf: &[u8],
) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    let fd = stream.as_raw_fd();
    while !buf.is_empty() {
        let ret = unsafe {
            libc::send(
                fd,
                buf.as_ptr() as *const libc::c_void,
                buf.len(),
                libc::MSG_NOSIGNAL,
            )
        };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        if ret == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "send returned 0",
            ));
        }
        buf = &buf[ret as usize..];
    }
    Ok(())
}

/// Read the XDG Desktop Portal `org.freedesktop.appearance` settings.
///
/// Returns `(color_scheme, accent_color_rgb)` where color_scheme is:
///   0 = no preference, 1 = dark, 2 = light.
/// Returns `None` if the portal is unavailable.
fn query_xdg_portal() -> Option<(u32, Option<(f64, f64, f64)>)> {
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    // Connect to session D-Bus
    let bus_addr = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok()?;
    // Parse "unix:path=/run/user/1000/bus" or similar
    let path = bus_addr.strip_prefix("unix:path=")?;
    // Handle additional parameters after comma
    let path = path.split(',').next()?;

    let mut stream = UnixStream::connect(path).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .ok()?;

    // D-Bus authentication: simplest method is EXTERNAL with uid
    let uid = unsafe { libc_getuid() };
    let auth_msg = alloc::format!("\0AUTH EXTERNAL {}\r\nBEGIN\r\n", hex_encode_uid(uid));
    send_all_nosignal(&stream, auth_msg.as_bytes()).ok()?;

    // Read auth response (we just need "OK <guid>")
    let mut buf = [0u8; 256];
    let n = stream.read(&mut buf).ok()?;
    let resp = core::str::from_utf8(&buf[..n]).ok()?;
    if !resp.contains("OK") {
        return None;
    }

    // Send Hello message to get our unique name (required before any method call)
    let hello_msg = build_dbus_method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "Hello",
        &[],
        1,
    );
    send_all_nosignal(&stream, &hello_msg).ok()?;
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
    send_all_nosignal(&stream, &read_msg).ok()?;

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
    send_all_nosignal(&stream, &accent_msg).ok()?;

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
                while body.len() % 4 != 0 {
                    body.push(0);
                }
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
        while header_fields.len() % 8 != 0 {
            header_fields.push(0);
        }
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
    while header_fields.len() % 8 != 0 {
        header_fields.push(0);
    }

    let mut msg = alloc::vec::Vec::new();
    // Fixed header: endianness(1) + type(1) + flags(1) + version(1)
    msg.push(b'l'); // little-endian
    msg.push(1); // METHOD_CALL
    msg.push(0); // flags
    msg.push(1); // protocol version
                 // body length (uint32)
    msg.extend_from_slice(&(body.len() as u32).to_le_bytes());
    // serial (uint32)
    msg.extend_from_slice(&serial.to_le_bytes());
    // header fields array length (uint32)
    msg.extend_from_slice(&(header_fields.len() as u32).to_le_bytes());
    // header fields
    msg.extend_from_slice(&header_fields);
    // Pad to 8-byte alignment before body
    while msg.len() % 8 != 0 {
        msg.push(0);
    }
    // body
    msg.extend_from_slice(&body);

    msg
}

/// Append a single D-Bus header field (struct aligned to 8 bytes) whose
/// value is a VARIANT containing a string or object-path.
fn append_header_field(buf: &mut alloc::vec::Vec<u8>, code: u8, sig: char, value: &str) {
    // Align to 8 bytes (start of struct)
    while buf.len() % 8 != 0 {
        buf.push(0);
    }
    buf.push(code);
    // variant signature
    buf.push(1); // sig length
    buf.push(sig as u8);
    buf.push(0); // NUL
                 // Pad to 4 bytes for the string/object-path value
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
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
    if data.len() < 16 {
        return None;
    }
    // Skip the 12-byte fixed header + header fields to find the body
    let body_len = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    let header_fields_len = u32::from_le_bytes(data[12..16].try_into().ok()?) as usize;
    let body_start = 16 + header_fields_len;
    // Align to 8
    let body_start = (body_start + 7) & !7;
    if body_start + body_len > data.len() {
        return None;
    }
    let body = &data[body_start..body_start + body_len];
    // The body is variant(variant(uint32)).  The uint32 is at the end.
    if body.len() >= 4 {
        let val = u32::from_le_bytes(body[body.len() - 4..].try_into().ok()?);
        if val <= 2 {
            return Some(val);
        }
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

extern "C" {
    fn getuid() -> u32;
}
unsafe fn libc_getuid() -> u32 {
    getuid()
}

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
fn gsettings_get_raw(schema: &str, key: &str) -> Option<String> {
    use std::process::{Command, Stdio};
    let out = Command::new("gsettings")
        .args(["get", schema, key])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if out.status.success() {
        Some(
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .trim_matches('\'')
                .to_string(),
        )
    } else {
        None
    }
}

/// The GSettings schema families that answer the same questions as GNOME's.
///
/// Cinnamon and MATE are GNOME forks: they kept the key NAMES and renamed the
/// SCHEMAS. A Cinnamon (Linux Mint) session therefore has a fully-configured
/// desktop that `org.gnome.desktop.interface` knows nothing about — every
/// query returned `None` and the whole style fell back to built-in defaults,
/// so Mint's font, theme, icon theme, cursor and titlebar layout were all
/// invisible to us. Mapping the schema names is the entire fix; the keys
/// already match.
fn schema_family(gnome_schema: &str) -> Vec<String> {
    let mut out = vec![gnome_schema.to_string()];
    // `org.gnome.desktop.interface` -> `org.cinnamon.desktop.interface`
    if let Some(rest) = gnome_schema.strip_prefix("org.gnome.") {
        out.push(alloc::format!("org.cinnamon.{rest}"));
        // MATE flattened `desktop.` away: `org.mate.interface`, `org.mate.sound`.
        let mate_rest = rest.strip_prefix("desktop.").unwrap_or(rest);
        out.push(match mate_rest {
            // Marco is MATE's window manager; its prefs live under its own name.
            "wm.preferences" => "org.mate.Marco.general".to_string(),
            other => alloc::format!("org.mate.{other}"),
        });
    }
    out
}

/// Which schema family answered last, so a Cinnamon session does not pay a
/// failed GNOME spawn on every single key.
static PREFERRED_SCHEMA_IDX: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// Read a desktop setting by its GNOME schema name, transparently accepting
/// the Cinnamon and MATE spellings of the same schema.
fn gsettings_get(schema: &str, key: &str) -> Option<String> {
    use core::sync::atomic::Ordering;

    let candidates = schema_family(schema);
    // Start with whichever family last answered — on Cinnamon that skips a
    // guaranteed-failing GNOME spawn per key, and there are dozens of keys.
    let start = PREFERRED_SCHEMA_IDX.load(Ordering::Relaxed);
    for i in 0..candidates.len() {
        let idx = (start + i) % candidates.len();
        if let Some(v) = gsettings_get_raw(&candidates[idx], key) {
            PREFERRED_SCHEMA_IDX.store(idx, Ordering::Relaxed);
            return Some(v);
        }
    }
    None
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
    // Button layout (determines button side for CSD).
    //
    // The layout is `left:right`, and the side is decided by where CLOSE is —
    // not by whether the string starts with ':'. The stock KDE/GNOME layout is
    // `icon:minimize,maximize,close`: the LEFT half holds the window-menu icon
    // and the controls are on the RIGHT. `starts_with(':')` reads that as
    // "left", so every default desktop was reported as buttons-on-left and CSD
    // drew its controls on the wrong side.
    if let Some(layout) = gsettings_get("org.gnome.desktop.wm.preferences", "button-layout") {
        style.metrics.titlebar.button_side = titlebar_side_from_layout(&layout);
        // Which buttons exist at all: a `:close`-only session must not get
        // minimize/maximize drawn into its titlebar.
        style.metrics.titlebar.buttons.has_close = layout.contains("close");
        style.metrics.titlebar.buttons.has_minimize = layout.contains("minimize");
        style.metrics.titlebar.buttons.has_maximize = layout.contains("maximize");
        style.linux.titlebar_button_layout = OptionString::Some(layout.into());
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
// ── KDE config files (read directly, no subprocess) ─────────────────────

/// One `[Group] key=value` INI file, parsed once.
///
/// `kdeglobals` and a `.colors` colour-scheme file share this exact format,
/// so one parser serves both. Reading the file beats shelling out to
/// `kreadconfig`: the palette alone is ~20 keys, and each `kreadconfig` call
/// is a process spawn with a timeout attached — that was up to 20 spawns on
/// the startup path, before the first frame. It also works on a session where
/// `kreadconfig` is not installed at all (a Qt app brings it; a pure GTK
/// desktop running a KDE colour scheme does not).
#[derive(Default)]
struct KdeIni {
    /// `(group, key) -> value`, in file order (later wins, like KConfig).
    entries: alloc::collections::BTreeMap<(String, String), String>,
}

impl KdeIni {
    fn parse(text: &str) -> Self {
        let mut out = Self::default();
        let mut group = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                // `[Colors:Header][Inactive]` — the trailing sub-group is a
                // state qualifier; the base group is everything up to the
                // first `]`, and a qualified entry must NOT overwrite the
                // plain one (`[Colors:Window][Inactive]` is not the window's
                // normal colour).
                if let Some(end) = rest.find(']') {
                    let base = &rest[..end];
                    let qualified = rest[end + 1..].contains('[');
                    group = if qualified {
                        // Park qualified groups under a name nothing reads.
                        alloc::format!("{base}::qualified")
                    } else {
                        base.to_string()
                    };
                }
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                out.entries.insert(
                    (group.clone(), k.trim().to_string()),
                    v.trim().to_string(),
                );
            }
        }
        out
    }

    fn read(path: &str) -> Option<Self> {
        std::fs::read_to_string(path).ok().map(|t| Self::parse(&t))
    }

    fn get(&self, group: &str, key: &str) -> Option<&str> {
        self.entries
            .get(&(group.to_string(), key.to_string()))
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// A `r,g,b` triple, the only colour spelling KDE config uses.
    fn color(&self, group: &str, key: &str) -> Option<ColorU> {
        let v = self.get(group, key)?;
        let p: Vec<&str> = v.split(',').collect();
        if p.len() < 3 {
            return None;
        }
        Some(ColorU::new_rgb(
            p[0].trim().parse().ok()?,
            p[1].trim().parse().ok()?,
            p[2].trim().parse().ok()?,
        ))
    }

    /// A KDE font spec: `Noto Sans,10,-1,5,50,0,0,0,0,0` -> (family, pt).
    fn font(&self, group: &str, key: &str) -> Option<(String, Option<f32>)> {
        let v = self.get(group, key)?;
        let mut parts = v.split(',');
        let family = parts.next()?.trim();
        if family.is_empty() {
            return None;
        }
        Some((
            family.to_string(),
            parts.next().and_then(|p| p.trim().parse::<f32>().ok()),
        ))
    }
}

/// The colour source for a KDE session: the user's `kdeglobals` first, then
/// the `.colors` file of the scheme it names.
///
/// A freshly-installed Plasma writes only `ColorScheme=BreezeDark` into
/// `kdeglobals` and leaves the actual `Colors:*` groups to the scheme file in
/// `/usr/share/color-schemes`, so reading `kdeglobals` alone finds a NAME and
/// no colours — and every palette slot silently kept its built-in default.
fn kde_color_sources() -> Vec<KdeIni> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut out = Vec::new();

    // The cursor theme lives in kcminputrc (`[Mouse] cursorTheme`), not in
    // kdeglobals — reading only kdeglobals found no cursor theme on any KDE
    // session. Pushed first so it is consulted like any other source.
    if let Some(ini) = KdeIni::read(&alloc::format!("{home}/.config/kcminputrc")) {
        out.push(ini);
    }

    let globals = KdeIni::read(&alloc::format!("{home}/.config/kdeglobals"));
    let scheme_name = globals
        .as_ref()
        .and_then(|g| g.get("General", "ColorScheme").map(String::from));
    if let Some(g) = globals {
        out.push(g);
    }

    if let Some(name) = scheme_name {
        // KConfig strips spaces from the scheme name when it looks for the
        // file ("Breeze Dark" -> BreezeDark.colors).
        let file = name.replace(' ', "");
        for dir in [
            alloc::format!("{home}/.local/share/color-schemes"),
            "/usr/share/color-schemes".to_string(),
            "/usr/local/share/color-schemes".to_string(),
        ] {
            if let Some(ini) = KdeIni::read(&alloc::format!("{dir}/{file}.colors")) {
                out.push(ini);
                break;
            }
        }
    }
    out
}

/// Which side of the titlebar the window CONTROLS sit on, from a
/// `left:right` button-layout string.
///
/// Decided by where CLOSE is. The stock KDE/GNOME layout is
/// `icon:minimize,maximize,close` — the left half holds the window-menu icon
/// and the controls are on the RIGHT — so "is the left half non-empty" and
/// "does the string start with ':'" both answer LEFT for a desktop whose
/// buttons are plainly on the right.
fn titlebar_side_from_layout(layout: &str) -> TitlebarButtonSide {
    let (left, _right) = layout.split_once(':').unwrap_or((layout, ""));
    if left.contains("close") {
        TitlebarButtonSide::Left
    } else {
        TitlebarButtonSide::Right
    }
}

fn discover_gnome_style() -> Result<SystemStyle, ()> {
    // Check color-scheme first (GNOME 42+)
    let color_scheme =
        gsettings_get("org.gnome.desktop.interface", "color-scheme").unwrap_or_default();

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

    // ── Text scaling ────────────────────────────────────────────────
    // GNOME's accessibility "Large Text" and every fractional-scaling setup
    // express themselves here, NOT in the font name — a user at 1.25 sees
    // every GTK app at 125 % and azul at 100 %, which is the complaint that
    // reads as "the fonts are tiny in this app". Applied to each detected
    // size, so it composes with whatever the font settings said.
    let text_scale = gsettings_get("org.gnome.desktop.interface", "text-scaling-factor")
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|f| *f > 0.0 && (*f - 1.0).abs() > f32::EPSILON);
    if let Some(scale) = text_scale {
        let apply = |sz: &mut OptionF32| {
            if let OptionF32::Some(v) = sz {
                *sz = OptionF32::Some(*v * scale);
            }
        };
        apply(&mut style.fonts.ui_font_size);
        apply(&mut style.fonts.monospace_font_size);
        apply(&mut style.fonts.title_font_size);
        apply(&mut style.fonts.menu_font_size);
        apply(&mut style.fonts.small_font_size);
    }

    // ── Font rendering hints ────────────────────────────────────────
    // What the desktop asked for, rather than what the rasteriser guessed.
    if let Some(aa) = gsettings_get("org.gnome.desktop.interface", "font-antialiasing") {
        match aa.trim() {
            "none" => {
                style.text_rendering.font_smoothing_enabled = false;
                style.text_rendering.subpixel_type = SubpixelType::None;
            }
            "rgba" => {
                style.text_rendering.font_smoothing_enabled = true;
                // The ORDER comes from its own key; default to the usual RGB.
                style.text_rendering.subpixel_type = SubpixelType::Rgb;
            }
            // "grayscale" and anything unknown: smoothing on, no subpixel.
            _ => {
                style.text_rendering.font_smoothing_enabled = true;
                style.text_rendering.subpixel_type = SubpixelType::None;
            }
        }
    }
    if style.text_rendering.subpixel_type != SubpixelType::None {
        if let Some(order) = gsettings_get("org.gnome.desktop.interface", "font-rgba-order") {
            style.text_rendering.subpixel_type = match order.trim() {
                "bgr" => SubpixelType::Bgr,
                "vrgb" => SubpixelType::VRgb,
                "vbgr" => SubpixelType::VBgr,
                _ => SubpixelType::Rgb,
            };
        }
    }

    // ── Titlebar buttons ────────────────────────────────────────────
    // `button-layout` is `left:right`, e.g. `appmenu:minimize,maximize,close`.
    // Which SIDE the close button is on decides the side; which buttons appear
    // at all decides the set — a GNOME session with `:close` alone must not
    // get minimize/maximize drawn into its CSD.
    if let Some(layout) = gsettings_get("org.gnome.desktop.wm.preferences", "button-layout") {
        style.metrics.titlebar.button_side = titlebar_side_from_layout(&layout);
        style.metrics.titlebar.buttons.has_close = layout.contains("close");
        style.metrics.titlebar.buttons.has_minimize = layout.contains("minimize");
        style.metrics.titlebar.buttons.has_maximize = layout.contains("maximize");
        style.linux.titlebar_button_layout = OptionString::Some(layout.into());
    }

    // ── Scrollbars ──────────────────────────────────────────────────
    // GNOME's overlay scrollbars are the thin ones that appear on hover.
    if let Some(overlay) = gsettings_get("org.gnome.desktop.interface", "overlay-scrolling") {
        style.scrollbar_preferences.visibility = if overlay.trim() == "false" {
            ScrollbarVisibility::Always
        } else {
            ScrollbarVisibility::WhenScrolling
        };
    }

    // Accent color (GNOME 47+)
    if let Some(accent) = gsettings_get("org.gnome.desktop.interface", "accent-color") {
        // GNOME accent-color is a named color like "blue", "teal", "green", etc.
        let color = match accent.trim() {
            "blue" => Some(ColorU::new_rgb(53, 132, 228)),
            "teal" => Some(ColorU::new_rgb(38, 162, 105)),
            "green" => Some(ColorU::new_rgb(46, 194, 82)),
            "yellow" => Some(ColorU::new_rgb(246, 211, 45)),
            "orange" => Some(ColorU::new_rgb(255, 120, 0)),
            "red" => Some(ColorU::new_rgb(237, 51, 59)),
            "pink" => Some(ColorU::new_rgb(220, 79, 133)),
            "purple" => Some(ColorU::new_rgb(145, 65, 172)),
            "slate" => Some(ColorU::new_rgb(111, 131, 150)),
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
    // Try kreadconfig6 first (Plasma 6), fall back to kreadconfig5. Its
    // ABSENCE is no longer fatal: the palette, fonts and themes are read out
    // of the config files directly (see `kde_color_sources`), and a session
    // can run a KDE colour scheme without the Qt tooling installed. The
    // binary is only consulted for keys the files did not carry.
    let kread = if run_command_with_timeout("kreadconfig6", &["--help"], 500).is_ok() {
        "kreadconfig6"
    } else if run_command_with_timeout("kreadconfig5", &["--help"], 500).is_ok() {
        "kreadconfig5"
    } else if std::env::var("HOME")
        .map(|h| std::path::Path::new(&alloc::format!("{h}/.config/kdeglobals")).exists())
        .unwrap_or(false)
    {
        // Nothing to spawn — every `run_command_with_timeout` below fails and
        // falls through to the parsed files, which is the whole point.
        "kreadconfig5"
    } else {
        return Err(());
    };

    // The parsed config files, in precedence order: the user's kdeglobals,
    // then the `.colors` file of the scheme it names. `kreadconfig` is the
    // last resort for a key none of them carried.
    let sources = kde_color_sources();

    // One reader for every `r,g,b` value. Declared up here because dark/light
    // detection uses it too — the window background IS the answer, and reading
    // it is more reliable than any name.
    let read_kde_color = |group: &str, key: &str| -> Option<ColorU> {
        for src in &sources {
            if let Some(c) = src.color(group, key) {
                return Some(c);
            }
        }
        let v = run_command_with_timeout(kread, &["--group", group, "--key", key], 1000).ok()?;
        let p: Vec<&str> = v.split(',').collect();
        if p.len() < 3 {
            return None;
        }
        Some(ColorU::new_rgb(
            p[0].trim().parse::<u8>().ok()?,
            p[1].trim().parse::<u8>().ok()?,
            p[2].trim().parse::<u8>().ok()?,
        ))
    };

    // Same shape for fonts and plain strings.
    let read_kde_font = |group: &str, key: &str| -> Option<(String, Option<f32>)> {
        for src in &sources {
            if let Some(f) = src.font(group, key) {
                return Some(f);
            }
        }
        let v = run_command_with_timeout(kread, &["--group", group, "--key", key], 1000).ok()?;
        let mut parts = v.split(',');
        let family = parts.next()?.trim().to_string();
        if family.is_empty() {
            return None;
        }
        Some((
            family,
            parts.next().and_then(|p| p.trim().parse::<f32>().ok()),
        ))
    };
    let read_kde_str = |group: &str, key: &str| -> Option<String> {
        for src in &sources {
            if let Some(v) = src.get(group, key) {
                return Some(v.to_string());
            }
        }
        run_command_with_timeout(kread, &["--group", group, "--key", key], 1000)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };

    // Dark or light, decided by the WINDOW BACKGROUND's luminance rather than
    // by whether the scheme's NAME contains "dark". A scheme is only obliged
    // to be a colour set: "Midnight" / "Nordic" / "Catppuccin Mocha" are dark
    // and say so nowhere, and "Darkly" is a light scheme whose name says the
    // opposite. The pixels cannot lie. Name matching stays as the fallback for
    // the case where the colour cannot be read at all.
    let color_scheme_name = read_kde_str("General", "ColorScheme").unwrap_or_default();
    let look_and_feel = read_kde_str("KDE", "LookAndFeelPackage").unwrap_or_default();

    let is_dark = match read_kde_color("Colors:Window", "BackgroundNormal") {
        // Rec. 601 luma, the same weighting the rest of the engine uses for
        // "is this surface dark?".
        Some(bg) => {
            let luma = 0.299 * f32::from(bg.r) + 0.587 * f32::from(bg.g) + 0.114 * f32::from(bg.b);
            luma < 128.0
        }
        None => {
            let name = color_scheme_name.to_lowercase();
            let laf = look_and_feel.to_lowercase();
            name.contains("dark") || laf.contains("dark")
        }
    };

    // Base on BREEZE, not on GNOME Adwaita. Every field read below overwrites
    // its slot; the base is what survives for the fields kdeglobals does not
    // carry (or that this session never customised) — and a KDE session's
    // unread fields should be Breeze's, not Adwaita's.
    let mut style = if is_dark {
        defaults::kde_breeze_dark()
    } else {
        defaults::kde_breeze_light()
    };
    style.theme = if is_dark { Theme::Dark } else { Theme::Light };

    // ── Fonts ───────────────────────────────────────────────────────
    // KDE font spec: "Noto Sans,10,-1,5,50,0,0,0,0,0" (family, point size, …).
    if let Some((family, size)) = read_kde_font("General", "font") {
        style.fonts.ui_font = OptionString::Some(family.into());
        if let Some(sz) = size {
            style.fonts.ui_font_size = OptionF32::Some(sz);
        }
    }
    if let Some((family, size)) = read_kde_font("General", "fixed") {
        style.fonts.monospace_font = OptionString::Some(family.into());
        if let Some(sz) = size {
            style.fonts.monospace_font_size = OptionF32::Some(sz);
        }
    }

    // The MENU font. Breeze lets the user set it separately from the general
    // font, and it is what a menu popup must be laid out in — nothing on Linux
    // populated `SystemFonts::menu_font` at all before, so every menu rendered
    // at the generic UI font and size.
    if let Some((family, size)) = read_kde_font("General", "menuFont") {
        style.fonts.menu_font = OptionString::Some(family.into());
        if let Some(sz) = size {
            style.fonts.menu_font_size = OptionF32::Some(sz);
        }
    }
    // An unset menuFont/activeFont means "use the general font" — mirror it
    // so a consumer never has to know which of them KDE actually filled in.
    // Leaving them None made every menu and titlebar fall back to a generic
    // face instead of the desktop's.
    if style.fonts.menu_font.is_none() {
        style.fonts.menu_font = style.fonts.ui_font.clone();
        style.fonts.menu_font_size = style.fonts.ui_font_size;
    }
    if style.fonts.title_font.is_none() {
        style.fonts.title_font = style.fonts.ui_font.clone();
        style.fonts.title_font_size = style.fonts.ui_font_size;
    }
    // A monospace family with no size is a half-answer: the fixed font is set
    // at the same point size as the UI font unless KDE said otherwise.
    if style.fonts.monospace_font_size.is_none() {
        style.fonts.monospace_font_size = style.fonts.ui_font_size;
    }

    // `smallestReadableFont` is Breeze's secondary/caption face.
    if let Some((family, size)) = read_kde_font("General", "smallestReadableFont") {
        style.fonts.small_font = OptionString::Some(family.into());
        if let Some(sz) = size {
            style.fonts.small_font_size = OptionF32::Some(sz);
        }
    }

    // The window title font (Breeze's `activeFont`), for CSD titlebars.
    if let Some((family, size)) = read_kde_font("WM", "activeFont") {
        style.fonts.title_font = OptionString::Some(family.into());
        if let Some(sz) = size {
            style.fonts.title_font_size = OptionF32::Some(sz);
        }
    }

    // ── The palette ─────────────────────────────────────────────────
    // Every group Breeze actually defines, not just three of them. The ones
    // left unread used to fall through to the (GNOME) base, so a KDE window's
    // buttons, disabled text, links and selection foreground were Adwaita's
    // even on a fully-detected KDE session.
    //
    // Colors:View = content surfaces · Colors:Window = chrome ·
    // Colors:Button = controls · Colors:Selection = highlights.
    if let Some(c) = read_kde_color("Colors:Window", "BackgroundNormal") {
        style.colors.window_background = OptionColorU::Some(c);
        style.colors.under_page_background = OptionColorU::Some(c);
    }
    if let Some(c) = read_kde_color("Colors:View", "BackgroundNormal") {
        style.colors.background = OptionColorU::Some(c);
    }
    if let Some(c) = read_kde_color("Colors:View", "ForegroundNormal") {
        style.colors.text = OptionColorU::Some(c);
    }
    if let Some(c) = read_kde_color("Colors:View", "ForegroundInactive") {
        style.colors.secondary_text = OptionColorU::Some(c);
    }
    if let Some(c) = read_kde_color("Colors:View", "ForegroundLink") {
        style.colors.link = OptionColorU::Some(c);
    }
    if let Some(c) = read_kde_color("Colors:Button", "BackgroundNormal") {
        style.colors.button_face = OptionColorU::Some(c);
    }
    if let Some(c) = read_kde_color("Colors:Button", "ForegroundNormal") {
        style.colors.button_text = OptionColorU::Some(c);
    }
    if let Some(c) = read_kde_color("Colors:Button", "ForegroundInactive") {
        style.colors.disabled_text = OptionColorU::Some(c);
    }
    if let Some(c) = read_kde_color("Colors:Selection", "BackgroundNormal") {
        style.colors.accent = OptionColorU::Some(c);
        style.colors.selection_background = OptionColorU::Some(c);
    }
    if let Some(c) = read_kde_color("Colors:Selection", "ForegroundNormal") {
        style.colors.accent_text = OptionColorU::Some(c);
        style.colors.selection_text = OptionColorU::Some(c);
    }
    // An unfocused window's selection: Breeze dims it through ColorEffects,
    // but the inactive foreground is the closest value it publishes directly.
    if let Some(c) = read_kde_color("Colors:Window", "ForegroundInactive") {
        style.colors.selection_background_inactive = OptionColorU::Some(c);
    }
    // Breeze draws separators in the window group's alternate background.
    if let Some(c) = read_kde_color("Colors:Window", "BackgroundAlternate") {
        style.colors.separator = OptionColorU::Some(c);
    }

    // ── Window decoration button side ───────────────────────────────
    // Read from kwinrc (where `org.kde.kdecoration2` actually lives — the old
    // read hit kdeglobals, which never carries that group, so it could only
    // ever return nothing), and decide by which side holds the CLOSE button.
    //
    // "ButtonsOnLeft is non-empty" was wrong even when it did read: KDE's
    // DEFAULT is `ButtonsOnLeft=M` — the application-menu button — with the
    // real controls (`IAX` = minimize/maximize/close) on the RIGHT. Every
    // stock KDE session would have been reported as buttons-on-left.
    let deco = |key: &str| -> Option<String> {
        run_command_with_timeout(
            kread,
            &["--file", "kwinrc", "--group", "org.kde.kdecoration2", "--key", key],
            1000,
        )
        .ok()
        .filter(|v| !v.trim().is_empty())
    };
    let buttons_left = deco("ButtonsOnLeft").unwrap_or_default();
    let buttons_right = deco("ButtonsOnRight").unwrap_or_else(|| "IAX".to_string());
    // 'X' is close in KDE's button-letter alphabet.
    style.metrics.titlebar.button_side = if buttons_left.contains('X') {
        TitlebarButtonSide::Left
    } else {
        TitlebarButtonSide::Right
    };
    let all_buttons = alloc::format!("{buttons_left}{buttons_right}");
    style.metrics.titlebar.buttons.has_close = all_buttons.contains('X');
    style.metrics.titlebar.buttons.has_minimize = all_buttons.contains('I');
    style.metrics.titlebar.buttons.has_maximize = all_buttons.contains('A');
    style.linux.titlebar_button_layout =
        OptionString::Some(alloc::format!("{buttons_left}:{buttons_right}").into());

    // ── Behaviour / motion ──────────────────────────────────────────
    // Plasma scales every animation by this factor; 0 means "instant", which
    // is the same thing the reduced-motion preference asks for.
    if let Some(factor) = read_kde_str("KDE", "AnimationDurationFactor") {
        if let Ok(f) = factor.trim().parse::<f32>() {
            style.animation.animation_duration_factor = f;
            if f <= 0.0 {
                style.animation.animations_enabled = false;
                style.prefers_reduced_motion = BoolCondition::True;
                style.accessibility.prefers_reduced_motion = true;
            }
        }
    }

    // ── Icon + cursor themes, from KDE rather than from gsettings ───
    // A KDE session usually has no gsettings values at all, so these two used
    // to come back empty (or worse, from a stale GNOME config).
    if let Some(icon) = read_kde_str("Icons", "Theme") {
        style.linux.icon_theme = OptionString::Some(icon.into());
    }
    // kcminputrc `[Mouse]` first (where KDE actually writes it), kdeglobals
    // `[General]` second for the older spelling.
    if let Some(cursor) =
        read_kde_str("Mouse", "cursorTheme").or_else(|| read_kde_str("General", "cursorTheme"))
    {
        style.linux.cursor_theme = OptionString::Some(cursor.into());
    }
    if let Some(size) = read_kde_str("Mouse", "cursorSize").and_then(|v| v.parse::<u32>().ok()) {
        style.linux.cursor_size = size;
    }

    // The widget style (Breeze, Oxygen, Fusion, a third-party QStyle) — the
    // closest thing KDE has to GTK's theme name, and what tells a consumer
    // which look the rest of the session is wearing.
    if let Some(widget_style) = read_kde_str("KDE", "widgetStyle") {
        style.linux.gtk_theme = OptionString::Some(widget_style.into());
    }
    // A GTK theme name, when the session has one, is the better answer for
    // GTK-hosted chrome and overwrites the QStyle above.
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
        if line.starts_with('#') {
            continue;
        }

        if let Some(val) = extract_config_value(line, "rounding") {
            if let Ok(px) = val.parse::<f32>() {
                style.metrics.corner_radius = OptionPixelValue::Some(PixelValue::from_metric(
                    azul_css::props::basic::length::SizeMetric::Px,
                    px,
                ));
            }
        }

        if let Some(val) = extract_config_value(line, "border_size") {
            if let Ok(px) = val.parse::<f32>() {
                style.focus_visuals.focus_border_width = OptionPixelValue::Some(
                    PixelValue::from_metric(azul_css::props::basic::length::SizeMetric::Px, px),
                );
            }
        }

        if let Some(val) = extract_config_value(line, "col.active_border") {
            // Hyprland colors: "rgba(33ccffee)" or "rgb(33ccff)"
            let color_str = val.trim();
            if let Some(hex) = color_str
                .strip_prefix("rgba(")
                .and_then(|s| s.strip_suffix(')'))
            {
                if let Ok(c) = parse_css_color(&alloc::format!("#{}", hex)) {
                    style.colors.accent = OptionColorU::Some(c);
                }
            } else if let Some(hex) = color_str
                .strip_prefix("rgb(")
                .and_then(|s| s.strip_suffix(')'))
            {
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
        if line.starts_with('#') {
            continue;
        }

        // "default_border pixel 2"
        if line.starts_with("default_border") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[1] == "pixel" {
                if let Ok(px) = parts[2].parse::<f32>() {
                    style.focus_visuals.focus_border_width = OptionPixelValue::Some(
                        PixelValue::from_metric(azul_css::props::basic::length::SizeMetric::Px, px),
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
    if !azul_css::system::ricing_enabled() {
        return None;
    }

    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))?;

    let config_dir = get_config_dir()?;

    let css_path = alloc::format!("{}/azul/styles/{}.css", config_dir, exe_name);
    let css_str = std::fs::read_to_string(&css_path).ok()?;
    let (css, _warnings) = new_from_str(&css_str);
    if css.is_empty() {
        None
    } else {
        Some(css)
    }
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
            // The value can be a font LIST with the size appended — Mint
            // ships `font-name: 'Noto Sans,  10'` — and a comma left inside
            // the family name makes every downstream cache lookup miss (the
            // measured "ui_font=Noto Sans," tofu). Take the first list entry.
            let name = name_part
                .split(',')
                .map(str::trim)
                .find(|part| !part.is_empty())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                return None;
            }
            return Some((name, size));
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
        crate::plog_debug!(
            "system style: xdg-desktop-portal color-scheme={}",
            color_scheme
        );
        let mut style = match color_scheme {
            1 => defaults::gnome_adwaita_dark(),  // prefer-dark
            2 => defaults::gnome_adwaita_light(), // prefer-light
            _ => defaults::gnome_adwaita_light(), // no preference
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

    // Portal probe unavailable or rejected (e.g. the raw-D-Bus handshake) —
    // non-fatal; fall back to CLI/defaults. Visible only with AZ_LOG on.
    crate::plog_debug!(
        "system style: xdg-desktop-portal unavailable; falling back to CLI/defaults"
    );

    // ── 2. CLI-based discovery ──────────────────────────────────────
    // `AZ_RICING=force` reorders the chain so riced-desktop sources
    // (Hyprland config, pywal cache) win over the GNOME/KDE detection.
    // Useful for tiling-WM users whose `XDG_CURRENT_DESKTOP` still says
    // `gnome` even though their system colors come from pywal.
    let force_riced = matches!(
        azul_css::system::ricing_mode(),
        azul_css::system::RicingMode::Force,
    );

    let mut style = if force_riced {
        discover_riced_style()
            .or_else(|_| discover_kde_style())
            .or_else(|_| discover_gnome_style())
            .unwrap_or_else(|_| defaults::gnome_adwaita_light())
    } else {
        // Normal priority: KDE > GNOME > riced > defaults
        let desktop_env = azul_css::system::detect_linux_desktop_env();
        match &desktop_env {
            DesktopEnvironment::Kde => discover_kde_style()
                .or_else(|_| discover_gnome_style())
                .unwrap_or_else(|_| defaults::gnome_adwaita_light()),
            DesktopEnvironment::Gnome => discover_gnome_style()
                .or_else(|_| discover_kde_style())
                .unwrap_or_else(|_| defaults::gnome_adwaita_light()),
            DesktopEnvironment::Other(_) => discover_riced_style()
                .or_else(|_| discover_gnome_style())
                .or_else(|_| discover_kde_style())
                .unwrap_or_else(|_| defaults::gnome_adwaita_light()),
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

// ============================================================================
// Runtime light/dark switching
// ============================================================================

/// The desktop's light/dark preference as last observed by the watcher thread.
///
/// 0 = not yet known / no preference expressed, 1 = dark, 2 = light. An
/// `AtomicU8` rather than a channel on purpose: the setting is process-wide, so
/// every window wants the same answer, and a channel would deliver the change to
/// exactly one of them. Each window instead compares this against the theme it
/// is already carrying (`current_window_state.theme`), which it has anyway — so
/// no window needs to store a "last seen" of its own.
static OBSERVED_COLOR_SCHEME: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

static THEME_WATCHER: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// Map the XDG `color-scheme` setting onto a [`Theme`].
///
/// Per the portal spec: 0 = no preference, 1 = prefer dark, 2 = prefer light.
/// "No preference" deliberately yields `None` rather than defaulting to light —
/// it means the desktop is not expressing one, so whatever full detection chose
/// at startup (GTK theme name, kdeglobals, pywal, ...) remains the better answer.
fn color_scheme_to_theme(scheme: u32) -> Option<Theme> {
    match scheme {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    }
}

/// The system theme, if a watcher has observed one.
///
/// Cheap: one relaxed atomic load. Safe to call every frame — the D-Bus round
/// trip happens on the watcher thread, never on the caller's.
pub(crate) fn observed_system_theme() -> Option<Theme> {
    ensure_theme_watcher();
    match OBSERVED_COLOR_SCHEME.load(core::sync::atomic::Ordering::Relaxed) {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    }
}

/// Start the watcher thread once per process.
///
/// It MUST NOT run on the event-loop thread. `query_xdg_portal` opens a Unix
/// socket to the session bus and does a synchronous request/response with two-
/// second read and write timeouts, so polling it inline would risk a two-second
/// freeze of the UI every time the portal was slow or absent — trading "dark
/// mode does not apply until restart" for "the window hangs", which is worse.
///
/// Polling rather than subscribing to `SettingChanged`: reading a signal needs a
/// match rule and a message loop against the bus, where a poll reuses the
/// request path that already exists here. Two seconds is far below human
/// tolerance for a theme switch and is one tiny D-Bus call.
fn ensure_theme_watcher() {
    THEME_WATCHER.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("azul-theme-watch".into())
            .spawn(|| loop {
                if let Some((scheme, _accent)) = query_xdg_portal() {
                    if color_scheme_to_theme(scheme).is_some() {
                        OBSERVED_COLOR_SCHEME
                            .store(scheme as u8, core::sync::atomic::Ordering::Relaxed);
                    }
                }
                std::thread::sleep(core::time::Duration::from_secs(2));
            });
    });
}

/// Adopt the watcher's theme into `common`, returning whether anything changed.
///
/// Split from the pump deliberately: this half is identical on X11 and Wayland,
/// while pumping is not (`process_window_events` is a trait method and each
/// backend follows it with its own redraw request). Returning `false` when the
/// theme already matches is what makes it safe to call every frame — a polled
/// backend re-asserting the current theme must not cost a relayout.
///
/// The caller still owes the event pump and
/// `request_regeneration(RelayoutReason::ThemeChange)`; see the call sites.
pub(crate) fn adopt_observed_theme(
    common: &mut crate::desktop::shell2::common::event::CommonWindowState,
) -> bool {
    use azul_core::window::WindowTheme;

    let Some(theme) = observed_system_theme() else {
        return false;
    };
    let theme = match theme {
        Theme::Dark => WindowTheme::DarkMode,
        Theme::Light => WindowTheme::LightMode,
    };
    if common.current_window_state().theme == theme {
        return false;
    }

    // The diff pipeline compares against previous_window_state to decide that a
    // ThemeChanged event fired; without this snapshot the event is never
    // determined and no callback runs.
    common.snapshot_window_state_baseline("linux.adopt_observed_theme");
    common.update_unsynced_state(|ws| ws.theme = theme);
    true
}

/// Dump the fully-discovered `SystemStyle` as text.
///
/// The verification seam for desktop detection: run it on a real session and
/// diff the values against what the desktop's own config says
/// (`~/.config/kdeglobals`, `/usr/share/color-schemes/*.colors`, `gsettings
/// get …`). Detection is the kind of code that silently returns plausible
/// defaults when it reads nothing at all, so "it ran and produced a style" is
/// not evidence — the values have to be compared against the source.
///
/// Reached from a normal build with `AZ_DUMP_SYSTEM_STYLE=1`.
#[must_use]
pub fn dump_discovered_style() -> String {
    use core::fmt::Write;

    let s = discover();
    let mut o = String::new();
    let c = |v: &OptionColorU| -> String {
        v.as_option().map_or_else(
            || "-".to_string(),
            |x| alloc::format!("#{:02x}{:02x}{:02x}", x.r, x.g, x.b),
        )
    };
    let f = |v: &OptionString| -> String {
        v.as_option()
            .map_or_else(|| "-".to_string(), |x| x.as_str().to_string())
    };
    let n = |v: OptionF32| -> String {
        v.into_option()
            .map_or_else(|| "-".to_string(), |x| alloc::format!("{x}"))
    };
    let px = |v: OptionPixelValue| -> String {
        v.into_option()
            .map_or_else(|| "-".to_string(), |x| alloc::format!("{x:?}"))
    };

    // Which desktop-settings family actually answered. A Cinnamon/MATE
    // session answers on its OWN schemas; probing with a key that exists in
    // only one of them is what distinguishes "detected" from "fell back".
    let settings_source = if gsettings_get_raw("org.gnome.desktop.interface", "font-name").is_some()
    {
        "gsettings:gnome"
    } else if gsettings_get_raw("org.cinnamon.desktop.interface", "font-name").is_some() {
        "gsettings:cinnamon"
    } else if gsettings_get_raw("org.mate.interface", "font-name").is_some() {
        "gsettings:mate"
    } else {
        "none"
    };
    let _ = writeln!(o, "platform            {:?}", s.platform);
    let _ = writeln!(o, "settings_source     {settings_source}");
    let _ = writeln!(
        o,
        "kde_config_files    {} source(s)",
        kde_color_sources().len()
    );
    let _ = writeln!(o, "theme               {:?}", s.theme);
    let _ = writeln!(o, "language            {}", s.language.as_str());
    let _ = writeln!(o, "-- fonts --");
    let _ = writeln!(
        o,
        "ui                  {} {}",
        f(&s.fonts.ui_font),
        n(s.fonts.ui_font_size)
    );
    let _ = writeln!(
        o,
        "menu                {} {}",
        f(&s.fonts.menu_font),
        n(s.fonts.menu_font_size)
    );
    let _ = writeln!(
        o,
        "title               {} {}",
        f(&s.fonts.title_font),
        n(s.fonts.title_font_size)
    );
    let _ = writeln!(
        o,
        "monospace           {} {}",
        f(&s.fonts.monospace_font),
        n(s.fonts.monospace_font_size)
    );
    let _ = writeln!(
        o,
        "small               {} {}",
        f(&s.fonts.small_font),
        n(s.fonts.small_font_size)
    );
    let _ = writeln!(o, "-- colors --");
    for (name, v) in [
        ("text", &s.colors.text),
        ("secondary_text", &s.colors.secondary_text),
        ("disabled_text", &s.colors.disabled_text),
        ("background", &s.colors.background),
        ("window_background", &s.colors.window_background),
        ("accent", &s.colors.accent),
        ("accent_text", &s.colors.accent_text),
        ("selection_background", &s.colors.selection_background),
        ("selection_text", &s.colors.selection_text),
        ("button_face", &s.colors.button_face),
        ("button_text", &s.colors.button_text),
        ("link", &s.colors.link),
        ("separator", &s.colors.separator),
    ] {
        let _ = writeln!(o, "{name:<20}{}", c(v));
    }
    let _ = writeln!(o, "-- metrics --");
    let _ = writeln!(o, "corner_radius       {}", px(s.metrics.corner_radius));
    let _ = writeln!(o, "border_width        {}", px(s.metrics.border_width));
    let _ = writeln!(
        o,
        "button_padding      {} / {}",
        px(s.metrics.button_padding_horizontal),
        px(s.metrics.button_padding_vertical)
    );
    let _ = writeln!(
        o,
        "titlebar_side       {:?}",
        s.metrics.titlebar.button_side
    );
    let _ = writeln!(
        o,
        "titlebar_buttons    close={} min={} max={}",
        s.metrics.titlebar.buttons.has_close,
        s.metrics.titlebar.buttons.has_minimize,
        s.metrics.titlebar.buttons.has_maximize
    );
    let _ = writeln!(o, "-- linux --");
    let _ = writeln!(o, "gtk/widget theme    {}", f(&s.linux.gtk_theme));
    let _ = writeln!(o, "icon_theme          {}", f(&s.linux.icon_theme));
    let _ = writeln!(
        o,
        "cursor              {} @{}",
        f(&s.linux.cursor_theme),
        s.linux.cursor_size
    );
    let _ = writeln!(
        o,
        "titlebar_layout     {}",
        f(&s.linux.titlebar_button_layout)
    );
    let _ = writeln!(o, "-- behaviour --");
    let _ = writeln!(
        o,
        "animations          enabled={} factor={}",
        s.animation.animations_enabled, s.animation.animation_duration_factor
    );
    let _ = writeln!(
        o,
        "reduced_motion      {:?}   high_contrast {:?}",
        s.prefers_reduced_motion, s.prefers_high_contrast
    );
    let _ = writeln!(
        o,
        "text_rendering      smoothing={} subpixel={:?}",
        s.text_rendering.font_smoothing_enabled, s.text_rendering.subpixel_type
    );
    let _ = writeln!(
        o,
        "scrollbars          {:?}",
        s.scrollbar_preferences.visibility
    );
    let _ = writeln!(o, "caret_blink_ms      {}", s.input.caret_blink_rate_ms);
    o
}

#[cfg(test)]
mod kde_ini_tests {
    use super::*;

    /// `kdeglobals` and a `.colors` scheme file are the same INI shape, and
    /// both are read directly — one parse instead of ~20 `kreadconfig`
    /// spawns on the startup path.
    #[test]
    fn it_parses_groups_keys_and_colors() {
        let ini = KdeIni::parse(
            "[General]\nColorScheme=BreezeDark\n\n[Colors:Window]\nBackgroundNormal=42,46,50\n",
        );
        assert_eq!(ini.get("General", "ColorScheme"), Some("BreezeDark"));
        assert_eq!(
            ini.color("Colors:Window", "BackgroundNormal"),
            Some(ColorU::new_rgb(42, 46, 50))
        );
        assert_eq!(ini.color("Colors:Window", "Missing"), None);
    }

    /// A STATE-QUALIFIED group (`[Colors:Window][Inactive]`) describes an
    /// unfocused window, not the normal colour. Letting it land in the plain
    /// group would silently repaint every window with its inactive palette —
    /// and kdeglobals ships several of these.
    ///
    /// NEGATIVE CONTROL: parse the qualified header as its base group.
    #[test]
    fn a_state_qualified_group_does_not_overwrite_the_plain_one() {
        let ini = KdeIni::parse(
            "[Colors:Window]\nBackgroundNormal=42,46,50\n\n[Colors:Window][Inactive]\nBackgroundNormal=1,2,3\n",
        );
        assert_eq!(
            ini.color("Colors:Window", "BackgroundNormal"),
            Some(ColorU::new_rgb(42, 46, 50)),
            "the [Inactive] variant must not become the window's normal colour"
        );
    }

    /// KDE font specs are `family,pointsize,…`. An empty family means "unset"
    /// and must not be reported as a font called "".
    #[test]
    fn it_parses_kde_font_specs() {
        let ini = KdeIni::parse("[General]\nfont=Noto Sans,10,-1,5,50,0,0,0,0,0\nmenuFont=\n");
        assert_eq!(
            ini.font("General", "font"),
            Some(("Noto Sans".to_string(), Some(10.0)))
        );
        assert_eq!(ini.font("General", "menuFont"), None);
    }

    /// The titlebar side is decided by where CLOSE is, not by whether the
    /// left half is empty. `icon:minimize,maximize,close` — the stock
    /// KDE/GNOME layout — has its controls on the RIGHT, and the old
    /// `starts_with(':')` test called that LEFT on every default desktop.
    ///
    /// NEGATIVE CONTROL: restore `layout.starts_with(':')` and the first case
    /// fails.
    #[test]
    fn the_titlebar_side_follows_the_close_button() {
        // The stock layout: menu icon left, controls right.
        assert_eq!(
            titlebar_side_from_layout("icon:minimize,maximize,close"),
            TitlebarButtonSide::Right
        );
        // GNOME's own default, left half empty.
        assert_eq!(
            titlebar_side_from_layout(":minimize,maximize,close"),
            TitlebarButtonSide::Right
        );
        // macOS-style: controls on the left.
        assert_eq!(
            titlebar_side_from_layout("close,minimize,maximize:"),
            TitlebarButtonSide::Left
        );
        assert_eq!(
            titlebar_side_from_layout("close:appmenu"),
            TitlebarButtonSide::Left
        );
    }

    /// Cinnamon and MATE are GNOME forks that kept the key names and renamed
    /// the schemas, so every GNOME query on a Mint session used to read
    /// nothing at all.
    #[test]
    fn the_schema_family_covers_the_gnome_forks() {
        let fam = schema_family("org.gnome.desktop.interface");
        assert!(fam.contains(&"org.gnome.desktop.interface".to_string()));
        assert!(fam.contains(&"org.cinnamon.desktop.interface".to_string()));
        assert!(fam.contains(&"org.mate.interface".to_string()));

        // MATE's window manager is Marco, under its own schema name.
        let wm = schema_family("org.gnome.desktop.wm.preferences");
        assert!(wm.contains(&"org.cinnamon.desktop.wm.preferences".to_string()));
        assert!(wm.contains(&"org.mate.Marco.general".to_string()));
    }
}
