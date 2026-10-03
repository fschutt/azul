//! The Format Cells dialog's model (Excel's Ctrl+1): the tabs Number,
//! Alignment, Font, Border and Fill edit a draft of the active cell's
//! style; OK applies what changed to the selection as style patches. Pure
//! data; no azul, no engine.

use crate::engine::{BorderPreset, CellStyle, StylePatch};

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
    let _ = (category, decimals, thousands);
    None
}

/// The category, decimals and thousands separator a code reads as (the
/// dialog opens on them).
#[must_use]
pub fn read_code(code: &str) -> (NumberCategory, usize, bool) {
    let _ = code;
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

    /// Picks a category: the code follows.
    pub fn set_category(&mut self, category: NumberCategory) {
        let _ = category;
    }

    /// One decimal more or fewer (0 ..= [`MAX_DECIMALS`]): the code follows.
    pub fn step_decimals(&mut self, more: bool) {
        let _ = more;
    }

    /// The thousands separator on or off: the code follows.
    pub fn set_thousands(&mut self, on: bool) {
        let _ = on;
    }

    /// What OK applies: one patch per property that changed, the border
    /// preset if one was picked.
    #[must_use]
    pub fn patches(&self) -> Vec<StylePatch> {
        Vec::new()
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
