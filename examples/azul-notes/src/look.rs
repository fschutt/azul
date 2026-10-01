//! The colours of what AzNotes draws itself (the editor's paper and text,
//! the sheets over the window), per mode. The shell, the list and every
//! widget follow the app theme (flat / flora) and the mode themselves.

/// One mode's colours, as CSS colour strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub dark: bool,
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
    /// Inline code and code blocks.
    pub code_bg: &'static str,
    /// A sheet's backdrop (the dialogs over the window).
    pub backdrop: &'static str,
    /// A sheet.
    pub sheet: &'static str,
    pub error: &'static str,
    /// The title row's background, as RGB (the Titlebar takes a ColorU).
    pub chrome_rgb: (u8, u8, u8),
    pub text_rgb: (u8, u8, u8),
}

pub const LIGHT: Look = Look {
    dark: false,
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
    chrome_rgb: (0xf6, 0xf7, 0xf9),
    text_rgb: (0x1d, 0x23, 0x30),
};

pub const DARK: Look = Look {
    dark: true,
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
    chrome_rgb: (0x2b, 0x2d, 0x31),
    text_rgb: (0xe6, 0xe7, 0xea),
};

/// The look of a mode.
#[must_use]
pub fn of(dark: bool) -> &'static Look {
    if dark {
        &DARK
    } else {
        &LIGHT
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
