//! The Format Cells dialog's model (Excel's Ctrl+1): the tabs Number,
//! Alignment, Font, Border and Fill edit a draft of the active cell's
//! style; OK applies what changed to the selection as style patches. Pure
//! data (colours read with azul's one hex reader); no window, no engine.

use azul::css::ColorU;

use crate::engine::{BorderPreset, CellStyle, StylePatch};

/// `hex` (`#rgb`, `#rrggbb`, `#rrggbbaa`, the `#` optional, any case) as
/// the workbook's `#RRGGBB` (the alpha dropped: a cell colour is opaque);
/// `None` for text that is no colour.
#[must_use]
pub fn cell_colour(hex: &str) -> Option<String> {
    let c = ColorU::parse_hex(hex).into_option()?;
    Some(format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b))
}

/// The dialog's tabs, in order.
pub const TABS: [&str; 5] = ["Number", "Alignment", "Font", "Border", "Fill"];

/// The most decimals the Number tab offers.
pub const MAX_DECIMALS: usize = 10;

/// The Number tab's categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumberCategory {
    General,
    Number,
    Currency,
    Percentage,
    Date,
    Time,
    Text,
    /// A code none of the others writes; the dialog keeps it as it is.
    Custom,
}

impl NumberCategory {
    /// Every category, in the dialog's order.
    pub const ALL: [Self; 8] = [
        Self::General,
        Self::Number,
        Self::Currency,
        Self::Percentage,
        Self::Date,
        Self::Time,
        Self::Text,
        Self::Custom,
    ];

    /// The name the dialog shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Number => "Number",
            Self::Currency => "Currency",
            Self::Percentage => "Percentage",
            Self::Date => "Date",
            Self::Time => "Time",
            Self::Text => "Text",
            Self::Custom => "Custom",
        }
    }

    /// Whether the decimals (and, for Number, the thousands separator)
    /// apply.
    #[must_use]
    pub const fn has_decimals(self) -> bool {
        matches!(self, Self::Number | Self::Currency | Self::Percentage)
    }
}

/// The format code a category writes with `decimals` and the thousands
/// separator; `None` for Custom (the code stays as it is).
#[must_use]
pub fn number_code(category: NumberCategory, decimals: usize, thousands: bool) -> Option<String> {
    let decimals = decimals.min(MAX_DECIMALS);
    let fraction = if decimals > 0 {
        format!(".{}", "0".repeat(decimals))
    } else {
        String::new()
    };
    Some(match category {
        NumberCategory::General => String::from("general"),
        NumberCategory::Number => format!("{}{fraction}", if thousands { "#,##0" } else { "0" }),
        NumberCategory::Currency => format!("\"$\"#,##0{fraction}"),
        NumberCategory::Percentage => format!("0{fraction}%"),
        NumberCategory::Date => String::from("yyyy-mm-dd"),
        NumberCategory::Time => String::from("hh:mm:ss"),
        NumberCategory::Text => String::from("@"),
        NumberCategory::Custom => return None,
    })
}

/// The category, decimals and thousands separator a code reads as (the
/// dialog opens on them).
#[must_use]
pub fn read_code(code: &str) -> (NumberCategory, usize, bool) {
    let code = code.trim();
    let lower = code.to_ascii_lowercase();
    if lower.is_empty() || lower == "general" {
        return (NumberCategory::General, 0, false);
    }
    if code == "@" {
        return (NumberCategory::Text, 0, false);
    }
    // The decimals: the zeros after the point of the (one) numeric section.
    let decimals = |body: &str| body.split_once('.').map_or(0, |(_, f)| f.chars().filter(|c| *c == '0').count());
    let numeric = |body: &str| !body.is_empty() && body.chars().all(|c| matches!(c, '#' | '0' | ',' | '.'));
    if let Some(body) = code.strip_suffix('%') {
        if numeric(body) {
            return (NumberCategory::Percentage, decimals(body), false);
        }
    }
    if let Some(body) = code.strip_prefix("\"$\"") {
        if numeric(body) {
            return (NumberCategory::Currency, decimals(body), true);
        }
    }
    if numeric(code) {
        return (NumberCategory::Number, decimals(code), code.contains(','));
    }
    if !code.contains(';') && !code.contains('[') {
        let only = |allowed: &str| lower.chars().all(|c| allowed.contains(c));
        if only("ymd-/. ") && (lower.contains('y') || lower.contains('d')) {
            return (NumberCategory::Date, 0, false);
        }
        if only("hms: ") && lower.contains('h') {
            return (NumberCategory::Time, 0, false);
        }
    }
    (NumberCategory::Custom, 0, false)
}

/// The dialog's state: the tab in front, the style as it was and as it is
/// being edited, the Number tab's choices and a border preset to apply.
#[derive(Clone, Debug, PartialEq)]
pub struct FormatDraft {
    pub tab: usize,
    pub original: CellStyle,
    pub style: CellStyle,
    pub category: NumberCategory,
    pub decimals: usize,
    pub thousands: bool,
    /// The border preset OK applies; `None` leaves the borders alone.
    pub border: Option<BorderPreset>,
    /// `#RRGGBB` of the borders drawn.
    pub border_color: String,
}

impl FormatDraft {
    /// The dialog opened on `style` (the active cell's), on the Number tab.
    #[must_use]
    pub fn open(style: CellStyle) -> Self {
        let (category, decimals, thousands) = read_code(&style.num_fmt);
        Self {
            tab: 0,
            original: style.clone(),
            style,
            category,
            decimals,
            thousands,
            border: None,
            border_color: String::from("#000000"),
        }
    }

    /// Picks a category: the code follows. Coming from a category without
    /// decimals, a number starts at two (Excel's default).
    pub fn set_category(&mut self, category: NumberCategory) {
        if category.has_decimals() && !self.category.has_decimals() {
            self.decimals = 2;
        }
        self.category = category;
        self.sync_code();
    }

    /// One decimal more or fewer (0 ..= [`MAX_DECIMALS`]): the code follows.
    pub fn step_decimals(&mut self, more: bool) {
        self.decimals = if more {
            (self.decimals + 1).min(MAX_DECIMALS)
        } else {
            self.decimals.saturating_sub(1)
        };
        self.sync_code();
    }

    /// The thousands separator on or off: the code follows.
    pub fn set_thousands(&mut self, on: bool) {
        self.thousands = on;
        self.sync_code();
    }

    /// The style's code from the Number tab's choices (Custom keeps it).
    fn sync_code(&mut self) {
        if let Some(code) = number_code(self.category, self.decimals, self.thousands) {
            self.style.num_fmt = code;
        }
    }

    /// Any font colour (the Font tab's picker, beyond its presets): a
    /// `#rrggbb[aa]` in any case becomes `#RRGGBB`; `None` is automatic. Text
    /// that is no colour changes nothing.
    pub fn set_font_color(&mut self, hex: Option<&str>) {
        match hex {
            None => self.style.font_color = None,
            Some(hex) => {
                if let Some(c) = cell_colour(hex) {
                    self.style.font_color = Some(c);
                }
            }
        }
    }

    /// Any fill (the Fill tab's picker); `None` is no fill.
    pub fn set_fill(&mut self, hex: Option<&str>) {
        match hex {
            None => self.style.fill = None,
            Some(hex) => {
                if let Some(c) = cell_colour(hex) {
                    self.style.fill = Some(c);
                }
            }
        }
    }

    /// Any colour for the borders OK draws (the Border tab's picker).
    pub fn set_border_color(&mut self, hex: &str) {
        if let Some(c) = cell_colour(hex) {
            self.border_color = c;
        }
    }

    /// What OK applies: one patch per property that changed, the border
    /// preset if one was picked.
    #[must_use]
    pub fn patches(&self) -> Vec<StylePatch> {
        let (a, b) = (&self.original, &self.style);
        let mut out = Vec::new();
        if a.bold != b.bold {
            out.push(StylePatch::Bold(b.bold));
        }
        if a.italic != b.italic {
            out.push(StylePatch::Italic(b.italic));
        }
        if a.underline != b.underline {
            out.push(StylePatch::Underline(b.underline));
        }
        if a.strike != b.strike {
            out.push(StylePatch::Strike(b.strike));
        }
        if a.font_size != b.font_size {
            out.push(StylePatch::FontSize(b.font_size));
        }
        if a.font_color != b.font_color {
            out.push(StylePatch::FontColor(b.font_color.clone()));
        }
        if a.fill != b.fill {
            out.push(StylePatch::Fill(b.fill.clone()));
        }
        if a.h_align != b.h_align {
            out.push(StylePatch::HAlign(b.h_align));
        }
        if a.v_align != b.v_align {
            out.push(StylePatch::VAlign(b.v_align));
        }
        if a.wrap != b.wrap {
            out.push(StylePatch::Wrap(b.wrap));
        }
        if a.num_fmt != b.num_fmt {
            out.push(StylePatch::NumberFormat(b.num_fmt.clone()));
        }
        if let Some(preset) = self.border {
            out.push(StylePatch::Borders {
                preset,
                color: self.border_color.clone(),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{HAlign, VAlign};

    #[test]
    fn each_category_writes_its_code() {
        use NumberCategory as C;
        assert_eq!(number_code(C::General, 2, true).as_deref(), Some("general"));
        assert_eq!(number_code(C::Number, 2, true).as_deref(), Some("#,##0.00"));
        assert_eq!(number_code(C::Number, 0, false).as_deref(), Some("0"));
        assert_eq!(number_code(C::Number, 1, false).as_deref(), Some("0.0"));
        assert_eq!(number_code(C::Currency, 2, false).as_deref(), Some("\"$\"#,##0.00"), "currency always groups");
        assert_eq!(number_code(C::Percentage, 1, true).as_deref(), Some("0.0%"));
        assert_eq!(number_code(C::Date, 0, false).as_deref(), Some("yyyy-mm-dd"));
        assert_eq!(number_code(C::Time, 0, false).as_deref(), Some("hh:mm:ss"));
        assert_eq!(number_code(C::Text, 0, false).as_deref(), Some("@"));
        assert_eq!(number_code(C::Custom, 0, false), None);
    }

    #[test]
    fn a_code_reads_back_as_its_category() {
        use NumberCategory as C;
        assert_eq!(read_code("general"), (C::General, 0, false));
        assert_eq!(read_code("General"), (C::General, 0, false));
        assert_eq!(read_code("#,##0.00"), (C::Number, 2, true));
        assert_eq!(read_code("0"), (C::Number, 0, false));
        assert_eq!(read_code("\"$\"#,##0.00"), (C::Currency, 2, true));
        assert_eq!(read_code("0.0%"), (C::Percentage, 1, false));
        assert_eq!(read_code("yyyy-mm-dd"), (C::Date, 0, false));
        assert_eq!(read_code("hh:mm:ss"), (C::Time, 0, false));
        assert_eq!(read_code("@"), (C::Text, 0, false));
        assert_eq!(read_code("[Red]0;0").0, C::Custom);
    }

    #[test]
    fn the_number_choices_drive_the_code() {
        let mut d = FormatDraft::open(CellStyle::default());
        assert_eq!(d.category, NumberCategory::General);
        d.set_category(NumberCategory::Number);
        assert_eq!(d.style.num_fmt, "0.00", "Number starts at two decimals");
        d.set_thousands(true);
        assert_eq!(d.style.num_fmt, "#,##0.00");
        d.step_decimals(false);
        d.step_decimals(false);
        d.step_decimals(false);
        assert_eq!((d.decimals, d.style.num_fmt.as_str()), (0, "#,##0"), "never below 0");
        d.set_category(NumberCategory::Percentage);
        assert_eq!(d.style.num_fmt, "0%");
        for _ in 0..20 {
            d.step_decimals(true);
        }
        assert_eq!(d.decimals, MAX_DECIMALS);
    }

    #[test]
    fn any_colour_is_a_font_colour_a_fill_or_a_border_colour() {
        let mut d = FormatDraft::open(CellStyle::default());
        d.set_font_color(Some("#12ab34ff"));
        assert_eq!(d.style.font_color.as_deref(), Some("#12AB34"), "#RRGGBB, opaque");
        d.set_fill(Some("00ff00"));
        assert_eq!(d.style.fill.as_deref(), Some("#00FF00"));
        d.set_border_color("#336699");
        d.border = Some(BorderPreset::All);
        let patches = d.patches();
        assert!(patches.contains(&StylePatch::FontColor(Some(String::from("#12AB34")))), "{patches:?}");
        assert!(patches.contains(&StylePatch::Fill(Some(String::from("#00FF00")))));
        assert!(patches.contains(&StylePatch::Borders {
            preset: BorderPreset::All,
            color: String::from("#336699"),
        }));
        d.set_font_color(Some("not a colour"));
        assert_eq!(d.style.font_color.as_deref(), Some("#12AB34"), "no colour, no change");
        d.set_fill(None);
        assert_eq!(d.style.fill, None, "no fill");
        d.set_font_color(None);
        assert_eq!(d.style.font_color, None, "automatic");
    }

    #[test]
    fn ok_applies_only_what_changed() {
        let original = CellStyle {
            bold: true,
            num_fmt: String::from("0.00"),
            ..CellStyle::default()
        };
        let mut d = FormatDraft::open(original);
        assert!(d.patches().is_empty(), "nothing changed, nothing applied");
        d.style.bold = false;
        d.style.h_align = HAlign::Center;
        d.style.v_align = VAlign::Top;
        d.style.fill = Some(String::from("#FFF2CC"));
        d.style.font_size = 16;
        d.set_category(NumberCategory::Percentage);
        d.border = Some(BorderPreset::Outer);
        let patches = d.patches();
        assert_eq!(
            patches,
            vec![
                StylePatch::Bold(false),
                StylePatch::FontSize(16),
                StylePatch::Fill(Some(String::from("#FFF2CC"))),
                StylePatch::HAlign(HAlign::Center),
                StylePatch::VAlign(VAlign::Top),
                StylePatch::NumberFormat(String::from("0.00%")),
                StylePatch::Borders {
                    preset: BorderPreset::Outer,
                    color: String::from("#000000"),
                },
            ]
        );
    }
}
