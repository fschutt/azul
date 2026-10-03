//! The reader's settings: the type (font, size, line spacing, alignment), the margins, the
//! paper and how many pages show side by side. Kept as values of azul-appkit's
//! `reader/settings.json` (beside the app theme and the mode).
//!
//! The PAPER is the reading surface's colours - never called a theme (the app theme is flat /
//! flora, the mode light / dark): `Auto` follows the mode, `White`, `Sepia` and `Night` are
//! fixed.

use azul_appkit::AppSettings;

/// The reading surface's colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paper {
    /// The mode's: white paper in the light mode, night paper in the dark one.
    Auto,
    White,
    Sepia,
    Night,
}

/// A paper's colours (CSS colour values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaperColors {
    pub background: &'static str,
    pub text: &'static str,
    /// Secondary text on the paper (the running head, the page number).
    pub muted: &'static str,
}

impl Paper {
    pub const ALL: [Paper; 4] = [Paper::Auto, Paper::White, Paper::Sepia, Paper::Night];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Paper::Auto => "auto",
            Paper::White => "white",
            Paper::Sepia => "sepia",
            Paper::Night => "night",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Paper::Auto => "Auto",
            Paper::White => "White",
            Paper::Sepia => "Sepia",
            Paper::Night => "Night",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Paper> {
        Self::ALL.iter().copied().find(|p| p.key() == key.trim())
    }

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    /// The colours of this paper when the window is in the dark mode or not.
    #[must_use]
    pub const fn colors(self, dark_mode: bool) -> PaperColors {
        match (self, dark_mode) {
            (Paper::White, _) | (Paper::Auto, false) => PaperColors {
                background: "#ffffff",
                text: "#1b1b1b",
                muted: "#6b6b6b",
            },
            (Paper::Sepia, _) => PaperColors {
                background: "#f4ecd8",
                text: "#5b4636",
                muted: "#8c7660",
            },
            (Paper::Night, _) | (Paper::Auto, true) => PaperColors {
                background: "#161616",
                text: "#d4d4d4",
                muted: "#8a8a8a",
            },
        }
    }
}

/// The reader's typeface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontChoice {
    Serif,
    Sans,
}

impl FontChoice {
    pub const ALL: [FontChoice; 2] = [FontChoice::Serif, FontChoice::Sans];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            FontChoice::Serif => "serif",
            FontChoice::Sans => "sans",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            FontChoice::Serif => "Serif",
            FontChoice::Sans => "Sans serif",
        }
    }

    /// The CSS family.
    #[must_use]
    pub const fn css(self) -> &'static str {
        match self {
            FontChoice::Serif => "serif",
            FontChoice::Sans => "sans-serif",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<FontChoice> {
        Self::ALL.iter().copied().find(|f| f.key() == key.trim())
    }

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// How many pages show side by side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLayout {
    /// Two when the window is wide enough for two pages of a comfortable width, else one.
    Auto,
    Single,
    Spread,
}

impl PageLayout {
    pub const ALL: [PageLayout; 3] = [PageLayout::Auto, PageLayout::Single, PageLayout::Spread];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            PageLayout::Auto => "auto",
            PageLayout::Single => "single",
            PageLayout::Spread => "spread",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            PageLayout::Auto => "Auto",
            PageLayout::Single => "One page",
            PageLayout::Spread => "Two pages",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<PageLayout> {
        Self::ALL.iter().copied().find(|l| l.key() == key.trim())
    }

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }
}

/// The font size range (px).
pub const FONT_MIN: u32 = 12;
pub const FONT_MAX: u32 = 32;
/// The line spacing range, in tenths (12 = 1.2).
pub const LINE_MIN: u32 = 12;
pub const LINE_MAX: u32 = 20;
/// The page margin range (px).
pub const MARGIN_MIN: u32 = 8;
pub const MARGIN_MAX: u32 = 96;
/// A page is never wider than this many px of text (a line stays readable).
pub const MAX_TEXT_WIDTH: f32 = 680.0;
/// Two pages show side by side (Auto) when each can have at least this much text width.
pub const SPREAD_MIN_TEXT_WIDTH: f32 = 380.0;

/// Everything the reader is set to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadingSettings {
    pub font_px: u32,
    /// Line spacing in tenths.
    pub line_tenths: u32,
    pub margin_px: u32,
    pub font: FontChoice,
    pub paper: Paper,
    pub layout: PageLayout,
    pub justify: bool,
}

impl Default for ReadingSettings {
    fn default() -> Self {
        Self {
            font_px: 18,
            line_tenths: 15,
            margin_px: 40,
            font: FontChoice::Serif,
            paper: Paper::Auto,
            layout: PageLayout::Auto,
            justify: true,
        }
    }
}

/// The keys in the settings file.
pub mod keys {
    pub const FONT_PX: &str = "font_px";
    pub const LINE: &str = "line_tenths";
    pub const MARGIN: &str = "margin_px";
    pub const FONT: &str = "font";
    pub const PAPER: &str = "paper";
    pub const LAYOUT: &str = "layout";
    pub const JUSTIFY: &str = "justify";
}

impl ReadingSettings {
    /// The settings in the settings file (a missing or unreadable value is the default's, a
    /// number outside its range the nearest end).
    #[must_use]
    pub fn from_settings(settings: &AppSettings) -> Self {
        let d = Self::default();
        let number = |key: &str, default: u32, min: u32, max: u32| {
            settings
                .get(key)
                .and_then(|v| v.trim().parse::<u32>().ok())
                .map_or(default, |n| n.clamp(min, max))
        };
        Self {
            font_px: number(keys::FONT_PX, d.font_px, FONT_MIN, FONT_MAX),
            line_tenths: number(keys::LINE, d.line_tenths, LINE_MIN, LINE_MAX),
            margin_px: number(keys::MARGIN, d.margin_px, MARGIN_MIN, MARGIN_MAX),
            font: settings
                .get(keys::FONT)
                .and_then(FontChoice::parse)
                .unwrap_or(d.font),
            paper: settings
                .get(keys::PAPER)
                .and_then(Paper::parse)
                .unwrap_or(d.paper),
            layout: settings
                .get(keys::LAYOUT)
                .and_then(PageLayout::parse)
                .unwrap_or(d.layout),
            justify: settings.get_bool(keys::JUSTIFY, d.justify),
        }
    }

    /// Writes these settings into the settings file's values.
    pub fn write_to(&self, settings: &mut AppSettings) {
        settings.set(keys::FONT_PX, &self.font_px.to_string());
        settings.set(keys::LINE, &self.line_tenths.to_string());
        settings.set(keys::MARGIN, &self.margin_px.to_string());
        settings.set(keys::FONT, self.font.key());
        settings.set(keys::PAPER, self.paper.key());
        settings.set(keys::LAYOUT, self.layout.key());
        settings.set_bool(keys::JUSTIFY, self.justify);
    }

    /// The line height as CSS (`1.5`).
    #[must_use]
    pub fn line_height(&self) -> String {
        format!("{}.{}", self.line_tenths / 10, self.line_tenths % 10)
    }

    /// The reading column's style at a text width of `width` px: everything that decides
    /// where the lines break, stated on the column itself, so the pagination (where the
    /// column is the root) and the screen (where it sits in the app) lay it out alike.
    #[must_use]
    pub fn column_css(&self, width: f32) -> String {
        format!(
            "display: block; box-sizing: border-box; width: {}px; margin: 0px; padding: 0px; \
             font-family: {}; font-size: {}px; line-height: {}; text-align: {}; \
             letter-spacing: normal; word-spacing: normal; white-space: normal; \
             font-weight: normal; font-style: normal;",
            width.floor().max(1.0),
            self.font.css(),
            self.font_px,
            self.line_height(),
            if self.justify { "justify" } else { "left" },
        )
    }

    /// What decides the pages of a chapter (a change of any of these lays it out again).
    #[must_use]
    pub fn layout_key(&self, width: f32, height: f32) -> String {
        format!(
            "{}x{}/{}/{}/{}/{}",
            width.floor(),
            height.floor(),
            self.font_px,
            self.line_tenths,
            self.font.key(),
            self.justify
        )
    }
}

/// The pages' geometry in a reading area of `area_w` x `area_h` px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeometry {
    /// Pages side by side (1 or 2).
    pub per_view: usize,
    /// One page's text area.
    pub text_width: f32,
    pub text_height: f32,
    /// The margin around the text on the page.
    pub margin: f32,
}

/// The geometry of the pages for `settings` in a reading area of `area_w` x `area_h` px
/// (what is left of the window for the pages).
#[must_use]
pub fn page_geometry(settings: &ReadingSettings, area_w: f32, area_h: f32) -> PageGeometry {
    let margin = settings.margin_px as f32;
    let gap = 24.0;
    let usable_w = (area_w - 2.0 * margin).max(120.0);
    let per_view = match settings.layout {
        PageLayout::Single => 1,
        PageLayout::Spread => 2,
        PageLayout::Auto => {
            if (area_w - gap) / 2.0 - 2.0 * margin >= SPREAD_MIN_TEXT_WIDTH {
                2
            } else {
                1
            }
        }
    };
    let text_width = if per_view == 2 {
        ((area_w - gap) / 2.0 - 2.0 * margin).max(120.0)
    } else {
        usable_w
    }
    .min(MAX_TEXT_WIDTH)
    .floor();
    let text_height = (area_h - 2.0 * margin).max(120.0).floor();
    PageGeometry {
        per_view,
        text_width,
        text_height,
        margin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_come_back_from_the_settings_file_and_out_of_range_numbers_are_clamped() {
        let mut file = AppSettings::default();
        let chosen = ReadingSettings {
            font_px: 22,
            line_tenths: 17,
            margin_px: 60,
            font: FontChoice::Sans,
            paper: Paper::Sepia,
            layout: PageLayout::Spread,
            justify: false,
        };
        chosen.write_to(&mut file);
        assert_eq!(ReadingSettings::from_settings(&file), chosen);
        file.set(keys::FONT_PX, "300");
        file.set(keys::MARGIN, "big");
        file.set(keys::PAPER, "purple");
        let back = ReadingSettings::from_settings(&file);
        assert_eq!(back.font_px, FONT_MAX);
        assert_eq!(back.margin_px, ReadingSettings::default().margin_px);
        assert_eq!(back.paper, Paper::Auto);
        assert_eq!(
            ReadingSettings::from_settings(&AppSettings::default()),
            ReadingSettings::default()
        );
    }

    #[test]
    fn the_auto_paper_follows_the_mode_and_the_others_do_not() {
        assert_eq!(Paper::Auto.colors(false), Paper::White.colors(true));
        assert_eq!(Paper::Auto.colors(true), Paper::Night.colors(false));
        assert_eq!(Paper::Sepia.colors(true), Paper::Sepia.colors(false));
        for p in Paper::ALL {
            assert_eq!(Paper::parse(p.key()), Some(p));
        }
    }

    #[test]
    fn the_column_states_its_type_and_the_layout_key_changes_with_it() {
        let s = ReadingSettings::default();
        let css = s.column_css(500.7);
        assert!(css.contains("width: 500px;"), "{css}");
        assert!(css.contains("font-size: 18px;"));
        assert!(css.contains("line-height: 1.5;"));
        assert!(css.contains("text-align: justify;"));
        assert!(css.contains("font-family: serif;"));
        let bigger = ReadingSettings { font_px: 20, ..s };
        assert_ne!(s.layout_key(500.0, 700.0), bigger.layout_key(500.0, 700.0));
        let sepia = ReadingSettings {
            paper: Paper::Sepia,
            ..s
        };
        assert_eq!(
            s.layout_key(500.0, 700.0),
            sepia.layout_key(500.0, 700.0),
            "the paper moves no line"
        );
    }

    #[test]
    fn a_wide_window_shows_two_pages_and_a_page_is_never_too_wide() {
        let s = ReadingSettings::default(); // margins 40
        let wide = page_geometry(&s, 1400.0, 900.0);
        assert_eq!(wide.per_view, 2);
        assert_eq!(wide.text_width, ((1400.0_f32 - 24.0) / 2.0 - 80.0).floor());
        assert_eq!(wide.text_height, 820.0);
        let narrow = page_geometry(&s, 800.0, 900.0);
        assert_eq!(narrow.per_view, 1);
        assert_eq!(
            narrow.text_width, MAX_TEXT_WIDTH,
            "720 px of room, a page of 680 at most"
        );
        let single = page_geometry(
            &ReadingSettings {
                layout: PageLayout::Single,
                ..s
            },
            1400.0,
            900.0,
        );
        assert_eq!(single.per_view, 1);
        let tiny = page_geometry(&s, 100.0, 100.0);
        assert_eq!((tiny.text_width, tiny.text_height), (120.0, 120.0));
    }
}
