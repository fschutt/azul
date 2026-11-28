//! What the reader's chrome decides without a window: where a tap on the page lands (a phone
//! turns the page from its sides and shows or hides the toolbar from its middle), the zoom as
//! the percent the toolbar says, the page the strip and the slider are on, and the room the
//! toolbar and the strip leave for the pages.

use crate::settings::{PageLayout, FONT_MAX, FONT_MIN};

/// The text size that is 100 %: the default of [`crate::settings::ReadingSettings`].
pub const BASE_FONT_PX: u32 = 18;

/// The toolbar's height on a desktop, px (a thin bar: icons of 20 px in a 36 px row).
pub const TOOLBAR_PX: f32 = 40.0;
/// The toolbar's height on a phone, px (touch targets of 48 dp).
pub const TOOLBAR_PHONE_PX: f32 = 56.0;
/// The slider row's height on a phone, px.
pub const SLIDER_ROW_PX: f32 = 56.0;
/// The width of the strip beside the pages on a desktop, px.
pub const STRIP_PX: f32 = 168.0;
/// The arrow zones beside a desktop's pages, px each.
pub const ARROW_PX: f32 = 36.0;
/// The space either side of a desktop's page (a phone's page has none), px.
pub const GUTTER_PX: f32 = 12.0;
/// The page's margin on a phone is at most this (the text keeps its width), px.
pub const PHONE_MARGIN_MAX_PX: u32 = 24;
/// The running head and the folio over and under the pages, px together.
pub const LINES_PX: f32 = 48.0;

/// Where a tap on the reading area lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tap {
    /// The left side: the page before.
    Previous,
    /// The middle: the toolbar and the slider show or hide.
    Toggle,
    /// The right side: the page after.
    Next,
}

/// The share of the area's width each side takes: a thumb reaches the page turns without
/// covering the text it reads.
pub const SIDE_SHARE: f32 = 0.3;

/// Where a tap `x` px from the left of an area `width` px wide lands. A tap off the area or an
/// area without width is the middle's.
#[must_use]
pub fn tap_at(x: f32, width: f32) -> Tap {
    if !(width > 0.0) || !x.is_finite() || x < 0.0 || x > width {
        return Tap::Toggle;
    }
    let share = x / width;
    if share < SIDE_SHARE {
        Tap::Previous
    } else if share > 1.0 - SIDE_SHARE {
        Tap::Next
    } else {
        Tap::Toggle
    }
}

/// The text size `font_px` as a percent of [`BASE_FONT_PX`], rounded to 5 (`100 %`).
#[must_use]
pub fn zoom_percent(font_px: u32) -> u32 {
    let exact = f64::from(font_px) * 100.0 / f64::from(BASE_FONT_PX);
    ((exact / 5.0).round() as u32) * 5
}

/// The lowest and the highest zoom the text sizes allow, in percent.
#[must_use]
pub fn zoom_range() -> (u32, u32) {
    (zoom_percent(FONT_MIN), zoom_percent(FONT_MAX))
}

/// The page layout the Fit button chooses next: Auto, then one page, then two, then Auto again.
#[must_use]
pub fn next_layout(layout: PageLayout) -> PageLayout {
    match layout {
        PageLayout::Auto => PageLayout::Single,
        PageLayout::Single => PageLayout::Spread,
        PageLayout::Spread => PageLayout::Auto,
    }
}

/// The icon of the Fit button for the layout it is on.
#[must_use]
pub const fn layout_icon(layout: PageLayout) -> &'static str {
    match layout {
        PageLayout::Auto => "fit_screen",
        PageLayout::Single => "crop_portrait",
        PageLayout::Spread => "auto_stories",
    }
}

/// `n` as the number the page field shows for page `page` (0-based) of `count`: at least 1, at
/// most `count`.
#[must_use]
pub fn page_number(page: usize, count: usize) -> usize {
    (page + 1).min(count.max(1))
}

/// What a page typed into the field means: the 0-based page it names in a chapter of `count`
/// pages (a number past either end is the nearest page; text that is no number is nothing).
#[must_use]
pub fn typed_page(text: &str, count: usize) -> Option<usize> {
    let n: i64 = text.trim().parse().ok()?;
    let last = i64::try_from(count.max(1) - 1).unwrap_or(i64::MAX);
    Some(usize::try_from((n - 1).clamp(0, last)).unwrap_or(0))
}

/// The slider's value for a book progress (`0..=1`): thousandths, a whole number the slider
/// steps through.
pub const SLIDER_MAX: f32 = 1000.0;

/// The slider's value at `progress`.
#[must_use]
pub fn slider_value(progress: f32) -> f32 {
    if progress.is_nan() {
        return 0.0;
    }
    (progress.clamp(0.0, 1.0) * SLIDER_MAX).round()
}

/// The book progress a slider value is.
#[must_use]
pub fn slider_progress(value: f32) -> f32 {
    if value.is_nan() {
        return 0.0;
    }
    (value / SLIDER_MAX).clamp(0.0, 1.0)
}

/// What the pages' frame takes of the reading area besides the pages' text and margins: across,
/// the arrow zones, the gutters and the borders of two pages (a phone: the borders only), and
/// down, the running head and the folio. The pages' geometry is made for what is left.
#[must_use]
pub fn page_allowance(phone: bool) -> (f32, f32) {
    let borders = 4.0;
    if phone {
        (borders, LINES_PX)
    } else {
        (2.0 * ARROW_PX + 2.0 * GUTTER_PX + borders, LINES_PX)
    }
}

/// The room the pages have in a window of `window` px: the toolbar (on a phone only while it
/// shows: it floats over the page) and the strip (on a desktop only while it is open: on a phone
/// it floats over the page too) taken off.
#[must_use]
pub fn reading_room(window: (f32, f32), phone: bool, strip_open: bool) -> (f32, f32) {
    if phone {
        return (window.0.max(120.0), window.1.max(120.0));
    }
    let strip = if strip_open { STRIP_PX } else { 0.0 };
    ((window.0 - strip).max(120.0), (window.1 - TOOLBAR_PX).max(120.0))
}

#[cfg(test)]
#[path = "chrome_test.rs"]
mod chrome_test;
