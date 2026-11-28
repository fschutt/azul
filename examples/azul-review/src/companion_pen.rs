//! What the phone is told of the review: where the window looks at the pages and where the pen (or the cursor) is, as the link crate's
//! `PenState`. Pure: what the window knows goes in as numbers and words, the state comes out; the host sends it at most 30 times a
//! second and only when it changed, so this is built whenever something that is in it moved.
//!
//! The pen is in the column's units of the phone's pad (widths of a sheet, `y` down the column of sheets: `page_view`), and what a button
//! of the phone would act on is the marked region if a loop waits to be taken to the whiteboard, else the line under the pen, else the page.

use azul_review_link::{
    access::ScreenId,
    frame::{Context, PenMark, PenState, ReturnPoint, Surface},
    limits::{cut_bytes, MAX_NAME_BYTES, MAX_PATH_BYTES},
    viewport,
};

use crate::{
    board_view,
    page_view::{pen_in_column, Geometry, Looking},
    whiteboard_geom::{Pt, View},
};

/// How long after it was last seen the pen still counts as being over the pages, ms.
pub const PEN_AWAY_MS: u64 = 2_000;

/// Where the pen was last seen on the pages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PenAt {
    /// The page, from 0.
    pub page: usize,
    /// Where on that page, px at zoom 1 from its top left.
    pub x: f32,
    pub y: f32,
    /// The tip is on the page (drawing), not hovering.
    pub down: bool,
}

/// The whiteboard under the pen, when it is what the pad is over: the board's geometry (the box round its items and its canvas), its view,
/// which board it is, and where the pointer is over its canvas.
#[derive(Debug, Clone)]
pub struct BoardInput {
    pub geometry: board_view::Geometry,
    pub view: View,
    pub board_id: u32,
    pub name: String,
    pub items: usize,
    /// The board point under the pointer (`None`: the pointer is elsewhere, or was not seen lately).
    pub pointer: Option<Pt>,
}

/// What the phone is told of the review, as the window knows it.
#[derive(Debug, Clone)]
pub struct Input<'a> {
    /// The whiteboard, when the pointer is over it: the pad is a free pan and zoom of the board then, not of the pages.
    pub board: Option<BoardInput>,
    /// The workspace folder's name.
    pub folder: &'a str,
    /// The file under review, relative to the workspace (empty: none open).
    pub file: &'a str,
    /// The line of the file under the pen, from 1 (0: none).
    pub line: usize,
    pub line_count: usize,
    pub geometry: Geometry,
    pub looking: Looking,
    /// Where the pen was last seen and how long ago, ms.
    pub pen: Option<(PenAt, u64)>,
    /// A loop waits to be taken to the whiteboard (the marked region).
    pub region: bool,
    /// The place the review was at before the jump to a conversation: the file, the page, the line and a few words for it.
    pub back: Option<(String, usize, u32, String)>,
}

/// What a button would act on right now: the marked region, else the line under the pen, else the page.
#[must_use]
pub fn context_of(region: bool, line: usize) -> Context {
    if region {
        Context::Region
    } else if line > 0 {
        Context::Line
    } else {
        Context::Page
    }
}

/// The pen state for `input`, its number left at 0 (the host numbers them itself).
#[must_use]
pub fn pen_state(input: &Input<'_>) -> PenState {
    if let Some(board) = &input.board {
        return board_state(input, board);
    }
    let g = input.geometry.sane();
    let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let pen = input.pen.filter(|(_, ago)| *ago <= PEN_AWAY_MS).map(|(at, _)| {
        let (x, y) = pen_in_column(&g, at.page, (at.x, at.y));
        PenMark { x, y, down: at.down }
    });
    // a pen that is away has no line: the buttons act on the page
    let line = if pen.is_some() { input.line } else { 0 };
    let view = input.looking.view(&g);
    PenState {
        seq: 0,
        screen: ScreenId::MAIN,
        folder: clean(input.folder, MAX_NAME_BYTES),
        surface: Surface::Page(g.surface(&clean(input.file, MAX_PATH_BYTES), input.looking.page, line, input.line_count)),
        view,
        zoom: viewport::zoom_of(view),
        pen,
        thumb_id: 0,
        context: context_of(input.region, line),
        return_point: input.back.as_ref().map(|(file, page, line, label)| ReturnPoint {
            file: clean(file, MAX_PATH_BYTES),
            page_index: to_u32(*page),
            line: *line,
            label: clean(label, MAX_NAME_BYTES),
        }),
    }
}

/// The pen state over the whiteboard: the board's name and shape, the window of the board the canvas shows, the pointer's place in the box's
/// units, and a button acts on the marked region if a loop waits, else on the page (there are no lines on a board).
fn board_state(input: &Input<'_>, board: &BoardInput) -> PenState {
    let g = board.geometry.sane();
    let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let window = g.window_of(board.view);
    let pen = board.pointer.map(|p| {
        let (x, y) = g.to_units(p);
        PenMark { x, y, down: false }
    });
    PenState {
        seq: 0,
        screen: ScreenId::MAIN,
        folder: clean(input.folder, MAX_NAME_BYTES),
        surface: Surface::Whiteboard(g.surface(board.board_id, &clean(&board.name, MAX_NAME_BYTES), board.items)),
        view: window,
        zoom: viewport::zoom_of(window),
        pen,
        thumb_id: 0,
        context: if input.region { Context::Region } else { Context::Page },
        return_point: input.back.as_ref().map(|(file, page, line, label)| ReturnPoint {
            file: clean(file, MAX_PATH_BYTES),
            page_index: to_u32(*page),
            line: *line,
            label: clean(label, MAX_NAME_BYTES),
        }),
    }
}

/// A text with no control character, cut to `max` bytes.
fn clean(text: &str, max: usize) -> String {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    cut_bytes(&plain, max, "text").0
}

#[cfg(test)]
#[path = "companion_pen_test.rs"]
mod companion_pen_test;
