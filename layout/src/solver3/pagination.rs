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
            &self.header_text,
            self.header_page_number,
            self.header_total_pages,
            self.number_format,
        )
    }

    /// Build the `MarginBoxContent` for the footer.
    fn build_footer_content(&self) -> MarginBoxContent {
        Self::build_margin_content(
            &self.footer_text,
            self.footer_page_number,
            self.footer_total_pages,
            self.number_format,
        )
    }

    /// Shared helper for building header/footer margin box content.
    fn build_margin_content(
        text: &Option<String>,
        page_number: bool,
        total_pages: bool,
        number_format: CounterFormat,
    ) -> MarginBoxContent {
        let mut parts = Vec::new();

        if let Some(ref text) = text {
            parts.push(MarginBoxContent::Text(text.clone()));
            if page_number {
                parts.push(MarginBoxContent::Text(" - ".to_string()));
            }
        }

        if page_number {
            if number_format == CounterFormat::Decimal {
                parts.push(MarginBoxContent::Text("Page ".to_string()));
                parts.push(MarginBoxContent::PageCounter);
            } else {
                parts.push(MarginBoxContent::Text("Page ".to_string()));
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
