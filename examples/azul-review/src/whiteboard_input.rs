//! The gestures on the whiteboard itself: what is under the pointer, moving and resizing a cutout,
//! drawing an arrow, bending it, the double click, the wheel. A pure state machine over the
//! [`Board`]: the window's events come in as points of the board's viewport, what changes is done to
//! the board (a drag is one step of its history), and what the window has to show is read back
//! ([`BoardInput::preview`], [`BoardInput::stroke`], [`BoardInput::selection`]). No window, no azul.
//!
//! - **Drag the body** of a cutout: it moves. **Drag its rim** (a band of [`RIM_PX`] screen px, inside and
//!   outside the edge): an arrow is drawn from that side to where the pointer lands - on a cutout (its rim
//!   fastens it to that side, its body leaves it to the line between the two), or on nothing (the end is
//!   loose). **The handle** at the corner of the selected cutout resizes it, its shape kept.
//! - **The pen** on empty space draws an arrow as one stroke: it starts and ends on cutouts it is near
//!   ([`SNAP_PX`]), or on a loose point, with at least one end on a cutout; the curve of the stroke is kept.
//!   **The mouse** on empty space moves the board.
//! - **The selected arrow** has an end at each of its ends (drag one to fasten it elsewhere or let it loose)
//!   and a grip in its middle (drag it to bend the arrow).
//! - **Double click** a cutout: [`Out::Jump`]; its label's place: [`Out::EditLabel`]. **Wheel**: moves the
//!   board; with the command key it zooms around the pointer.
//!
//! Every distance a person sees (a band, a handle, how near is near) is in SCREEN px: it does not
//! grow with the zoom.

use crate::{
    whiteboard::{Anchor, Board, End, ItemId, Notice, Which},
    whiteboard_geom::{fit_bend, path_length, ray_exit, rim_side, side_mid, Pt, Rect, Side, MAX_BEND},
};

/// The band along the edge of a cutout, inside and outside, where a drag draws an arrow, screen px.
pub const RIM_PX: f32 = 10.0;
/// The handle at the corner of the selected cutout, screen px.
pub const HANDLE_PX: f32 = 14.0;
/// How near the line of an arrow a press is on it, screen px.
pub const ARROW_HIT_PX: f32 = 7.0;
/// How near an end or the middle of the selected arrow a press is on it, screen px.
pub const END_PX: f32 = 12.0;
/// How near a cutout an arrow's end is fastened to it, screen px.
pub const SNAP_PX: f32 = 18.0;
/// How far a press moves before it is a drag, screen px.
pub const DRAG_PX: f32 = 4.0;
/// The height of the place for a cutout's label, under it, board px.
pub const LABEL_H: f32 = 18.0;
/// The shortest arrow, screen px: a shorter stroke is a tap.
pub const MIN_ARROW_PX: f32 = 24.0;
/// How much the wheel zooms: a factor of `exp(delta * this)` for each pixel of it.
pub const WHEEL_ZOOM: f32 = 0.01;

/// What presses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Pen,
    Mouse,
}

/// The modifier keys held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    /// The command key (control elsewhere).
    pub cmd: bool,
    pub alt: bool,
}

/// What a point of the board is over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Empty,
    /// The handle of the selected cutout.
    Handle(ItemId),
    /// An end of the selected arrow.
    ArrowEnd(ItemId, Which),
    /// The grip in the middle of the selected arrow.
    ArrowBend(ItemId),
    /// The rim of a cutout, and the side the point is at.
    Rim(ItemId, Side),
    Arrow(ItemId),
    Body(ItemId),
    /// The place of a cutout's label.
    Label(ItemId),
}

/// The pointer to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    Default,
    Move,
    Resize,
    Connect,
    Pointer,
}

/// What a gesture asks of the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Nothing,
    /// The board or what is drawn over it changed.
    Redraw,
    /// Go to the lines of this cutout.
    Jump(ItemId),
    /// Type this item's label.
    EditLabel(ItemId),
    /// Tell the person.
    Notice(Notice),
}

/// An arrow that is being drawn, bent or fastened elsewhere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preview {
    pub from: Pt,
    pub to: Pt,
    pub bend: f32,
    /// The cutout it would land on.
    pub target: Option<ItemId>,
    /// The arrow it replaces while it is shown (to be left out of the drawing).
    pub replaces: Option<ItemId>,
}

/// What is under `p` (a point of the board) when `selected` is selected, at `zoom`.
#[must_use]
pub fn hit_test(board: &Board, selected: Option<ItemId>, p: Pt, zoom: f32) -> Hit {
    let zoom = if zoom > 0.0 { zoom } else { 1.0 };
    // What the selection offers first.
    if let Some(sel) = selected {
        if let Some(c) = board.cutout(sel) {
            let h = HANDLE_PX / zoom;
            if Rect::new(c.rect.right() - h, c.rect.bottom() - h, h * 1.5, h * 1.5).contains(p) {
                return Hit::Handle(sel);
            }
        } else if let Some(placed) = board.place_arrow(sel) {
            let near = END_PX / zoom;
            if p.dist(placed.from) <= near {
                return Hit::ArrowEnd(sel, Which::From);
            }
            if p.dist(placed.to) <= near {
                return Hit::ArrowEnd(sel, Which::To);
            }
            if p.dist(placed.curve.at(0.5)) <= near {
                return Hit::ArrowBend(sel);
            }
        }
    }
    // The cutouts, the top one first: its rim (inside or out), or its body (an arrow over it comes first).
    let mut cutouts: Vec<_> = board.cutouts().collect();
    cutouts.reverse();
    let band = RIM_PX / zoom;
    let mut body = None;
    for c in &cutouts {
        if c.rect.contains(p) {
            match rim_side(&c.rect, p, band) {
                Some(side) => return Hit::Rim(c.id, side),
                None => {
                    body = Some(c.id);
                    break;
                }
            }
        } else if is_label_place(c, selected, p) {
            // Under a cutout whose label is said or being typed, the place of the label comes before the
            // outer band of its rim.
            return Hit::Label(c.id);
        } else if let Some(side) = rim_side(&c.rect, p, band) {
            return Hit::Rim(c.id, side);
        }
    }
    let mut arrows: Vec<_> = board.arrows().collect();
    arrows.reverse();
    for a in arrows {
        if board.place_arrow(a.id).is_some_and(|pl| pl.curve.distance_to(p) <= ARROW_HIT_PX / zoom) {
            return Hit::Arrow(a.id);
        }
    }
    if let Some(id) = body {
        return Hit::Body(id);
    }
    Hit::Empty
}

/// Whether `p` is in the place under cutout `c` where its label is: it has one when there is a label to
/// show or the cutout is selected (to type one).
fn is_label_place(c: &crate::whiteboard::Cutout, selected: Option<ItemId>, p: Pt) -> bool {
    (!c.label.is_empty() || selected == Some(c.id)) && Rect::new(c.rect.x, c.rect.bottom(), c.rect.w, LABEL_H).contains(p)
}

/// The cutout `tip` lands on - the top one within [`SNAP_PX`] of it - and how an arrow is fastened there:
/// to the side whose rim `tip` is in, else where the line to the other end meets it.
fn landing(board: &Board, tip: Pt, zoom: f32) -> Option<(ItemId, Anchor)> {
    let mut cutouts: Vec<_> = board.cutouts().collect();
    cutouts.reverse();
    cutouts.into_iter().find(|c| c.rect.inflate(SNAP_PX / zoom).contains(tip)).map(|c| {
        let anchor = rim_side(&c.rect, tip, RIM_PX / zoom).map_or(Anchor::Auto, Anchor::Side);
        (c.id, anchor)
    })
}

fn end_at(landing: Option<(ItemId, Anchor)>, free: Pt) -> End {
    landing.map_or(End::Free(free), |(id, anchor)| End::Item { id, anchor })
}

#[derive(Debug, Clone, Default)]
enum Gesture {
    #[default]
    Idle,
    /// Pressed on a cutout: a drag past [`DRAG_PX`] moves it; let go sooner it was a click.
    Pressed { id: ItemId, at: Pt, grab: Pt },
    Moving { id: ItemId, grab: Pt },
    Resizing { id: ItemId, fixed: Pt, aspect: f32, grab: Pt },
    /// An arrow from `from` (fastened to a rim) to where the pointer is.
    Wiring { from: End, start: Pt, points: Vec<Pt>, tip: Pt, kind: PointerKind },
    /// A pen stroke on empty space: an arrow when it ends well.
    Stroke { points: Vec<Pt> },
    EndDrag { arrow: ItemId, which: Which, tip: Pt },
    Bending { arrow: ItemId, bend: f32 },
    Panning { last: Pt },
}

/// The gestures on one board.
#[derive(Debug, Clone, Default)]
pub struct BoardInput {
    gesture: Gesture,
    selection: Option<ItemId>,
}

impl BoardInput {
    #[must_use]
    pub const fn selection(&self) -> Option<ItemId> {
        self.selection
    }

    pub fn select(&mut self, id: Option<ItemId>) {
        self.selection = id;
    }

    /// Whether a gesture is going on.
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        !matches!(self.gesture, Gesture::Idle)
    }

    /// Whether the gesture going on moves the cutouts (a move, a resize, the board itself): their nodes
    /// have to be laid out again as it goes. An arrow being drawn, bent or fastened moves nothing but itself.
    #[must_use]
    pub const fn moves_nodes(&self) -> bool {
        matches!(self.gesture, Gesture::Moving { .. } | Gesture::Resizing { .. } | Gesture::Panning { .. })
    }

    /// The pen stroke being drawn, in board coordinates (to show as it is drawn).
    #[must_use]
    pub fn stroke(&self) -> &[Pt] {
        match &self.gesture {
            Gesture::Stroke { points } | Gesture::Wiring { points, kind: PointerKind::Pen, .. } => points,
            _ => &[],
        }
    }

    /// The point `from` of a fastening: the middle of its side, the place on the cutout, or - where the
    /// line to the other end decides - the edge towards `toward`.
    fn anchor_point(board: &Board, end: &End, toward: Pt) -> Option<Pt> {
        match end {
            End::Free(p) => Some(*p),
            End::Item { id, anchor } => {
                let r = board.cutout(*id)?.rect;
                Some(match anchor {
                    Anchor::Auto => ray_exit(&r, toward),
                    Anchor::Side(s) => side_mid(&r, *s),
                    Anchor::Point { u, v } => Pt::new(r.x + r.w * u, r.y + r.h * v),
                })
            }
        }
    }

    /// The arrow that is being drawn, bent or fastened elsewhere, as it would be.
    #[must_use]
    pub fn preview(&self, board: &Board) -> Option<Preview> {
        let zoom = board.view().zoom;
        match &self.gesture {
            Gesture::Wiring { from, points, tip, kind, .. } => {
                let land = landing(board, *tip, zoom).filter(|(id, _)| Some(*id) != from.item_id());
                let start = Self::anchor_point(board, from, *tip)?;
                let to = match land {
                    Some((id, anchor)) => Self::anchor_point(board, &End::Item { id, anchor }, start)?,
                    None => *tip,
                };
                let bend = if *kind == PointerKind::Pen { fit_bend(points) } else { 0.0 };
                Some(Preview { from: start, to, bend, target: land.map(|(id, _)| id), replaces: None })
            }
            Gesture::Stroke { points } => {
                let (first, last) = (*points.first()?, *points.last()?);
                let (a, b) = (landing(board, first, zoom), landing(board, last, zoom));
                let from = a.and_then(|(id, anchor)| Self::anchor_point(board, &End::Item { id, anchor }, last)).unwrap_or(first);
                let to = b.and_then(|(id, anchor)| Self::anchor_point(board, &End::Item { id, anchor }, first)).unwrap_or(last);
                Some(Preview { from, to, bend: fit_bend(points), target: b.map(|(id, _)| id), replaces: None })
            }
            Gesture::EndDrag { arrow, which, tip } => {
                let a = board.arrow(*arrow)?;
                let other = match which {
                    Which::From => &a.to,
                    Which::To => &a.from,
                };
                let land = landing(board, *tip, zoom).filter(|(id, _)| Some(*id) != other.item_id());
                let fixed = Self::anchor_point(board, other, *tip)?;
                let moved = match land {
                    Some((id, anchor)) => Self::anchor_point(board, &End::Item { id, anchor }, fixed)?,
                    None => *tip,
                };
                let (from, to) = if *which == Which::From { (moved, fixed) } else { (fixed, moved) };
                Some(Preview { from, to, bend: a.bend, target: land.map(|(id, _)| id), replaces: Some(*arrow) })
            }
            Gesture::Bending { arrow, bend } => {
                let placed = board.place_arrow(*arrow)?;
                Some(Preview { from: placed.from, to: placed.to, bend: *bend, target: None, replaces: Some(*arrow) })
            }
            _ => None,
        }
    }

    /// The pointer to show over `at` (a point of the viewport).
    #[must_use]
    pub fn hover(&self, board: &Board, at: Pt) -> Cursor {
        let view = board.view();
        match hit_test(board, self.selection, view.to_board(at), view.zoom) {
            Hit::Body(_) | Hit::ArrowEnd(..) | Hit::ArrowBend(_) => Cursor::Move,
            Hit::Rim(..) => Cursor::Connect,
            Hit::Handle(_) => Cursor::Resize,
            Hit::Arrow(_) | Hit::Label(_) => Cursor::Pointer,
            Hit::Empty => Cursor::Default,
        }
    }

    /// A press at `at` (a point of the board's viewport).
    pub fn press(&mut self, board: &mut Board, at: Pt, kind: PointerKind, _mods: Mods) -> Out {
        let view = board.view();
        let p = view.to_board(at);
        self.gesture = Gesture::Idle;
        match hit_test(board, self.selection, p, view.zoom) {
            Hit::Handle(id) => {
                let r = board.cutout(id).map(|c| c.rect).unwrap_or_default();
                let corner = Pt::new(r.right(), r.bottom());
                self.gesture = Gesture::Resizing { id, fixed: r.origin(), aspect: (r.w / r.h.max(1.0)).max(0.01), grab: corner - p };
            }
            Hit::ArrowEnd(arrow, which) => {
                self.gesture = Gesture::EndDrag { arrow, which, tip: p };
            }
            Hit::ArrowBend(arrow) => {
                let bend = board.arrow(arrow).map_or(0.0, |a| a.bend);
                self.gesture = Gesture::Bending { arrow, bend };
            }
            Hit::Rim(id, side) => {
                self.selection = Some(id);
                let from = End::Item { id, anchor: Anchor::Side(side) };
                self.gesture = Gesture::Wiring { from, start: p, points: vec![p], tip: p, kind };
            }
            Hit::Arrow(id) => self.selection = Some(id),
            Hit::Label(id) => self.selection = Some(id),
            Hit::Body(id) => {
                self.selection = Some(id);
                board.raise(id);
                let origin = board.cutout(id).map(|c| c.rect.origin()).unwrap_or_default();
                self.gesture = Gesture::Pressed { id, at, grab: p - origin };
            }
            Hit::Empty => match kind {
                PointerKind::Pen => self.gesture = Gesture::Stroke { points: vec![p] },
                PointerKind::Mouse => {
                    self.selection = None;
                    self.gesture = Gesture::Panning { last: at };
                }
            },
        }
        Out::Redraw
    }

    /// The pointer moved to `at` with the button down.
    pub fn motion(&mut self, board: &mut Board, at: Pt, _mods: Mods) -> Out {
        let view = board.view();
        let p = view.to_board(at);
        match &mut self.gesture {
            Gesture::Idle => return Out::Nothing,
            Gesture::Pressed { id, at: start, grab } => {
                if start.dist(at) <= DRAG_PX {
                    return Out::Nothing;
                }
                let (id, grab) = (*id, *grab);
                board.move_live(id, p - grab);
                self.gesture = Gesture::Moving { id, grab };
            }
            Gesture::Moving { id, grab } => board.move_live(*id, p - *grab),
            Gesture::Resizing { id, fixed, aspect, grab } => {
                let corner = p + *grab;
                let least = board.limits().min_cutout * aspect.max(1.0);
                let w = (corner.x - fixed.x).max(least);
                board.resize_live(*id, Rect::new(fixed.x, fixed.y, w, w / *aspect));
            }
            Gesture::Wiring { points, tip, .. } => {
                points.push(p);
                *tip = p;
            }
            Gesture::Stroke { points } => points.push(p),
            Gesture::EndDrag { tip, .. } => *tip = p,
            Gesture::Bending { arrow, bend } => {
                if let Some(placed) = board.place_arrow(*arrow) {
                    let chord = placed.to - placed.from;
                    let len = chord.len();
                    if len > 1.0 {
                        let sagitta = (p - placed.from).dot(chord.perp().unit());
                        *bend = (2.0 * sagitta / len).clamp(-MAX_BEND, MAX_BEND);
                    }
                }
            }
            Gesture::Panning { last } => {
                let mut v = view;
                v.pan = v.pan - (at - *last) * (1.0 / view.zoom);
                *last = at;
                board.set_view(v);
            }
        }
        Out::Redraw
    }

    /// The button went up at `at`.
    pub fn release(&mut self, board: &mut Board, at: Pt, _mods: Mods) -> Out {
        let view = board.view();
        let p = view.to_board(at);
        let zoom = view.zoom;
        match std::mem::take(&mut self.gesture) {
            Gesture::Idle => Out::Nothing,
            Gesture::Pressed { .. } | Gesture::Panning { .. } => Out::Redraw,
            Gesture::Moving { .. } | Gesture::Resizing { .. } => {
                board.commit_live();
                Out::Redraw
            }
            Gesture::Wiring { from, start, mut points, kind, .. } => {
                points.push(p);
                let land = landing(board, p, zoom);
                if land.is_some_and(|(id, _)| Some(id) == from.item_id()) || start.dist(p) < MIN_ARROW_PX / zoom {
                    return Out::Redraw;
                }
                let bend = if kind == PointerKind::Pen { fit_bend(&points) } else { 0.0 };
                match board.add_arrow(from, end_at(land, p), bend, "") {
                    Ok(id) => {
                        self.selection = Some(id);
                        Out::Redraw
                    }
                    Err(n) => Out::Notice(n),
                }
            }
            Gesture::Stroke { mut points } => {
                points.push(p);
                if path_length(&points) < MIN_ARROW_PX / zoom {
                    return Out::Nothing;
                }
                let (first, last) = (points[0], p);
                let (a, b) = (landing(board, first, zoom), landing(board, last, zoom));
                if a.is_none() && b.is_none() {
                    return Out::Notice(Notice::ArrowNeedsACutout);
                }
                if a.zip(b).is_some_and(|(x, y)| x.0 == y.0) {
                    return Out::Redraw;
                }
                match board.add_arrow(end_at(a, first), end_at(b, last), fit_bend(&points), "") {
                    Ok(id) => {
                        self.selection = Some(id);
                        Out::Redraw
                    }
                    Err(n) => Out::Notice(n),
                }
            }
            Gesture::EndDrag { arrow, which, .. } => {
                let Some(a) = board.arrow(arrow) else {
                    return Out::Nothing;
                };
                let other = if which == Which::From { a.to } else { a.from };
                let land = landing(board, p, zoom);
                if land.is_some_and(|(id, _)| Some(id) == other.item_id()) {
                    return Out::Redraw;
                }
                match board.rewire(arrow, which, end_at(land, p)) {
                    Ok(()) => Out::Redraw,
                    Err(n) => Out::Notice(n),
                }
            }
            Gesture::Bending { arrow, bend } => match board.set_bend(arrow, bend) {
                Ok(()) => Out::Redraw,
                Err(n) => Out::Notice(n),
            },
        }
    }

    /// Escape: what is going on is dropped, the cutout put back where it was.
    pub fn cancel(&mut self, board: &mut Board) {
        if matches!(self.gesture, Gesture::Moving { .. } | Gesture::Resizing { .. }) {
            board.cancel_live();
        }
        self.gesture = Gesture::Idle;
    }

    /// A double click at `at`: go to the cutout's lines, or type the label of a cutout (under it) or of an arrow
    /// (on its line, where its label sits).
    pub fn double_click(&mut self, board: &mut Board, at: Pt) -> Out {
        let view = board.view();
        let p = view.to_board(at);
        let on = |id: ItemId| board.cutout(id).is_some_and(|c| c.rect.contains(p));
        match hit_test(board, self.selection, p, view.zoom) {
            Hit::Body(id) | Hit::Handle(id) => Out::Jump(id),
            Hit::Rim(id, _) if on(id) => Out::Jump(id),
            Hit::Label(id) | Hit::Arrow(id) | Hit::ArrowBend(id) => Out::EditLabel(id),
            _ => Out::Nothing,
        }
    }

    /// The wheel (or a trackpad) at `at`: `delta` moves the board; with the command key it zooms around `at`.
    pub fn wheel(&mut self, board: &mut Board, at: Pt, delta: Pt, mods: Mods) -> Out {
        let view = board.view();
        let next = if mods.cmd {
            let limits = *board.limits();
            view.zoomed_at(at, (-delta.y * WHEEL_ZOOM).exp(), limits.zoom_min, limits.zoom_max)
        } else {
            let mut v = view;
            v.pan = v.pan + delta * (1.0 / view.zoom);
            v
        };
        board.set_view(next);
        Out::Redraw
    }

    /// Delete: the selection is taken off the board (a cutout with its arrows), one step.
    pub fn delete_selection(&mut self, board: &mut Board) -> Out {
        let Some(id) = self.selection.take() else {
            return Out::Nothing;
        };
        match board.remove(id) {
            Ok(_) => Out::Redraw,
            Err(_) => Out::Nothing,
        }
    }
}

#[cfg(test)]
#[path = "whiteboard_input_test.rs"]
mod whiteboard_input_test;
