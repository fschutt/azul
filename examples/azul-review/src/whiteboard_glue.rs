//! The whiteboard in the review window, as numbers: where its bar is (what a region dropped from a page is dropped on), what
//! a loop drawn on a page is measured on (the page's lines as they are laid out), where the words of an armed region are put
//! on the page, which lines a jump brings to the eye and for how long. Pure: the window's size and the layout in, rectangles
//! out; the glue that holds the board and reaches it from the window's callbacks is in `whiteboard_plug.rs`.

use crate::{
    layout::{self, Panels, Solved},
    line_map::LineMap,
    ui,
    whiteboard_drag::Bar,
    whiteboard_geom::{Pt, Rect, Size},
    whiteboard_jump::JumpTarget,
    whiteboard_lasso::PageLines,
};

/// A number that is a number, or 0.
fn finite_or_zero(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// The width of the line between the page's right bar and the panel that holds the board (the panel's own left border): the
/// board's area, where a drop is measured from, begins after it.
pub const PANEL_BORDER_PX: f32 = 1.0;

/// The whiteboard's bar as the window has it now (logical px): the open panel, at the window's right edge (the thin bar is
/// the panel's left neighbour), between the top bar and the bottom bar; or, while it is closed, that thin bar at the edge (a
/// region dropped there asks the panel to open). While the panel slides it is as wide as it is now.
#[must_use]
pub fn bar_of(window: Size, panels: &Panels, solved: &Solved) -> Bar {
    let (w, h) = (finite_or_zero(window.w).max(0.0), finite_or_zero(window.h).max(0.0));
    let height = (h - layout::TOP_H - layout::BOTTOM_H).max(0.0);
    let panel_w = finite_or_zero(solved.right_w).max(0.0);
    let open = panels.right_open && panel_w > 0.5;
    let rect = if open {
        // The panel's border is the panel's own: the board's area begins inside it.
        let inner = (panel_w - PANEL_BORDER_PX).max(0.0);
        Rect::new((w - inner).max(0.0), layout::TOP_H, inner, height)
    } else {
        Rect::new((w - layout::RIGHT_BAR_W).max(0.0), layout::TOP_H, layout::RIGHT_BAR_W.min(w), height)
    };
    Bar { rect, open, window_w: w }
}

/// How many characters each line of a page has (characters, not bytes: a column is a character).
#[must_use]
pub fn page_chars(lines: &[String]) -> Vec<usize> {
    lines.iter().map(|l| l.chars().count()).collect()
}

/// What a loop is measured on: the page whose first line is number `first_number`, its lines `chars` long, laid out as the page
/// lays them out (the same numbers the page's code column and the cutouts use).
#[must_use]
pub fn page_lines(first_number: usize, chars: &[usize]) -> PageLines<'_> {
    PageLines {
        first_number: u32::try_from(first_number).unwrap_or(u32::MAX),
        chars,
        top: ui::PAGE_PAD,
        line_h: ui::LINE_H,
        text_x: ui::CODE_X,
        char_w: ui::CHAR_W,
        max_w: ui::CODE_W - ui::GUTTER_W,
    }
}

// ---- the words by an armed region

/// How wide the words (and the key) by an armed region are, px.
pub const CHIP_W: f32 = 236.0;
/// How tall they are, px: the line that says what to do, the line that says how it is done, the key.
pub const CHIP_H: f32 = 74.0;
/// The room between the region and its words, px.
pub const CHIP_GAP: f32 = 6.0;

/// Where the words and the key of an armed region are put on its page (top left, in the page's px as it is SHOWN): under the
/// loop, at its left; over it when there is no room under it on the page; kept inside the page. `bounds` is the loop's box
/// as it was drawn (the page as it was first drawn); `map` says where its lines are shown now, `page` is the page's size
/// as it is shown.
#[must_use]
pub fn chip_place(bounds: Rect, map: &LineMap, page: Size) -> Pt {
    let (top, bottom) = (map.to_display(bounds.y), map.to_display(bounds.bottom()));
    let left = bounds.x.clamp(ui::PAGE_PAD * 0.5, (page.w - CHIP_W - ui::PAGE_PAD * 0.5).max(ui::PAGE_PAD * 0.5));
    let under = bottom + CHIP_GAP;
    let y = if under + CHIP_H <= page.h { under } else { (top - CHIP_H - CHIP_GAP).max(0.0) };
    Pt::new(finite_or_zero(left), finite_or_zero(y))
}

// ---- send it to the board without dragging

/// Where on the bar (from its top-left corner, the header included) a region that was sent with a key lands: in the board's
/// upper part, each next one a little aside from the last (`already` is how many cutouts the board has), so that they do not
/// lie one over the other. The bar is `bar_w` wide.
#[must_use]
pub fn send_point(bar_w: f32, already: usize) -> Pt {
    let w = finite_or_zero(bar_w).max(0.0);
    let n = already % 6;
    let aside = (n % 3) as f32 * 36.0 - 36.0;
    Pt::new((w * 0.5 + aside).clamp(0.0, w), crate::whiteboard_app::HEADER_PX + 90.0 + n as f32 * 52.0)
}

// ---- the lines a jump went to

/// How long the lines a jump went to are lit, ms.
pub const FLASH_MS: u64 = 1_500;

/// The lines a jump brought to the eye, and until when (ms of the panel's clock).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flash {
    /// The file, relative to the workspace.
    pub file: String,
    /// The lines (from 1, both included).
    pub first: u32,
    pub last: u32,
    pub until_ms: u64,
}

impl Flash {
    /// The flash of a jump made at `now_ms`.
    #[must_use]
    pub fn of(target: &JumpTarget, now_ms: u64) -> Self {
        let (first, last) = (target.first_line.min(target.last_line), target.first_line.max(target.last_line));
        Self { file: target.file.clone(), first, last, until_ms: now_ms.saturating_add(FLASH_MS) }
    }

    /// Whether the time of the flash is up at `now_ms`.
    #[must_use]
    pub const fn is_over(&self, now_ms: u64) -> bool {
        now_ms >= self.until_ms
    }

    /// The rows of the page (from 0) the flash lights, first and last, when the page is of file `file`, begins at line
    /// number `page_first` and has `count` lines; `None` when none of them is on it.
    #[must_use]
    pub fn rows_on(&self, file: &str, page_first: usize, count: usize) -> Option<(usize, usize)> {
        if self.file != file || count == 0 {
            return None;
        }
        let (first, last) = (self.first as usize, self.last as usize);
        let page_last = page_first.checked_add(count)?.checked_sub(1)?;
        if last < page_first || first > page_last {
            return None;
        }
        Some((first.max(page_first) - page_first, last.min(page_last) - page_first))
    }
}

/// The top and the height, in the page's px as it is shown, of the rows `first..=last` of a page whose lines are shown as `map`
/// says (a block of lines put in between them is lit with them).
#[must_use]
pub fn rows_box(map: &LineMap, first: usize, last: usize) -> (f32, f32) {
    let top = map.line_top(first);
    let bottom = map.line_top(last) + ui::LINE_H;
    (top, (bottom - top).max(ui::LINE_H))
}

#[cfg(test)]
#[path = "whiteboard_glue_test.rs"]
mod whiteboard_glue_test;
