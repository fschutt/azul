//! CSS Fragmentation Engine for Paged Media
//!
//! This module implements the CSS Fragmentation specification (css-break-3) for
//! breaking content across pages, columns, or regions.
//!
//! ## Key Concepts
//!
//! - **Fragmentainer**: A container (page, column, region) that holds a portion of content
//! - **FragmentationContext**: Tracks layout state during fragmentation
//! - **BoxBreakBehavior**: How a box should be handled at page breaks
//! - **PageTemplate**: Headers, footers, and running content for pages
//!
//! ## Algorithm Overview
//!
//! Unlike post-layout splitting, this module integrates fragmentation INTO layout:
//!
//! 1. Classify each box's break behavior (splittable, keep-together, monolithic)
//! 2. During layout, check if content fits in current fragmentainer
//! 3. Apply break-before/break-after rules
//! 4. Split or defer content as needed
//! 5. Handle orphans/widows for text content

use alloc::{boxed::Box, collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use core::fmt;

use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::props::layout::fragmentation::{
    BoxDecorationBreak, BreakInside, Orphans, PageBreak, Widows,
};

#[cfg(all(feature = "text_layout", feature = "font_loading"))]
use crate::solver3::display_list::{DisplayList, DisplayListItem};

// Stub types when text_layout or font_loading is disabled
#[cfg(not(all(feature = "text_layout", feature = "font_loading")))]
#[derive(Debug, Clone, Default)]
pub struct DisplayList {
    pub items: Vec<DisplayListItem>,
}

#[cfg(not(all(feature = "text_layout", feature = "font_loading")))]
#[derive(Debug, Clone)]
pub struct DisplayListItem;

// Page Templates (Headers, Footers, Running Content)

/// Counter that tracks page numbers and other running content
#[derive(Debug, Clone)]
pub struct PageCounter {
    /// Current page number (1-indexed)
    pub page_number: usize,
    /// Total page count (may be unknown during first pass)
    pub total_pages: Option<usize>,
    /// Chapter or section number
    pub chapter: Option<usize>,
    /// Custom named counters (CSS counter() function)
    pub named_counters: BTreeMap<String, i32>,
}

impl Default for PageCounter {
    fn default() -> Self {
        Self {
            page_number: 1,
            total_pages: None,
            chapter: None,
            named_counters: BTreeMap::new(),
        }
    }
}

impl PageCounter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_page_number(mut self, page: usize) -> Self {
        self.page_number = page;
        self
    }

    pub fn with_total_pages(mut self, total: usize) -> Self {
        self.total_pages = Some(total);
        self
    }

    /// Format page number as string (e.g., "3", "iii", "C")
    pub fn format_page_number(&self, style: PageNumberStyle) -> String {
        match style {
            PageNumberStyle::Decimal => format!("{}", self.page_number),
            PageNumberStyle::LowerRoman => to_lower_roman(self.page_number),
            PageNumberStyle::UpperRoman => to_upper_roman(self.page_number),
            PageNumberStyle::LowerAlpha => to_lower_alpha(self.page_number),
            PageNumberStyle::UpperAlpha => to_upper_alpha(self.page_number),
        }
    }

    /// Get "Page X of Y" string
    pub fn format_page_of_total(&self) -> String {
        match self.total_pages {
            Some(total) => format!("Page {} of {}", self.page_number, total),
            None => format!("Page {}", self.page_number),
        }
    }
}

/// Style for page number formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageNumberStyle {
    /// 1, 2, 3, ...
    Decimal,
    /// i, ii, iii, iv, ...
    LowerRoman,
    /// I, II, III, IV, ...
    UpperRoman,
    /// a, b, c, ..., z, aa, ab, ...
    LowerAlpha,
    /// A, B, C, ..., Z, AA, AB, ...
    UpperAlpha,
}

impl Default for PageNumberStyle {
    fn default() -> Self {
        Self::Decimal
    }
}

/// Slot position for dynamic content in page template
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSlotPosition {
    /// Top-left corner
    TopLeft,
    /// Top-center
    TopCenter,
    /// Top-right corner
    TopRight,
    /// Bottom-left corner
    BottomLeft,
    /// Bottom-center
    BottomCenter,
    /// Bottom-right corner
    BottomRight,
}

/// Content that can be placed in a page template slot
#[derive(Clone)]
pub enum PageSlotContent {
    /// Static text
    Text(String),
    /// Page number with formatting
    PageNumber(PageNumberStyle),
    /// "Page X of Y"
    PageOfTotal,
    /// Chapter/section title (from running headers)
    RunningHeader(String),
    /// Custom function that generates content per page
    Dynamic(Arc<DynamicSlotContentFn>),
}

/// Wrapper for dynamic slot content functions to allow Debug impl
pub struct DynamicSlotContentFn {
    func: Box<dyn Fn(&PageCounter) -> String + Send + Sync>,
}

impl DynamicSlotContentFn {
    pub fn new<F: Fn(&PageCounter) -> String + Send + Sync + 'static>(f: F) -> Self {
        Self { func: Box::new(f) }
    }

    pub fn call(&self, counter: &PageCounter) -> String {
        (self.func)(counter)
    }
}

impl fmt::Debug for DynamicSlotContentFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<dynamic content fn>")
    }
}

impl fmt::Debug for PageSlotContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageSlotContent::Text(s) => write!(f, "Text({:?})", s),
            PageSlotContent::PageNumber(style) => write!(f, "PageNumber({:?})", style),
            PageSlotContent::PageOfTotal => write!(f, "PageOfTotal"),
            PageSlotContent::RunningHeader(s) => write!(f, "RunningHeader({:?})", s),
            PageSlotContent::Dynamic(_) => write!(f, "Dynamic(<fn>)"),
        }
    }
}

/// A slot in the page template
#[derive(Debug, Clone)]
pub struct PageSlot {
    /// Position of this slot
    pub position: PageSlotPosition,
    /// Content to display
    pub content: PageSlotContent,
    /// Font size in points (optional override)
    pub font_size_pt: Option<f32>,
    /// Color (optional override)
    pub color: Option<azul_css::props::basic::ColorU>,
}

/// Template for page headers, footers, and margins
#[derive(Debug, Clone)]
pub struct PageTemplate {
    /// Header height in points (0 = no header)
    pub header_height: f32,
    /// Footer height in points (0 = no footer)
    pub footer_height: f32,
    /// Slots for dynamic content
    pub slots: Vec<PageSlot>,
    /// Whether to show header on first page
    pub header_on_first_page: bool,
    /// Whether to show footer on first page
    pub footer_on_first_page: bool,
    /// Different template for left (even) pages
    pub left_page_slots: Option<Vec<PageSlot>>,
    /// Different template for right (odd) pages  
    pub right_page_slots: Option<Vec<PageSlot>>,
}

impl Default for PageTemplate {
    fn default() -> Self {
        Self {
            header_height: 0.0,
            footer_height: 0.0,
            slots: Vec::new(),
            header_on_first_page: true,
            footer_on_first_page: true,
            left_page_slots: None,
            right_page_slots: None,
        }
    }
}

impl PageTemplate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a simple page number footer (centered)
    pub fn with_page_number_footer(mut self, height: f32) -> Self {
        self.footer_height = height;
        self.slots.push(PageSlot {
            position: PageSlotPosition::BottomCenter,
            content: PageSlotContent::PageNumber(PageNumberStyle::Decimal),
            font_size_pt: Some(10.0),
            color: None,
        });
        self
    }

    /// Add "Page X of Y" footer
    pub fn with_page_of_total_footer(mut self, height: f32) -> Self {
        self.footer_height = height;
        self.slots.push(PageSlot {
            position: PageSlotPosition::BottomCenter,
            content: PageSlotContent::PageOfTotal,
            font_size_pt: Some(10.0),
            color: None,
        });
        self
    }

    /// Add a header with title on left and page number on right
    pub fn with_book_header(mut self, title: String, height: f32) -> Self {
        self.header_height = height;
        self.slots.push(PageSlot {
            position: PageSlotPosition::TopLeft,
            content: PageSlotContent::Text(title),
            font_size_pt: Some(10.0),
            color: None,
        });
        self.slots.push(PageSlot {
            position: PageSlotPosition::TopRight,
            content: PageSlotContent::PageNumber(PageNumberStyle::Decimal),
            font_size_pt: Some(10.0),
            color: None,
        });
        self
    }

    /// Get slots for a specific page (handles left/right page differences)
    pub fn slots_for_page(&self, page_number: usize) -> &[PageSlot] {
        let is_left_page = page_number % 2 == 0;
        if is_left_page {
            if let Some(ref left_slots) = self.left_page_slots {
                return left_slots;
            }
        } else {
            if let Some(ref right_slots) = self.right_page_slots {
                return right_slots;
            }
        }
        &self.slots
    }

    /// Check if header should be shown on this page
    pub fn show_header(&self, page_number: usize) -> bool {
        if page_number == 1 && !self.header_on_first_page {
            return false;
        }
        self.header_height > 0.0
    }

    /// Check if footer should be shown on this page
    pub fn show_footer(&self, page_number: usize) -> bool {
        if page_number == 1 && !self.footer_on_first_page {
            return false;
        }
        self.footer_height > 0.0
    }

    /// Get the content area height (page height minus header and footer)
    pub fn content_area_height(&self, page_height: f32, page_number: usize) -> f32 {
        let header = if self.show_header(page_number) {
            self.header_height
        } else {
            0.0
        };
        let footer = if self.show_footer(page_number) {
            self.footer_height
        } else {
            0.0
        };
        page_height - header - footer
    }
}

// Box Break Behavior Classification

/// How a box should behave at fragmentation breaks
#[derive(Debug, Clone)]
pub enum BoxBreakBehavior {
    /// Can be split at any internal break point (paragraphs, containers)
    Splittable {
        /// Minimum content height before a break (orphans-like)
        min_before_break: f32,
        /// Minimum content height after a break (widows-like)
        min_after_break: f32,
    },
    /// Should be kept together if possible (headers, small blocks)
    KeepTogether {
        /// Estimated total height of this box
        estimated_height: f32,
        /// Priority level (higher = more important to keep together)
        priority: KeepTogetherPriority,
    },
    /// Cannot be split (images, replaced elements, overflow:scroll)
    Monolithic {
        /// Fixed height of this element
        height: f32,
    },
}

/// Priority for keeping content together
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeepTogetherPriority {
    /// Low priority - can break if needed
    Low = 0,
    /// Normal priority (default for break-inside: avoid)
    Normal = 1,
    /// High priority (headers with following content)
    High = 2,
    /// Critical (figure with caption, table headers)
    Critical = 3,
}

/// Information about a potential break point
#[derive(Debug, Clone)]
pub struct BreakPoint {
    /// Y position of this break point (in content coordinates)
    pub y_position: f32,
    /// Type of break point (Class A, B, or C)
    pub break_class: BreakClass,
    /// Break-before value at this point
    pub break_before: PageBreak,
    /// Break-after value at this point  
    pub break_after: PageBreak,
    /// Whether ancestors have break-inside: avoid
    pub ancestor_avoid_depth: usize,
    /// Node that precedes this break point
    pub preceding_node: Option<NodeId>,
    /// Node that follows this break point
    pub following_node: Option<NodeId>,
}

/// CSS Fragmentation break point class
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakClass {
    /// Between sibling block-level boxes
    ClassA,
    /// Between line boxes inside a block container
    ClassB,
    /// Between content edge and child margin edge
    ClassC,
}

impl BreakPoint {
    /// Check if this break point is allowed (respecting all break rules)
    pub fn is_allowed(&self) -> bool {
        // Rule 1: Check break-after/break-before
        if is_forced_break(&self.break_before) || is_forced_break(&self.break_after) {
            return true; // Forced breaks are always allowed
        }

        if is_avoid_break(&self.break_before) || is_avoid_break(&self.break_after) {
            return false; // Avoid breaks
        }

        // Rule 2: Check ancestor break-inside: avoid
        if self.ancestor_avoid_depth > 0 {
            return false;
        }

        // Rules 3 & 4 are handled at a higher level (orphans/widows, etc.)
        true
    }

    /// Check if this is a forced break
    pub fn is_forced(&self) -> bool {
        is_forced_break(&self.break_before) || is_forced_break(&self.break_after)
    }
}

// Fragmentation Layout Context

/// A fragment of content placed on a specific page
#[derive(Debug)]
pub struct PageFragment {
    /// Which page this fragment belongs to (0-indexed)
    pub page_index: usize,
    /// Bounds of this fragment on the page (in page coordinates)
    pub bounds: LogicalRect,
    /// Display list items for this fragment
    pub items: Vec<DisplayListItem>,
    /// Node ID that this fragment belongs to
    pub source_node: Option<NodeId>,
    /// Whether this is a continuation from previous page
    pub is_continuation: bool,
    /// Whether this continues on the next page
    pub continues_on_next: bool,
}

/// Context for fragmentation-aware layout
#[derive(Debug)]
pub struct FragmentationLayoutContext {
    /// Page size (including margins)
    pub page_size: LogicalSize,
    /// Content area margins
    pub margins: PageMargins,
    /// Page template for headers/footers
    pub template: PageTemplate,
    /// Current page being laid out (0-indexed)
    pub current_page: usize,
    /// Y position on current page (0 = top of content area)
    pub current_y: f32,
    /// Available height remaining on current page
    pub available_height: f32,
    /// Page content height (without margins and headers/footers)
    pub page_content_height: f32,
    /// Accumulated break-inside: avoid depth from ancestors
    pub break_inside_avoid_depth: usize,
    /// Current orphans setting (inherited)
    pub orphans: u32,
    /// Current widows setting (inherited)
    pub widows: u32,
    /// All page fragments generated so far
    pub fragments: Vec<PageFragment>,
    /// Page counter for headers/footers
    pub counter: PageCounter,
    /// Fragmentation defaults (smart behavior settings)
    pub defaults: FragmentationDefaults,
    /// Break points encountered during layout
    pub break_points: Vec<BreakPoint>,
    /// Whether to avoid break before next box
    pub avoid_break_before_next: bool,
}

/// Page margins in points
#[derive(Debug, Clone, Copy, Default)]
pub struct PageMargins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl PageMargins {
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub fn uniform(margin: f32) -> Self {
        Self {
            top: margin,
            right: margin,
            bottom: margin,
            left: margin,
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Configuration for intelligent fragmentation defaults
#[derive(Debug, Clone)]
pub struct FragmentationDefaults {
    /// Keep headers (h1-h6) with following content
    pub keep_headers_with_content: bool,
    /// Minimum lines to keep together for short paragraphs
    pub min_paragraph_lines: u32,
    /// Keep figure/figcaption together
    pub keep_figures_together: bool,
    /// Keep table headers with first data row
    pub keep_table_headers: bool,
    /// Keep list item markers with content
    pub keep_list_markers: bool,
    /// Treat small blocks as monolithic (height threshold in lines)
    pub small_block_threshold_lines: u32,
    /// Default orphans value
    pub default_orphans: u32,
    /// Default widows value
    pub default_widows: u32,
}

impl Default for FragmentationDefaults {
    fn default() -> Self {
        Self {
            keep_headers_with_content: true,
            min_paragraph_lines: 3,
            keep_figures_together: true,
            keep_table_headers: true,
            keep_list_markers: true,
            small_block_threshold_lines: 3,
            default_orphans: 2,
            default_widows: 2,
        }
    }
}

impl FragmentationLayoutContext {
    /// Create a new fragmentation context for paged layout
    pub fn new(page_size: LogicalSize, margins: PageMargins) -> Self {
        let template = PageTemplate::default();

        let page_content_height =
            page_size.height - margins.vertical() - template.header_height - template.footer_height;

        Self {
            page_size,
            margins,
            template,
            current_page: 0,
            current_y: 0.0,
            available_height: page_content_height,
            page_content_height,
            break_inside_avoid_depth: 0,
            orphans: 2,
            widows: 2,
            fragments: Vec::new(),
            counter: PageCounter::new(),
            defaults: FragmentationDefaults::default(),
            break_points: Vec::new(),
            avoid_break_before_next: false,
        }
    }

    /// Create context with a page template
    pub fn with_template(mut self, template: PageTemplate) -> Self {
        self.template = template;
        self.recalculate_content_height();
        self
    }

    /// Create context with custom defaults
    pub fn with_defaults(mut self, defaults: FragmentationDefaults) -> Self {
        self.orphans = defaults.default_orphans;
        self.widows = defaults.default_widows;
        self.defaults = defaults;
        self
    }

    /// Recalculate content height based on template
    fn recalculate_content_height(&mut self) {
        let header = if self.template.show_header(self.current_page + 1) {
            self.template.header_height
        } else {
            0.0
        };
        let footer = if self.template.show_footer(self.current_page + 1) {
            self.template.footer_height
        } else {
            0.0
        };
        self.page_content_height =
            self.page_size.height - self.margins.vertical() - header - footer;
        self.available_height = self.page_content_height - self.current_y;
    }

    /// Get the content area origin for the current page
    pub fn content_origin(&self) -> LogicalPosition {
        let header = if self.template.show_header(self.current_page + 1) {
            self.template.header_height
        } else {
            0.0
        };
        LogicalPosition {
            x: self.margins.left,
            y: self.margins.top + header,
        }
    }

    /// Get the content area size for the current page
    pub fn content_size(&self) -> LogicalSize {
        LogicalSize {
            width: self.page_size.width - self.margins.horizontal(),
            height: self.page_content_height,
        }
    }

    /// Use space on the current page
    pub fn use_space(&mut self, height: f32) {
        self.current_y += height;
        self.available_height = (self.page_content_height - self.current_y).max(0.0);
    }

    /// Check if content of given height can fit on current page
    pub fn can_fit(&self, height: f32) -> bool {
        self.available_height >= height
    }

    /// Check if content would fit on an empty page
    pub fn would_fit_on_empty_page(&self, height: f32) -> bool {
        height <= self.page_content_height
    }

    /// Advance to the next page
    pub fn advance_page(&mut self) {
        self.current_page += 1;
        self.current_y = 0.0;
        self.counter.page_number += 1;
        self.recalculate_content_height();
        self.avoid_break_before_next = false;
    }

    /// Advance to a left (even) page
    pub fn advance_to_left_page(&mut self) {
        self.advance_page();
        if self.current_page % 2 != 0 {
            // Current page is odd (right), advance one more
            self.advance_page();
        }
    }

    /// Advance to a right (odd) page
    pub fn advance_to_right_page(&mut self) {
        self.advance_page();
        if self.current_page % 2 == 0 {
            // Current page is even (left), advance one more
            self.advance_page();
        }
    }

    /// Enter a box with break-inside: avoid
    pub fn enter_avoid_break(&mut self) {
        self.break_inside_avoid_depth += 1;
    }

    /// Exit a box with break-inside: avoid
    pub fn exit_avoid_break(&mut self) {
        self.break_inside_avoid_depth = self.break_inside_avoid_depth.saturating_sub(1);
    }

    /// Set flag to avoid break before next content
    pub fn set_avoid_break_before_next(&mut self) {
        self.avoid_break_before_next = true;
    }

    /// Add a page fragment
    pub fn add_fragment(&mut self, fragment: PageFragment) {
        self.fragments.push(fragment);
    }

    /// Get the total number of pages so far
    pub fn page_count(&self) -> usize {
        self.current_page + 1
    }

    /// Set total page count (for "Page X of Y" footers)
    pub fn set_total_pages(&mut self, total: usize) {
        self.counter.total_pages = Some(total);
    }

    /// Convert fragments to display lists (one per page)
    pub fn into_display_lists(self) -> Vec<DisplayList> {
        let page_count = self.page_count();
        let mut display_lists: Vec<DisplayList> =
            (0..page_count).map(|_| DisplayList::default()).collect();

        for fragment in self.fragments {
            if fragment.page_index < display_lists.len() {
                display_lists[fragment.page_index]
                    .items
                    .extend(fragment.items);
            }
        }

        display_lists
    }

    /// Generate header/footer display list items for a specific page
    pub fn generate_page_chrome(&self, page_index: usize) -> Vec<DisplayListItem> {
        let mut items = Vec::new();
        let page_number = page_index + 1;

        let counter = PageCounter {
            page_number,
            total_pages: self.counter.total_pages,
            chapter: self.counter.chapter,
            named_counters: self.counter.named_counters.clone(),
        };

        let slots = self.template.slots_for_page(page_number);

        for slot in slots {
            let _text = match &slot.content {
                PageSlotContent::Text(s) => s.clone(),
                PageSlotContent::PageNumber(style) => counter.format_page_number(*style),
                PageSlotContent::PageOfTotal => counter.format_page_of_total(),
                PageSlotContent::RunningHeader(s) => s.clone(),
                PageSlotContent::Dynamic(f) => f.call(&counter),
            };

            // Calculate position based on slot
            let (_x, _y) = self.slot_position(slot.position, page_number);

            // TODO: Create proper text DisplayListItem
            // For now we'll need to integrate with text layout
            // This is a placeholder that shows where the text would go
        }

        items
    }

    /// Calculate position for a page slot
    fn slot_position(&self, position: PageSlotPosition, page_number: usize) -> (f32, f32) {
        let content_width = self.page_size.width - self.margins.horizontal();

        let x = match position {
            PageSlotPosition::TopLeft | PageSlotPosition::BottomLeft => self.margins.left,
            PageSlotPosition::TopCenter | PageSlotPosition::BottomCenter => {
                self.margins.left + content_width / 2.0
            }
            PageSlotPosition::TopRight | PageSlotPosition::BottomRight => {
                self.page_size.width - self.margins.right
            }
        };

        let y = match position {
            PageSlotPosition::TopLeft
            | PageSlotPosition::TopCenter
            | PageSlotPosition::TopRight => self.margins.top + self.template.header_height / 2.0,
            PageSlotPosition::BottomLeft
            | PageSlotPosition::BottomCenter
            | PageSlotPosition::BottomRight => {
                self.page_size.height - self.margins.bottom - self.template.footer_height / 2.0
            }
        };

        (x, y)
    }
}

// Break Decision Logic

/// Result of deciding how to handle a box at a potential break point
#[derive(Debug, Clone)]
pub enum BreakDecision {
    /// Place the entire box on the current page
    FitOnCurrentPage,
    /// Move the entire box to the next page
    MoveToNextPage,
    /// Split the box across pages
    SplitAcrossPages {
        /// Height to place on current page
        height_on_current: f32,
        /// Height to place on next page(s)
        height_remaining: f32,
    },
    /// Force a page break before this box
    ForceBreakBefore,
    /// Force a page break after this box
    ForceBreakAfter,
}

/// Make a break decision for a box with given behavior
pub fn decide_break(
    behavior: &BoxBreakBehavior,
    ctx: &FragmentationLayoutContext,
    break_before: PageBreak,
    break_after: PageBreak,
) -> BreakDecision {
    // Check for forced break before
    if is_forced_break(&break_before) {
        if ctx.current_y > 0.0 {
            return BreakDecision::ForceBreakBefore;
        }
    }

    match behavior {
        BoxBreakBehavior::Monolithic { height } => {
            decide_monolithic_break(*height, ctx, break_before)
        }
        BoxBreakBehavior::KeepTogether {
            estimated_height,
            priority,
        } => decide_keep_together_break(*estimated_height, *priority, ctx, break_before),
        BoxBreakBehavior::Splittable {
            min_before_break,
            min_after_break,
        } => decide_splittable_break(*min_before_break, *min_after_break, ctx, break_before),
    }
}

fn decide_monolithic_break(
    height: f32,
    ctx: &FragmentationLayoutContext,
    break_before: PageBreak,
) -> BreakDecision {
    // Monolithic content cannot be split
    if ctx.can_fit(height) {
        BreakDecision::FitOnCurrentPage
    } else if ctx.current_y > 0.0 && ctx.would_fit_on_empty_page(height) {
        // Doesn't fit but would fit on empty page
        BreakDecision::MoveToNextPage
    } else {
        // Too large for any page - place anyway (will overflow)
        BreakDecision::FitOnCurrentPage
    }
}

fn decide_keep_together_break(
    height: f32,
    priority: KeepTogetherPriority,
    ctx: &FragmentationLayoutContext,
    break_before: PageBreak,
) -> BreakDecision {
    if ctx.can_fit(height) {
        BreakDecision::FitOnCurrentPage
    } else if ctx.would_fit_on_empty_page(height) {
        // Would fit on empty page, move there
        BreakDecision::MoveToNextPage
    } else {
        // Too tall for any page - must split despite keep-together
        // Calculate split point
        let height_on_current = ctx.available_height;
        let height_remaining = height - height_on_current;
        BreakDecision::SplitAcrossPages {
            height_on_current,
            height_remaining,
        }
    }
}

fn decide_splittable_break(
    min_before: f32,
    min_after: f32,
    ctx: &FragmentationLayoutContext,
    break_before: PageBreak,
) -> BreakDecision {
    // For splittable content, we need to consider orphans/widows
    let available = ctx.available_height;

    if available < min_before && ctx.current_y > 0.0 {
        // Can't fit minimum orphan content, move to next page
        BreakDecision::MoveToNextPage
    } else {
        // Can split - but actual split point determined during text layout
        BreakDecision::FitOnCurrentPage
    }
}

// Helper Functions

fn is_forced_break(page_break: &PageBreak) -> bool {
    matches!(
        page_break,
        PageBreak::Always
            | PageBreak::Page
            | PageBreak::Left
            | PageBreak::Right
            | PageBreak::Recto
            | PageBreak::Verso
            | PageBreak::All
    )
}

fn is_avoid_break(page_break: &PageBreak) -> bool {
    matches!(page_break, PageBreak::Avoid | PageBreak::AvoidPage)
}

// Roman numeral conversion
fn to_lower_roman(n: usize) -> String {
    to_upper_roman(n).to_lowercase()
}

fn to_upper_roman(mut n: usize) -> String {
    if n == 0 {
        return String::from("0");
    }

    let numerals = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];

    let mut result = String::new();
    for (value, numeral) in numerals.iter() {
        while n >= *value {
            result.push_str(numeral);
            n -= value;
        }
    }
    result
}

fn to_lower_alpha(n: usize) -> String {
    to_upper_alpha(n).to_lowercase()
}

fn to_upper_alpha(mut n: usize) -> String {
    if n == 0 {
        return String::from("0");
    }

    let mut result = String::new();
    while n > 0 {
        n -= 1;
        result.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    result
}
