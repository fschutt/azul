//! The app's colour palette, derived from the OS theme.
//!
//! AzWriter's chrome is an Office-2013 pastiche, and the office look is a
//! PALETTE over a fixed layout: a coloured band, an accent-filled status
//! strip, grey canvas, white sheets. Every one of those is a colour the
//! desktop already has an opinion about, so the layout stays exactly where it
//! is and only the colours move — on Breeze Dark the canvas goes charcoal,
//! the sheets keep contrasting with it, and the ribbon reads as a KDE window
//! rather than a white rectangle pasted into a dark session.
//!
//! ONE struct, built once per `layout()` call and threaded down. Not a global:
//! the theme can change between two frames (see
//! `LayoutCallbackInfo::depends_on_system_style`), and a `static` palette is
//! exactly the thing that would still be light after the switch.
//!
//! Every field falls back to its own Office-2013 constant when the desktop
//! reports nothing, and no field is derived from another — the same
//! discipline the widget themes use (`RibbonTheme::from_system` & co.), so a
//! partially-detected desktop degrades one colour at a time instead of
//! cascading into a palette nobody designed.

use azul::css::{ColorU, SystemStyle};
use azul::window::WindowTheme;

/// A colour the desktop may or may not report.
type Opt = azul::option::OptionColorU;

/// The generated FFI `ColorU` derives neither `PartialEq` nor `Debug`
/// (api.json types carry only what the bindings need), so the palette spells
/// both out over its own fields. `parts()` is the ONE list of them, and both
/// impls go through it - a field added to the struct but forgotten here would
/// silently drop out of every comparison, so it is deliberately the only
/// place that enumerates them.
type Rgba = (u8, u8, u8, u8);

const fn parts_of(c: ColorU) -> Rgba {
    (c.r, c.g, c.b, c.a)
}

const fn rgb(r: u8, g: u8, b: u8) -> ColorU {
    ColorU { r, g, b, a: 255 }
}

/// Office 2013 brand blue (#2B579A) - the app's, in every session.
pub const OFFICE_BLUE: ColorU = rgb(43, 87, 154);
pub const WHITE: ColorU = rgb(255, 255, 255);

/// The app palette. Field names say what the colour IS FOR, never what it
/// looks like — `canvas` is grey in the light theme and charcoal in the dark
/// one, and code that reads `canvas` is right in both.
#[derive(Clone, Copy)]
pub struct Palette {
    /// Is this the dark variant? Read by the few places that need a
    /// polarity, not a colour (shadow strength, sheet border).
    pub dark: bool,
    /// The APP's brand fill: the "W" logo square, the status strip, the
    /// backstage nav column, the FILE button. Never the desktop's accent -
    /// see the module docs.
    pub brand: ColorU,
    /// Text on a brand fill.
    pub on_brand: ColorU,
    /// The brand as TEXT on a chrome surface (backstage headings, pane
    /// icons). The only brand value that moves with the theme, and only far
    /// enough to stay legible: #2B579A on a charcoal window is a smudge.
    pub brand_text: ColorU,
    /// The app's window/chrome surface (behind the ribbon and the panes).
    pub chrome: ColorU,
    /// Regular chrome text.
    pub text: ColorU,
    /// Secondary grey (descriptions, property labels).
    pub text_gray: ColorU,
    /// Faint grey (locations, sub-labels).
    pub text_faint: ColorU,
    /// Big pane titles in the backstage.
    pub title_gray: ColorU,
    /// The print-layout canvas AROUND the sheets.
    pub canvas: ColorU,
    /// A page sheet's fill. PAPER, in both themes: a word processor's page
    /// is a preview of a printed sheet, and printed sheets are white. Dark
    /// mode dims it rather than inverting it, so the page stays the lightest
    /// thing on screen and the document's own styling (which is fixed, like
    /// the ribbon's style previews) stays legible on it.
    pub sheet: ColorU,
    /// The sheet's hairline border.
    pub sheet_border: ColorU,
    /// Body text ON a sheet.
    pub sheet_text: ColorU,
    /// Heading text on a sheet (h1/h2).
    pub sheet_heading: ColorU,
    /// Deeper heading (h3).
    pub sheet_heading_deep: ColorU,
    /// Quoted / de-emphasised text on a sheet.
    pub sheet_quiet: ColorU,
    /// Rules, quote bars and table borders on a sheet.
    pub sheet_rule: ColorU,
    /// Inline code / pre fill on a sheet.
    pub sheet_code_bg: ColorU,
    /// A bordered control's outline (Browse button, info tiles).
    pub control_border: ColorU,
    /// A bordered control's fill.
    pub control_bg: ColorU,
    /// Hover fill on a list row or tile.
    pub hover_bg: ColorU,
    /// Fill of a selected list row.
    pub selected_bg: ColorU,
    /// The line between the chrome and the canvas below it. Its own field
    /// because the ribbon's bottom edge needs MORE contrast than an ordinary
    /// separator: it is the boundary between the app's controls and the
    /// document, and on a dark theme two adjacent charcoals with a hairline
    /// between them read as one surface.
    pub chrome_edge: ColorU,
}

impl Palette {
    /// Every colour in the palette, in declaration order - see [`Rgba`].
    const fn parts(&self) -> [Rgba; 22] {
        [
            parts_of(self.brand),
            parts_of(self.on_brand),
            parts_of(self.brand_text),
            parts_of(self.chrome),
            parts_of(self.text),
            parts_of(self.text_gray),
            parts_of(self.text_faint),
            parts_of(self.title_gray),
            parts_of(self.canvas),
            parts_of(self.sheet),
            parts_of(self.sheet_border),
            parts_of(self.sheet_text),
            parts_of(self.sheet_heading),
            parts_of(self.sheet_heading_deep),
            parts_of(self.sheet_quiet),
            parts_of(self.sheet_rule),
            parts_of(self.sheet_code_bg),
            parts_of(self.control_border),
            parts_of(self.control_bg),
            parts_of(self.hover_bg),
            parts_of(self.selected_bg),
            parts_of(self.chrome_edge),
        ]
    }
}

impl PartialEq for Palette {
    fn eq(&self, other: &Self) -> bool {
        self.dark == other.dark && self.parts() == other.parts()
    }
}

impl Eq for Palette {}

impl std::fmt::Debug for Palette {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Palette {{ dark: {}", self.dark)?;
        for (r, g, b, a) in self.parts() {
            write!(f, ", #{r:02x}{g:02x}{b:02x}{a:02x}")?;
        }
        write!(f, " }}")
    }
}

/// The Office-2013 palette, used verbatim when the desktop reports nothing.
const OFFICE_2013: Palette = Palette {
    dark: false,
    brand: OFFICE_BLUE,
    on_brand: WHITE,
    brand_text: OFFICE_BLUE,
    chrome: WHITE,
    text: rgb(68, 68, 68),
    text_gray: rgb(128, 128, 128),
    text_faint: rgb(148, 148, 148),
    title_gray: rgb(86, 86, 86),
    canvas: rgb(227, 227, 227),
    sheet: WHITE,
    sheet_border: rgb(166, 166, 166),
    sheet_text: rgb(26, 26, 26),
    sheet_heading: rgb(46, 116, 181),
    sheet_heading_deep: rgb(31, 77, 120),
    sheet_quiet: rgb(85, 85, 85),
    sheet_rule: rgb(187, 187, 187),
    sheet_code_bg: rgb(242, 242, 242),
    control_border: rgb(197, 197, 197),
    control_bg: WHITE,
    hover_bg: rgb(242, 247, 252),
    selected_bg: rgb(213, 225, 242),
    chrome_edge: rgb(171, 171, 171),
};

/// The dark counterpart of [`OFFICE_2013`] - the fallback for a DARK desktop
/// that reports no colours of its own.
///
/// Not derived from the light one by inversion: inverting `#e3e3e3` gives a
/// canvas that is lighter than the sheet it is supposed to sit behind, and
/// the whole point of the canvas is to be the darker surround. Hand-picked
/// once, in Breeze Dark's neighbourhood.
const OFFICE_2013_DARK: Palette = Palette {
    dark: true,
    // The SAME brand fill: a dark session does not change who the app is.
    brand: OFFICE_BLUE,
    on_brand: WHITE,
    // ... but the brand as text is lifted until it is legible on charcoal.
    brand_text: rgb(106, 156, 219),
    chrome: rgb(49, 54, 59),
    text: rgb(252, 252, 252),
    text_gray: rgb(189, 195, 199),
    text_faint: rgb(150, 156, 161),
    title_gray: rgb(220, 224, 227),
    canvas: rgb(27, 30, 32),
    // DIMMED PAPER, not inverted paper: still the lightest thing on screen,
    // toned down so it does not glare out of a dark session.
    sheet: rgb(226, 226, 226),
    sheet_border: rgb(20, 22, 24),
    // The document's own styling, unchanged - see the `sheet` note.
    sheet_text: rgb(26, 26, 26),
    sheet_heading: rgb(46, 116, 181),
    sheet_heading_deep: rgb(31, 77, 120),
    sheet_quiet: rgb(85, 85, 85),
    sheet_rule: rgb(187, 187, 187),
    sheet_code_bg: rgb(242, 242, 242),
    control_border: rgb(80, 87, 93),
    control_bg: rgb(49, 54, 59),
    hover_bg: rgb(61, 67, 73),
    selected_bg: rgb(48, 89, 118),
    chrome_edge: rgb(20, 22, 24),
};

fn opt(c: Opt) -> Option<ColorU> {
    c.into_option()
}

/// Rough perceived lightness. Only ORDER matters here (which of two surfaces
/// is the paper), so a plain channel sum is enough and stays obvious; a
/// weighted luma would rank two near-neutral greys the same way.
const fn lum(c: ColorU) -> u32 {
    c.r as u32 + c.g as u32 + c.b as u32
}

impl Palette {
    /// The compile-time palette for a polarity, with nothing detected.
    #[must_use]
    pub const fn fallback(theme: WindowTheme) -> Self {
        match theme {
            WindowTheme::DarkMode => OFFICE_2013_DARK,
            _ => OFFICE_2013,
        }
    }

    /// Derive the palette from the live OS style.
    ///
    /// `theme` is the WINDOW's polarity, which is what picks the fallback
    /// set; the reported colours then override field by field. A desktop that
    /// reports a full palette (KDE, GNOME) replaces almost all of it; one that
    /// reports only an accent moves only the accent.
    #[must_use]
    pub fn from_system(style: &SystemStyle, theme: WindowTheme) -> Self {
        let d = Self::fallback(theme);
        let c = &style.colors;

        // NOTE what is NOT read: `colors.accent`. The brand is the app's.
        let text = opt(c.text);
        let secondary = opt(c.secondary_text);
        let tertiary = opt(c.tertiary_text);
        let window_bg = opt(c.window_background);
        let view_bg = opt(c.background);
        let separator = opt(c.separator);
        let selection = opt(c.selection_background);
        let selection_inactive = opt(c.selection_background_inactive);

        // PAPER ON A DESK, and the paper does not change colour with the
        // desktop.
        //
        // The sheet is a preview of a PRINTED page, so it stays paper in both
        // themes and the document's own styling - fixed, like the ribbon's
        // style previews - stays legible on it. The dark palette dims the
        // paper instead of inverting it (the same thing Word does), so it is
        // still the lightest surface on screen without glaring next to a dark
        // UI.
        //
        // Tying the sheet to a system surface is what produced the bug this
        // replaces: on Breeze Dark the view background (#1b1e20) is DARKER
        // than the window background, so "sheet = view background" punched a
        // near-black hole into a lighter surround and the black document text
        // on it was invisible.
        let sheet = d.sheet;

        // The canvas is the surround, and it is the desktop's: the DARKER of
        // the two reported surfaces, so the desk reads as the session's while
        // still sitting behind the paper.
        let canvas = match (view_bg, window_bg.or(opt(c.under_page_background))) {
            (Some(a), Some(b)) => {
                if lum(a) <= lum(b) {
                    a
                } else {
                    b
                }
            }
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => d.canvas,
        };
        // ... but never lighter than the paper it sits behind. A very light
        // desktop (a high-contrast white theme) would otherwise dissolve the
        // page outline entirely; there the hand-picked office grey stands in.
        let canvas = if lum(canvas) < lum(sheet) {
            canvas
        } else {
            d.canvas
        };

        Self {
            dark: d.dark,
            brand: d.brand,
            on_brand: d.on_brand,
            brand_text: d.brand_text,
            chrome: window_bg.unwrap_or(d.chrome),
            text: text.unwrap_or(d.text),
            text_gray: secondary.unwrap_or(d.text_gray),
            text_faint: tertiary.unwrap_or(d.text_faint),
            title_gray: secondary.unwrap_or(d.title_gray),
            canvas,
            sheet,
            sheet_border: separator.unwrap_or(d.sheet_border),
            // Document styling, not desktop styling.
            sheet_text: d.sheet_text,
            // Headings are DOCUMENT styling - Word's heading blue, not the
            // desktop's accent and not the app's brand either. They move only
            // with the paper they sit on (light vs dark), the way the rest of
            // the document's own styles do.
            sheet_heading: d.sheet_heading,
            sheet_heading_deep: d.sheet_heading_deep,
            sheet_quiet: d.sheet_quiet,
            sheet_rule: d.sheet_rule,
            // No system role for a code block's fill; the hand-picked value
            // is the one that stays legible against `sheet`.
            sheet_code_bg: d.sheet_code_bg,
            control_border: separator.unwrap_or(d.control_border),
            control_bg: window_bg.unwrap_or(d.control_bg),
            hover_bg: selection_inactive.unwrap_or(d.hover_bg),
            selected_bg: selection.unwrap_or(d.selected_bg),
            chrome_edge: d.chrome_edge,
        }
    }

    /// `#rrggbb` for embedding in a `with_css` string.
    #[must_use]
    pub fn hex(c: ColorU) -> String {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }
}

impl Default for Palette {
    fn default() -> Self {
        OFFICE_2013
    }
}

/// Widget palettes: the desktop's surfaces with AzWriter's brand put back.
///
/// `*Theme::from_system` maps the OS accent onto every accent-shaped field,
/// which is right for a generic widget and wrong here - those fields ARE the
/// app's identity (the status strip, the nav column, the FILE button). Each
/// builder below therefore takes the system theme for its neutrals and then
/// re-asserts the brand, in ONE place, so no call site has to remember which
/// of a dozen fields is brand and which is chrome.
pub mod widgets {
    use azul::css::SystemStyle;
    use azul::widgets::{
        BackstageStyle, BackstageTheme, RibbonStyle, RibbonTheme, StatusBarStyle, StatusBarTheme,
    };

    use super::Palette;

    /// Ribbon chrome from the desktop; app button and active-tab accent from
    /// the brand.
    #[must_use]
    pub fn ribbon(pal: &Palette, sys: &SystemStyle) -> RibbonStyle {
        let mut t = RibbonTheme::from_system(SystemStyle::clone(sys));
        t.accent = pal.brand;
        t.accent_hover = pal.brand;
        t.accent_text = pal.on_brand;
        // `hover_border` follows the accent in `from_system`; a brand-coloured
        // outline on a hovered control is the office look, and it keeps the
        // hover readable when the desktop accent is far from the brand.
        t.hover_border = pal.brand;
        t.border = pal.chrome_edge;
        RibbonStyle::from_theme(t)
    }

    /// The status strip is a BRAND-filled band, not an accent-filled one.
    #[must_use]
    pub fn status_bar(pal: &Palette, sys: &SystemStyle) -> StatusBarStyle {
        let mut t = StatusBarTheme::from_system(SystemStyle::clone(sys));
        t.bar_bg = pal.brand;
        t.text = pal.on_brand;
        t.thumb = pal.on_brand;
        StatusBarStyle::from_theme(t)
    }

    /// Same for the backstage nav column; the pane behind it stays the
    /// desktop's window surface.
    #[must_use]
    pub fn backstage(pal: &Palette, sys: &SystemStyle) -> BackstageStyle {
        let mut t = BackstageTheme::from_system(SystemStyle::clone(sys));
        t.nav_bg = pal.brand;
        t.nav_text = pal.on_brand;
        t.back_ring = pal.on_brand;
        t.content_bg = pal.chrome;
        BackstageStyle::from_theme(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_style() -> SystemStyle {
        // A desktop that reports NOTHING - the fallback contract.
        let mut s = SystemStyle::default();
        s.colors = azul::css::SystemColors::default();
        s
    }

    #[test]
    fn an_undetected_desktop_gets_the_office_palette_for_its_polarity() {
        assert_eq!(
            Palette::from_system(&empty_style(), WindowTheme::LightMode),
            OFFICE_2013
        );
        assert_eq!(
            Palette::from_system(&empty_style(), WindowTheme::DarkMode),
            OFFICE_2013_DARK
        );
    }

    /// The layout depends on the canvas staying DARKER than the sheet - that
    /// contrast IS the print-layout look. Inverting the light palette breaks
    /// exactly this, which is why the dark set is hand-picked.
    #[test]
    fn the_canvas_stays_behind_the_sheet_in_both_themes() {
        for p in [OFFICE_2013, OFFICE_2013_DARK] {
            let lum = |c: ColorU| u32::from(c.r) + u32::from(c.g) + u32::from(c.b);
            assert!(
                lum(p.canvas) < lum(p.sheet),
                "the canvas must read as the surround, not the paper (dark={})",
                p.dark
            );
            assert!(
                lum(p.sheet_text).abs_diff(lum(p.sheet)) > 300,
                "sheet text must contrast with the sheet (dark={})",
                p.dark
            );
        }
    }

    #[test]
    fn a_reported_colour_overrides_its_field_and_only_its_field() {
        let reported = rgb(9, 99, 199);
        let mut s = empty_style();
        s.colors.text = Some(reported).into();

        let p = Palette::from_system(&s, WindowTheme::LightMode);
        assert_eq!(parts_of(p.text), parts_of(reported));
        assert_eq!(
            parts_of(p.text_gray),
            parts_of(OFFICE_2013.text_gray),
            "an unreported colour keeps its own office value"
        );
        assert_eq!(parts_of(p.canvas), parts_of(OFFICE_2013.canvas));
    }

    /// THE user ruling: the brand is the app's, not the desktop's. Breeze
    /// being blue too is a coincidence, and a green accent must not repaint
    /// the "W".
    #[test]
    fn the_desktop_accent_never_becomes_the_brand() {
        let desktop_green = rgb(39, 174, 96);
        let mut s = empty_style();
        s.colors.accent = Some(desktop_green).into();
        s.colors.accent_text = Some(rgb(0, 0, 0)).into();

        for theme in [WindowTheme::LightMode, WindowTheme::DarkMode] {
            let p = Palette::from_system(&s, theme);
            assert_eq!(
                parts_of(p.brand),
                parts_of(OFFICE_BLUE),
                "the brand fill is the app's in every session"
            );
            assert_eq!(parts_of(p.on_brand), parts_of(WHITE));
            assert_ne!(parts_of(p.brand), parts_of(desktop_green));
            assert_ne!(
                parts_of(p.sheet_heading),
                parts_of(desktop_green),
                "document headings are document styling, not desktop accent"
            );
        }
    }

    /// The one brand value that moves, and only far enough to be readable:
    /// #2B579A as TEXT on a charcoal window is a smudge.
    #[test]
    fn the_brand_text_is_lifted_on_dark_but_the_fill_is_not() {
        let light = Palette::fallback(WindowTheme::LightMode);
        let dark = Palette::fallback(WindowTheme::DarkMode);
        assert_eq!(parts_of(light.brand_text), parts_of(OFFICE_BLUE));
        assert_eq!(parts_of(dark.brand), parts_of(light.brand), "same fill");
        assert!(
            lum(dark.brand_text) > lum(light.brand_text),
            "the dark brand text must be lighter than the light one"
        );
        assert!(
            lum(dark.brand_text) - lum(dark.chrome) > 150,
            "and it must actually separate from the chrome it is written on"
        );
    }

    /// The ribbon's bottom edge separates the app's controls from the
    /// document. Two adjacent charcoals with a hairline between them read as
    /// one surface, so the edge is asserted to CONTRAST with the chrome.
    #[test]
    fn the_chrome_edge_stands_out_from_the_chrome_in_both_themes() {
        for p in [OFFICE_2013, OFFICE_2013_DARK] {
            assert!(
                lum(p.chrome).abs_diff(lum(p.chrome_edge)) > 90,
                "the chrome/canvas boundary must be visible (dark={})",
                p.dark
            );
        }
    }

    /// THE regression this rule exists for: the sheet is PAPER, and Breeze
    /// Dark's view surface (#1b1e20) is darker than its window surface. When
    /// the sheet took a system surface, a dark session punched a near-black
    /// hole into a lighter surround and the document's black text vanished.
    #[test]
    fn the_paper_never_takes_a_system_surface() {
        // Breeze Dark: Window #2a2e32, View #1b1e20.
        let mut dark = empty_style();
        dark.colors.window_background = Some(rgb(42, 46, 50)).into();
        dark.colors.background = Some(rgb(27, 30, 32)).into();
        dark.colors.under_page_background = Some(rgb(42, 46, 50)).into();

        let p = Palette::from_system(&dark, WindowTheme::DarkMode);
        assert_eq!(
            parts_of(p.sheet),
            parts_of(OFFICE_2013_DARK.sheet),
            "the page is paper in a dark session too"
        );
        assert_eq!(
            parts_of(p.canvas),
            parts_of(rgb(27, 30, 32)),
            "the DESK is the desktop's, and the darker of its two surfaces"
        );
        assert!(lum(p.canvas) < lum(p.sheet), "the page reads as a page");
        assert!(
            lum(p.sheet_text).abs_diff(lum(p.sheet)) > 300,
            "black-on-paper stays black-on-paper"
        );
    }

    /// A desktop lighter than the paper (a high-contrast white theme) must not
    /// dissolve the page outline: the office grey stands in as the desk.
    #[test]
    fn a_desktop_lighter_than_the_paper_falls_back_to_the_office_desk() {
        let mut s = empty_style();
        s.colors.window_background = Some(rgb(255, 255, 255)).into();
        s.colors.background = Some(rgb(255, 255, 255)).into();

        let p = Palette::from_system(&s, WindowTheme::LightMode);
        assert_eq!(parts_of(p.canvas), parts_of(OFFICE_2013.canvas));
        assert!(lum(p.canvas) < lum(p.sheet));
    }

    #[test]
    fn hex_round_trips_through_css() {
        assert_eq!(Palette::hex(rgb(43, 87, 154)), "#2b579a");
        assert_eq!(Palette::hex(rgb(0, 0, 0)), "#000000");
        assert_eq!(Palette::hex(rgb(255, 255, 255)), "#ffffff");
    }
}

