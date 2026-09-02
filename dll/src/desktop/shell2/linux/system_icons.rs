//! The `"system"` icon pack: the DESKTOP's own icons, loaded at startup.
//!
//! A freedesktop icon theme is a directory of SVGs on disk
//! (`/usr/share/icons/<theme>/actions/16/arrow-right.svg`), and the theme the
//! session actually uses is already discovered
//! (`SystemStyle::linux.icon_theme`). Loading a handful of them means a
//! drop-down's arrow, a submenu's indicator and a tree's expander are the same
//! shapes Qt and GTK draw, instead of the hardcoded glyphs azul reached for
//! (`"▶"` in the menu renderer, which is a different weight, size and baseline
//! from every native one).
//!
//! Breeze's icons are written to be recoloured — the paths carry
//! `fill="currentColor"` — so ONE asset serves light and dark, tinted with the
//! palette we already detected. `currentColor` is substituted textually before
//! parsing, because it is a CSS cascade value and there is no cascade here.
//!
//! Registered under the pack name `system`, addressable either bare
//! (`arrow-right`, first-match-wins across packs) or explicitly
//! (`system:arrow-right`) when a name would otherwise collide with an app's
//! own pack.

use azul_core::icon::IconProviderHandle;
use azul_css::props::basic::color::ColorU;
use azul_css::system::SystemStyle;

/// The icons worth having: every one is a shape a widget draws today.
///
/// Kept small on purpose — this runs on the startup path, and each entry is a
/// file probe plus an SVG rasterisation.
const WANTED: &[&str] = &[
    // Indicators: submenu arrows, drop-down chevrons, tree expanders,
    // spinner steppers. Four shapes cover all of them.
    "arrow-up",
    "arrow-down",
    "arrow-left",
    "arrow-right",
    // A menu's checked mark and a checkbox's tick.
    "checkmark",
    "dialog-ok",
    // Close affordances: a toast's dismiss, an alert's ✕, a CSD close button.
    "window-close",
    "dialog-close",
    // Navigation, for back/forward affordances.
    "go-previous",
    "go-next",
    // Steppers on a number input.
    "list-add",
    "list-remove",
    // A search field's magnifier and a hamburger.
    "edit-find",
    "application-menu",
    // CSD window controls — the most-looked-at icons in the whole window.
    "window-minimize",
    "window-maximize",
    "window-restore",
];

/// Where freedesktop themes live, most specific first.
/// The size a `scalable` icon is drawn at when the theme states none.
///
/// freedesktop names its directories by NOMINAL size (`actions/16`), and that
/// number is the size the icon was DESIGNED for - it is what the desktop draws
/// it at, and what this app must lay it out at. `scalable` has no number, so
/// an indicator-sized default stands in.
const SCALABLE_NOMINAL_PX: u32 = 16;

/// Every directory an icon may live in, paired with its NOMINAL size.
fn icon_theme_dirs(theme: &str) -> Vec<(String, u32)> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut out = Vec::new();
    for root in [
        alloc::format!("{home}/.local/share/icons"),
        alloc::format!("{home}/.icons"),
        "/usr/share/icons".to_string(),
        "/usr/local/share/icons".to_string(),
    ] {
        // The sizes a UI indicator is drawn at. 16 first: these are pixel-hinted
        // designs, and the 16px variant is the one drawn beside 10-11pt text.
        for (size, nominal) in [
            ("16", 16),
            ("22", 22),
            ("24", 24),
            ("scalable", SCALABLE_NOMINAL_PX),
        ] {
            out.push((alloc::format!("{root}/{theme}/actions/{size}"), nominal));
        }
    }
    out
}

/// Read one named icon out of the theme, following the theme's `Inherits`
/// chain one level (Breeze Dark inherits Breeze, and only some icons are
/// overridden in the dark variant).
/// Returns the SVG source and the NOMINAL size of the directory it came from
/// - the size the desktop draws this icon at. The bitmap is rasterised larger
/// than that for crispness, so the nominal size has to travel with it or the
/// icon lays out at its oversampled bitmap size (see
/// `azul_layout::icon::register_image_icon_sized`).
fn read_icon_svg(theme: &str, name: &str) -> Option<(Vec<u8>, u32)> {
    for (dir, nominal) in icon_theme_dirs(theme) {
        let path = alloc::format!("{dir}/{name}.svg");
        if let Ok(bytes) = std::fs::read(&path) {
            return Some((bytes, nominal));
        }
    }
    // One level of inheritance, read straight out of index.theme.
    let parent = icon_theme_parent(theme)?;
    for (dir, nominal) in icon_theme_dirs(&parent) {
        let path = alloc::format!("{dir}/{name}.svg");
        if let Ok(bytes) = std::fs::read(&path) {
            return Some((bytes, nominal));
        }
    }
    None
}

/// The first entry of a theme's `Inherits=` line, if it has one.
fn icon_theme_parent(theme: &str) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    for root in [
        alloc::format!("{home}/.local/share/icons"),
        "/usr/share/icons".to_string(),
    ] {
        let Ok(text) = std::fs::read_to_string(alloc::format!("{root}/{theme}/index.theme")) else {
            continue;
        };
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("Inherits=") {
                if let Some(first) = rest.split(',').next() {
                    let first = first.trim();
                    if !first.is_empty() && first != theme {
                        return Some(first.to_string());
                    }
                }
            }
        }
    }
    None
}

/// The recursion limit for reference resolution.
///
/// An icon that references itself (`<use href="#a">` inside `#a`) is malformed
/// but perfectly possible, and this code reads FILES FROM DISK that no build
/// of ours produced. A bound is not defensive programming here, it is the only
/// thing between a broken theme and a hung startup.
const MAX_REFERENCE_DEPTH: usize = 8;

/// Pre-resolve everything an icon points at, so the markup that reaches the
/// DOM stands on its own.
///
/// A freedesktop icon is not a flat drawing. It carries an embedded
/// stylesheet (`.ColorScheme-Text { color:#232629; }`), its paths take their
/// paint from it indirectly (`style="fill:currentColor"` + a
/// `class="ColorScheme-Text"`), and it may pull shapes in through
/// `<use href="#id">`. None of those resolve by themselves here: the SVG-to-DOM
/// path builds nodes, not a cascade, so a `currentColor` that is never
/// substituted falls through to SVG's default of opaque BLACK - which is
/// exactly how every themed icon came out black on a dark panel.
///
/// So the references are resolved FIRST, in the source:
///
/// * `.ColorScheme-Text` is repointed at the palette tint, while
///   `NegativeText` / `PositiveText` / `NeutralText` keep the colours their
///   author chose (a close button is red on purpose);
/// * every element's `currentColor` becomes the colour ITS class names, not a
///   single global substitution - that is what kept the negative red;
/// * `<use href="#id">` is replaced by the element it names, recursively, to
///   [`MAX_REFERENCE_DEPTH`].
///
/// `url(#gradient)` references are deliberately LEFT ALONE: the definitions
/// travel with the document, and the DOM renderer resolves them properly
/// (`SvgNodeData::{LinearGradient, RadialGradient, GradientStop}`) - flattening
/// them here would throw away the one thing that path does better.
fn resolve_svg_references(svg: &[u8], tint: ColorU) -> Vec<u8> {
    let Ok(text) = core::str::from_utf8(svg) else {
        return svg.to_vec();
    };
    let hex = hex_of(tint);
    let classes = color_scheme_classes(text, &hex);
    let inlined = inline_use_elements(text, MAX_REFERENCE_DEPTH);
    resolve_current_color(&inlined, &classes, &hex).into_bytes()
}

fn hex_of(c: ColorU) -> String {
    alloc::format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// The `.ColorScheme-*` class colours declared in the icon's own `<style>`
/// block, with `ColorScheme-Text` repointed at the palette tint.
///
/// Everything else keeps its authored value: `NegativeText` is the red a
/// close/delete icon is drawn in, and repainting it in the window's text
/// colour would erase the one thing that marks it as destructive.
fn color_scheme_classes(text: &str, tint_hex: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(dot) = rest.find(".ColorScheme-") {
        rest = &rest[dot + 1..];
        let Some(brace) = rest.find('{') else { break };
        let name = rest[..brace].trim().to_string();
        let Some(color_at) = rest[brace..].find("color:") else {
            continue;
        };
        let after = &rest[brace + color_at + "color:".len()..];
        let end = after.find(';').unwrap_or(after.len());
        let value = after[..end].trim().to_string();
        let value = if name == "ColorScheme-Text" {
            tint_hex.to_string()
        } else {
            value
        };
        out.push((name, value));
    }
    out
}

/// Replace `currentColor` inside each element with the colour that element's
/// `class` resolves to (falling back to the tint).
///
/// Scoped per element rather than globally: a single document mixes
/// `ColorScheme-Text` paths with `ColorScheme-NegativeText` ones, and one
/// blanket substitution paints the whole icon in the same colour.
fn resolve_current_color(text: &str, classes: &[(String, String)], tint_hex: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let (tag, after) = rest.split_at(end);
        let color = element_color(tag, classes).unwrap_or_else(|| tint_hex.to_string());
        out.push_str(&tag.replace("currentColor", &color));
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The colour one element's `class` attribute resolves to.
fn element_color(tag: &str, classes: &[(String, String)]) -> Option<String> {
    let class_at = tag.find("class=")?;
    let after = &tag[class_at + "class=".len()..];
    let quote = after.chars().next()?;
    let rest = after.get(1..)?;
    let end = rest.find(quote)?;
    let names = &rest[..end];
    names
        .split_whitespace()
        .find_map(|n| classes.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone()))
}

/// Replace `<use href="#id">` with the element it names, to `depth` levels.
///
/// The DOM path models `<use>` (`SvgNodeData::Use`) but resolving it needs the
/// document, which the icon renderer does not hand it - so the reference is
/// pre-resolved here, where the whole file is in scope.
fn inline_use_elements(text: &str, depth: usize) -> String {
    if depth == 0 || !text.contains("<use") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut substituted = false;
    while let Some(at) = rest.find("<use") {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let (tag, after) = rest.split_at(end);
        match referenced_id(tag).and_then(|id| element_with_id(text, &id)) {
            Some(target) => {
                out.push_str(&target);
                substituted = true;
            }
            // A dangling reference: drop the `<use>` rather than emit markup
            // pointing at nothing.
            None => {}
        }
        rest = after;
    }
    out.push_str(rest);
    if substituted {
        // The injected element may itself contain a `<use>`.
        return inline_use_elements(&out, depth - 1);
    }
    out
}

/// The `#id` a `<use>` tag points at, from `href` or `xlink:href`.
fn referenced_id(tag: &str) -> Option<String> {
    for key in ["xlink:href=", "href="] {
        let Some(at) = tag.find(key) else { continue };
        let after = &tag[at + key.len()..];
        let quote = after.chars().next()?;
        let rest = after.get(1..)?;
        let end = rest.find(quote)?;
        if let Some(id) = rest[..end].strip_prefix('#') {
            return Some(id.to_string());
        }
    }
    None
}

/// The full source of the element carrying `id`, children included.
///
/// A depth counter over `<`/`>` rather than a parse: the input is already
/// well-formed XML by the time it gets here (the theme ships it, and a
/// malformed file simply fails to parse later), and this runs once per icon at
/// startup.
fn element_with_id(text: &str, id: &str) -> Option<String> {
    let needle = alloc::format!("id=\"{id}\"");
    let at = text.find(&needle)?;
    let start = text[..at].rfind('<')?;
    let rest = &text[start..];
    let first_end = rest.find('>')?;
    // Self-closing: the element IS the tag.
    if rest[..first_end].ends_with('/') {
        return Some(rest[..=first_end].to_string());
    }
    let tag_name: String = rest[1..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == ':' || *c == '-')
        .collect();
    let open = alloc::format!("<{tag_name}");
    let close = alloc::format!("</{tag_name}>");
    let mut depth = 1usize;
    let mut cursor = first_end + 1;
    while depth > 0 {
        let next_open = rest[cursor..].find(&open).map(|i| cursor + i);
        let next_close = rest[cursor..].find(&close).map(|i| cursor + i)?;
        match next_open {
            Some(o) if o < next_close => {
                depth += 1;
                cursor = o + open.len();
            }
            _ => {
                depth -= 1;
                cursor = next_close + close.len();
            }
        }
    }
    Some(rest[..cursor].to_string())
}

/// Load the desktop's icons into the `"system"` pack.
///
/// Best-effort by design: a session with no icon theme, a theme that ships no
/// SVGs, or a build without the rasteriser simply registers nothing, and every
/// caller keeps the glyph fallback it already had.
pub fn register_system_icons(provider: &mut IconProviderHandle, style: &SystemStyle) {
    let Some(theme) = style.linux.icon_theme.as_option() else {
        return;
    };
    let theme = theme.as_str();
    // The tint: what the desktop paints its own foreground with.
    let tint = style
        .colors
        .text
        .into_option()
        .unwrap_or(ColorU::new_rgb(0, 0, 0));

    let (icons, from_cache) = rendered_icons(theme, tint);
    let registered = icons.len();
    for (name, image, nominal) in icons {
        azul_layout::icon::register_image_icon_sized(
            provider, "system", &name, image, nominal, nominal,
        );
    }

    crate::plog_debug!(
        "[system-icons] icon theme '{}': registered {}/{} into the \"system\" pack{}",
        theme,
        registered,
        WANTED.len(),
        if from_cache { " (cached)" } else { "" }
    );
}

/// Re-read the desktop's icons into a LIVE provider after a theme change.
///
/// `register_system_icons` runs once, at `App::create`, against the provider
/// handle the config owns - and the artwork it produces is theme-dependent
/// twice over: KDE ships `breeze` and `breeze-dark` as two directories, and
/// the tint comes from the palette. A session that switched to dark kept
/// serving the light-tinted set, which is dark glyphs on a dark panel.
///
/// Cheap when nothing moved: the render cache is keyed by (theme, tint), so a
/// call that changes neither costs one map lookup and a clone of the handles.
pub(crate) fn refresh_system_icons(
    provider: &azul_core::icon::SharedIconProvider,
    style: &SystemStyle,
) {
    let Some(theme) = style.linux.icon_theme.as_option() else {
        return;
    };
    let tint = style
        .colors
        .text
        .into_option()
        .unwrap_or(ColorU::new_rgb(0, 0, 0));
    let (icons, from_cache) = rendered_icons(theme.as_str(), tint);
    if from_cache {
        // Same theme AND same tint: what is registered is already right, and
        // re-registering would only flush the resolution cache for nothing.
        return;
    }
    let count = icons.len();
    for (name, image, nominal) in icons {
        let data = azul_layout::icon::ImageIconData {
            image,
            width: nominal,
            height: nominal,
        };
        provider.register_icon("system", &name, azul_core::refany::RefAny::new(data));
    }
    crate::plog_debug!(
        "[system-icons] theme changed to '{}': re-registered {} icons",
        theme.as_str(),
        count
    );
}

/// One rasterised icon: its name, its pixels, and the size the desktop draws
/// it at.
type RenderedIcon = (String, azul_core::resources::ImageRef, f32);

/// The rendered icon set for a theme, CACHED BY THEME.
///
/// Keyed by `(icon theme, tint)` because both change together and both change
/// the pixels: KDE ships `breeze` and `breeze-dark` as two directories, and a
/// colour-scheme switch repoints `Icons/Theme` at the other one AND moves the
/// foreground colour the icons are tinted with. Without the key, a session
/// that started light kept serving light-tinted icons after the switch -
/// dark glyphs on a dark panel.
///
/// Cached rather than re-read because this is not cheap: a file probe per
/// icon, an XML parse, a layout pass and a rasterisation, times
/// `WANTED.len()`. A switch BACK to the previous theme re-renders rather than
/// serving a stale entry - the cache holds one theme, which is the only one
/// that can be on screen.
fn rendered_icons(theme: &str, tint: ColorU) -> (Vec<RenderedIcon>, bool) {
    use std::sync::Mutex;

    type Key = (String, [u8; 4]);
    static CACHE: Mutex<Option<(Key, Vec<RenderedIcon>)>> = Mutex::new(None);

    let key: Key = (theme.to_string(), [tint.r, tint.g, tint.b, tint.a]);
    let mut guard = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((cached_key, icons)) = guard.as_ref() {
        if *cached_key == key {
            return (icons.clone(), true);
        }
    }

    let mut out: Vec<RenderedIcon> = Vec::new();
    for name in WANTED {
        let Some((svg, nominal_px)) = read_icon_svg(theme, name) else {
            continue;
        };
        let Some(image) = render_icon(&svg, nominal_px, tint) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss)] // nominal icon sizes are 16..48
        let nominal = nominal_px as f32;
        out.push(((*name).to_string(), image, nominal));
    }
    *guard = Some((key, out.clone()));
    (out, false)
}

/// One theme SVG -> pixels, through the DOM.
///
/// ONE code path: the SVG becomes a real `Dom` (the XML parser maps `<path>`,
/// `<use>`, `<linearGradient>` and `<stop>` onto `SvgNodeData` nodes, an
/// `<svg>` carries its viewBox and its intrinsic size, and a shape's `fill` is
/// an ordinary CSS background clipped to its own geometry) and is drawn by the
/// ORDINARY renderer. No second rasteriser: `render_svg_to_png` is a parallel
/// implementation with its own gaps.
///
/// Rasterised at 2x the nominal size so it stays crisp on a HiDPI display;
/// registered at the NOMINAL size, which is the size the desktop draws it at
/// (see `azul_layout::icon::register_image_icon_sized`).
///
/// The background is fully TRANSPARENT: an icon composites over whatever is
/// behind it, and rendered on opaque white it arrives as a white tile sitting
/// in the titlebar instead of a glyph on it.
fn render_icon(
    svg: &[u8],
    nominal_px: u32,
    tint: ColorU,
) -> Option<azul_core::resources::ImageRef> {
    let rendered = render_icon_pixels(svg, nominal_px, tint)?;
    let raw = azul_core::resources::RawImage {
        tag: Vec::new().into(),
        pixels: azul_core::resources::RawImageData::U8(rendered.rgba.into()),
        width: rendered.pixel_width as usize,
        height: rendered.pixel_height as usize,
        premultiplied_alpha: false,
        data_format: azul_core::resources::RawImageFormat::RGBA8,
    };
    azul_core::resources::ImageRef::new_rawimage(raw)
}

/// [`render_icon`] stopping at the raw pixels, so a test can count what
/// actually got painted rather than only what size the frame is.
fn render_icon_pixels(
    svg: &[u8],
    nominal_px: u32,
    tint: ColorU,
) -> Option<azul_layout::cpurender::ComponentPreviewResult> {
    let resolved = resolve_svg_references(svg, tint);
    let markup = core::str::from_utf8(&resolved).ok()?;
    let parsed = azul_layout::xml::parse_xml(markup).ok()?;
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);

    #[allow(clippy::cast_precision_loss)] // nominal icon sizes are 16..48
    let nominal = nominal_px as f32;
    let transparent = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    // An icon is a DRAWING, not a document: the UA `<body>` margin would inset
    // it by 8px and push most of a 16px glyph out of its own frame.
    let no_page_chrome = azul_css::css::Css::from_string(
        "html, body { margin: 0; padding: 0; border: none; }".into(),
    );
    azul_layout::cpurender::render_dom_to_rgba(
        dom,
        no_page_chrome,
        nominal,
        nominal,
        2.0,
        transparent,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINT: ColorU = ColorU {
        r: 0x12,
        g: 0x34,
        b: 0x56,
        a: 255,
    };

    fn resolved(svg: &str) -> String {
        String::from_utf8(resolve_svg_references(svg.as_bytes(), TINT)).unwrap()
    }

    /// Breeze's icons are authored for recolouring — the paths say
    /// `fill="currentColor"` and the stylesheet sets `.ColorScheme-Text`.
    /// Neither resolves without a cascade, so the reference is resolved in the
    /// SOURCE. Without this an icon renders black on a dark panel.
    #[test]
    fn the_tint_replaces_both_spellings_of_the_theme_colour() {
        let text = resolved(
            r#"<svg><style>.ColorScheme-Text { color:#eff0f1; }</style>
            <path d="M0 0" class="ColorScheme-Text" fill="currentColor"/></svg>"#,
        );
        assert!(
            text.contains("fill=\"#123456\""),
            "currentColor must become the tint: {text}"
        );
        assert!(
            !text.contains("currentColor"),
            "no unresolved currentColor may survive: {text}"
        );
    }

    /// THE bug a single global substitution causes: one document mixes
    /// `ColorScheme-Text` with `ColorScheme-NegativeText`, and a close icon is
    /// red ON PURPOSE. Repainting it in the window's text colour erases the
    /// only thing marking it destructive.
    #[test]
    fn each_element_takes_the_colour_its_own_class_names() {
        let text = resolved(
            r#"<svg><defs><style>
                .ColorScheme-Text { color:#eff0f1; }
                .ColorScheme-NegativeText { color:#da4453; }
            </style></defs>
            <path id="a" class="ColorScheme-Text" style="fill:currentColor"/>
            <path id="b" class="ColorScheme-NegativeText" style="fill:currentColor"/>
            </svg>"#,
        );
        let a = text.split("id=\"a\"").nth(1).unwrap().split('>').next().unwrap();
        let b = text.split("id=\"b\"").nth(1).unwrap().split('>').next().unwrap();
        assert!(a.contains("#123456"), "the Text path takes the tint: {a}");
        assert!(
            b.contains("#da4453"),
            "the NegativeText path keeps its authored red: {b}"
        );
    }

    /// An element with no class still has to resolve to SOMETHING - the SVG
    /// default is opaque black, which is invisible on a dark panel.
    #[test]
    fn an_unclassed_element_falls_back_to_the_tint() {
        let text = resolved(r#"<svg><path style="fill:currentColor"/></svg>"#);
        assert!(text.contains("#123456"), "{text}");
    }

    /// `<use href="#id">` is pre-resolved: the DOM path models the node but
    /// has no document to look the target up in.
    #[test]
    fn a_use_element_is_replaced_by_what_it_points_at() {
        let text = resolved(
            r##"<svg><defs><path id="shape" d="M1 2"/></defs><use href="#shape"/></svg>"##,
        );
        assert_eq!(
            text.matches("M1 2").count(),
            2,
            "the referenced path must be injected at the use site: {text}"
        );
        assert!(!text.contains("<use"), "no unresolved <use> may survive");
    }

    /// A `<use>` pointing at nothing must not leave markup addressing a
    /// missing id, and must not loop.
    #[test]
    fn a_dangling_or_self_referential_use_terminates() {
        let dangling = resolved(r##"<svg><use href="#nope"/></svg>"##);
        assert!(!dangling.contains("<use"), "{dangling}");

        // Self-reference: bounded by MAX_REFERENCE_DEPTH rather than hanging.
        let cyclic = resolved(r##"<svg><g id="a"><use href="#a"/></g></svg>"##);
        assert!(cyclic.contains("<svg"), "must still return markup");
    }

    /// Gradients are LEFT ALONE on purpose: the definitions travel with the
    /// document and the DOM renderer resolves them properly. Rewriting them
    /// here would throw away the one thing that path does better.
    #[test]
    fn gradient_references_are_left_for_the_dom_renderer() {
        let text = resolved(
            r##"<svg><defs><linearGradient id="g"><stop stop-color="#f00"/></linearGradient></defs>
            <rect fill="url(#g)"/></svg>"##,
        );
        assert!(text.contains("url(#g)"), "the reference survives: {text}");
        assert!(
            text.contains("linearGradient"),
            "and so does its definition: {text}"
        );
    }

    /// The whole point of the DOM path: an icon has to come out as PIXELS.
    ///
    /// A blank icon is the failure mode every step here can produce silently -
    /// an unsized `<svg>` lays out 0x0, a dropped viewBox scales the art out
    /// of frame, a missing fill paints nothing - and the only way to tell is
    /// to count the pixels that actually got painted.
    #[test]
    fn a_theme_svg_renders_to_visible_pixels() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">
          <defs><style>.ColorScheme-Text { color:#232629; }</style></defs>
          <path style="fill:currentColor" class="ColorScheme-Text"
                d="M 2,2 L 14,2 L 14,14 L 2,14 Z"/>
        </svg>"##;
        let rendered = render_icon_pixels(svg, 16, ColorU::new_rgb(0xff, 0x00, 0x00))
            .expect("a well-formed icon must render");
        assert!(
            rendered.pixel_width >= 32 && rendered.pixel_height >= 32,
            "oversampled to 2x the nominal size for HiDPI, got {}x{}",
            rendered.pixel_width,
            rendered.pixel_height
        );
        let total = (rendered.pixel_width * rendered.pixel_height) as usize;
        let painted = rendered.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(
            painted * 4 > total,
            "the 12x12 square covers over half the frame; only {painted}/{total} \
             pixels got painted"
        );
        // ... and the rest is TRANSPARENT, not white: an icon composites over
        // whatever is behind it.
        assert!(
            painted < total,
            "the corners outside the square must stay transparent"
        );
    }

    /// Non-UTF8 bytes are not an SVG we can rewrite; hand them back untouched
    /// rather than panicking on the startup path.
    #[test]
    fn invalid_utf8_is_passed_through() {
        let bytes = [0xff, 0xfe, 0x00];
        assert_eq!(
            resolve_svg_references(&bytes, ColorU::new_rgb(1, 2, 3)),
            bytes.to_vec()
        );
    }

    /// The search covers the per-size directories a freedesktop theme uses,
    /// user themes before system ones — a user override must win.
    #[test]
    fn the_search_path_prefers_user_themes_and_covers_the_size_dirs() {
        let dirs = icon_theme_dirs("breeze-dark");
        assert!(dirs
            .iter()
            .any(|(d, _)| d.ends_with("/breeze-dark/actions/16")));
        assert!(dirs
            .iter()
            .any(|(d, _)| d.ends_with("/breeze-dark/actions/scalable")));
        let first_system = dirs.iter().position(|(d, _)| d.starts_with("/usr/share"));
        let first_user = dirs.iter().position(|(d, _)| d.contains("/.local/share"));
        if let (Some(sys), Some(user)) = (first_system, first_user) {
            assert!(user < sys, "a user-installed theme must be searched first");
        }
    }

    /// Each directory carries the NOMINAL size freedesktop names it by - the
    /// size the desktop draws that icon at, which has to survive the trip to
    /// the icon provider or the icon lays out at its oversampled bitmap size.
    #[test]
    fn every_size_directory_reports_the_size_it_is_named_after() {
        for (dir, nominal) in icon_theme_dirs("breeze") {
            let leaf = dir.rsplit('/').next().unwrap_or("");
            match leaf {
                "scalable" => assert_eq!(
                    nominal, SCALABLE_NOMINAL_PX,
                    "a scalable icon has no stated size; the indicator default stands in"
                ),
                px => assert_eq!(
                    px.parse::<u32>().ok(),
                    Some(nominal),
                    "{dir} must report {px}, not {nominal}"
                ),
            }
        }
    }
}
