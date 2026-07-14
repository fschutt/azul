//! CSS Paged Media page decoration: headers, footers, margin boxes, and counters.
//!
//! This module is the canonical home for paged-media page *decoration*. It provides:
//!
//! - `FakePageConfig` / `HeaderFooterConfig` — programmatic header/footer setup
//!   (a temporary interface until full CSS `@page` rule parsing exists)
//! - `MarginBoxContent` / `CounterFormat` — the CSS GCPM margin-box content model and
//!   page-counter number formatting (formatting delegates to `super::counters`)
//! - `PageInfo` — per-page metadata passed to content generators
//! - `TableHeaderInfo` / `TableHeaderTracker` — repeated table headers across pages
//!
//! The actual page *splitting* is performed by the display-list slicer
//! (`paginate_display_list_with_slicer_and_breaks` in `super::display_list`), which
//! consumes `HeaderFooterConfig` via its `SlicerConfig`. The continuous-vs-paged media
//! decision and page geometry are carried by `crate::paged::FragmentationContext`, and
//! CSS break properties are read via `super::getters` (`get_break_before`,
//! `get_break_after`, `is_forced_page_break`).
//!
//! **Note:** Running elements, named strings, and per-page `@page` selectors are not
//! yet implemented; only page counters and header/footer configuration are functional.
//!
//! See: <https://www.w3.org/TR/css-gcpm-3>/

use std::sync::Arc;

use azul_css::props::basic::ColorU;

/// Content that can appear in a page margin box.
///
/// This enum represents the various types of content that CSS GCPM
/// allows in margin boxes.
#[derive(Clone)]
pub enum MarginBoxContent {
    /// Empty margin box
    None,
    /// A running element referenced by name: `content: element(header)`
    RunningElement(String),
    /// A named string: `content: string(chapter)`
    NamedString(String),
    /// Page counter: `content: counter(page)`
    PageCounter,
    /// Total pages counter: `content: counter(pages)`
    PagesCounter,
    /// Page counter with format: `content: counter(page, lower-roman)`
    PageCounterFormatted { format: CounterFormat },
    /// Combined content (e.g., "Page " counter(page) " of " counter(pages))
    Combined(Vec<MarginBoxContent>),
    /// Literal text
    Text(String),
    /// Custom callback for dynamic content generation
    Custom(Arc<dyn Fn(PageInfo) -> String + Send + Sync>),
}

impl std::fmt::Debug for MarginBoxContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::RunningElement(s) => f.debug_tuple("RunningElement").field(s).finish(),
            Self::NamedString(s) => f.debug_tuple("NamedString").field(s).finish(),
            Self::PageCounter => write!(f, "PageCounter"),
            Self::PagesCounter => write!(f, "PagesCounter"),
            Self::PageCounterFormatted { format } => f
                .debug_struct("PageCounterFormatted")
                .field("format", format)
                .finish(),
            Self::Combined(v) => f.debug_tuple("Combined").field(v).finish(),
            Self::Text(s) => f.debug_tuple("Text").field(s).finish(),
            Self::Custom(_) => write!(f, "Custom(<fn>)"),
        }
    }
}

/// Counter formatting styles (subset of CSS list-style-type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterFormat {
    Decimal,
    DecimalLeadingZero,
    LowerRoman,
    UpperRoman,
    LowerAlpha,
    UpperAlpha,
    LowerGreek,
}

impl Default for CounterFormat {
    fn default() -> Self {
        Self::Decimal
    }
}

impl CounterFormat {
    /// Format a number according to this counter style.
    #[must_use] pub fn format(&self, n: usize) -> String {
        use super::counters::{to_alphabetic, to_greek, to_roman};
        match self {
            Self::Decimal => n.to_string(),
            Self::DecimalLeadingZero => format!("{n:02}"),
            Self::LowerRoman => to_roman(n, false),
            Self::UpperRoman => to_roman(n, true),
            Self::LowerAlpha => to_alphabetic(n, false),
            Self::UpperAlpha => to_alphabetic(n, true),
            Self::LowerGreek => to_greek(n, false),
        }
    }
}

/// Information about the current page, passed to content generators.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)] // independent page-position flags (first/last/left/right)
pub struct PageInfo {
    /// Current page number (1-indexed for display)
    pub page_number: usize,
    /// Total number of pages (may be 0 if unknown during first pass)
    pub total_pages: usize,
    /// Whether this is the first page
    pub is_first: bool,
    /// Whether this is the last page
    pub is_last: bool,
    /// Whether this is a left (verso) page (for duplex printing)
    pub is_left: bool,
    /// Whether this is a right (recto) page
    pub is_right: bool,
    /// Whether this is a blank page (inserted for left/right alignment)
    pub is_blank: bool,
}

impl PageInfo {
    /// Create `PageInfo` for a specific page.
    #[must_use] pub const fn new(page_number: usize, total_pages: usize) -> Self {
        Self {
            page_number,
            total_pages,
            is_first: page_number == 1,
            is_last: total_pages > 0 && page_number == total_pages,
            is_left: page_number.is_multiple_of(2), // Even pages are left (verso)
            is_right: page_number % 2 == 1, // Odd pages are right (recto)
            is_blank: false,
        }
    }
}

/// Default height for page headers and footers (in points).
const DEFAULT_HEADER_FOOTER_HEIGHT: f32 = 30.0;

/// Default font size for header/footer text (in points).
const DEFAULT_HEADER_FOOTER_FONT_SIZE: f32 = 10.0;

/// Configuration for page headers and footers.
///
/// This is a simplified interface for the common case of adding
/// headers and footers, consumed by the display-list slicer via its `SlicerConfig`.
#[derive(Debug, Clone)]
pub struct HeaderFooterConfig {
    /// Whether to show a header on each page
    pub show_header: bool,
    /// Whether to show a footer on each page
    pub show_footer: bool,
    /// Height of the header area (if shown)
    pub header_height: f32,
    /// Height of the footer area (if shown)  
    pub footer_height: f32,
    /// Content generator for the header
    pub header_content: MarginBoxContent,
    /// Content generator for the footer
    pub footer_content: MarginBoxContent,
    /// Font size for header/footer text
    pub font_size: f32,
    /// Text color for header/footer
    pub text_color: ColorU,
    /// Whether to skip header/footer on first page
    pub skip_first_page: bool,
}

impl Default for HeaderFooterConfig {
    fn default() -> Self {
        Self {
            show_header: false,
            show_footer: false,
            header_height: DEFAULT_HEADER_FOOTER_HEIGHT,
            footer_height: DEFAULT_HEADER_FOOTER_HEIGHT,
            header_content: MarginBoxContent::None,
            footer_content: MarginBoxContent::None,
            font_size: DEFAULT_HEADER_FOOTER_FONT_SIZE,
            text_color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            skip_first_page: false,
        }
    }
}

impl HeaderFooterConfig {
    /// Create a config with page numbers in the footer.
    #[must_use] pub fn with_page_numbers() -> Self {
        Self {
            show_footer: true,
            footer_content: MarginBoxContent::Combined(vec![
                MarginBoxContent::Text("Page ".to_string()),
                MarginBoxContent::PageCounter,
                MarginBoxContent::Text(" of ".to_string()),
                MarginBoxContent::PagesCounter,
            ]),
            ..Default::default()
        }
    }

    /// Create a config with page numbers in both header and footer.
    #[must_use] pub fn with_header_and_footer_page_numbers() -> Self {
        Self {
            show_header: true,
            show_footer: true,
            header_content: MarginBoxContent::Combined(vec![
                MarginBoxContent::Text("Page ".to_string()),
                MarginBoxContent::PageCounter,
            ]),
            footer_content: MarginBoxContent::Combined(vec![
                MarginBoxContent::Text("Page ".to_string()),
                MarginBoxContent::PageCounter,
                MarginBoxContent::Text(" of ".to_string()),
                MarginBoxContent::PagesCounter,
            ]),
            ..Default::default()
        }
    }

    /// Set custom header text.
    #[must_use]
    pub fn with_header_text(mut self, text: impl Into<String>) -> Self {
        self.show_header = true;
        self.header_content = MarginBoxContent::Text(text.into());
        self
    }

    /// Set custom footer text.
    #[must_use]
    pub fn with_footer_text(mut self, text: impl Into<String>) -> Self {
        self.show_footer = true;
        self.footer_content = MarginBoxContent::Text(text.into());
        self
    }

    /// Generate the text content for a margin box given page info.
    // `&self` is only reached via the recursive Combined arm; it is kept because this is a
    // public method and converting to an associated fn would break the `x.generate_content(..)` API.
    #[allow(clippy::only_used_in_recursion)]
    #[must_use] pub fn generate_content(&self, content: &MarginBoxContent, info: PageInfo) -> String {
        match content {
            MarginBoxContent::None => String::new(),
            MarginBoxContent::Text(s) => s.clone(),
            MarginBoxContent::PageCounter => info.page_number.to_string(),
            MarginBoxContent::PagesCounter => {
                if info.total_pages > 0 {
                    info.total_pages.to_string()
                } else {
                    "?".to_string()
                }
            }
            MarginBoxContent::PageCounterFormatted { format } => format.format(info.page_number),
            MarginBoxContent::Combined(parts) => parts
                .iter()
                .map(|p| self.generate_content(p, info))
                .collect(),
            MarginBoxContent::NamedString(name) => {
                // TODO: Look up named string from document context
                format!("[string:{name}]")
            }
            MarginBoxContent::RunningElement(name) => {
                // Running elements are rendered as display items, not text
                format!("[element:{name}]")
            }
            MarginBoxContent::Custom(f) => f(info),
        }
    }

    /// Get the header text for a specific page.
    #[must_use] pub fn header_text(&self, info: PageInfo) -> String {
        if !self.show_header {
            return String::new();
        }
        if self.skip_first_page && info.is_first {
            return String::new();
        }
        self.generate_content(&self.header_content, info)
    }

    /// Get the footer text for a specific page.
    #[must_use] pub fn footer_text(&self, info: PageInfo) -> String {
        if !self.show_footer {
            return String::new();
        }
        if self.skip_first_page && info.is_first {
            return String::new();
        }
        self.generate_content(&self.footer_content, info)
    }
}

/// Temporary configuration for page headers/footers without CSS `@page` parsing.
///
/// Provides programmatic control over page decoration until full CSS `@page`
/// rule support is implemented.
///
/// ## Supported Features
///
/// - Page numbers in header and/or footer
/// - Custom text in header and/or footer
/// - Number format (decimal, roman numerals, alphabetic, greek)
/// - Skip first page option
///
/// ## Example
///
/// ```rust
/// use azul_layout::solver3::pagination::FakePageConfig;
///
/// let config = FakePageConfig::new()
///     .with_footer_page_numbers()
///     .with_header_text("My Document")
///     .skip_first_page(true);
///
/// let header_footer = config.to_header_footer_config();
/// ```
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // independent header/footer toggle flags
pub struct FakePageConfig {
    /// Show header on pages
    pub show_header: bool,
    /// Show footer on pages
    pub show_footer: bool,
    /// Header text (static text, or None for page numbers only)
    pub header_text: Option<String>,
    /// Footer text (static text, or None for page numbers only)
    pub footer_text: Option<String>,
    /// Include page number in header
    pub header_page_number: bool,
    /// Include page number in footer
    pub footer_page_number: bool,
    /// Include total pages count ("of Y") in header
    pub header_total_pages: bool,
    /// Include total pages count ("of Y") in footer
    pub footer_total_pages: bool,
    /// Number format for page counters
    pub number_format: CounterFormat,
    /// Skip header/footer on first page
    pub skip_first_page: bool,
    /// Header height in points
    pub header_height: f32,
    /// Footer height in points
    pub footer_height: f32,
    /// Font size for header/footer text
    pub font_size: f32,
    /// Text color for header/footer
    pub text_color: ColorU,
}

impl Default for FakePageConfig {
    fn default() -> Self {
        Self {
            show_header: false,
            show_footer: false,
            header_text: None,
            footer_text: None,
            header_page_number: false,
            footer_page_number: false,
            header_total_pages: false,
            footer_total_pages: false,
            number_format: CounterFormat::Decimal,
            skip_first_page: false,
            header_height: DEFAULT_HEADER_FOOTER_HEIGHT,
            footer_height: DEFAULT_HEADER_FOOTER_HEIGHT,
            font_size: DEFAULT_HEADER_FOOTER_FONT_SIZE,
            text_color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }
    }
}

impl FakePageConfig {
    /// Create a new empty configuration (no headers/footers).
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    /// Enable footer with "Page X of Y" format.
    #[must_use] pub const fn with_footer_page_numbers(mut self) -> Self {
        self.show_footer = true;
        self.footer_page_number = true;
        self.footer_total_pages = true;
        self
    }

    /// Enable header with "Page X" format.
    #[must_use] pub const fn with_header_page_numbers(mut self) -> Self {
        self.show_header = true;
        self.header_page_number = true;
        self
    }

    /// Enable both header and footer with page numbers.
    #[must_use] pub const fn with_header_and_footer_page_numbers(mut self) -> Self {
        self.show_header = true;
        self.show_footer = true;
        self.header_page_number = true;
        self.footer_page_number = true;
        self.footer_total_pages = true;
        self
    }

    /// Set custom header text.
    #[must_use]
    pub fn with_header_text(mut self, text: impl Into<String>) -> Self {
        self.show_header = true;
        self.header_text = Some(text.into());
        self
    }

    /// Set custom footer text.
    #[must_use]
    pub fn with_footer_text(mut self, text: impl Into<String>) -> Self {
        self.show_footer = true;
        self.footer_text = Some(text.into());
        self
    }

    /// Set the number format for page counters.
    #[must_use] pub const fn with_number_format(mut self, format: CounterFormat) -> Self {
        self.number_format = format;
        self
    }

    /// Skip header/footer on the first page.
    #[must_use] pub const fn skip_first_page(mut self, skip: bool) -> Self {
        self.skip_first_page = skip;
        self
    }

    /// Set header height.
    #[must_use] pub const fn with_header_height(mut self, height: f32) -> Self {
        self.header_height = height;
        self
    }

    /// Set footer height.
    #[must_use] pub const fn with_footer_height(mut self, height: f32) -> Self {
        self.footer_height = height;
        self
    }

    /// Set font size for header/footer text.
    #[must_use] pub const fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Set text color for header/footer.
    #[must_use] pub const fn with_text_color(mut self, color: ColorU) -> Self {
        self.text_color = color;
        self
    }

    /// Convert this fake config to the internal `HeaderFooterConfig`.
    ///
    /// This is the bridge between the user-facing API and the internal
    /// pagination engine.
    #[must_use] pub fn to_header_footer_config(&self) -> HeaderFooterConfig {
        HeaderFooterConfig {
            show_header: self.show_header,
            show_footer: self.show_footer,
            header_height: self.header_height,
            footer_height: self.footer_height,
            header_content: self.build_header_content(),
            footer_content: self.build_footer_content(),
            skip_first_page: self.skip_first_page,
            font_size: self.font_size,
            text_color: self.text_color,
        }
    }

    /// Build the `MarginBoxContent` for the header.
    fn build_header_content(&self) -> MarginBoxContent {
        Self::build_margin_content(
            self.header_text.as_deref(),
            self.header_page_number,
            self.header_total_pages,
            self.number_format,
        )
    }

    /// Build the `MarginBoxContent` for the footer.
    fn build_footer_content(&self) -> MarginBoxContent {
        Self::build_margin_content(
            self.footer_text.as_deref(),
            self.footer_page_number,
            self.footer_total_pages,
            self.number_format,
        )
    }

    /// Shared helper for building header/footer margin box content.
    fn build_margin_content(
        text: Option<&str>,
        page_number: bool,
        total_pages: bool,
        number_format: CounterFormat,
    ) -> MarginBoxContent {
        let mut parts = Vec::new();

        if let Some(text) = text {
            parts.push(MarginBoxContent::Text(text.to_string()));
            if page_number {
                parts.push(MarginBoxContent::Text(" - ".to_string()));
            }
        }

        if page_number {
            parts.push(MarginBoxContent::Text("Page ".to_string()));
            if number_format == CounterFormat::Decimal {
                parts.push(MarginBoxContent::PageCounter);
            } else {
                parts.push(MarginBoxContent::PageCounterFormatted {
                    format: number_format,
                });
            }

            if total_pages {
                parts.push(MarginBoxContent::Text(" of ".to_string()));
                parts.push(MarginBoxContent::PagesCounter);
            }
        }

        if parts.is_empty() {
            MarginBoxContent::None
        } else if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            MarginBoxContent::Combined(parts)
        }
    }
}

/// Information about a table that may need header repetition.
#[derive(Debug, Clone)]
pub struct TableHeaderInfo {
    /// The table's node index in the layout tree
    pub table_node_index: usize,
    /// The Y position where the table starts
    pub table_start_y: f32,
    /// The Y position where the table ends
    pub table_end_y: f32,
    /// The thead's display list items (captured during initial render)
    pub thead_items: Vec<super::display_list::DisplayListItem>,
    /// Height of the thead
    pub thead_height: f32,
    /// The Y position of the thead relative to table start
    pub thead_offset_y: f32,
}

/// Context for tracking table headers across pages.
#[derive(Debug, Default, Clone)]
pub struct TableHeaderTracker {
    /// All tables with theads that might need repetition
    pub tables: Vec<TableHeaderInfo>,
}

impl TableHeaderTracker {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    /// Register a table's thead for potential repetition.
    pub fn register_table_header(&mut self, info: TableHeaderInfo) {
        self.tables.push(info);
    }

    /// Get theads that should be repeated on a specific page.
    ///
    /// Returns the thead items that need to be injected at the top of the page,
    /// along with the Y offset where they should appear.
    #[must_use] pub fn get_repeated_headers_for_page(
        &self,
        page_index: usize,
        page_top_y: f32,
        page_bottom_y: f32,
    ) -> Vec<(f32, &[super::display_list::DisplayListItem], f32)> {
        let mut headers = Vec::new();

        for table in &self.tables {
            // Check if this table spans into this page (but didn't start on this page)
            let table_starts_before_page = table.table_start_y < page_top_y;
            let table_continues_on_page = table.table_end_y > page_top_y;

            if table_starts_before_page && table_continues_on_page {
                // This table needs its header repeated on this page
                // The header should appear at the top of the page content area
                headers.push((
                    0.0, // Y offset from page top (header goes at very top)
                    table.thead_items.as_slice(),
                    table.thead_height,
                ));
            }
        }

        headers
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::display_list::DisplayListItem;
    use super::*;

    // ------------------------------------------------------------------
    // Independent decoders — used to round-trip `CounterFormat::format`
    // without reusing the encoder's own arithmetic.
    // ------------------------------------------------------------------

    /// Decode a lowercase roman numeral. Uses `i64` so the subtractive
    /// prefix ("iv") cannot underflow the accumulator.
    fn decode_roman(s: &str) -> Option<i64> {
        let mut vals = Vec::new();
        for c in s.chars() {
            vals.push(match c {
                'i' => 1_i64,
                'v' => 5,
                'x' => 10,
                'l' => 50,
                'c' => 100,
                'd' => 500,
                'm' => 1000,
                _ => return None,
            });
        }
        let mut total = 0_i64;
        for i in 0..vals.len() {
            if i + 1 < vals.len() && vals[i] < vals[i + 1] {
                total -= vals[i];
            } else {
                total += vals[i];
            }
        }
        Some(total)
    }

    /// Decode a lowercase bijective base-26 string ("a" == 1, "z" == 26, "aa" == 27).
    fn decode_alpha(s: &str) -> Option<usize> {
        if s.is_empty() {
            return None;
        }
        let mut n = 0_usize;
        for c in s.chars() {
            let digit = match c {
                'a'..='z' => c as usize - 'a' as usize + 1,
                _ => return None,
            };
            n = n.checked_mul(26)?.checked_add(digit)?;
        }
        Some(n)
    }

    /// Decode a lowercase bijective base-24 greek string ("α" == 1, "ω" == 24, "αα" == 25).
    fn decode_greek(s: &str) -> Option<usize> {
        const LOWER: &[char] = &[
            'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 'λ', 'μ', 'ν', 'ξ', 'ο', 'π', 'ρ',
            'σ', 'τ', 'υ', 'φ', 'χ', 'ψ', 'ω',
        ];
        if s.is_empty() {
            return None;
        }
        let mut n = 0_usize;
        for c in s.chars() {
            let digit = LOWER.iter().position(|l| *l == c)? + 1;
            n = n.checked_mul(LOWER.len())?.checked_add(digit)?;
        }
        Some(n)
    }

    fn table(start_y: f32, end_y: f32, thead_height: f32) -> TableHeaderInfo {
        TableHeaderInfo {
            table_node_index: 0,
            table_start_y: start_y,
            table_end_y: end_y,
            thead_items: vec![DisplayListItem::PopClip],
            thead_height,
            thead_offset_y: 0.0,
        }
    }

    // ==================================================================
    // CounterFormat::format — serializer: edge values, huge n, round-trip
    // ==================================================================

    #[test]
    fn counter_format_decimal_handles_zero_and_usize_max() {
        assert_eq!(CounterFormat::Decimal.format(0), "0");
        assert_eq!(CounterFormat::Decimal.format(1), "1");
        assert_eq!(
            CounterFormat::Decimal.format(usize::MAX),
            usize::MAX.to_string()
        );
    }

    #[test]
    fn counter_format_default_is_decimal_and_does_not_panic_on_zero() {
        let default = CounterFormat::default();
        assert_eq!(default, CounterFormat::Decimal);
        assert_eq!(default.format(0), "0");
    }

    #[test]
    fn counter_format_leading_zero_pads_to_two_but_never_truncates() {
        assert_eq!(CounterFormat::DecimalLeadingZero.format(0), "00");
        assert_eq!(CounterFormat::DecimalLeadingZero.format(7), "07");
        assert_eq!(CounterFormat::DecimalLeadingZero.format(9), "09");
        assert_eq!(CounterFormat::DecimalLeadingZero.format(10), "10");
        // A width-2 pad must not *clip* wider numbers.
        assert_eq!(CounterFormat::DecimalLeadingZero.format(12345), "12345");
        assert_eq!(
            CounterFormat::DecimalLeadingZero.format(usize::MAX),
            usize::MAX.to_string()
        );
    }

    #[test]
    fn counter_format_every_variant_survives_zero_one_and_usize_max() {
        // The contract we are pinning: no panic, no hang, output is valid UTF-8
        // with all char boundaries intact for every (variant, extreme) pair.
        let variants = [
            CounterFormat::Decimal,
            CounterFormat::DecimalLeadingZero,
            CounterFormat::LowerRoman,
            CounterFormat::UpperRoman,
            CounterFormat::LowerAlpha,
            CounterFormat::UpperAlpha,
            CounterFormat::LowerGreek,
        ];
        for v in variants {
            for n in [0_usize, 1, 25, 26, 27, 3999, 4000, usize::MAX] {
                let s = v.format(n);
                assert!(
                    s.is_char_boundary(0) && s.is_char_boundary(s.len()),
                    "{v:?}.format({n}) produced a malformed string"
                );
            }
        }
    }

    #[test]
    fn counter_format_alphabetic_and_greek_return_empty_at_zero() {
        // KNOWN DIVERGENCE (asserted, not papered over): `format_counter` in
        // `super::counters` applies a CSS decimal fallback when an alphabetic or
        // greek style cannot represent a value, but `CounterFormat::format` does
        // not — so a page counter at 0 renders as a *blank* margin box here while
        // roman renders "0". Page numbers are 1-indexed so this is unreachable in
        // the current pipeline; the test exists to catch it becoming reachable.
        assert_eq!(CounterFormat::LowerAlpha.format(0), "");
        assert_eq!(CounterFormat::UpperAlpha.format(0), "");
        assert_eq!(CounterFormat::LowerGreek.format(0), "");
        assert_eq!(CounterFormat::LowerRoman.format(0), "0");
        assert_eq!(CounterFormat::UpperRoman.format(0), "0");
    }

    #[test]
    fn counter_format_roman_falls_back_to_decimal_past_3999() {
        assert_eq!(CounterFormat::LowerRoman.format(3999), "mmmcmxcix");
        assert_eq!(CounterFormat::UpperRoman.format(3999), "MMMCMXCIX");
        // 4000 is not representable -> decimal, in *both* cases (no stray casing).
        assert_eq!(CounterFormat::LowerRoman.format(4000), "4000");
        assert_eq!(CounterFormat::UpperRoman.format(4000), "4000");
        assert_eq!(
            CounterFormat::UpperRoman.format(usize::MAX),
            usize::MAX.to_string()
        );
    }

    #[test]
    fn counter_format_roman_round_trips_over_its_whole_representable_range() {
        for n in 1..=3999_usize {
            let lower = CounterFormat::LowerRoman.format(n);
            let upper = CounterFormat::UpperRoman.format(n);
            assert_eq!(
                decode_roman(&lower),
                Some(n as i64),
                "lower-roman round-trip failed for {n} -> {lower}"
            );
            assert_eq!(upper, lower.to_uppercase(), "casing mismatch for {n}");
        }
    }

    #[test]
    fn counter_format_alphabetic_round_trips_and_is_injective() {
        let mut seen = std::collections::HashSet::new();
        for n in 1..=3000_usize {
            let lower = CounterFormat::LowerAlpha.format(n);
            let upper = CounterFormat::UpperAlpha.format(n);
            assert_eq!(
                decode_alpha(&lower),
                Some(n),
                "lower-alpha round-trip failed for {n} -> {lower}"
            );
            assert_eq!(upper, lower.to_uppercase(), "casing mismatch for {n}");
            assert!(seen.insert(lower), "two page numbers collided at {n}");
        }
        // Bijective base-26 boundaries — the classic off-by-one zone.
        assert_eq!(CounterFormat::LowerAlpha.format(26), "z");
        assert_eq!(CounterFormat::LowerAlpha.format(27), "aa");
        assert_eq!(CounterFormat::LowerAlpha.format(52), "az");
        assert_eq!(CounterFormat::LowerAlpha.format(53), "ba");
    }

    #[test]
    fn counter_format_greek_round_trips_and_emits_whole_code_points() {
        for n in 1..=2000_usize {
            let s = CounterFormat::LowerGreek.format(n);
            assert_eq!(
                decode_greek(&s),
                Some(n),
                "lower-greek round-trip failed for {n} -> {s}"
            );
            // Every greek letter is 2 bytes: byte len must be exactly 2x char count,
            // i.e. the encoder's `insert(0, ..)` never split a code point.
            assert_eq!(s.len(), s.chars().count() * 2, "sliced a code point at {n}");
        }
        assert_eq!(CounterFormat::LowerGreek.format(24), "ω");
        assert_eq!(CounterFormat::LowerGreek.format(25), "αα");
    }

    #[test]
    fn counter_format_usize_max_terminates_for_the_positional_styles() {
        // `(n - 1) / base` strictly decreases, so these must halt rather than hang.
        let alpha = CounterFormat::LowerAlpha.format(usize::MAX);
        let greek = CounterFormat::LowerGreek.format(usize::MAX);
        assert!(!alpha.is_empty() && alpha.chars().all(|c| c.is_ascii_lowercase()));
        assert!(!greek.is_empty());
        assert_eq!(greek.len(), greek.chars().count() * 2);
    }

    // ==================================================================
    // PageInfo::new — constructor invariants
    // ==================================================================

    #[test]
    fn page_info_new_sets_flags_for_a_representative_page() {
        let info = PageInfo::new(1, 3);
        assert_eq!(info.page_number, 1);
        assert_eq!(info.total_pages, 3);
        assert!(info.is_first);
        assert!(!info.is_last);
        assert!(info.is_right, "page 1 (odd) is a recto page");
        assert!(!info.is_left);
        assert!(!info.is_blank);
    }

    #[test]
    fn page_info_left_and_right_are_always_mutually_exclusive() {
        for n in [0_usize, 1, 2, 3, 100, 101, usize::MAX - 1, usize::MAX] {
            let info = PageInfo::new(n, 0);
            assert!(
                info.is_left != info.is_right,
                "page {n} claimed to be both/neither verso and recto"
            );
            assert_eq!(info.is_left, n % 2 == 0);
            assert!(!info.is_blank, "new() must never fabricate a blank page");
        }
    }

    #[test]
    fn page_info_is_last_is_false_when_the_total_is_unknown() {
        // total_pages == 0 means "unknown during the first pass" — nothing is last.
        for n in [0_usize, 1, 7, usize::MAX] {
            assert!(!PageInfo::new(n, 0).is_last, "page {n} of 0 claimed is_last");
        }
    }

    #[test]
    fn page_info_page_zero_is_degenerate_but_does_not_panic() {
        let info = PageInfo::new(0, 0);
        assert!(!info.is_first, "1-indexed: page 0 is not the first page");
        assert!(!info.is_last);
        assert!(info.is_left, "0 is even, so it lands on the verso branch");
    }

    #[test]
    fn page_info_out_of_range_page_number_does_not_claim_to_be_last() {
        // page_number > total_pages is nonsense input; it must not silently
        // become `is_last` (which would duplicate the last-page decoration).
        let info = PageInfo::new(9, 3);
        assert!(!info.is_last);
        assert!(!info.is_first);
    }

    #[test]
    fn page_info_usize_max_extremes_do_not_overflow() {
        let info = PageInfo::new(usize::MAX, usize::MAX);
        assert!(info.is_last, "the final page of a MAX-page document is last");
        assert!(!info.is_first);
        assert!(info.is_right, "usize::MAX is odd");
        let single = PageInfo::new(1, 1);
        assert!(single.is_first && single.is_last);
    }

    // ==================================================================
    // HeaderFooterConfig — constructors + content generation
    // ==================================================================

    #[test]
    fn header_footer_default_renders_nothing_on_any_page() {
        let cfg = HeaderFooterConfig::default();
        assert!(!cfg.show_header && !cfg.show_footer);
        assert!(matches!(cfg.header_content, MarginBoxContent::None));
        assert!(matches!(cfg.footer_content, MarginBoxContent::None));
        assert_eq!(cfg.header_text(PageInfo::new(1, 1)), "");
        assert_eq!(cfg.footer_text(PageInfo::new(usize::MAX, usize::MAX)), "");
        assert_eq!(cfg.text_color.a, 255);
    }

    #[test]
    fn header_footer_with_page_numbers_only_enables_the_footer() {
        let cfg = HeaderFooterConfig::with_page_numbers();
        assert!(cfg.show_footer);
        assert!(!cfg.show_header, "with_page_numbers must not enable a header");
        assert_eq!(cfg.footer_text(PageInfo::new(2, 7)), "Page 2 of 7");
        assert_eq!(cfg.header_text(PageInfo::new(2, 7)), "");
    }

    #[test]
    fn header_footer_unknown_total_renders_a_question_mark_not_a_zero() {
        let cfg = HeaderFooterConfig::with_page_numbers();
        assert_eq!(cfg.footer_text(PageInfo::new(1, 0)), "Page 1 of ?");
    }

    #[test]
    fn header_footer_with_header_and_footer_page_numbers_fills_both() {
        let cfg = HeaderFooterConfig::with_header_and_footer_page_numbers();
        assert!(cfg.show_header && cfg.show_footer);
        let info = PageInfo::new(3, 10);
        assert_eq!(cfg.header_text(info), "Page 3");
        assert_eq!(cfg.footer_text(info), "Page 3 of 10");
        // Extremes must not panic or produce truncated numbers.
        let extreme = PageInfo::new(usize::MAX, usize::MAX);
        assert_eq!(
            cfg.header_text(extreme),
            format!("Page {}", usize::MAX)
        );
    }

    #[test]
    fn header_footer_with_text_enables_the_box_and_preserves_unicode_exactly() {
        let text = "Ünïcödé — 日本語 🎉\u{200b}\u{0}";
        let cfg = HeaderFooterConfig::default()
            .with_header_text(text)
            .with_footer_text(text);
        assert!(cfg.show_header && cfg.show_footer);
        let info = PageInfo::new(1, 1);
        // Byte-for-byte: no normalization, no NUL truncation, no BOM stripping.
        assert_eq!(cfg.header_text(info), text);
        assert_eq!(cfg.footer_text(info), text);
        assert_eq!(cfg.header_text(info).len(), text.len());
    }

    #[test]
    fn header_footer_empty_text_still_switches_the_box_on() {
        // Quirk worth pinning: an empty string is indistinguishable from "no header"
        // in the rendered output, yet it *does* flip `show_header` — so the slicer
        // will still reserve `header_height` for a blank box.
        let cfg = HeaderFooterConfig::default().with_header_text("");
        assert!(cfg.show_header);
        assert_eq!(cfg.header_text(PageInfo::new(1, 1)), "");
        assert!(cfg.header_height > 0.0);
    }

    #[test]
    fn header_footer_text_of_huge_length_round_trips_without_truncation() {
        let huge = "x".repeat(200_000);
        let cfg = HeaderFooterConfig::default().with_footer_text(huge.clone());
        assert_eq!(cfg.footer_text(PageInfo::new(1, 1)).len(), huge.len());
    }

    #[test]
    fn header_footer_last_builder_call_wins() {
        let cfg = HeaderFooterConfig::default()
            .with_header_text("first")
            .with_header_text("second");
        assert_eq!(cfg.header_text(PageInfo::new(1, 1)), "second");
    }

    #[test]
    fn header_footer_skip_first_page_blanks_only_page_one() {
        let mut cfg = HeaderFooterConfig::with_header_and_footer_page_numbers();
        cfg.skip_first_page = true;
        assert_eq!(cfg.header_text(PageInfo::new(1, 5)), "");
        assert_eq!(cfg.footer_text(PageInfo::new(1, 5)), "");
        assert_eq!(cfg.header_text(PageInfo::new(2, 5)), "Page 2");
        assert_eq!(cfg.footer_text(PageInfo::new(2, 5)), "Page 2 of 5");
        // The gate keys off `is_first`, not off `page_number == 1`: a hand-built
        // PageInfo with is_first forced on is skipped regardless of its number.
        let mut forged = PageInfo::new(4, 5);
        forged.is_first = true;
        assert_eq!(cfg.header_text(forged), "");
    }

    #[test]
    fn header_footer_generate_content_covers_every_margin_box_variant() {
        let cfg = HeaderFooterConfig::default();
        let info = PageInfo::new(4, 9);
        assert_eq!(cfg.generate_content(&MarginBoxContent::None, info), "");
        assert_eq!(
            cfg.generate_content(&MarginBoxContent::Text(String::new()), info),
            ""
        );
        assert_eq!(cfg.generate_content(&MarginBoxContent::PageCounter, info), "4");
        assert_eq!(
            cfg.generate_content(&MarginBoxContent::PagesCounter, info),
            "9"
        );
        assert_eq!(
            cfg.generate_content(
                &MarginBoxContent::PageCounterFormatted {
                    format: CounterFormat::LowerRoman
                },
                info
            ),
            "iv"
        );
        // Not-yet-implemented variants must degrade to a placeholder, not panic.
        assert_eq!(
            cfg.generate_content(&MarginBoxContent::NamedString("chapter".into()), info),
            "[string:chapter]"
        );
        assert_eq!(
            cfg.generate_content(&MarginBoxContent::RunningElement("hdr".into()), info),
            "[element:hdr]"
        );
    }

    #[test]
    fn header_footer_generate_content_of_an_empty_combined_is_empty() {
        let cfg = HeaderFooterConfig::default();
        assert_eq!(
            cfg.generate_content(&MarginBoxContent::Combined(Vec::new()), PageInfo::new(1, 1)),
            ""
        );
    }

    #[test]
    fn header_footer_generate_content_recurses_through_nested_combined() {
        // `Combined` recursion is unbounded in the impl; a deeply nested tree (as could
        // arrive from a future @page parser) must still resolve. Depth is kept modest
        // on purpose — a stack overflow would abort the whole test binary, so this
        // pins "reasonable nesting works" rather than probing for the cliff.
        let cfg = HeaderFooterConfig::default();
        let mut content = MarginBoxContent::PageCounter;
        for _ in 0..128 {
            content = MarginBoxContent::Combined(vec![content]);
        }
        assert_eq!(cfg.generate_content(&content, PageInfo::new(42, 99)), "42");
    }

    #[test]
    fn header_footer_generate_content_calls_a_custom_hook_exactly_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let content = MarginBoxContent::Custom(Arc::new(move |info: PageInfo| {
            seen.fetch_add(1, Ordering::SeqCst);
            format!("{}/{}", info.page_number, info.total_pages)
        }));
        let cfg = HeaderFooterConfig::default();
        assert_eq!(cfg.generate_content(&content, PageInfo::new(2, 5)), "2/5");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Nested inside a Combined, it is still invoked (once per occurrence).
        let combined = MarginBoxContent::Combined(vec![
            MarginBoxContent::Text("[".to_string()),
            content,
            MarginBoxContent::Text("]".to_string()),
        ]);
        assert_eq!(cfg.generate_content(&combined, PageInfo::new(2, 5)), "[2/5]");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn header_footer_hidden_box_short_circuits_before_generating_content() {
        // show_header == false must win even when header_content would panic-free
        // produce text — otherwise a disabled box still costs a callback call.
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut cfg = HeaderFooterConfig::default();
        cfg.header_content = MarginBoxContent::Custom(Arc::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            "leaked".to_string()
        }));
        assert_eq!(cfg.header_text(PageInfo::new(1, 1)), "");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    // ==================================================================
    // FakePageConfig — builders, defaults, and the HeaderFooterConfig bridge
    // ==================================================================

    #[test]
    fn fake_page_new_is_inert_and_matches_default() {
        let cfg = FakePageConfig::new();
        assert!(!cfg.show_header && !cfg.show_footer);
        assert!(cfg.header_text.is_none() && cfg.footer_text.is_none());
        assert!(!cfg.header_page_number && !cfg.footer_page_number);
        assert!(!cfg.header_total_pages && !cfg.footer_total_pages);
        assert!(!cfg.skip_first_page);
        assert_eq!(cfg.number_format, CounterFormat::Decimal);

        let hf = cfg.to_header_footer_config();
        assert!(matches!(hf.header_content, MarginBoxContent::None));
        assert!(matches!(hf.footer_content, MarginBoxContent::None));
        assert_eq!(hf.header_text(PageInfo::new(1, 1)), "");
        assert_eq!(hf.footer_text(PageInfo::new(1, 1)), "");
    }

    #[test]
    fn fake_page_footer_page_numbers_render_page_x_of_y() {
        let hf = FakePageConfig::new()
            .with_footer_page_numbers()
            .to_header_footer_config();
        assert!(hf.show_footer && !hf.show_header);
        assert_eq!(hf.footer_text(PageInfo::new(2, 7)), "Page 2 of 7");
        assert_eq!(hf.footer_text(PageInfo::new(2, 0)), "Page 2 of ?");
    }

    #[test]
    fn fake_page_header_page_numbers_omit_the_total() {
        let hf = FakePageConfig::new()
            .with_header_page_numbers()
            .to_header_footer_config();
        assert_eq!(hf.header_text(PageInfo::new(3, 7)), "Page 3");
        assert_eq!(hf.footer_text(PageInfo::new(3, 7)), "");
    }

    #[test]
    fn fake_page_header_and_footer_page_numbers_agree_with_the_pair_of_setters() {
        let both = FakePageConfig::new()
            .with_header_and_footer_page_numbers()
            .to_header_footer_config();
        let info = PageInfo::new(5, 11);
        assert_eq!(both.header_text(info), "Page 5");
        assert_eq!(both.footer_text(info), "Page 5 of 11");
    }

    #[test]
    fn fake_page_text_and_page_number_are_joined_by_a_separator() {
        let mut cfg = FakePageConfig::new()
            .with_header_text("My Document")
            .with_header_page_numbers();
        cfg.header_total_pages = true;
        let hf = cfg.to_header_footer_config();
        assert_eq!(
            hf.header_text(PageInfo::new(4, 9)),
            "My Document - Page 4 of 9"
        );
    }

    #[test]
    fn fake_page_number_format_flows_into_the_rendered_counter() {
        let hf = FakePageConfig::new()
            .with_footer_page_numbers()
            .with_number_format(CounterFormat::LowerRoman)
            .to_header_footer_config();
        // Only the page counter is formatted; the *total* stays decimal — pin that,
        // because a mismatched pair ("Page iv of 9") is easy to regress into.
        assert_eq!(hf.footer_text(PageInfo::new(4, 9)), "Page iv of 9");

        let greek = FakePageConfig::new()
            .with_header_page_numbers()
            .with_number_format(CounterFormat::LowerGreek)
            .to_header_footer_config();
        assert_eq!(greek.header_text(PageInfo::new(2, 9)), "Page β");
    }

    #[test]
    fn fake_page_decimal_format_takes_the_unformatted_counter_branch() {
        let decimal = FakePageConfig::new().with_header_page_numbers();
        match decimal.to_header_footer_config().header_content {
            MarginBoxContent::Combined(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(parts[1], MarginBoxContent::PageCounter));
            }
            other => panic!("expected Combined, got {other:?}"),
        }
        let roman = FakePageConfig::new()
            .with_header_page_numbers()
            .with_number_format(CounterFormat::UpperRoman);
        match roman.to_header_footer_config().header_content {
            MarginBoxContent::Combined(parts) => {
                assert!(matches!(
                    parts[1],
                    MarginBoxContent::PageCounterFormatted {
                        format: CounterFormat::UpperRoman
                    }
                ));
            }
            other => panic!("expected Combined, got {other:?}"),
        }
    }

    #[test]
    fn fake_page_total_pages_without_page_number_is_silently_dropped() {
        // Adversarial: `footer_total_pages` is only honoured *inside* the
        // `page_number` branch of `build_margin_content`. Setting the total flag
        // alone yields an empty box rather than "of Y" — assert the real behavior
        // so a future fix has to update this test deliberately.
        let mut cfg = FakePageConfig::new();
        cfg.show_footer = true;
        cfg.footer_total_pages = true;
        let hf = cfg.to_header_footer_config();
        assert!(matches!(hf.footer_content, MarginBoxContent::None));
        assert_eq!(hf.footer_text(PageInfo::new(1, 5)), "");
    }

    #[test]
    fn fake_page_single_text_part_is_not_wrapped_in_a_combined() {
        let hf = FakePageConfig::new()
            .with_footer_text("plain")
            .to_header_footer_config();
        assert!(matches!(hf.footer_content, MarginBoxContent::Text(ref s) if s == "plain"));
        assert_eq!(hf.footer_text(PageInfo::new(1, 1)), "plain");
    }

    #[test]
    fn fake_page_empty_text_plus_page_number_leaks_a_leading_separator() {
        // Adversarial edge: an empty custom text is still pushed as a `Text("")`
        // part, so the " - " joiner is emitted with nothing before it.
        let hf = FakePageConfig::new()
            .with_header_text("")
            .with_header_page_numbers()
            .to_header_footer_config();
        assert_eq!(hf.header_text(PageInfo::new(1, 1)), " - Page 1");
    }

    #[test]
    fn fake_page_unicode_text_survives_the_bridge_byte_for_byte() {
        let text = "Ünïcödé — 日本語 🎉";
        let hf = FakePageConfig::new()
            .with_header_text(text)
            .with_footer_text(text)
            .to_header_footer_config();
        let info = PageInfo::new(1, 1);
        assert_eq!(hf.header_text(info), text);
        assert_eq!(hf.footer_text(info), text);
    }

    #[test]
    fn fake_page_huge_text_survives_the_bridge() {
        let huge = "λ".repeat(100_000);
        let hf = FakePageConfig::new()
            .with_footer_text(huge.clone())
            .to_header_footer_config();
        let out = hf.footer_text(PageInfo::new(1, 1));
        assert_eq!(out.len(), huge.len());
        assert_eq!(out.chars().count(), 100_000);
    }

    #[test]
    fn fake_page_skip_first_page_toggles_both_ways() {
        let on = FakePageConfig::new()
            .with_footer_page_numbers()
            .skip_first_page(true)
            .to_header_footer_config();
        assert!(on.skip_first_page);
        assert_eq!(on.footer_text(PageInfo::new(1, 3)), "");
        assert_eq!(on.footer_text(PageInfo::new(2, 3)), "Page 2 of 3");

        let off = FakePageConfig::new()
            .with_footer_page_numbers()
            .skip_first_page(true)
            .skip_first_page(false)
            .to_header_footer_config();
        assert!(!off.skip_first_page);
        assert_eq!(off.footer_text(PageInfo::new(1, 3)), "Page 1 of 3");
    }

    #[test]
    fn fake_page_non_finite_geometry_is_stored_and_forwarded_verbatim() {
        // There is no validation/clamping anywhere on this path: NaN and infinite
        // heights reach the slicer unchanged. Pin it so any future clamp is a
        // deliberate, reviewed change rather than a silent behavior swap.
        let cfg = FakePageConfig::new()
            .with_header_height(f32::NAN)
            .with_footer_height(f32::INFINITY)
            .with_font_size(-0.0);
        assert!(cfg.header_height.is_nan());
        let hf = cfg.to_header_footer_config();
        assert!(hf.header_height.is_nan(), "NaN header height was swallowed");
        assert_eq!(hf.footer_height, f32::INFINITY);
        assert_eq!(hf.font_size, -0.0);
    }

    #[test]
    fn fake_page_extreme_but_finite_geometry_is_preserved_exactly() {
        let hf = FakePageConfig::new()
            .with_header_height(f32::MAX)
            .with_footer_height(f32::MIN)
            .with_font_size(f32::MIN_POSITIVE)
            .to_header_footer_config();
        assert_eq!(hf.header_height, f32::MAX);
        assert_eq!(hf.footer_height, f32::MIN);
        assert_eq!(hf.font_size, f32::MIN_POSITIVE);

        let negative = FakePageConfig::new()
            .with_header_height(-100.0)
            .to_header_footer_config();
        assert_eq!(negative.header_height, -100.0);
    }

    #[test]
    fn fake_page_text_color_crosses_the_bridge_unchanged() {
        let color = ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 0,
        };
        let hf = FakePageConfig::new()
            .with_text_color(color)
            .to_header_footer_config();
        assert_eq!(hf.text_color, color);
        assert_eq!(hf.text_color.a, 0, "a fully transparent color must survive");
    }

    #[test]
    fn fake_page_to_header_footer_config_is_a_pure_read() {
        // The bridge is a getter: calling it repeatedly must be stable and must not
        // mutate the source config.
        let cfg = FakePageConfig::new()
            .with_header_and_footer_page_numbers()
            .with_number_format(CounterFormat::UpperAlpha)
            .skip_first_page(true);
        let info = PageInfo::new(3, 4);
        let first = cfg.to_header_footer_config();
        let second = cfg.to_header_footer_config();
        assert_eq!(first.header_text(info), second.header_text(info));
        assert_eq!(first.footer_text(info), second.footer_text(info));
        assert_eq!(first.header_text(info), "Page C");
        assert!(cfg.show_header && cfg.show_footer && cfg.skip_first_page);
    }

    #[test]
    fn fake_page_build_margin_content_shapes_match_the_part_count() {
        // Private helper, exercised directly: 0 parts -> None, 1 part -> that part,
        // >1 -> Combined. The `parts.pop().unwrap()` in the 1-part arm is the risk.
        assert!(matches!(
            FakePageConfig::build_margin_content(None, false, false, CounterFormat::Decimal),
            MarginBoxContent::None
        ));
        assert!(matches!(
            FakePageConfig::build_margin_content(None, false, true, CounterFormat::Decimal),
            MarginBoxContent::None
        ));
        assert!(matches!(
            FakePageConfig::build_margin_content(Some("t"), false, false, CounterFormat::Decimal),
            MarginBoxContent::Text(ref s) if s == "t"
        ));
        assert!(matches!(
            FakePageConfig::build_margin_content(Some(""), false, true, CounterFormat::Decimal),
            MarginBoxContent::Text(ref s) if s.is_empty()
        ));
        match FakePageConfig::build_margin_content(
            Some("Doc"),
            true,
            true,
            CounterFormat::LowerRoman,
        ) {
            MarginBoxContent::Combined(parts) => {
                assert_eq!(parts.len(), 6, "text + sep + label + counter + of + total");
                assert!(matches!(parts[1], MarginBoxContent::Text(ref s) if s == " - "));
                assert!(matches!(
                    parts[3],
                    MarginBoxContent::PageCounterFormatted { .. }
                ));
                assert!(matches!(parts[5], MarginBoxContent::PagesCounter));
            }
            other => panic!("expected Combined, got {other:?}"),
        }
    }

    #[test]
    fn fake_page_build_header_and_footer_content_read_their_own_fields() {
        // Guards against the classic copy-paste bug: header building from the
        // footer's flags (or vice versa).
        let mut cfg = FakePageConfig::new();
        cfg.header_text = Some("H".to_string());
        cfg.footer_text = Some("F".to_string());
        cfg.header_page_number = true;
        cfg.footer_page_number = false;
        let hf = HeaderFooterConfig::default();
        let info = PageInfo::new(2, 4);
        assert_eq!(
            hf.generate_content(&cfg.build_header_content(), info),
            "H - Page 2"
        );
        assert_eq!(hf.generate_content(&cfg.build_footer_content(), info), "F");
    }

    // ==================================================================
    // TableHeaderTracker — numeric edges on the page-overlap arithmetic
    // ==================================================================

    #[test]
    fn tracker_new_is_empty_and_matches_default() {
        let tracker = TableHeaderTracker::new();
        assert!(tracker.tables.is_empty());
        assert!(TableHeaderTracker::default().tables.is_empty());
        assert!(tracker
            .get_repeated_headers_for_page(0, 0.0, 0.0)
            .is_empty());
    }

    #[test]
    fn tracker_empty_returns_nothing_for_every_extreme_page_geometry() {
        let tracker = TableHeaderTracker::new();
        for page_index in [0_usize, 1, usize::MAX] {
            for y in [
                0.0_f32,
                -0.0,
                f32::MIN,
                f32::MAX,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NAN,
            ] {
                assert!(
                    tracker.get_repeated_headers_for_page(page_index, y, y).is_empty(),
                    "empty tracker produced a header at page {page_index}, y={y}"
                );
            }
        }
    }

    #[test]
    fn tracker_register_appends_in_order_and_allows_duplicates() {
        let mut tracker = TableHeaderTracker::new();
        for i in 0..1000 {
            let mut info = table(0.0, 100.0, 10.0);
            info.table_node_index = i;
            tracker.register_table_header(info);
        }
        // Same table registered twice must not be deduplicated silently.
        tracker.register_table_header(table(0.0, 100.0, 10.0));
        tracker.register_table_header(table(0.0, 100.0, 10.0));
        assert_eq!(tracker.tables.len(), 1002);
        assert_eq!(tracker.tables[0].table_node_index, 0);
        assert_eq!(tracker.tables[999].table_node_index, 999);
    }

    #[test]
    fn tracker_repeats_only_tables_that_straddle_the_page_top() {
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(0.0, 500.0, 20.0)); // straddles -> repeat
        tracker.register_table_header(table(0.0, 50.0, 20.0)); // ends above -> no
        tracker.register_table_header(table(200.0, 500.0, 20.0)); // starts on page -> no
        let headers = tracker.get_repeated_headers_for_page(1, 100.0, 900.0);
        assert_eq!(headers.len(), 1);
        let (offset, items, height) = headers[0];
        assert_eq!(offset, 0.0, "a repeated thead sits at the page top");
        assert_eq!(height, 20.0);
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], DisplayListItem::PopClip));
    }

    #[test]
    fn tracker_page_boundary_comparisons_are_strict() {
        let mut tracker = TableHeaderTracker::new();
        // start_y == page_top: the table *begins* on this page -> its own thead is
        // already there, so no repeat.
        tracker.register_table_header(table(100.0, 500.0, 20.0));
        assert!(tracker
            .get_repeated_headers_for_page(1, 100.0, 900.0)
            .is_empty());

        // end_y == page_top: the table finished exactly at the boundary -> no repeat.
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(0.0, 100.0, 20.0));
        assert!(tracker
            .get_repeated_headers_for_page(1, 100.0, 900.0)
            .is_empty());

        // One representable step past the boundary on both sides -> repeat. (`f32::EPSILON`
        // is useless here: at magnitude 100 it is far below one ulp and would round
        // straight back to 100.0, silently re-testing the equality case above.)
        let below = f32::from_bits(100.0_f32.to_bits() - 1);
        let above = f32::from_bits(100.0_f32.to_bits() + 1);
        assert!(below < 100.0 && above > 100.0, "ulp step collapsed");
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(below, above, 20.0));
        assert_eq!(
            tracker.get_repeated_headers_for_page(1, 100.0, 900.0).len(),
            1
        );
    }

    #[test]
    fn tracker_nan_page_top_yields_no_headers_and_no_panic() {
        // Every f32 comparison against NaN is false, so both guards fail: the
        // defined result is "no repeated headers" rather than a panic or a
        // spurious header at an unrenderable offset.
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(0.0, 500.0, 20.0));
        assert!(tracker
            .get_repeated_headers_for_page(1, f32::NAN, 900.0)
            .is_empty());
        // A NaN *bottom* changes nothing, because the bottom is never read.
        assert_eq!(
            tracker
                .get_repeated_headers_for_page(1, 100.0, f32::NAN)
                .len(),
            1
        );
    }

    #[test]
    fn tracker_nan_table_geometry_yields_no_headers() {
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(f32::NAN, 500.0, 20.0));
        tracker.register_table_header(table(0.0, f32::NAN, 20.0));
        tracker.register_table_header(table(f32::NAN, f32::NAN, 20.0));
        assert!(
            tracker
                .get_repeated_headers_for_page(1, 100.0, 900.0)
                .is_empty(),
            "a NaN-positioned table must not be repeated"
        );
    }

    #[test]
    fn tracker_infinite_page_top_behaves_deterministically() {
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(0.0, 500.0, 20.0));
        // +inf page top: the table starts before it, but nothing can extend past
        // +inf -> no repeat.
        assert!(tracker
            .get_repeated_headers_for_page(1, f32::INFINITY, f32::INFINITY)
            .is_empty());
        // -inf page top: nothing starts before -inf -> no repeat.
        assert!(tracker
            .get_repeated_headers_for_page(1, f32::NEG_INFINITY, 900.0)
            .is_empty());
    }

    #[test]
    fn tracker_infinite_table_extent_repeats_forever_without_overflow() {
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(f32::NEG_INFINITY, f32::INFINITY, f32::INFINITY));
        let headers = tracker.get_repeated_headers_for_page(usize::MAX, f32::MAX, f32::MAX);
        assert_eq!(headers.len(), 1);
        // The thead height is forwarded verbatim — no clamping, no NaN laundering.
        assert!(headers[0].2.is_infinite());
    }

    #[test]
    fn tracker_nan_thead_height_is_forwarded_not_sanitized() {
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(0.0, 500.0, f32::NAN));
        let headers = tracker.get_repeated_headers_for_page(1, 100.0, 900.0);
        assert_eq!(headers.len(), 1);
        assert!(headers[0].2.is_nan(), "height NaN was silently rewritten");
    }

    #[test]
    fn tracker_page_index_is_ignored_by_the_current_implementation() {
        // Documented reality check: `page_index` never enters the arithmetic, so a
        // straddling table repeats identically for page 0 and page usize::MAX. If
        // per-page logic is ever added, this test must be revisited.
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(0.0, 500.0, 20.0));
        let a = tracker.get_repeated_headers_for_page(0, 100.0, 900.0);
        let b = tracker.get_repeated_headers_for_page(usize::MAX, 100.0, 900.0);
        assert_eq!(a.len(), b.len());
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn tracker_page_bottom_is_ignored_even_when_the_page_is_inverted() {
        // `page_bottom_y` is unused: an inverted page (bottom above top) still
        // yields a header. Pinned so the parameter's dead status is visible.
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(0.0, 500.0, 20.0));
        assert_eq!(
            tracker.get_repeated_headers_for_page(1, 100.0, -900.0).len(),
            1
        );
        assert_eq!(
            tracker.get_repeated_headers_for_page(1, 100.0, 100.0).len(),
            1
        );
    }

    #[test]
    fn tracker_preserves_registration_order_across_many_straddling_tables() {
        let mut tracker = TableHeaderTracker::new();
        for i in 0..64_u32 {
            #[allow(clippy::cast_precision_loss)]
            tracker.register_table_header(table(0.0, 500.0, i as f32));
        }
        let headers = tracker.get_repeated_headers_for_page(3, 100.0, 900.0);
        assert_eq!(headers.len(), 64);
        for (i, (offset, _, height)) in headers.iter().enumerate() {
            assert_eq!(*offset, 0.0);
            #[allow(clippy::cast_precision_loss)]
            let expected = i as f32;
            assert_eq!(*height, expected, "headers came back out of order");
        }
    }

    #[test]
    fn tracker_zero_height_page_returns_nothing() {
        // A degenerate zero-extent page (top == bottom == 0) is the "zero" case:
        // no table can both start before 0 and end after 0 unless it is negative.
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(0.0, 500.0, 20.0));
        assert!(tracker
            .get_repeated_headers_for_page(0, 0.0, 0.0)
            .is_empty());
        // ...but a table with a negative start does straddle y=0.
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(table(-10.0, 500.0, 20.0));
        assert_eq!(tracker.get_repeated_headers_for_page(0, 0.0, 0.0).len(), 1);
    }
}
