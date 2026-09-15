use azul::{
    css::{ColorU, SystemStyle},
    window::WindowTheme,
};

type Opt = azul::option::OptionColorU;

type Rgba = (u8, u8, u8, u8);

const fn parts_of(c: ColorU) -> Rgba {
    (c.r, c.g, c.b, c.a)
}

const fn rgb(r: u8, g: u8, b: u8) -> ColorU {
    ColorU { r, g, b, a: 255 }
}

pub const OFFICE_BLUE: ColorU = rgb(43, 87, 154);
pub const WHITE: ColorU = rgb(255, 255, 255);

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub brand: ColorU,
    pub on_brand: ColorU,
    pub brand_text: ColorU,
    pub chrome: ColorU,
    pub text: ColorU,
    pub text_gray: ColorU,
    pub text_faint: ColorU,
    pub title_gray: ColorU,
    pub canvas: ColorU,
    pub sheet: ColorU,
    pub sheet_border: ColorU,
    pub sheet_text: ColorU,
    pub sheet_heading: ColorU,
    pub sheet_heading_deep: ColorU,
    pub sheet_quiet: ColorU,
    pub sheet_rule: ColorU,
    pub sheet_code_bg: ColorU,
    pub control_border: ColorU,
    pub control_bg: ColorU,
    pub hover_bg: ColorU,
    pub selected_bg: ColorU,
    pub chrome_edge: ColorU,
}

impl Palette {
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

pub fn sample_ink(ink: ColorU, pal: &Palette) -> ColorU {
    ink.ensure_contrast(pal.chrome, 4.5)
}

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

const OFFICE_2013_DARK: Palette = Palette {
    dark: true,
    brand: OFFICE_BLUE,
    on_brand: WHITE,
    brand_text: rgb(106, 156, 219),
    chrome: rgb(49, 54, 59),
    text: rgb(252, 252, 252),
    text_gray: rgb(189, 195, 199),
    text_faint: rgb(150, 156, 161),
    title_gray: rgb(220, 224, 227),
    canvas: rgb(27, 30, 32),
    sheet: rgb(226, 226, 226),
    sheet_border: rgb(20, 22, 24),
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

const fn lum(c: ColorU) -> u32 {
    c.r as u32 + c.g as u32 + c.b as u32
}

impl Palette {
    #[must_use]
    pub const fn fallback(theme: WindowTheme) -> Self {
        match theme {
            WindowTheme::DarkMode => OFFICE_2013_DARK,
            _ => OFFICE_2013,
        }
    }

    #[must_use]
    pub fn from_system(style: &SystemStyle, theme: WindowTheme) -> Self {
        let d = Self::fallback(theme);
        let c = &style.colors;

        let text = opt(c.text);
        let secondary = opt(c.secondary_text);
        let tertiary = opt(c.tertiary_text);
        let window_bg = opt(c.window_background);
        let view_bg = opt(c.background);
        let separator = opt(c.separator);
        let selection = opt(c.selection_background);
        let selection_inactive = opt(c.selection_background_inactive);

        let sheet = d.sheet;

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
            sheet_text: d.sheet_text,
            sheet_heading: d.sheet_heading,
            sheet_heading_deep: d.sheet_heading_deep,
            sheet_quiet: d.sheet_quiet,
            sheet_rule: d.sheet_rule,
            sheet_code_bg: d.sheet_code_bg,
            control_border: separator.unwrap_or(d.control_border),
            control_bg: window_bg.unwrap_or(d.control_bg),
            hover_bg: selection_inactive.unwrap_or(d.hover_bg),
            selected_bg: selection.unwrap_or(d.selected_bg),
            chrome_edge: d.chrome_edge,
        }
    }

    #[must_use]
    pub fn hex(c: ColorU) -> String {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }

    #[must_use]
    pub fn rgba(c: ColorU) -> String {
        format!(
            "rgba({}, {}, {}, {:.3})",
            c.r,
            c.g,
            c.b,
            f32::from(c.a) / 255.0
        )
    }

    pub const TRANSPARENT: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    #[must_use]
    pub const fn content_film(&self) -> ColorU {
        if self.dark {
            ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 18,
            }
        } else {
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 12,
            }
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        OFFICE_2013
    }
}

pub mod widgets {
    use azul::{
        css::{ColorU, SystemStyle},
        widgets::{
            BackstageStyle, BackstageTheme, RibbonStyle, RibbonTheme, StatusBarStyle,
            StatusBarTheme,
        },
    };

    use super::Palette;

    #[must_use]
    pub fn ribbon(pal: &Palette, sys: &SystemStyle) -> RibbonStyle {
        let mut t = RibbonTheme::from_system(SystemStyle::clone(sys));
        t.accent = pal.brand;
        t.accent_hover = pal.brand;
        t.accent_text = pal.on_brand;
        t.hover_border = pal.brand;
        t.border = pal.chrome_edge;
        t.chrome_bg = Palette::TRANSPARENT;
        t.content_bg = pal.content_film();
        RibbonStyle::from_theme(t)
    }

    #[must_use]
    pub fn header_bg(sys: &SystemStyle) -> ColorU {
        azul::widgets::QuickAccessTheme::from_system(SystemStyle::clone(sys)).bg
    }

    #[must_use]
    pub fn status_bar(pal: &Palette, sys: &SystemStyle) -> StatusBarStyle {
        let mut t = StatusBarTheme::from_system(SystemStyle::clone(sys));
        t.bar_bg = pal.brand;
        t.text = pal.on_brand;
        t.thumb = pal.on_brand;
        StatusBarStyle::from_theme(t)
    }

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

    #[test]
    fn the_paper_never_takes_a_system_surface() {
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

    #[test]
    fn every_gallery_ink_is_legible_on_its_own_chrome() {
        const INKS: &[ColorU] = &[
            rgb(68, 68, 68),
            rgb(46, 116, 181),
            rgb(38, 38, 38),
            rgb(90, 90, 90),
            rgb(128, 128, 128),
            rgb(68, 114, 196),
        ];

        for pal in [&OFFICE_2013, &OFFICE_2013_DARK] {
            for ink in INKS {
                let shown = sample_ink(*ink, pal);
                let ratio = shown.contrast_ratio(pal.chrome);
                assert!(
                    ratio >= 4.5,
                    "ink {} on chrome {} reads at {ratio:.2}:1 (need 4.5:1)",
                    Palette::hex(shown),
                    Palette::hex(pal.chrome),
                );
            }
        }
    }

    #[test]
    fn lifting_an_ink_keeps_its_hue() {
        let same = |a: ColorU, b: ColorU| a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a;

        let heading = rgb(46, 116, 181);
        let lifted = sample_ink(heading, &OFFICE_2013_DARK);
        assert!(
            !same(lifted, heading),
            "on charcoal this ink has to move at all"
        );
        assert!(
            lifted.b > lifted.r && lifted.b > lifted.g,
            "still a blue: {}",
            Palette::hex(lifted)
        );
        assert!(
            same(sample_ink(heading, &OFFICE_2013), heading),
            "a light chrome must leave the document ink exactly as it is"
        );
    }
}
