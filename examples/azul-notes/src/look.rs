//! The colours of what AzNotes draws itself (the editor's paper and text,
//! the sheets over the window), per app theme and mode. The shell, the list
//! and every widget follow the app theme (flat / flora) and the mode
//! themselves.
//!
//! FLAT keeps AzNotes' own palette: a white page, a blue accent, its own
//! title row colour. FLORA is flora's (`doc/templates/flora.css`), named by
//! the `system:` colour keywords, which the engine resolves to flora's
//! tokens under flora - the field paper, the ink, the rules, the stone of
//! the theme's spin, the brass link ink - so the editor's paper and the
//! sheets sit in the same room as the flora widgets around them and a spin
//! recolours the accent. The few colours without a keyword (the sheets'
//! dusk, the danger ink) are flora.css's by mode. Flora's text is Garamond:
//! the editor's text inherits it from the theme scope, at flora's reading
//! size ([`TextSize::px_for`]).

/// One look's colours, as CSS colour strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub dark: bool,
    /// Flora: the title row and the type follow the theme; flat paints its
    /// own title row colour.
    pub flora: bool,
    /// The editor's paper.
    pub paper: &'static str,
    pub text: &'static str,
    /// Secondary text: dates, a checked item, a quote.
    pub muted: &'static str,
    /// Hairlines: under the title row, the quote bar, a rule.
    pub line: &'static str,
    pub accent: &'static str,
    /// A link in the text.
    pub link: &'static str,
    /// Inline code and code blocks; the Trash notice's band.
    pub code_bg: &'static str,
    /// A sheet's backdrop (the dialogs over the window).
    pub backdrop: &'static str,
    /// A sheet.
    pub sheet: &'static str,
    pub error: &'static str,
    /// A sheet's corner radius (flora: nothing rounder than 5px).
    pub sheet_radius: &'static str,
    /// The title row's background, as RGB (the Titlebar takes a ColorU);
    /// flat only.
    pub chrome_rgb: (u8, u8, u8),
    pub text_rgb: (u8, u8, u8),
}

pub const LIGHT: Look = Look {
    dark: false,
    flora: false,
    paper: "#ffffff",
    text: "#1d2330",
    muted: "#6b7280",
    line: "#dfe2e8",
    accent: "#2f6fde",
    link: "#1f5fc4",
    code_bg: "#f2f4f7",
    backdrop: "rgba(15, 20, 30, 0.35)",
    sheet: "#ffffff",
    error: "#b3261e",
    sheet_radius: "8px",
    chrome_rgb: (0xf6, 0xf7, 0xf9),
    text_rgb: (0x1d, 0x23, 0x30),
};

pub const DARK: Look = Look {
    dark: true,
    flora: false,
    paper: "#1e1f22",
    text: "#e6e7ea",
    muted: "#9aa0aa",
    line: "#3a3c42",
    accent: "#6ea0ff",
    link: "#8ab4ff",
    code_bg: "#2b2d31",
    backdrop: "rgba(0, 0, 0, 0.55)",
    sheet: "#26282c",
    error: "#ff8a80",
    sheet_radius: "8px",
    chrome_rgb: (0x2b, 0x2d, 0x31),
    text_rgb: (0xe6, 0xe7, 0xea),
};

/// Flora by day: the field paper (`--fl-fld`) under the ink, the leaf
/// (`--fl-sur`) for a sheet, the house rule, the stone, brass links, the
/// desk (`--fl-desk`) for the Trash band; the dusk the command palette
/// throws (`rgba(40, 34, 24, .39)`), the clay stone for danger.
pub const FLORA_LIGHT: Look = Look {
    dark: false,
    flora: true,
    paper: "system:background",
    text: "system:text",
    muted: "system:secondary-text",
    line: "system:separator",
    accent: "system:accent",
    link: "system:link",
    code_bg: "system:under-page-background",
    backdrop: "rgba(40, 34, 24, 0.39)",
    sheet: "system:window-background",
    error: "#7E4A42",
    sheet_radius: "5px",
    chrome_rgb: (0xe9, 0xe7, 0xe2),
    text_rgb: (0x26, 0x25, 0x21),
};

/// Flora at night: the same keywords (the engine gives their night values),
/// night's backdrop, the clay stone's glow for danger.
pub const FLORA_DARK: Look = Look {
    dark: true,
    flora: true,
    backdrop: "rgba(0, 0, 0, 0.63)",
    error: "#B3837A",
    chrome_rgb: (0x1f, 0x1f, 0x1f),
    text_rgb: (0xe7, 0xe7, 0xe7),
    ..FLORA_LIGHT
};

/// Whether the app theme `name` is flora or one of its spins (`flora`,
/// `flora:green`, ...).
#[must_use]
pub fn is_flora(name: &str) -> bool {
    name == "flora" || name.starts_with("flora:")
}

/// The look of an app theme (flora or not) in a mode.
#[must_use]
pub fn of(flora: bool, dark: bool) -> &'static Look {
    match (flora, dark) {
        (true, true) => &FLORA_DARK,
        (true, false) => &FLORA_LIGHT,
        (false, true) => &DARK,
        (false, false) => &LIGHT,
    }
}

/// The family a note's text is written in on paper that has no theme scope
/// around it (the PDF export): flora's Garamond (the bundled face), else the
/// sans flat writes in.
#[must_use]
pub fn paper_font(flora: bool) -> &'static str {
    if flora {
        "'EB Garamond', Georgia, serif"
    } else {
        "sans-serif"
    }
}

/// The editor's text size, as the settings name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl TextSize {
    pub const ALL: [TextSize; 3] = [TextSize::Small, TextSize::Medium, TextSize::Large];

    #[must_use]
    pub fn px(self) -> f32 {
        match self {
            TextSize::Small => 14.0,
            TextSize::Medium => 16.0,
            TextSize::Large => 19.0,
        }
    }

    /// The size the editor sets this setting at under the app theme:
    /// flora's Garamond sets small - flora.css: "its x-height is well below
    /// a grotesque at the same nominal size", so its reading size (20px)
    /// "sits where 17px would on a sans" - and is scaled by 20/17 to read
    /// at the same size as flat's sans, to the whole pixel.
    #[must_use]
    pub fn px_for(self, flora: bool) -> f32 {
        if flora {
            (self.px() * 20.0 / 17.0).round()
        } else {
            self.px()
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            TextSize::Small => "Small",
            TextSize::Medium => "Medium",
            TextSize::Large => "Large",
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            TextSize::Small => "small",
            TextSize::Medium => "medium",
            TextSize::Large => "large",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<TextSize> {
        TextSize::ALL.into_iter().find(|s| s.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flora_and_its_spins_take_the_flora_look_and_flat_keeps_its_own() {
        for name in ["flora", "flora:green", "flora:rose"] {
            assert!(is_flora(name), "{name}");
            assert!(of(is_flora(name), false).flora);
        }
        for name in ["flat", "native", "floral", ""] {
            assert!(!is_flora(name), "{name}");
        }
        assert_eq!(*of(false, false), LIGHT, "flat by day is unchanged");
        assert_eq!(*of(false, true), DARK, "flat at night is unchanged");
        assert!(of(true, true).dark && !of(true, false).dark);
    }

    #[test]
    fn the_flora_look_names_the_theme_s_colours_never_a_white_field() {
        for look in [FLORA_LIGHT, FLORA_DARK] {
            for colour in [look.paper, look.text, look.muted, look.line, look.accent, look.link, look.sheet] {
                assert!(colour.starts_with("system:"), "{colour}: the theme's colour, by keyword");
            }
            assert_ne!(look.paper, "#ffffff");
            assert_eq!(look.sheet_radius, "5px", "nothing rounder than 5px in flora");
        }
    }

    #[test]
    fn flora_reads_garamond_at_its_reading_size() {
        assert_eq!(TextSize::Medium.px_for(false), 16.0, "flat keeps its sizes");
        assert_eq!(TextSize::Small.px_for(true), 16.0);
        assert_eq!(TextSize::Medium.px_for(true), 19.0);
        assert_eq!(TextSize::Large.px_for(true), 22.0);
        assert!(paper_font(true).starts_with("'EB Garamond'"));
        assert_eq!(paper_font(false), "sans-serif");
    }
}
