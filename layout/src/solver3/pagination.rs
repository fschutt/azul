//! CSS Paged Media Pagination Engine - "Infinite Canvas with Physical Spacers"
//!
//! This module implements a pagination architecture where content is laid out
//! on a single "infinite" vertical canvas, with "dead zones" representing page
//! breaks (including headers, footers, and margins).
//!
//! ## Core Concept: Physical Spacers
//!
//! Instead of assigning nodes to logical pages, we map pages onto a single vertical
//! coordinate system where page breaks are physical empty spaces:
//!
//! ```text
//! 0px      ─────────────────────────────
//!          │ Page 1 Content             │
//! 1000px   ─────────────────────────────
//!          │ Dead Space (Footer+Margin) │  ← Page break zone
//! 1100px   ─────────────────────────────
//!          │ Page 2 Content             │
//! 2100px   ─────────────────────────────
//!          │ Dead Space (Footer+Margin) │
//! 2200px   ─────────────────────────────
//! ```
//!
//! ## Benefits
//!
//! 1. **No coordinate desynchronization**: Nodes don't need to track which page they're on
//! 2. **Simple background splitting**: A tall element just gets clipped at page boundaries
//! 3. **Flex/Grid compatibility**: Containers can use page height for `height: 100%`
//! 4. **Simple rendering**: Just clip and translate the display list per page
//!
//! ## CSS Break Properties
//!
//! - `break-before: page` - Force page break before element
//! - `break-after: page` - Force page break after element  
//! - `break-inside: avoid` - Avoid breaking inside element (move to next page if needed)
//! - `orphans`/`widows` - Minimum lines at start/end of page (for text)
//!
//! ## CSS Generated Content for Paged Media (GCPM) Level 3 Support
//!
//! This module provides the foundation for CSS GCPM Level 3 features:
//! - **Running Elements** (`position: running(name)`) - Elements extracted from flow
//!   and displayed in margin boxes (headers/footers)
//! - **Page Selectors** (`@page :first`, `@page :left/:right`) - Per-page styling
//! - **Named Strings** (`string-set`, `content: string(name)`) - Captured text for headers
//! - **Page Counters** (`counter(page)`, `counter(pages)`) - Page numbering
//!
//! See: https://www.w3.org/TR/css-gcpm-3/
//!
//! ## Current Implementation Status
//!
//! **NOTE**: Full CSS `@page` parsing is not yet implemented. This module provides
//! a "fake" configuration API (`FakePageConfig`) that allows programmatic control
//! over page headers and footers. Currently supported features:
//!
//! - ✅ Page numbers in header/footer (`counter(page)`, `counter(pages)`)
//! - ✅ Custom text in header/footer
//! - ✅ Skip first page option
//! - ✅ Different formats (decimal, roman, alpha, greek)
//! - ⏳ Running elements (`position: running(name)`) - infrastructure ready
//! - ⏳ Named strings (`string-set`) - infrastructure ready
//! - ❌ Full `@page` CSS rule parsing
//! - ❌ Page selectors (`:first`, `:left`, `:right`, `:blank`)
//! - ❌ Margin box positioning (16 positions per CSS spec)

use std::collections::BTreeMap;
use std::sync::Arc;

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_css::props::layout::fragmentation::{PageBreak, BreakInside};
use azul_css::props::basic::ColorU;

/// Manages the infinite canvas coordinate system with page boundaries.
///
/// The `PageGeometer` tracks page dimensions and provides utilities for:
/// - Determining which page a Y coordinate falls on
/// - Calculating the next page start position
/// - Checking if content crosses page boundaries
#[derive(Debug, Clone)]
pub struct PageGeometer {
    /// Total height of each page (including margins, headers, footers)
    pub page_size: LogicalSize,
    /// Content area margins (space reserved at top/bottom of each page)
    pub page_margins: PageMargins,
    /// Height reserved for page header (if any)
    pub header_height: f32,
    /// Height reserved for page footer (if any)
    pub footer_height: f32,
    /// Current Y position on the infinite canvas
    pub current_y: f32,
}

/// Page margin configuration
#[derive(Debug, Clone, Copy, Default)]
pub struct PageMargins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl PageMargins {
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self { top, right, bottom, left }
    }
    
    pub fn uniform(margin: f32) -> Self {
        Self { top: margin, right: margin, bottom: margin, left: margin }
    }
}

impl PageGeometer {
    /// Create a new PageGeometer for paged layout.
    pub fn new(page_size: LogicalSize, margins: PageMargins) -> Self {
        Self {
            page_size,
            page_margins: margins,
            header_height: 0.0,
            footer_height: 0.0,
            current_y: 0.0,
        }
    }
    
    /// Create with header and footer space reserved.
    pub fn with_header_footer(mut self, header: f32, footer: f32) -> Self {
        self.header_height = header;
        self.footer_height = footer;
        self
    }
    
    /// Get the usable content height per page (page height minus margins/headers/footers).
    pub fn content_height(&self) -> f32 {
        self.page_size.height 
            - self.page_margins.top 
            - self.page_margins.bottom
            - self.header_height
            - self.footer_height
    }
    
    /// Get the usable content width per page (page width minus left/right margins).
    pub fn content_width(&self) -> f32 {
        self.page_size.width - self.page_margins.left - self.page_margins.right
    }
    
    /// Calculate which page a given Y coordinate falls on (0-indexed).
    pub fn page_for_y(&self, y: f32) -> usize {
        let content_h = self.content_height();
        if content_h <= 0.0 {
            return 0;
        }
        
        // Account for dead zones between pages
        let full_page_slot = content_h + self.dead_zone_height();
        (y / full_page_slot).floor() as usize
    }
    
    /// Get the Y coordinate where a page's content area starts.
    pub fn page_content_start_y(&self, page_index: usize) -> f32 {
        let full_page_slot = self.content_height() + self.dead_zone_height();
        page_index as f32 * full_page_slot
    }
    
    /// Get the Y coordinate where a page's content area ends.
    pub fn page_content_end_y(&self, page_index: usize) -> f32 {
        self.page_content_start_y(page_index) + self.content_height()
    }
    
    /// Get the height of the "dead zone" between pages (footer + margin + header of next page).
    pub fn dead_zone_height(&self) -> f32 {
        self.footer_height 
            + self.page_margins.bottom 
            + self.page_margins.top 
            + self.header_height
    }
    
    /// Calculate the Y coordinate where the NEXT page's content starts from a given position.
    pub fn next_page_start_y(&self, current_y: f32) -> f32 {
        let current_page = self.page_for_y(current_y);
        self.page_content_start_y(current_page + 1)
    }
    
    /// Check if a range [start_y, end_y) crosses a page boundary.
    pub fn crosses_page_break(&self, start_y: f32, end_y: f32) -> bool {
        let start_page = self.page_for_y(start_y);
        let end_page = self.page_for_y(end_y - 0.01); // Subtract epsilon for exclusive end
        start_page != end_page
    }
    
    /// Get remaining space on the current page from a given Y position.
    pub fn remaining_on_page(&self, y: f32) -> f32 {
        let page = self.page_for_y(y);
        let page_end = self.page_content_end_y(page);
        (page_end - y).max(0.0)
    }
    
    /// Check if content of given height can fit starting at Y position.
    pub fn can_fit(&self, y: f32, height: f32) -> bool {
        self.remaining_on_page(y) >= height
    }
    
    /// Calculate the additional Y offset needed to push content to the next page.
    /// Returns 0 if content fits on current page.
    pub fn page_break_offset(&self, y: f32, height: f32) -> f32 {
        if self.can_fit(y, height) {
            return 0.0;
        }
        
        // Content doesn't fit - calculate offset to move to next page
        let next_start = self.next_page_start_y(y);
        next_start - y
    }
    
    /// Get the number of pages needed to contain content ending at Y.
    pub fn page_count(&self, total_content_height: f32) -> usize {
        if total_content_height <= 0.0 {
            return 1;
        }
        self.page_for_y(total_content_height - 0.01) + 1
    }
}

/// CSS break behavior classification for a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakBehavior {
    /// Box can be split at internal break points (paragraphs, containers)
    Splittable,
    /// Box should be kept together if possible (break-inside: avoid)
    AvoidBreak,
    /// Box cannot be split (replaced elements, overflow:scroll, etc.)
    Monolithic,
}

/// Result of evaluating break properties for a box.
#[derive(Debug, Clone)]
pub struct BreakEvaluation {
    /// Whether to force a page break before this element
    pub force_break_before: bool,
    /// Whether to force a page break after this element  
    pub force_break_after: bool,
    /// How this box should behave at potential break points
    pub behavior: BreakBehavior,
    /// For text: minimum lines to keep at page start (orphans)
    pub orphans: u32,
    /// For text: minimum lines to keep at page end (widows)
    pub widows: u32,
}

impl Default for BreakEvaluation {
    fn default() -> Self {
        Self {
            force_break_before: false,
            force_break_after: false,
            behavior: BreakBehavior::Splittable,
            orphans: 2,
            widows: 2,
        }
    }
}

/// Check if a break-before/after value forces a page break.
pub fn is_forced_break(page_break: PageBreak) -> bool {
    matches!(
        page_break,
        PageBreak::Always | PageBreak::Page | PageBreak::Left | 
        PageBreak::Right | PageBreak::Recto | PageBreak::Verso | PageBreak::All
    )
}

/// Check if a break-before/after value avoids breaks.
pub fn is_avoid_break(page_break: PageBreak) -> bool {
    matches!(page_break, PageBreak::Avoid | PageBreak::AvoidPage)
}

/// Metadata about table header repetition for a specific page.
#[derive(Debug, Clone)]
pub struct RepeatedTableHeader {
    /// The Y position on the infinite canvas where this header should appear
    pub inject_at_y: f32,
    /// The display list items for the table header (cloned from original)
    pub header_items: Vec<usize>, // Indices into the original display list
    /// The height of the header
    pub header_height: f32,
}

/// Context for pagination during layout.
/// 
/// This is passed into layout functions to allow them to make page-aware decisions.
#[derive(Debug)]
pub struct PaginationContext<'a> {
    /// The page geometry calculator
    pub geometer: &'a PageGeometer,
    /// Accumulated break-inside: avoid depth from ancestors
    pub break_avoid_depth: usize,
    /// Track table headers that need to repeat on new pages
    pub repeated_headers: Vec<RepeatedTableHeader>,
}

impl<'a> PaginationContext<'a> {
    pub fn new(geometer: &'a PageGeometer) -> Self {
        Self {
            geometer,
            break_avoid_depth: 0,
            repeated_headers: Vec::new(),
        }
    }
    
    /// Enter a box with break-inside: avoid
    pub fn enter_avoid_break(&mut self) {
        self.break_avoid_depth += 1;
    }
    
    /// Exit a box with break-inside: avoid
    pub fn exit_avoid_break(&mut self) {
        self.break_avoid_depth = self.break_avoid_depth.saturating_sub(1);
    }
    
    /// Check if we're inside an ancestor with break-inside: avoid
    pub fn is_avoiding_breaks(&self) -> bool {
        self.break_avoid_depth > 0
    }
    
    /// Register a table header for repetition on subsequent pages.
    pub fn register_repeated_header(&mut self, inject_at_y: f32, header_items: Vec<usize>, header_height: f32) {
        self.repeated_headers.push(RepeatedTableHeader {
            inject_at_y,
            header_items,
            header_height,
        });
    }
}

/// Calculate the position adjustment for a child element considering pagination.
/// 
/// This is called during BFC/IFC layout to determine if content needs to be
/// pushed to the next page.
/// 
/// # Arguments
/// * `geometer` - Page geometry calculator
/// * `main_pen` - Current Y position in infinite canvas coordinates
/// * `child_height` - Estimated height of the child element
/// * `break_eval` - Break property evaluation for the child
/// * `is_avoiding_breaks` - Whether an ancestor has break-inside: avoid
/// 
/// # Returns
/// The Y offset to add to `main_pen` (0 if no adjustment needed, positive if pushing to next page)
pub fn calculate_pagination_offset(
    geometer: &PageGeometer,
    main_pen: f32,
    child_height: f32,
    break_eval: &BreakEvaluation,
    is_avoiding_breaks: bool,
) -> f32 {
    // 1. Handle forced break-before
    if break_eval.force_break_before {
        let remaining = geometer.remaining_on_page(main_pen);
        if remaining < geometer.content_height() {
            // Not at the start of a page - force break
            return geometer.page_break_offset(main_pen, f32::MAX);
        }
    }
    
    // 2. Check if content fits on current page
    let remaining = geometer.remaining_on_page(main_pen);
    
    // 3. Handle monolithic content (cannot be split)
    if break_eval.behavior == BreakBehavior::Monolithic {
        if child_height <= remaining {
            // Fits on current page
            return 0.0;
        }
        if child_height <= geometer.content_height() {
            // Doesn't fit but would fit on empty page - move to next
            return geometer.page_break_offset(main_pen, child_height);
        }
        // Too large for any page - let it overflow (no adjustment)
        return 0.0;
    }
    
    // 4. Handle avoid-break content
    if break_eval.behavior == BreakBehavior::AvoidBreak || is_avoiding_breaks {
        if child_height <= remaining {
            // Fits on current page
            return 0.0;
        }
        if child_height <= geometer.content_height() {
            // Move to next page to keep together
            return geometer.page_break_offset(main_pen, child_height);
        }
        // Too large to keep together - must allow splitting
    }
    
    // 5. Splittable content - check orphans/widows constraints
    // For now, just ensure we have at least some minimum space
    let min_before_break = 20.0; // ~1-2 lines minimum
    if remaining < min_before_break && remaining < geometer.content_height() {
        // Not enough space for even a small amount - move to next page
        return geometer.page_break_offset(main_pen, child_height);
    }
    
    0.0
}

// =============================================================================
// CSS GCPM Level 3: Running Elements & Page Margin Boxes
// =============================================================================
//
// This section provides infrastructure for CSS Generated Content for Paged Media
// Level 3 (https://www.w3.org/TR/css-gcpm-3/).
//
// Key concepts:
//
// 1. **Running Elements** - Elements with `position: running(header)` are removed
//    from the normal flow and available for display in page margin boxes.
//
// 2. **Page Margin Boxes** - 16 margin boxes around each page (@top-left, @top-center,
//    @top-right, @bottom-left, etc.) that can contain running elements or generated
//    content.
//
// 3. **Named Strings** - Text captured with `string-set: header content(text)` and
//    displayed with `content: string(header)`.
//
// 4. **Page Counters** - `counter(page)` and `counter(pages)` for page numbering.

/// Position of a margin box on a page (CSS GCPM margin box names).
/// 
/// CSS defines 16 margin boxes around the page content area:
/// ```text
/// ┌─────────┬─────────────────┬─────────┐
/// │top-left │   top-center    │top-right│
/// ├─────────┼─────────────────┼─────────┤
/// │         │                 │         │
/// │  left   │                 │  right  │
/// │  -top   │                 │  -top   │
/// │         │                 │         │
/// │  left   │    CONTENT      │  right  │
/// │-middle  │      AREA       │-middle  │
/// │         │                 │         │
/// │  left   │                 │  right  │
/// │-bottom  │                 │-bottom  │
/// │         │                 │         │
/// ├─────────┼─────────────────┼─────────┤
/// │bot-left │  bottom-center  │bot-right│
/// └─────────┴─────────────────┴─────────┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MarginBoxPosition {
    // Top row
    TopLeftCorner,
    TopLeft,
    TopCenter,
    TopRight,
    TopRightCorner,
    // Left column
    LeftTop,
    LeftMiddle,
    LeftBottom,
    // Right column
    RightTop,
    RightMiddle,
    RightBottom,
    // Bottom row
    BottomLeftCorner,
    BottomLeft,
    BottomCenter,
    BottomRight,
    BottomRightCorner,
}

impl MarginBoxPosition {
    /// Returns true if this margin box is in the top margin area.
    pub fn is_top(&self) -> bool {
        matches!(self, 
            Self::TopLeftCorner | Self::TopLeft | Self::TopCenter | 
            Self::TopRight | Self::TopRightCorner
        )
    }
    
    /// Returns true if this margin box is in the bottom margin area.
    pub fn is_bottom(&self) -> bool {
        matches!(self, 
            Self::BottomLeftCorner | Self::BottomLeft | Self::BottomCenter | 
            Self::BottomRight | Self::BottomRightCorner
        )
    }
}

/// A running element that was extracted from the document flow.
/// 
/// CSS GCPM allows elements to be "running" - removed from normal flow
/// and made available for display in page margin boxes.
/// 
/// ```css
/// h1 { position: running(chapter-title); }
/// @page { @top-center { content: element(chapter-title); } }
/// ```
#[derive(Debug, Clone)]
pub struct RunningElement {
    /// The name of this running element (e.g., "chapter-title")
    pub name: String,
    /// The display list items for this element (captured when encountered in flow)
    pub display_items: Vec<super::display_list::DisplayListItem>,
    /// The size of this element when rendered
    pub size: LogicalSize,
    /// Which page this element was defined on (for `running()` selector specificity)
    pub source_page: usize,
}

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
            Self::PageCounterFormatted { format } => f.debug_struct("PageCounterFormatted").field("format", format).finish(),
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
    pub fn format(&self, n: usize) -> String {
        match self {
            Self::Decimal => n.to_string(),
            Self::DecimalLeadingZero => format!("{:02}", n),
            Self::LowerRoman => to_roman(n, false),
            Self::UpperRoman => to_roman(n, true),
            Self::LowerAlpha => to_alpha(n, false),
            Self::UpperAlpha => to_alpha(n, true),
            Self::LowerGreek => to_greek(n),
        }
    }
}

/// Convert number to roman numerals.
fn to_roman(mut n: usize, uppercase: bool) -> String {
    if n == 0 { return "0".to_string(); }
    
    let numerals = [
        (1000, "m"), (900, "cm"), (500, "d"), (400, "cd"),
        (100, "c"), (90, "xc"), (50, "l"), (40, "xl"),
        (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")
    ];
    
    let mut result = String::new();
    for (value, numeral) in &numerals {
        while n >= *value {
            result.push_str(numeral);
            n -= value;
        }
    }
    
    if uppercase { result.to_uppercase() } else { result }
}

/// Convert number to alphabetic (a-z, aa-az, etc.).
fn to_alpha(n: usize, uppercase: bool) -> String {
    if n == 0 { return "0".to_string(); }
    
    let mut result = String::new();
    let mut remaining = n;
    
    while remaining > 0 {
        remaining -= 1;
        let c = ((remaining % 26) as u8 + if uppercase { b'A' } else { b'a' }) as char;
        result.insert(0, c);
        remaining /= 26;
    }
    
    result
}

/// Convert number to Greek letters (α, β, γ, ...).
fn to_greek(n: usize) -> String {
    const GREEK: &[char] = &['α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 
                             'λ', 'μ', 'ν', 'ξ', 'ο', 'π', 'ρ', 'σ', 'τ', 'υ',
                             'φ', 'χ', 'ψ', 'ω'];
    if n == 0 { return "0".to_string(); }
    if n <= GREEK.len() { return GREEK[n - 1].to_string(); }
    
    // For numbers > 24, use αα, αβ, etc.
    let mut result = String::new();
    let mut remaining = n;
    while remaining > 0 {
        remaining -= 1;
        result.insert(0, GREEK[remaining % GREEK.len()]);
        remaining /= GREEK.len();
    }
    result
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
    /// Create PageInfo for a specific page.
    pub fn new(page_number: usize, total_pages: usize) -> Self {
        Self {
            page_number,
            total_pages,
            is_first: page_number == 1,
            is_last: total_pages > 0 && page_number == total_pages,
            is_left: page_number % 2 == 0,  // Even pages are left (verso)
            is_right: page_number % 2 == 1, // Odd pages are right (recto)
            is_blank: false,
        }
    }
}

/// Configuration for page headers and footers.
/// 
/// This is a simplified interface for the common case of adding
/// headers and footers. For full GCPM support, use `PageTemplate`.
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
            header_height: 30.0,
            footer_height: 30.0,
            header_content: MarginBoxContent::None,
            footer_content: MarginBoxContent::None,
            font_size: 10.0,
            text_color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            skip_first_page: false,
        }
    }
}

impl HeaderFooterConfig {
    /// Create a config with page numbers in the footer.
    pub fn with_page_numbers() -> Self {
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
    pub fn with_header_and_footer_page_numbers() -> Self {
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
    pub fn with_header_text(mut self, text: impl Into<String>) -> Self {
        self.show_header = true;
        self.header_content = MarginBoxContent::Text(text.into());
        self
    }
    
    /// Set custom footer text.
    pub fn with_footer_text(mut self, text: impl Into<String>) -> Self {
        self.show_footer = true;
        self.footer_content = MarginBoxContent::Text(text.into());
        self
    }
    
    /// Generate the text content for a margin box given page info.
    pub fn generate_content(&self, content: &MarginBoxContent, info: PageInfo) -> String {
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
            MarginBoxContent::Combined(parts) => {
                parts.iter()
                    .map(|p| self.generate_content(p, info))
                    .collect()
            }
            MarginBoxContent::NamedString(name) => {
                // TODO: Look up named string from document context
                format!("[string:{}]", name)
            }
            MarginBoxContent::RunningElement(name) => {
                // Running elements are rendered as display items, not text
                format!("[element:{}]", name)
            }
            MarginBoxContent::Custom(f) => f(info),
        }
    }
    
    /// Get the header text for a specific page.
    pub fn header_text(&self, info: PageInfo) -> String {
        if !self.show_header {
            return String::new();
        }
        if self.skip_first_page && info.is_first {
            return String::new();
        }
        self.generate_content(&self.header_content, info)
    }
    
    /// Get the footer text for a specific page.
    pub fn footer_text(&self, info: PageInfo) -> String {
        if !self.show_footer {
            return String::new();
        }
        if self.skip_first_page && info.is_first {
            return String::new();
        }
        self.generate_content(&self.footer_content, info)
    }
}

/// Full page template with all 16 margin boxes (CSS GCPM @page support).
/// 
/// This provides complete control over page layout following the CSS
/// Paged Media and GCPM specifications.
#[derive(Debug, Clone, Default)]
pub struct PageTemplate {
    /// Content for each margin box position
    pub margin_boxes: BTreeMap<MarginBoxPosition, MarginBoxContent>,
    /// Page margins (space allocated for margin boxes)
    pub margins: PageMargins,
    /// Named strings captured from the document
    pub named_strings: BTreeMap<String, String>,
    /// Running elements available for this page
    pub running_elements: BTreeMap<String, RunningElement>,
}

impl PageTemplate {
    /// Create a new empty page template.
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set content for a specific margin box.
    pub fn set_margin_box(&mut self, position: MarginBoxPosition, content: MarginBoxContent) {
        self.margin_boxes.insert(position, content);
    }
    
    /// Create a simple template with centered page numbers in the footer.
    pub fn with_centered_page_numbers() -> Self {
        let mut template = Self::new();
        template.set_margin_box(
            MarginBoxPosition::BottomCenter,
            MarginBoxContent::PageCounter,
        );
        template
    }
    
    /// Create a template with "Page X of Y" in the bottom right.
    pub fn with_page_x_of_y() -> Self {
        let mut template = Self::new();
        template.set_margin_box(
            MarginBoxPosition::BottomRight,
            MarginBoxContent::Combined(vec![
                MarginBoxContent::Text("Page ".to_string()),
                MarginBoxContent::PageCounter,
                MarginBoxContent::Text(" of ".to_string()),
                MarginBoxContent::PagesCounter,
            ]),
        );
        template
    }
}

// ============================================================================
// FAKE @PAGE SUPPORT
// ============================================================================
//
// The following structures provide a programmatic API to configure page headers
// and footers WITHOUT full CSS @page rule parsing. This is a temporary solution
// until proper CSS @page support is implemented.
//
// Usage example:
// ```rust
// let config = FakePageConfig::new()
//     .with_footer_page_numbers()
//     .with_header_text("My Document")
//     .skip_first_page(true);
//
// let header_footer = config.to_header_footer_config();
// ```

/// Temporary configuration for page headers/footers without CSS @page parsing.
///
/// This is a "fake" implementation that provides programmatic control over
/// page decoration until full CSS `@page` rule support is implemented.
///
/// ## Supported Features
///
/// - Page numbers in header and/or footer
/// - Custom text in header and/or footer
/// - Number format (decimal, roman numerals, alphabetic, greek)
/// - Skip first page option
///
/// ## Not Yet Supported
///
/// - CSS `@page` rule parsing
/// - Page selectors (`:first`, `:left`, `:right`, `:blank`)
/// - Running elements (`position: running(name)`)
/// - Named strings (`string-set`)
/// - Full margin box positioning (16 positions)
///
/// ## Example
///
/// ```rust,ignore
/// use azul_layout::solver3::pagination::FakePageConfig;
///
/// // Simple footer with "Page X of Y"
/// let config = FakePageConfig::new().with_footer_page_numbers();
///
/// // Custom header and footer
/// let config = FakePageConfig::new()
///     .with_header_text("Company Report 2024")
///     .with_footer_page_numbers()
///     .skip_first_page(true);
///
/// // Roman numeral page numbers
/// let config = FakePageConfig::new()
///     .with_footer_page_numbers()
///     .with_number_format(CounterFormat::LowerRoman);
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
            header_height: 30.0,
            footer_height: 30.0,
            font_size: 10.0,
            text_color: ColorU { r: 0, g: 0, b: 0, a: 255 },
        }
    }
}

impl FakePageConfig {
    /// Create a new empty configuration (no headers/footers).
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Enable footer with "Page X of Y" format.
    pub fn with_footer_page_numbers(mut self) -> Self {
        self.show_footer = true;
        self.footer_page_number = true;
        self.footer_total_pages = true;
        self
    }
    
    /// Enable header with "Page X" format.
    pub fn with_header_page_numbers(mut self) -> Self {
        self.show_header = true;
        self.header_page_number = true;
        self
    }
    
    /// Enable both header and footer with page numbers.
    pub fn with_header_and_footer_page_numbers(mut self) -> Self {
        self.show_header = true;
        self.show_footer = true;
        self.header_page_number = true;
        self.footer_page_number = true;
        self.footer_total_pages = true;
        self
    }
    
    /// Set custom header text.
    pub fn with_header_text(mut self, text: impl Into<String>) -> Self {
        self.show_header = true;
        self.header_text = Some(text.into());
        self
    }
    
    /// Set custom footer text.
    pub fn with_footer_text(mut self, text: impl Into<String>) -> Self {
        self.show_footer = true;
        self.footer_text = Some(text.into());
        self
    }
    
    /// Set the number format for page counters.
    pub fn with_number_format(mut self, format: CounterFormat) -> Self {
        self.number_format = format;
        self
    }
    
    /// Skip header/footer on the first page.
    pub fn skip_first_page(mut self, skip: bool) -> Self {
        self.skip_first_page = skip;
        self
    }
    
    /// Set header height.
    pub fn with_header_height(mut self, height: f32) -> Self {
        self.header_height = height;
        self
    }
    
    /// Set footer height.
    pub fn with_footer_height(mut self, height: f32) -> Self {
        self.footer_height = height;
        self
    }
    
    /// Set font size for header/footer text.
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }
    
    /// Set text color for header/footer.
    pub fn with_text_color(mut self, color: ColorU) -> Self {
        self.text_color = color;
        self
    }
    
    /// Convert this fake config to the internal HeaderFooterConfig.
    ///
    /// This is the bridge between the user-facing API and the internal
    /// pagination engine.
    pub fn to_header_footer_config(&self) -> HeaderFooterConfig {
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
    
    /// Build the MarginBoxContent for the header.
    fn build_header_content(&self) -> MarginBoxContent {
        let mut parts = Vec::new();
        
        // Add custom text if present
        if let Some(ref text) = self.header_text {
            parts.push(MarginBoxContent::Text(text.clone()));
            if self.header_page_number {
                parts.push(MarginBoxContent::Text(" - ".to_string()));
            }
        }
        
        // Add page number if enabled
        if self.header_page_number {
            if self.number_format == CounterFormat::Decimal {
                parts.push(MarginBoxContent::Text("Page ".to_string()));
                parts.push(MarginBoxContent::PageCounter);
            } else {
                parts.push(MarginBoxContent::Text("Page ".to_string()));
                parts.push(MarginBoxContent::PageCounterFormatted { 
                    format: self.number_format 
                });
            }
            
            if self.header_total_pages {
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
    
    /// Build the MarginBoxContent for the footer.
    fn build_footer_content(&self) -> MarginBoxContent {
        let mut parts = Vec::new();
        
        // Add custom text if present
        if let Some(ref text) = self.footer_text {
            parts.push(MarginBoxContent::Text(text.clone()));
            if self.footer_page_number {
                parts.push(MarginBoxContent::Text(" - ".to_string()));
            }
        }
        
        // Add page number if enabled
        if self.footer_page_number {
            if self.number_format == CounterFormat::Decimal {
                parts.push(MarginBoxContent::Text("Page ".to_string()));
                parts.push(MarginBoxContent::PageCounter);
            } else {
                parts.push(MarginBoxContent::Text("Page ".to_string()));
                parts.push(MarginBoxContent::PageCounterFormatted { 
                    format: self.number_format 
                });
            }
            
            if self.footer_total_pages {
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
    pub fn new() -> Self {
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
    pub fn get_repeated_headers_for_page(
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
mod tests {
    use super::*;
    
    #[test]
    fn test_page_geometer_basic() {
        let geometer = PageGeometer::new(
            LogicalSize::new(600.0, 800.0),
            PageMargins::uniform(50.0),
        );
        
        // Content height = 800 - 50 - 50 = 700
        assert_eq!(geometer.content_height(), 700.0);
        assert_eq!(geometer.content_width(), 500.0);
    }
    
    #[test]
    fn test_page_for_y() {
        let geometer = PageGeometer::new(
            LogicalSize::new(600.0, 1000.0),
            PageMargins::new(0.0, 0.0, 0.0, 0.0),
        );
        
        // With no margins/headers, content height = 1000, dead zone = 0
        assert_eq!(geometer.page_for_y(0.0), 0);
        assert_eq!(geometer.page_for_y(500.0), 0);
        assert_eq!(geometer.page_for_y(999.0), 0);
        assert_eq!(geometer.page_for_y(1000.0), 1);
        assert_eq!(geometer.page_for_y(1500.0), 1);
    }
    
    #[test]
    fn test_page_for_y_with_margins() {
        let geometer = PageGeometer::new(
            LogicalSize::new(600.0, 1000.0),
            PageMargins::new(50.0, 0.0, 50.0, 0.0),
        );
        
        // Content height = 1000 - 50 - 50 = 900
        // Dead zone = 50 + 50 = 100
        // Full page slot = 900 + 100 = 1000
        assert_eq!(geometer.content_height(), 900.0);
        assert_eq!(geometer.dead_zone_height(), 100.0);
        
        assert_eq!(geometer.page_for_y(0.0), 0);
        assert_eq!(geometer.page_for_y(899.0), 0);
        assert_eq!(geometer.page_for_y(900.0), 0); // In dead zone, still page 0
        assert_eq!(geometer.page_for_y(1000.0), 1); // Page 1 content start
    }
    
    #[test]
    fn test_crosses_page_break() {
        let geometer = PageGeometer::new(
            LogicalSize::new(600.0, 1000.0),
            PageMargins::uniform(0.0),
        );
        
        assert!(!geometer.crosses_page_break(0.0, 500.0));
        assert!(!geometer.crosses_page_break(0.0, 1000.0));
        assert!(geometer.crosses_page_break(0.0, 1001.0));
        assert!(geometer.crosses_page_break(500.0, 1500.0));
    }
    
    #[test]
    fn test_pagination_offset() {
        let geometer = PageGeometer::new(
            LogicalSize::new(600.0, 1000.0),
            PageMargins::uniform(0.0),
        );
        
        let default_break = BreakEvaluation::default();
        
        // Content fits on current page
        let offset = calculate_pagination_offset(&geometer, 0.0, 500.0, &default_break, false);
        assert_eq!(offset, 0.0);
        
        // Content doesn't fit, is splittable - no forced move
        let offset = calculate_pagination_offset(&geometer, 800.0, 500.0, &default_break, false);
        assert_eq!(offset, 0.0); // Will be split
        
        // Monolithic content that doesn't fit but would fit on empty page
        let monolithic_break = BreakEvaluation {
            behavior: BreakBehavior::Monolithic,
            ..Default::default()
        };
        let offset = calculate_pagination_offset(&geometer, 800.0, 500.0, &monolithic_break, false);
        assert_eq!(offset, 200.0); // Push to next page
    }
    
    #[test]
    fn test_forced_break_before() {
        let geometer = PageGeometer::new(
            LogicalSize::new(600.0, 1000.0),
            PageMargins::uniform(0.0),
        );
        
        let forced_break = BreakEvaluation {
            force_break_before: true,
            ..Default::default()
        };
        
        // Already at page start - no extra offset
        let offset = calculate_pagination_offset(&geometer, 0.0, 100.0, &forced_break, false);
        assert_eq!(offset, 0.0);
        
        // In middle of page - force to next page
        let offset = calculate_pagination_offset(&geometer, 500.0, 100.0, &forced_break, false);
        assert_eq!(offset, 500.0); // Push to start of next page
    }
    
    #[test]
    fn test_table_header_tracker_basic() {
        let mut tracker = TableHeaderTracker::new();
        
        // Register a table that spans pages 0-2 (Y: 100-2500)
        tracker.register_table_header(TableHeaderInfo {
            table_node_index: 0,
            table_start_y: 100.0,
            table_end_y: 2500.0,
            thead_items: vec![], // Empty for test
            thead_height: 50.0,
            thead_offset_y: 0.0,
        });
        
        // Page 0: table starts here, no repeated header needed
        let headers = tracker.get_repeated_headers_for_page(0, 0.0, 1000.0);
        assert!(headers.is_empty(), "Page 0 should not have repeated header (table starts here)");
        
        // Page 1: table continues, need repeated header
        let headers = tracker.get_repeated_headers_for_page(1, 1000.0, 2000.0);
        assert_eq!(headers.len(), 1, "Page 1 should have repeated header");
        assert_eq!(headers[0].2, 50.0, "Header height should be 50.0");
        
        // Page 2: table continues, need repeated header
        let headers = tracker.get_repeated_headers_for_page(2, 2000.0, 3000.0);
        assert_eq!(headers.len(), 1, "Page 2 should have repeated header");
        
        // Page 3: table ends at 2500, but we're past that
        let headers = tracker.get_repeated_headers_for_page(3, 3000.0, 4000.0);
        assert!(headers.is_empty(), "Page 3 should not have repeated header (table ended)");
    }
    
    #[test]
    fn test_table_header_tracker_multiple_tables() {
        let mut tracker = TableHeaderTracker::new();
        
        // Table 1: spans Y 100-1500
        tracker.register_table_header(TableHeaderInfo {
            table_node_index: 0,
            table_start_y: 100.0,
            table_end_y: 1500.0,
            thead_items: vec![],
            thead_height: 40.0,
            thead_offset_y: 0.0,
        });
        
        // Table 2: spans Y 2000-4000
        tracker.register_table_header(TableHeaderInfo {
            table_node_index: 1,
            table_start_y: 2000.0,
            table_end_y: 4000.0,
            thead_items: vec![],
            thead_height: 60.0,
            thead_offset_y: 0.0,
        });
        
        // Page 0 (0-1000): Table 1 starts here, no repeat
        let headers = tracker.get_repeated_headers_for_page(0, 0.0, 1000.0);
        assert!(headers.is_empty());
        
        // Page 1 (1000-2000): Table 1 continues (needs repeat), Table 2 starts here
        let headers = tracker.get_repeated_headers_for_page(1, 1000.0, 2000.0);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].2, 40.0); // Table 1 header
        
        // Page 2 (2000-3000): Table 1 ended, Table 2 starts here (no repeat for 2)
        let headers = tracker.get_repeated_headers_for_page(2, 2000.0, 3000.0);
        assert!(headers.is_empty());
        
        // Page 3 (3000-4000): Table 2 continues (needs repeat)
        let headers = tracker.get_repeated_headers_for_page(3, 3000.0, 4000.0);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].2, 60.0); // Table 2 header
    }
    
    #[test]
    fn test_header_footer_config() {
        // Default config should not show headers/footers
        let config = HeaderFooterConfig::default();
        assert!(!config.show_header);
        assert!(!config.show_footer);
        
        // with_page_numbers() enables only footer
        let config = HeaderFooterConfig::with_page_numbers();
        assert!(!config.show_header);
        assert!(config.show_footer);
        
        // with_header_and_footer_page_numbers() enables both
        let config = HeaderFooterConfig::with_header_and_footer_page_numbers();
        assert!(config.show_header);
        assert!(config.show_footer);
        
        // Test page info generation
        let page_info = PageInfo::new(1, 10);
        assert!(page_info.is_first);
        assert!(!page_info.is_last);
        assert_eq!(page_info.page_number, 1);
        assert_eq!(page_info.total_pages, 10);
        
        let page_info = PageInfo::new(10, 10);
        assert!(!page_info.is_first);
        assert!(page_info.is_last);
    }
    
    #[test]
    fn test_counter_format() {
        assert_eq!(CounterFormat::Decimal.format(1), "1");
        assert_eq!(CounterFormat::Decimal.format(10), "10");
        
        assert_eq!(CounterFormat::LowerRoman.format(1), "i");
        assert_eq!(CounterFormat::LowerRoman.format(4), "iv");
        assert_eq!(CounterFormat::LowerRoman.format(9), "ix");
        
        assert_eq!(CounterFormat::UpperRoman.format(1), "I");
        assert_eq!(CounterFormat::UpperRoman.format(50), "L");
        
        assert_eq!(CounterFormat::LowerAlpha.format(1), "a");
        assert_eq!(CounterFormat::LowerAlpha.format(26), "z");
        assert_eq!(CounterFormat::LowerAlpha.format(27), "aa");
        
        assert_eq!(CounterFormat::UpperAlpha.format(1), "A");
        assert_eq!(CounterFormat::UpperAlpha.format(26), "Z");
        
        assert_eq!(CounterFormat::LowerGreek.format(1), "α");
        assert_eq!(CounterFormat::LowerGreek.format(2), "β");
    }
    
    #[test]
    fn test_fake_page_config_default() {
        let config = FakePageConfig::new();
        assert!(!config.show_header);
        assert!(!config.show_footer);
        
        let hf = config.to_header_footer_config();
        assert!(!hf.show_header);
        assert!(!hf.show_footer);
    }
    
    #[test]
    fn test_fake_page_config_footer_page_numbers() {
        let config = FakePageConfig::new().with_footer_page_numbers();
        assert!(!config.show_header);
        assert!(config.show_footer);
        assert!(config.footer_page_number);
        assert!(config.footer_total_pages);
        
        let hf = config.to_header_footer_config();
        assert!(hf.show_footer);
        
        // Test footer text generation
        let page_info = PageInfo::new(3, 10);
        let footer_text = hf.footer_text(page_info);
        assert_eq!(footer_text, "Page 3 of 10");
    }
    
    #[test]
    fn test_fake_page_config_header_and_footer() {
        let config = FakePageConfig::new()
            .with_header_text("My Document")
            .with_footer_page_numbers()
            .skip_first_page(true);
        
        assert!(config.show_header);
        assert!(config.show_footer);
        assert!(config.skip_first_page);
        
        let hf = config.to_header_footer_config();
        assert!(hf.skip_first_page);
        
        // First page should skip header/footer
        let page_info = PageInfo::new(1, 5);
        assert!(hf.header_text(page_info).is_empty());
        assert!(hf.footer_text(page_info).is_empty());
        
        // Second page should show header/footer
        let page_info = PageInfo::new(2, 5);
        assert_eq!(hf.header_text(page_info), "My Document");
        assert_eq!(hf.footer_text(page_info), "Page 2 of 5");
    }
    
    #[test]
    fn test_fake_page_config_roman_numerals() {
        let config = FakePageConfig::new()
            .with_footer_page_numbers()
            .with_number_format(CounterFormat::LowerRoman);
        
        let hf = config.to_header_footer_config();
        
        let page_info = PageInfo::new(4, 10);
        let footer_text = hf.footer_text(page_info);
        assert_eq!(footer_text, "Page iv of 10"); // Note: total_pages still decimal
    }
    
    #[test]
    fn test_fake_page_config_combined_header() {
        let config = FakePageConfig::new()
            .with_header_text("Report 2024")
            .with_header_page_numbers();
        
        let hf = config.to_header_footer_config();
        
        let page_info = PageInfo::new(1, 5);
        let header_text = hf.header_text(page_info);
        assert_eq!(header_text, "Report 2024 - Page 1");
    }
}
