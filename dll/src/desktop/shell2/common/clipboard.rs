//! System-clipboard seam: typed multi-flavor payloads between azul and the OS.
//!
//! Clipboard support has three layers (rich-clipboard's `plan/INTEGRATION.md`):
//!
//! 1. **Transport** — `shell2/<platform>/clipboard.rs`: OS calls only
//!    (`NSPasteboard`, `OpenClipboard`, ICCCM selections, `wl_data_offer`),
//!    no format knowledge. Produces and consumes [`ClipboardPayload`]: every
//!    encoding the source offered, still as bytes.
//! 2. **Codecs + policy** — the `rich-clipboard` crate: picks the richest
//!    flavor on a read (RTF over HTML over plain text), and fans one item out
//!    to every flavor the platform wants on a write, so styled text is
//!    published as RTF *and* HTML *and* plain text simultaneously and a paste
//!    lands in Word styled rather than flattened.
//! 3. **azul** — [`ClipboardContent`] (the FFI type: plain text plus styled
//!    runs). This module owns the conversions in both directions.
//!
//! The platform transports currently ship only the plain-text flavor, so
//! [`get_system_clipboard`] wraps their text into a payload and
//! [`set_system_clipboard`] reduces the payload back to text. Each backend
//! graduates to real payload transport in its own step without the callers in
//! `event.rs` changing again.

use azul_css::props::basic::ColorU;
use azul_layout::managers::selection::{ClipboardContent, StyledTextRun, StyledTextRunVec};
use rich_clipboard::{decode_payload, encode, Rgb, RichItem, RichText, Style};
pub use rich_clipboard::{ClipboardPayload, Platform};

/// 1 CSS pt = 4/3 CSS px (the 96 dpi reference used across azul).
///
/// `rich-clipboard`'s [`Style::size_pt`] speaks points because RTF and CSS
/// clipboard fragments do; azul's [`StyledTextRun::font_size_px`] speaks
/// pixels. Both converters below use this one constant.
const PX_PER_PT: f32 = 4.0 / 3.0;

/// Largest single flavor a transport will copy off the OS clipboard.
///
/// This is the FIRST line of defence and it belongs to the transport, because
/// only the transport can act before the bytes are resident: Windows and macOS
/// can state the exact size and X11 a lower bound, all of them before any copy
/// happens. Refusing a flavor here just falls through to the next-best one, so
/// a 400 MB TIFF goes and the plain text alongside it stays.
///
/// The decode limits below are the second line, and they cannot substitute:
/// by the time a [`ClipboardPayload`] exists the bytes are already in this
/// process. What they stop is the *amplification* — a 60 MB 8-bit `CF_DIB`
/// becomes 240 MB of RGBA.
///
/// 64 MiB matches `rclip_core::Limits::default().max_flavor_bytes`, so a
/// flavor that survives the transport is one the decoder will also accept.
pub const MAX_FLAVOR_BYTES: u64 = 64 * 1024 * 1024;

/// Read the OS clipboard as a typed payload.
///
/// `None` means an empty clipboard, an unreachable clipboard, or a platform
/// without one wired up — callers treat all three as "nothing to paste".
pub fn get_system_clipboard() -> Option<ClipboardPayload> {
    // Every flavor the source offered — one group per pasteboard item on
    // macOS, one flat set everywhere else.
    #[cfg(target_os = "windows")]
    {
        crate::desktop::shell2::windows::clipboard::read_payload()
    }
    #[cfg(target_os = "macos")]
    {
        crate::desktop::shell2::macos::clipboard::read_payload()
    }
    #[cfg(target_os = "linux")]
    {
        // Route reads like writes: native Wayland first on a Wayland session
        // (it falls back to the X11 worker itself when the compositor has no
        // selection), the X11 worker directly otherwise.
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            crate::desktop::shell2::linux::wayland::clipboard::read_payload()
        } else {
            crate::desktop::shell2::linux::x11::clipboard::read_payload()
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Publish a typed payload to the OS clipboard.
///
/// Returns `true` only when the platform transport accepted the content —
/// `CutToClipboard` gates the DELETION of the selected text on this, so a
/// failed copy must never report success.
pub fn set_system_clipboard(payload: &ClipboardPayload) -> bool {
    // macOS and Windows publish EVERY flavor of the fan-out, which is what
    // makes a paste land in Word as styled text rather than flattened.
    #[cfg(target_os = "macos")]
    {
        crate::desktop::shell2::macos::clipboard::write_payload(payload)
    }
    #[cfg(target_os = "windows")]
    {
        crate::desktop::shell2::windows::clipboard::write_payload(payload)
    }
    #[cfg(target_os = "linux")]
    {
        // Route to the active session backend. This previously hardcoded the
        // X11 write, so copy never reached the clipboard under a Wayland
        // session.
        //
        // Wayland publishes the whole fan-out natively; X11 cannot (its
        // selection owner serves one target — see `x11/clipboard.rs`), so it
        // gets the plain-text reading.
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            crate::desktop::shell2::linux::wayland::clipboard::write_payload(payload).is_ok()
        } else {
            let Some(text) = payload_plain_text(payload) else {
                return false;
            };
            crate::desktop::shell2::linux::x11::clipboard::write_to_clipboard(&text).is_ok()
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = payload;
        false
    }
}

/// Wrap plain text into a payload under the platform's native identifier
/// (`public.utf8-plain-text` / `CF_UNICODETEXT` / `text/plain;charset=utf-8`).
///
/// Every transport speaks payloads now, so this is only what the tests below
/// build their fixtures from — and what a future platform without a payload
/// transport would wrap its string read in.
#[cfg_attr(not(test), allow(dead_code))]
fn payload_of_text(text: String) -> Option<ClipboardPayload> {
    encode(&RichItem::Text(text), Platform::native()).ok()
}

/// The plain-text reading of a payload, through the full decode policy
/// (richest flavor first), so RTF/HTML-only payloads still paste as text.
///
/// Used by the transports that cannot publish a fan-out — X11, whose selection
/// owner serves exactly one target.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn payload_plain_text(payload: &ClipboardPayload) -> Option<String> {
    let item = decode_payload(payload).ok()?;
    item.plain_text().map(str::to_owned)
}

/// azul's FFI clipboard type → a typed item ready for the write fan-out.
///
/// `None` only when encoding failed outright; an empty `ClipboardContent`
/// still encodes (as empty plain text), preserving the old "copy nothing
/// clears the clipboard" behavior.
pub fn clipboard_content_to_payload(content: &ClipboardContent) -> Option<ClipboardPayload> {
    encode(&content_to_rich_item(content), Platform::native()).ok()
}

fn content_to_rich_item(content: &ClipboardContent) -> RichItem {
    let plain = content.plain_text.as_str();
    let runs = content.styled_runs.as_slice();
    if !runs.is_empty() {
        let mut rich = RichText::default();
        for run in runs {
            rich.push(
                run.text.as_str(),
                Style {
                    bold: run.is_bold,
                    italic: run.is_italic,
                    size_pt: (run.font_size_px > 0.0).then(|| run.font_size_px / PX_PER_PT),
                    font_family: run
                        .font_family
                        .as_ref()
                        .map(|family| family.as_str().to_owned()),
                    color: Some(Rgb::new(run.color.r, run.color.g, run.color.b)),
                    ..Style::default()
                },
            );
        }
        // The runs are authoritative only when they spell the same characters
        // as the plain text. A producer that filled the two fields
        // inconsistently must not publish a rich flavor that disagrees with
        // the plain one — receivers would paste different text depending on
        // which flavor they picked.
        if rich.as_str() == plain {
            return RichItem::RichText(rich);
        }
    }
    RichItem::Text(plain.to_owned())
}

/// Typed payload → azul's FFI clipboard type.
///
/// Styled flavors (RTF/HTML) arrive as populated `styled_runs`; anything
/// text-shaped arrives as plain text. `None` for payloads with no text
/// reading at all (an image, a file list) — those gain their own
/// `ClipboardContent` representation in a later step.
pub fn payload_to_clipboard_content(payload: &ClipboardPayload) -> Option<ClipboardContent> {
    match decode_payload(payload).ok()? {
        RichItem::RichText(rich) => Some(rich_text_to_content(&rich)),
        item => item.plain_text().map(|plain| ClipboardContent {
            plain_text: plain.into(),
            styled_runs: StyledTextRunVec::from_const_slice(&[]),
        }),
    }
}

fn rich_text_to_content(rich: &RichText) -> ClipboardContent {
    /// [`StyledTextRun`]'s color is not optional; opaque black stands in for
    /// "inherit", matching what azul renders for unstyled text.
    const INHERIT: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    let runs: Vec<StyledTextRun> = rich
        .spans()
        .map(|(text, style)| StyledTextRun {
            text: text.into(),
            font_family: style
                .font_family
                .as_deref()
                .map(azul_css::AzString::from)
                .into(),
            font_size_px: style.size_pt.map(|pt| pt * PX_PER_PT).unwrap_or(0.0),
            color: style
                .color
                .map(|c| ColorU {
                    r: c.r,
                    g: c.g,
                    b: c.b,
                    a: 255,
                })
                .unwrap_or(INHERIT),
            is_bold: style.bold,
            is_italic: style.italic,
        })
        .collect();
    ClipboardContent {
        plain_text: rich.as_str().into(),
        styled_runs: runs.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(text: &str, bold: bool, size_px: f32) -> StyledTextRun {
        StyledTextRun {
            text: text.into(),
            font_family: None.into(),
            font_size_px: size_px,
            color: ColorU {
                r: 10,
                g: 20,
                b: 30,
                a: 255,
            },
            is_bold: bold,
            is_italic: false,
        }
    }

    /// Styled runs must survive azul → payload → azul: this is the round trip
    /// a copy in one azul window and a paste in another takes, and it crosses
    /// the wire as real RTF/HTML.
    #[test]
    fn styled_runs_round_trip_through_the_payload() {
        let content = ClipboardContent {
            plain_text: "boldplain".into(),
            styled_runs: vec![styled("bold", true, 16.0), styled("plain", false, 16.0)].into(),
        };
        let payload = clipboard_content_to_payload(&content).expect("encodes");
        let back = payload_to_clipboard_content(&payload).expect("decodes");

        assert_eq!(back.plain_text.as_str(), "boldplain");
        let runs = back.styled_runs.as_slice();
        assert_eq!(runs.len(), 2, "two style spans must come back as two runs");
        assert!(runs[0].is_bold && !runs[1].is_bold);
        assert_eq!(runs[0].text.as_str(), "bold");
        assert_eq!(runs[1].text.as_str(), "plain");
        // pt→px→pt conversion must not drift (16px = 12pt exactly).
        assert!((runs[0].font_size_px - 16.0).abs() < 0.01);
        assert_eq!(
            (runs[0].color.r, runs[0].color.g, runs[0].color.b),
            (10, 20, 30)
        );
    }

    /// A `ClipboardContent` whose runs disagree with its plain text (a
    /// producer bug) must publish the PLAIN text, not the runs — receivers
    /// must never paste different characters depending on the flavor they
    /// picked.
    #[test]
    fn inconsistent_runs_fall_back_to_plain_text() {
        let content = ClipboardContent {
            plain_text: "the real text".into(),
            styled_runs: vec![styled("other", false, 0.0)].into(),
        };
        match content_to_rich_item(&content) {
            RichItem::Text(t) => assert_eq!(t, "the real text"),
            other => panic!("expected the plain fallback, got {other:?}"),
        }
    }

    /// Plain text through the whole seam: what every existing caller does
    /// today, and what phase 1 must not change.
    #[test]
    fn plain_text_round_trips_and_reduces() {
        let payload = payload_of_text("hello\nworld".to_owned()).expect("encodes");
        assert_eq!(
            payload_plain_text(&payload).as_deref(),
            Some("hello\nworld")
        );
        let content = payload_to_clipboard_content(&payload).expect("decodes");
        assert_eq!(content.plain_text.as_str(), "hello\nworld");
        assert!(content.styled_runs.as_slice().is_empty());
    }

    /// An unstyled `ClipboardContent` must encode as plain text, not as a
    /// rich flavor claiming a fidelity it does not have.
    #[test]
    fn unstyled_content_encodes_as_plain_text() {
        let content = ClipboardContent {
            plain_text: "just text".into(),
            styled_runs: StyledTextRunVec::from_const_slice(&[]),
        };
        match content_to_rich_item(&content) {
            RichItem::Text(t) => assert_eq!(t, "just text"),
            other => panic!("expected plain text, got {other:?}"),
        }
    }
}
