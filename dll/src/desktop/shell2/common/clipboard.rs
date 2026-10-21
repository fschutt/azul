//! System-clipboard seam: typed multi-flavor payloads between azul and the OS.
//!
//! Clipboard support has three layers (rich-clipboard's `plan/INTEGRATION.md`):
//!
//! 1. **Transport** — `shell2/<platform>/clipboard.rs`: OS calls only (`NSPasteboard`,
//!    `OpenClipboard`, ICCCM selections, `wl_data_offer`), no format knowledge. Produces and
//!    consumes [`ClipboardPayload`]: every encoding the source offered, still as bytes.
//! 2. **Codecs + policy** — the `rich-clipboard` crate: picks the richest flavor on a read (RTF
//!    over HTML over plain text), and fans one item out to every flavor the platform wants on a
//!    write, so styled text is published as RTF *and* HTML *and* plain text simultaneously and a
//!    paste lands in Word styled rather than flattened.
//! 3. **azul** — [`ClipboardContent`] (the FFI type: plain text plus styled runs). This module owns
//!    the conversions in both directions.
//!
//! The platform transports currently ship only the plain-text flavor, so
//! [`get_system_clipboard`] wraps their text into a payload and
//! [`set_system_clipboard`] reduces the payload back to text. Each backend
//! graduates to real payload transport in its own step without the callers in
//! `event.rs` changing again.

use azul_css::props::basic::ColorU;
use azul_layout::managers::selection::{ClipboardContent, StyledTextRun};
use rich_clipboard::{
    decode_payload, decode_with, encode, Flavor, HtmlFragment, Options, Rgb, RichItem, RichText,
    Style,
};
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
/// `window` is the asking window's key in the backend's registry
/// (`PlatformWindow::registry_window_id`); 0 means "no window to name". Most
/// platforms have one app-wide clipboard and ignore it. Wayland does not: a
/// selection there belongs to a SEAT, reached through one window's
/// `wl_data_device` and validated against that window's input serial, so a
/// paste has to say which window it is for. See
/// `linux/wayland/clipboard.rs::route_selection`.
///
/// `None` means an empty clipboard, an unreachable clipboard, or a platform
/// without one wired up — callers treat all three as "nothing to paste".
pub fn get_system_clipboard(window: u64) -> Option<ClipboardPayload> {
    let _ = window;
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
            crate::desktop::shell2::linux::wayland::clipboard::read_payload(asking_window(window))
        } else {
            crate::desktop::shell2::linux::x11::clipboard::read_payload()
        }
    }
    #[cfg(target_os = "ios")]
    {
        crate::desktop::shell2::ios::clipboard::read_payload()
    }
    // Android had no arm here at all and fell through to the `None` below,
    // so Paste found nothing on a platform that has had a clipboard since
    // API 1. `ClipData` speaks MIME types, so the payload is tagged
    // `Platform::Unix` like X11 and Wayland.
    #[cfg(target_os = "android")]
    {
        crate::desktop::shell2::android::clipboard::read_payload()
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "ios",
        target_os = "android"
    )))]
    {
        None
    }
}

/// A registry id, as the Wayland router wants it: `None` for "nobody named a
/// window", which is what 0 means at a call site that has no handle yet.
#[cfg(target_os = "linux")]
fn asking_window(window: u64) -> Option<crate::desktop::shell2::linux::registry::LinuxWindowId> {
    (window != 0).then_some(window)
}

/// Publish a typed payload to the OS clipboard.
///
/// `window` names the asking window, for the same reason it does on the read
/// side — see [`get_system_clipboard`]. A Wayland copy goes out on THAT
/// window's `wl_data_source` with THAT window's input serial; sending another
/// window's is how a copy made in the second window of an app got dropped by
/// the compositor without a word.
///
/// Returns `true` only when the platform transport accepted the content —
/// `CutToClipboard` gates the DELETION of the selected text on this, so a
/// failed copy must never report success.
pub fn set_system_clipboard(window: u64, payload: &ClipboardPayload) -> bool {
    let _ = window;
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
            crate::desktop::shell2::linux::wayland::clipboard::write_payload(
                asking_window(window),
                payload,
            )
            .is_ok()
        } else {
            let Some(text) = payload_plain_text(payload) else {
                return false;
            };
            crate::desktop::shell2::linux::x11::clipboard::write_to_clipboard(&text).is_ok()
        }
    }
    #[cfg(target_os = "ios")]
    {
        crate::desktop::shell2::ios::clipboard::write_payload(payload)
    }
    // See the read side: Android had no arm and answered `false`, which
    // `CutToClipboard` reads as "the copy failed, do not delete" - so Cut
    // was inert too, not just Copy. A clip carries one HTML representation
    // plus its plain-text coercion, so the fan-out lands as
    // `ClipData.newHtmlText` when the payload has markup.
    #[cfg(target_os = "android")]
    {
        crate::desktop::shell2::android::clipboard::write_payload(payload)
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "ios",
        target_os = "android"
    )))]
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
    let text = text_payload(content)?;
    let extras = extra_items(content, Platform::native());
    if extras.is_empty() {
        return Some(text);
    }
    // Files, an image and a link next to the text, in one payload: on macOS
    // the first file's URL and the text share the first pasteboard item, the
    // other files take one item each. A flavour the text already set on an
    // item (a link's plain-text twin) is not set twice.
    let mut out = if has_text(content) {
        text
    } else {
        ClipboardPayload::new(Platform::native())
    };
    for extra in extras {
        let taken = out
            .items()
            .iter()
            .any(|i| i.item == extra.item && i.native == extra.native);
        if !taken {
            out.push(extra);
        }
    }
    Some(out)
}

/// Whether `content` carries text of its own (an empty copy still publishes
/// the empty text: copying nothing clears the clipboard).
fn has_text(content: &ClipboardContent) -> bool {
    !content.plain_text.as_str().is_empty()
        || !content.styled_runs.as_slice().is_empty()
        || content.html.is_some()
}

/// The flavours of `content`'s files, image and link, as `platform` spells
/// them (`rich-clipboard`'s fan-out of each).
fn extra_items(content: &ClipboardContent, platform: Platform) -> Vec<rich_clipboard::ClipboardItem> {
    let mut items = Vec::new();
    let mut add = |item: RichItem| {
        if let Ok(payload) = encode(&item, platform) {
            items.extend(payload.items().iter().cloned());
        }
    };
    let files = content.files.as_ref();
    if !files.is_empty() {
        add(RichItem::Files(rich_clipboard::FileList::of_paths(
            files.iter().map(|f| f.as_str().to_owned()),
        )));
    }
    if let Some(image) = content.image.as_ref() {
        let bytes = image.as_ref().to_vec();
        add(RichItem::Image(rich_clipboard::Image::Encoded {
            format: image_format_of(&bytes),
            bytes,
        }));
    }
    if let Some(link) = content.link.as_ref() {
        add(RichItem::Link(rich_clipboard::Link::to_url(link.as_str())));
    }
    if content.concealed {
        for (native, bytes) in concealed_markers(platform) {
            items.push(rich_clipboard::ClipboardItem::new(*native, bytes.to_vec()));
        }
    }
    items
}

/// The flavours that tell a platform's clipboard managers, history tools
/// and cloud clipboards to leave a copy alone: the nspasteboard.org
/// convention on macOS, the Windows clipboard history and monitor formats
/// (a DWORD 0 where the format carries a value), KDE's password manager
/// hint (Klipper, and the Wayland managers that follow it).
fn concealed_markers(platform: Platform) -> &'static [(&'static str, &'static [u8])] {
    match platform {
        Platform::MacOs => &[("org.nspasteboard.ConcealedType", b"")],
        Platform::Windows => &[
            ("ExcludeClipboardContentFromMonitorProcessing", &[0, 0, 0, 0]),
            ("CanIncludeInClipboardHistory", &[0, 0, 0, 0]),
            ("CanUploadToCloudClipboard", &[0, 0, 0, 0]),
        ],
        Platform::Unix => &[("x-kde-passwordManagerHint", b"secret")],
    }
}

/// Whether the source marked `payload` concealed ([`concealed_markers`]).
fn is_concealed(payload: &ClipboardPayload) -> bool {
    concealed_markers(payload.platform())
        .iter()
        .any(|(marker, _)| payload.items().iter().any(|i| i.native == *marker))
}

/// The clipboard as a callback reads it outside a paste
/// (`CallbackInfo::get_system_clipboard`): installed as the engine's reader
/// at startup.
pub extern "C" fn read_clipboard_for_callbacks() -> azul_layout::managers::selection::OptionClipboardContent {
    get_system_clipboard(0)
        .as_ref()
        .and_then(payload_to_clipboard_content)
        .into()
}

/// The format of an encoded image, from its first bytes; PNG for anything
/// else (a copy is documented to take PNG).
fn image_format_of(bytes: &[u8]) -> rich_clipboard::ImageFormat {
    use rich_clipboard::ImageFormat;
    match bytes {
        [0xFF, 0xD8, ..] => ImageFormat::Jpeg,
        [b'G', b'I', b'F', b'8', ..] => ImageFormat::Gif,
        [b'I', b'I', 0x2A, 0x00, ..] | [b'M', b'M', 0x00, 0x2A, ..] => ImageFormat::Tiff,
        _ => ImageFormat::Png,
    }
}

/// The text flavours of `content`: the styled runs' fan-out (RTF, HTML, plain
/// text), with the content's own markup as the HTML flavour when it has one.
fn text_payload(content: &ClipboardContent) -> Option<ClipboardPayload> {
    let platform = Platform::native();
    let payload = encode(&content_to_rich_item(content), platform).ok()?;
    let Some(html) = content.html.as_ref() else {
        return Some(payload);
    };
    // The content's OWN markup is the HTML flavour (a mail's reply with its
    // quotes and links), in place of the one the runs would have given; the
    // RTF and plain flavours of the fan-out stay. Through the encoder, so
    // Windows gets its `CF_HTML` wrapper.
    let fragment = HtmlFragment {
        markup: html.as_str().to_owned(),
        context: None,
        source_url: None,
        plain: Some(content.plain_text.as_str().to_owned()),
    };
    let html_payload = encode(&RichItem::Html(fragment), platform).ok()?;
    let mut out = ClipboardPayload::new(platform);
    for item in payload.items() {
        if item.flavor(platform) != Flavor::Html {
            out.push(item.clone());
        }
    }
    for item in html_payload.items() {
        if item.flavor(platform) == Flavor::Html {
            out.push(item.clone());
        }
    }
    Some(out)
}

/// The HTML flavour of `payload` as markup, when the source offered one:
/// bare markup on macOS and Unix, the fragment of Windows' `CF_HTML`. Over
/// [`MAX_FLAVOR_BYTES`] it is left out, as the transports leave it.
fn html_flavour_of(payload: &ClipboardPayload) -> Option<String> {
    let item = payload.get(Flavor::Html)?;
    if item.bytes.len() as u64 > MAX_FLAVOR_BYTES {
        return None;
    }
    let options = Options::new().keep_html_markup(true);
    match decode_with(item, payload.platform(), &options).ok()? {
        RichItem::Html(fragment) => Some(fragment.markup),
        _ => None,
    }
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
/// text-shaped arrives as plain text; files, an image and a link arrive in
/// their own fields, next to the text the source offered with them. `None`
/// for a payload with none of these.
pub fn payload_to_clipboard_content(payload: &ClipboardPayload) -> Option<ClipboardContent> {
    let plain = |text: String| ClipboardContent {
        plain_text: text.into(),
        ..Default::default()
    };
    let text = match decode_payload(payload).ok() {
        Some(RichItem::RichText(rich)) => Some(rich_text_to_content(&rich)),
        Some(item) => item
            .plain_text()
            .map(str::to_owned)
            .or_else(|| plain_text_flavor_of(payload))
            .map(plain),
        None => plain_text_flavor_of(payload).map(plain),
    };
    let files = files_of(payload);
    let image = image_of(payload);
    let link = link_of(payload);
    if text.is_none() && files.is_empty() && image.is_none() && link.is_none() {
        return None;
    }
    let concealed = is_concealed(payload);
    let mut content = text.unwrap_or_default();
    // The markup NEXT to the runs: paragraphs, lists, quotes and links have
    // nowhere to go in the runs, and a rich editor's paste (or a mail app's
    // sanitizer) wants them.
    content.html = html_flavour_of(payload).map(azul_css::AzString::from).into();
    content.files = files
        .into_iter()
        .map(azul_css::AzString::from)
        .collect::<Vec<_>>()
        .into();
    content.image = image.map(azul_css::U8Vec::from).into();
    content.link = link.map(azul_css::AzString::from).into();
    content.concealed = concealed;
    Some(content)
}

/// The text of `payload`'s plain-text flavour. The richest item of a payload that carries
/// files or a link is their list, which has no text of its own, so the text that came with
/// them is read here.
fn plain_text_flavor_of(payload: &ClipboardPayload) -> Option<String> {
    let platform = payload.platform();
    let item = payload
        .items()
        .iter()
        .find(|i| i.flavor(platform) == Flavor::PlainText)?;
    if item.bytes.len() as u64 > MAX_FLAVOR_BYTES {
        return None;
    }
    match decode_with(item, platform, &Options::new()).ok()? {
        RichItem::Text(text) => Some(text),
        _ => None,
    }
}

/// Every local file `payload` carries, in the source's order: one per
/// pasteboard item on macOS, the list of `CF_HDROP` or `text/uri-list`
/// elsewhere. A URI that names no local file (an `smb://` share a file
/// manager copied from) is not a path, and is left out.
fn files_of(payload: &ClipboardPayload) -> Vec<String> {
    let platform = payload.platform();
    let mut files = Vec::new();
    for item in payload.items() {
        if item.flavor(platform) != Flavor::FileList || item.bytes.len() as u64 > MAX_FLAVOR_BYTES {
            continue;
        }
        if let Ok(RichItem::Files(list)) = decode_with(item, platform, &Options::new()) {
            for entry in list.entries {
                if let rich_clipboard::FileEntry::Path(path) = entry {
                    if !files.contains(&path) {
                        files.push(path);
                    }
                }
            }
        }
    }
    files
}

/// The image `payload` carries, as encoded bytes: the first of PNG, JPEG,
/// GIF and TIFF the source offered, or a Windows bitmap encoded as PNG.
fn image_of(payload: &ClipboardPayload) -> Option<Vec<u8>> {
    let platform = payload.platform();
    let preference = [
        Flavor::Png,
        Flavor::Jpeg,
        Flavor::Gif,
        Flavor::Tiff,
        Flavor::DibV5,
        Flavor::Dib,
    ];
    preference.iter().find_map(|wanted| {
        let item = payload
            .items()
            .iter()
            .find(|i| i.flavor(platform) == *wanted)?;
        if item.bytes.len() as u64 > MAX_FLAVOR_BYTES {
            return None;
        }
        match decode_with(item, platform, &Options::new()).ok()? {
            RichItem::Image(rich_clipboard::Image::Encoded { bytes, .. }) => Some(bytes),
            RichItem::Image(rich_clipboard::Image::Rgba(rgba)) => png_of_rgba(rgba),
            _ => None,
        }
    })
}

/// A decoded bitmap (Windows' `CF_DIB`) as PNG bytes.
#[cfg(feature = "png")]
fn png_of_rgba(rgba: rich_clipboard::RgbaImage) -> Option<Vec<u8>> {
    use azul_core::resources::{RawImage, RawImageData, RawImageFormat};
    let raw = RawImage {
        pixels: RawImageData::U8(rgba.pixels.into()),
        width: usize::try_from(rgba.width).ok()?,
        height: usize::try_from(rgba.height).ok()?,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    azul_layout::image::encode::encode_png(&raw)
        .into_result()
        .ok()
        .map(|bytes| bytes.as_ref().to_vec())
}

/// Without a PNG encoder a bitmap has no encoded form to hand an app.
#[cfg(not(feature = "png"))]
fn png_of_rgba(_rgba: rich_clipboard::RgbaImage) -> Option<Vec<u8>> {
    None
}

/// The link `payload` carries (`public.url`, `UniformResourceLocatorW`, a
/// `text/uri-list` of a web address), as its address.
fn link_of(payload: &ClipboardPayload) -> Option<String> {
    let platform = payload.platform();
    let fits = |i: &&rich_clipboard::ClipboardItem| i.bytes.len() as u64 <= MAX_FLAVOR_BYTES;
    if let Some(item) = payload
        .items()
        .iter()
        .find(|i| i.flavor(platform) == Flavor::Url)
        .filter(fits)
    {
        return match decode_with(item, platform, &Options::new()).ok()? {
            RichItem::Link(link) => Some(link.target.as_str().to_owned()),
            _ => None,
        };
    }
    // On X11 and Wayland a link is a `text/uri-list` as files are: a list of one address
    // that names no local file is the link.
    if platform != Platform::Unix {
        return None;
    }
    payload
        .items()
        .iter()
        .filter(|i| i.flavor(platform) == Flavor::FileList)
        .filter(fits)
        .find_map(|item| match decode_with(item, platform, &Options::new()).ok()? {
            RichItem::Files(list) => match list.entries.as_slice() {
                [rich_clipboard::FileEntry::Uri(uri)] => Some(uri.clone()),
                _ => None,
            },
            _ => None,
        })
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
        html: azul_css::OptionString::None,
        ..Default::default()
    }
}

/// What a node drag carries that another application can take when the
/// drag leaves the window - files to drop on Finder or Explorer, text, an
/// image - as clipboard content (the OS drag session is published like a
/// copy). The drag data names its parts by MIME type, as the web's
/// `dataTransfer` does: `text/uri-list` (files, as `file://` URIs),
/// `text/plain`, `text/html`, `image/png` / `jpeg` / `gif` / `tiff`. `None`
/// for a drag that carries only the app's own types: it stays inside the app.
pub fn drag_data_to_clipboard_content(data: &azul_core::drag::DragData) -> Option<ClipboardContent> {
    // The MIME names are the flavours' own names on X11 and Wayland, so the
    // parts decode through the codecs a paste there takes.
    let mut payload = ClipboardPayload::new(Platform::Unix);
    for part in data.data.as_ref() {
        let mime = part.mime_type.as_str();
        let known = matches!(
            mime,
            "text/uri-list"
                | "text/plain"
                | "text/plain;charset=utf-8"
                | "text/html"
                | "image/png"
                | "image/jpeg"
                | "image/gif"
                | "image/tiff"
        );
        if known {
            payload.push(rich_clipboard::ClipboardItem::new(
                mime,
                part.data.as_ref().to_vec(),
            ));
        }
    }
    if payload.is_empty() {
        return None;
    }
    payload_to_clipboard_content(&payload)
}

#[cfg(test)]
mod tests {
    use azul_layout::managers::selection::StyledTextRunVec;

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
            html: azul_css::OptionString::None,
            ..Default::default()
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
            html: azul_css::OptionString::None,
            ..Default::default()
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
            html: azul_css::OptionString::None,
            ..Default::default()
        };
        match content_to_rich_item(&content) {
            RichItem::Text(t) => assert_eq!(t, "just text"),
            other => panic!("expected plain text, got {other:?}"),
        }
    }

    /// E-PASTE: a paste keeps the HTML flavour NEXT to the plain text - the
    /// markup as the source put it, which the styled runs cannot hold
    /// (paragraphs, lists, links). What the default paste inserts, and what
    /// an app's `Paste` callback (a mail app's sanitizer) reads.
    #[test]
    fn a_paste_keeps_the_html_flavour_next_to_the_plain_text() {
        let payload = ClipboardPayload::new(Platform::MacOs)
            .with("public.utf8-plain-text", &b"a\nb"[..])
            .with("public.html", &b"<p>a</p><p><b>b</b></p>"[..]);
        let content = payload_to_clipboard_content(&payload).expect("decodes");
        assert_eq!(content.plain_text.as_str(), "a\nb");
        assert_eq!(
            content.html.as_ref().map(|h| h.as_str()),
            Some("<p>a</p><p><b>b</b></p>")
        );
    }

    /// A copy whose content carries HTML publishes THAT as the HTML flavour
    /// (next to the plain text), and it reads back as it was written.
    #[test]
    fn a_copy_with_html_publishes_it_as_the_html_flavour() {
        let content = ClipboardContent {
            plain_text: "a\nb".into(),
            styled_runs: StyledTextRunVec::from_const_slice(&[]),
            html: azul_css::OptionString::Some("<p>a</p><p>b</p>".into()),
            ..Default::default()
        };
        let payload = clipboard_content_to_payload(&content).expect("encodes");
        let back = payload_to_clipboard_content(&payload).expect("decodes");
        assert_eq!(back.plain_text.as_str(), "a\nb");
        assert_eq!(
            back.html.as_ref().map(|h| h.as_str()),
            Some("<p>a</p><p>b</p>")
        );
    }

    /// Three files copied in Finder are three pasteboard items, one
    /// `public.file-url` each: a paste of them carries all three paths (the
    /// URLs decoded), whatever else it carries.
    #[test]
    fn files_copied_in_finder_paste_as_their_paths() {
        let mut payload = ClipboardPayload::new(Platform::MacOs);
        for (i, url) in [
            "file:///Users/me/Pictures/a%20b.png",
            "file:///Users/me/notes.txt",
            "file:///Volumes/Disk/c.pdf",
        ]
        .iter()
        .enumerate()
        {
            payload.push(rich_clipboard::ClipboardItem::in_item(
                i,
                "public.file-url",
                url.as_bytes().to_vec(),
            ));
        }
        let content = payload_to_clipboard_content(&payload).expect("a paste of files");
        let files: Vec<&str> = content.files.as_ref().iter().map(|f| f.as_str()).collect();
        assert_eq!(
            files,
            ["/Users/me/Pictures/a b.png", "/Users/me/notes.txt", "/Volumes/Disk/c.pdf"]
        );
    }

    /// A copy of files publishes them as files - so Finder, Explorer or a file
    /// manager pastes them - and reads back as the same paths.
    #[test]
    fn a_copy_of_files_reads_back_as_the_same_files() {
        let content = ClipboardContent {
            plain_text: "a b.png\nnotes.txt".into(),
            files: vec![
                azul_css::AzString::from("/Users/me/a b.png"),
                azul_css::AzString::from("/Users/me/notes.txt"),
            ]
            .into(),
            ..Default::default()
        };
        let payload = clipboard_content_to_payload(&content).expect("encodes");
        let back = payload_to_clipboard_content(&payload).expect("decodes");
        let files: Vec<&str> = back.files.as_ref().iter().map(|f| f.as_str()).collect();
        assert_eq!(files, ["/Users/me/a b.png", "/Users/me/notes.txt"]);
        assert_eq!(back.plain_text.as_str(), "a b.png\nnotes.txt");
    }

    /// A screenshot copied to the clipboard is an image and nothing else: the
    /// paste carries its encoded bytes as the source offered them.
    #[test]
    fn an_image_on_the_pasteboard_pastes_as_its_bytes() {
        let png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
        let payload = ClipboardPayload::new(Platform::MacOs).with("public.png", png.clone());
        let content = payload_to_clipboard_content(&payload).expect("a paste of an image");
        assert_eq!(content.image.as_ref().map(|i| i.as_ref().to_vec()), Some(png));
    }

    /// A copy of an image publishes it as an image, and it reads back as the
    /// same bytes.
    #[test]
    fn a_copy_of_an_image_reads_back_as_the_same_image() {
        let png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 9, 9];
        let content = ClipboardContent {
            image: azul_css::OptionU8Vec::Some(png.clone().into()),
            ..Default::default()
        };
        let payload = clipboard_content_to_payload(&content).expect("encodes");
        let back = payload_to_clipboard_content(&payload).expect("decodes");
        assert_eq!(back.image.as_ref().map(|i| i.as_ref().to_vec()), Some(png));
    }

    /// A link (a browser's "Copy link", `public.url`) pastes as the link, with
    /// its address as the plain text; a copy of a link reads back as it.
    #[test]
    fn a_link_pastes_as_the_link_and_its_address() {
        let payload = ClipboardPayload::new(Platform::MacOs)
            .with("public.url", &b"https://example.com/a?b=1"[..]);
        let content = payload_to_clipboard_content(&payload).expect("a paste of a link");
        assert_eq!(
            content.link.as_ref().map(|l| l.as_str()),
            Some("https://example.com/a?b=1")
        );
        assert_eq!(content.plain_text.as_str(), "https://example.com/a?b=1");

        let copied = ClipboardContent {
            plain_text: "https://azul.rs".into(),
            link: azul_css::OptionString::Some("https://azul.rs".into()),
            ..Default::default()
        };
        let payload = clipboard_content_to_payload(&copied).expect("encodes");
        let back = payload_to_clipboard_content(&payload).expect("decodes");
        assert_eq!(back.link.as_ref().map(|l| l.as_str()), Some("https://azul.rs"));
        assert_eq!(back.plain_text.as_str(), "https://azul.rs");
    }

    fn drag_data(parts: &[(&str, &[u8])]) -> azul_core::drag::DragData {
        let mut data = azul_core::drag::DragData::new();
        for (mime, bytes) in parts {
            data.set_data(*mime, bytes.to_vec());
        }
        data
    }

    /// A drag of files out of the window offers the files (and the text the
    /// app gave with them) to the application it is dropped on.
    #[test]
    fn a_drag_of_files_offers_them_to_other_applications() {
        let data = drag_data(&[
            (
                "text/uri-list",
                b"file:///Users/me/Pictures/a%20b.png\r\nfile:///Users/me/c.txt\r\n",
            ),
            ("text/plain", b"a b.png\nc.txt"),
        ]);
        let content = drag_data_to_clipboard_content(&data).expect("an OS drag");
        let files: Vec<&str> = content.files.as_ref().iter().map(|f| f.as_str()).collect();
        assert_eq!(files, ["/Users/me/Pictures/a b.png", "/Users/me/c.txt"]);
        assert_eq!(content.plain_text.as_str(), "a b.png\nc.txt");
        // ... and it publishes as files, as a copy of them would.
        let payload = clipboard_content_to_payload(&content).expect("encodes");
        let back = payload_to_clipboard_content(&payload).expect("decodes");
        assert_eq!(back.files.as_ref().len(), 2);
    }

    /// Text and an image leave the window too.
    #[test]
    fn a_drag_of_text_or_an_image_offers_it_to_other_applications() {
        let text = drag_data_to_clipboard_content(&drag_data(&[("text/plain", b"hello")]))
            .expect("a text drag");
        assert_eq!(text.plain_text.as_str(), "hello");
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 7];
        let image = drag_data_to_clipboard_content(&drag_data(&[("image/png", &png)]))
            .expect("an image drag");
        assert_eq!(image.image.as_ref().map(|i| i.as_ref().to_vec()), Some(png.to_vec()));
    }

    /// A drag that carries only the app's own type (a task moved between lists)
    /// has nothing for another application: it stays a drag inside the app.
    #[test]
    fn a_drag_of_the_apps_own_data_stays_inside_the_app() {
        assert!(
            drag_data_to_clipboard_content(&drag_data(&[("application/x-aztasks-task", b"42")]))
                .is_none()
        );
        assert!(drag_data_to_clipboard_content(&azul_core::drag::DragData::new()).is_none());
    }

    /// A secret copied by a password manager is marked for the platform's
    /// clipboard managers to leave alone, and a paste of it says so.
    #[test]
    fn a_concealed_copy_tells_clipboard_managers_to_leave_it_alone() {
        let content = ClipboardContent {
            plain_text: "hunter2".into(),
            concealed: true,
            ..Default::default()
        };
        let payload = clipboard_content_to_payload(&content).expect("encodes");
        let natives: Vec<&str> = payload.items().iter().map(|i| i.native.as_str()).collect();
        let marker = match Platform::native() {
            Platform::MacOs => "org.nspasteboard.ConcealedType",
            Platform::Windows => "ExcludeClipboardContentFromMonitorProcessing",
            Platform::Unix => "x-kde-passwordManagerHint",
        };
        assert!(natives.contains(&marker), "{natives:?}");
        let back = payload_to_clipboard_content(&payload).expect("decodes");
        assert!(back.concealed);
        assert_eq!(back.plain_text.as_str(), "hunter2");

        let plain = clipboard_content_to_payload(&ClipboardContent {
            plain_text: "hello".into(),
            ..Default::default()
        })
        .expect("encodes");
        assert!(!payload_to_clipboard_content(&plain).expect("decodes").concealed);
    }
}
