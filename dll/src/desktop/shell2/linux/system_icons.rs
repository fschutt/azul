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
fn icon_theme_dirs(theme: &str) -> Vec<String> {
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
        for size in ["16", "22", "24", "scalable"] {
            out.push(alloc::format!("{root}/{theme}/actions/{size}"));
        }
    }
    out
}

/// Read one named icon out of the theme, following the theme's `Inherits`
/// chain one level (Breeze Dark inherits Breeze, and only some icons are
/// overridden in the dark variant).
fn read_icon_svg(theme: &str, name: &str) -> Option<Vec<u8>> {
    for dir in icon_theme_dirs(theme) {
        let path = alloc::format!("{dir}/{name}.svg");
        if let Ok(bytes) = std::fs::read(&path) {
            return Some(bytes);
        }
    }
    // One level of inheritance, read straight out of index.theme.
    let parent = icon_theme_parent(theme)?;
    for dir in icon_theme_dirs(&parent) {
        let path = alloc::format!("{dir}/{name}.svg");
        if let Ok(bytes) = std::fs::read(&path) {
            return Some(bytes);
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

/// Substitute `currentColor` with the palette tint.
///
/// The icons are authored against a cascade they will never see here, so the
/// value is replaced in the source before parsing. Also rewrites the
/// `ColorScheme-Text` stylesheet colour, which some Breeze icons use INSTEAD
/// of `currentColor`.
fn tint_svg(svg: &[u8], tint: ColorU) -> Vec<u8> {
    let Ok(text) = core::str::from_utf8(svg) else {
        return svg.to_vec();
    };
    let hex = alloc::format!("#{:02x}{:02x}{:02x}", tint.r, tint.g, tint.b);
    let mut out = text.replace("currentColor", &hex);
    // `.ColorScheme-Text { color:#eff0f1; }` — the class the paths reference.
    if let Some(start) = out.find("color:") {
        if let Some(end) = out[start..].find(';') {
            let range = start..start + end;
            out.replace_range(range, &alloc::format!("color:{hex}"));
        }
    }
    out.into_bytes()
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

    let mut registered = 0usize;
    for name in WANTED {
        let Some(svg) = read_icon_svg(theme, name) else {
            continue;
        };
        let tinted = tint_svg(&svg, tint);
        let Ok(parsed) = azul_layout::xml::svg::svg_parse(
            &tinted,
            azul_core::svg::SvgParseOptions::default(),
        ) else {
            continue;
        };
        // 16 logical px: the size an indicator is drawn at beside body text.
        // The renderer scales from here, so this is a quality floor, not the
        // final size — 32 keeps it crisp on a 2x display.
        let options = azul_core::svg::SvgRenderOptions {
            target_size: azul_css::props::basic::geometry::OptionLayoutSize::Some(
                azul_css::props::basic::geometry::LayoutSize::new(32, 32),
            ),
            ..Default::default()
        };
        let Some(raw) = azul_layout::xml::svg::svg_render(&parsed, options) else {
            continue;
        };
        let Some(image) = azul_core::resources::ImageRef::new_rawimage(raw) else {
            continue;
        };
        azul_layout::icon::register_image_icon(provider, "system", name, image);
        registered += 1;
    }

    crate::plog_debug!(
        "[system-icons] icon theme '{}': registered {}/{} into the \"system\" pack",
        theme,
        registered,
        WANTED.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Breeze's icons are authored for recolouring — the paths say
    /// `fill="currentColor"` and the stylesheet sets `.ColorScheme-Text`.
    /// Neither resolves without a cascade, so the tint is substituted in the
    /// SOURCE. Without this an icon renders black on a dark panel, or not at
    /// all.
    #[test]
    fn the_tint_replaces_both_spellings_of_the_theme_colour() {
        let svg = br#"<svg><style>.ColorScheme-Text { color:#eff0f1; }</style>
            <path d="M0 0" class="ColorScheme-Text" fill="currentColor"/></svg>"#;
        let out = tint_svg(svg, ColorU::new_rgb(0x12, 0x34, 0x56));
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("fill=\"#123456\""),
            "currentColor must become the tint: {text}"
        );
        assert!(
            text.contains("color:#123456"),
            "the ColorScheme-Text colour must become the tint too: {text}"
        );
        assert!(
            !text.contains("currentColor"),
            "no unresolved currentColor may survive"
        );
    }

    /// Non-UTF8 bytes are not an SVG we can rewrite; hand them back untouched
    /// rather than panicking on the startup path.
    #[test]
    fn invalid_utf8_is_passed_through() {
        let bytes = [0xff, 0xfe, 0x00];
        assert_eq!(tint_svg(&bytes, ColorU::new_rgb(1, 2, 3)), bytes.to_vec());
    }

    /// The search covers the per-size directories a freedesktop theme uses,
    /// user themes before system ones — a user override must win.
    #[test]
    fn the_search_path_prefers_user_themes_and_covers_the_size_dirs() {
        let dirs = icon_theme_dirs("breeze-dark");
        assert!(dirs.iter().any(|d| d.ends_with("/breeze-dark/actions/16")));
        assert!(dirs.iter().any(|d| d.ends_with("/breeze-dark/actions/scalable")));
        let first_system = dirs.iter().position(|d| d.starts_with("/usr/share"));
        let first_user = dirs.iter().position(|d| d.contains("/.local/share"));
        if let (Some(sys), Some(user)) = (first_system, first_user) {
            assert!(user < sys, "a user-installed theme must be searched first");
        }
    }
}
