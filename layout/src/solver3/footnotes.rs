//! Footnotes on paged media: `float: footnote`.
//!
//! CSS Generated Content for Paged Media (W3C Working Draft), "Footnotes":
//! "An element becomes a footnote by applying `float: footnote` to that
//! element. This triggers the following changes: The footnote element is
//! removed from the flow, and a `::footnote-call` pseudo-element is inserted
//! in its place"; the footnote area is by default at the bottom of the page.
//!
//! How this engine does it. A paged layout is one continuous canvas, cut
//! into pages afterwards:
//!
//! 1. LAYOUT. A footnote is an out-of-flow box (`getters::get_position` reads it as an absolute
//!    one), laid out as a block as wide as the page's content area and PARKED at the block offset
//!    of its static position - the line, or the place in the block flow, it was taken out of: its
//!    ANCHOR (`positioning::place_footnote`).
//! 2. PAGE BREAKS. A page's notes are those whose anchor lands on it. The room its flow has is
//!    the page's height less the heights of its notes ([`Footnotes::room_for_page`]), decided in
//!    the one forward pass of `page_breaks::compute_page_breaks_impl`.
//! 3. SLICING. The display items of a note are left out of the page they are parked on and drawn
//!    at the bottom of the content area of the page of their anchor, the notes of a page stacked
//!    in document order, the last one ending at the bottom edge
//!    (`display_list::paginate_pages_impl`).
//!
//! MULTI-COLUMN containers lay their columns out page by page during the
//! layout, before any page is cut, so they are told about the notes
//! themselves: a pass measures the notes (`multicol::ColumnPages::note_heights`),
//! the next fills the columns of each page only down to the notes anchored
//! in them (`multicol::plan_paged_columns`) and gives a container the room
//! the notes above it on its page leave (`multicol::plan_page_rooms`). The
//! break analysis then finds the page's flow ending where the row does.
//!
//! A NOTE THAT FITS NO PAGE is split between two of its lines and goes on
//! in the footnote area of the next page, before that page's own notes
//! ([`Footnotes::room_for_page`], [`Footnotes::fragments_for_page`]). The
//! draft has no rule for it; `footnote-policy: auto` leaves the rendering to
//! the user agent. A page that carries flow gives its footnote area at most
//! [`FOOTNOTE_AREA_MAX_SHARE`] of its height (the draft's "user agents may
//! set a default max-height value on the footnote area"); what is left
//! over at the end of the document gets pages of footnotes only. The rows
//! of a multi-column container follow a note that is split in it
//! (`multicol::plan_row`). Limits: a note that starts BEFORE a container
//! and goes on into the pages of its later rows is not counted by those
//! rows; and the pages of footnotes only are made by the slicer,
//! `PaginationInfo::page_count` does not count them.
//!
//! `footnote-policy` (same draft): this user agent's `auto` is the behaviour
//! of `line` - a line whose note does not fit under it on the page goes to
//! the next page together with the note. The property itself is not
//! implemented.
//!
//! THE MARKS are generated (`azul_core::generated_content`): a
//! `::footnote-call` in the place the footnote was taken from and a
//! `::footnote-marker` at the start of the note, both showing the predefined
//! `footnote` counter, which every footnote element increments ("The
//! Footnote Counter"). `content: none` on either turns it off - what a
//! document that writes its own marks sets. They are made only where the
//! element is a footnote: on paged media (`getters::is_hidden_footnote_mark`).
//!
//! NOT in this version:
//! - the draft's default style of the call is `font-variant-position: super` at `font-size:
//!   100%`; this engine has no `font-variant-position`, the call is raised and smaller like a
//!   `<sup>` (`azul_core::ua_css`);
//! - `::footnote-marker::after` (the draft's `'. '` after the marker): the `". "` is part of the
//!   marker's default content instead;
//! - resetting the counter per page (`@page { counter-reset: footnote }`, "The footnote counter
//!   may be reset on each page"): counters are counted on the document before it is laid out;
//!   `counter-reset: footnote` on an element restarts the numbering;
//! - the call of a footnote written between two blocks is a line of its own there;
//! - of the footnote area's rule (`@page { @footnote { ... } }`, [`FootnoteArea`]) only
//!   `margin-top`, `border-top`, the paddings and `max-height` are honoured; the area is
//!   `float: bottom` and nothing else; `footnote-display` and `footnote-policy` are not
//!   implemented;
//! - the margins of two notes stacked on a page do not collapse;
//! - a page sequence with pages of different heights is not tested with notes.

use std::collections::BTreeMap;

use azul_core::{dom::NodeId, geom::LogicalSize, styled_dom::StyledDom};
use azul_css::{
    css::CssPropertyValue,
    props::style::{LayoutBorderTopWidth, StyleBorderTopColor, StyleBorderTopStyle},
};

use crate::solver3::{
    display_list::DisplayList,
    getters::{declares_footnote, is_footnote},
    layout_tree::{LayoutNodeId, LayoutTree},
    PositionVec,
};

/// Sub-1/100-px differences are float noise.
const EPS: f32 = 0.01;

/// A line that starts within this distance of a page's top is the page's
/// first: breaks closer together than this are one boundary
/// (`page_breaks::MERGE_WINDOW_PX`), so a page cannot end before it.
const FIRST_LINE: f32 = 1.0;

/// Tells the readers of `position` and `float` whether the layout pass that
/// starts now lays `styled_dom` out on paged media with footnotes in it
/// (`azul_core::prop_cache::PagedFootnotes`). Every layout pass says so
/// before it reads any style: a paged one `paged`, a continuous one not.
pub(crate) fn announce(styled_dom: &StyledDom, paged: bool) {
    let any = paged
        && (0..styled_dom.node_data.len())
            .any(|i| declares_footnote(styled_dom, NodeId::new(i)));
    styled_dom.css_property_cache.ptr.paged_footnotes.set(any);
}

/// One footnote of a paged canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footnote {
    /// The footnote element.
    pub node: NodeId,
    /// Where its margin box is parked on the canvas: the top ...
    pub top: f32,
    /// ... and how tall it is.
    pub height: f32,
    /// The top of its anchor: of the line it was taken out of, or its place
    /// in the block flow.
    pub anchor_top: f32,
    /// The bottom of that line (the same as `anchor_top` in a block flow).
    pub anchor_bottom: f32,
}

/// The room the flow has on one page that carries footnotes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRoom {
    /// Where the page's flow ends at the latest: the page's bottom less its
    /// notes, or the top of the line whose note no longer fits.
    pub flow_end: f32,
    /// Whether `flow_end` is the top of such a line (the page ends early).
    pub ends_before_a_line: bool,
}

/// The box of the footnote area, as `@page { @footnote { ... } }` styles it
/// (CSS Generated Content for Paged Media, "The footnote area": "This rule
/// defines a box that, if used, will contain all the footnote elements that
/// appear on that page"; `css::footnote_area_path`). Lengths in px.
///
/// Honoured: `margin-top` (between the flow and the area), `border-top`
/// (the footnote rule: "A horizontal rule is often used to separate the
/// footnote area from the rest of the page"), the four paddings and
/// `max-height`. The area is `float: bottom`, its default and only
/// position ("The bottom margin edge of the footnote area is positioned so
/// that it touches the bottom of the page area"); every other declaration
/// of the rule is ignored. The box is there on the pages that have notes
/// and only on those: "The separator (and the entire footnote area) cannot
/// be rendered on a page with no footnotes."
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FootnoteArea {
    pub margin_top: f32,
    /// The width of the rule above the notes; 0 without one.
    pub border_top: f32,
    pub padding_top: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    pub padding_right: f32,
    /// `max-height`: the most the area's content may be tall on a page
    /// that carries flow ("The max-height property on the footnote area
    /// limits the size of this area, unless the page contains only
    /// footnotes"). `None`: this engine's default,
    /// [`FOOTNOTE_AREA_MAX_SHARE`] of the page for the whole box.
    pub max_height: Option<f32>,
}

impl FootnoteArea {
    /// What stands between the flow and the first note: margin, rule,
    /// padding.
    #[must_use]
    pub fn above(&self) -> f32 {
        self.margin_top + self.border_top + self.padding_top
    }

    /// What the area takes of a page besides its notes.
    #[must_use]
    pub fn chrome(&self) -> f32 {
        self.above() + self.padding_bottom
    }

    /// The most the notes of a page `page_height` tall that carries flow
    /// may take.
    #[must_use]
    pub fn content_max(&self, page_height: f32) -> f32 {
        self.max_height
            .unwrap_or(page_height * FOOTNOTE_AREA_MAX_SHARE - self.chrome())
            .max(0.0)
    }

    /// What a page gives its footnote area when its notes are `content`
    /// tall: nothing without notes.
    #[must_use]
    pub fn taken(&self, content: f32) -> f32 {
        if content > EPS {
            content + self.chrome()
        } else {
            0.0
        }
    }
}

/// The `border-top` of the footnote area as its rule declares it, for the
/// display list.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FootnoteRule {
    pub width: Option<CssPropertyValue<LayoutBorderTopWidth>>,
    pub style: Option<CssPropertyValue<StyleBorderTopStyle>>,
    pub color: Option<CssPropertyValue<StyleBorderTopColor>>,
}

/// The footnote area of `styled_dom`'s author stylesheet for pages whose
/// content area is `page_size`: the declarations of its `@page { @footnote
/// { ... } }` rules, a later one over an earlier one.
///
/// Lengths: `em` is the initial font size (the area is no element and has
/// no font size of its own - a choice), percentages are of the page's
/// content width (of its height for `max-height`). A `border-top` with a
/// style other than `none` / `hidden` and no width is `medium`, the
/// engine's `LayoutBorderTopWidth::default()`.
#[must_use]
pub fn footnote_area(styled_dom: &StyledDom, page_size: LogicalSize) -> (FootnoteArea, FootnoteRule) {
    use azul_css::{
        css::CssDeclaration,
        props::{
            basic::{PhysicalSize, PixelValue, PropertyContext, ResolutionContext},
            property::CssProperty,
            style::BorderStyle,
        },
    };
    let mut area = FootnoteArea::default();
    let mut rule = FootnoteRule::default();
    let resolve = |px: &PixelValue, containing: f32| -> f32 {
        let font_size = azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
        px.resolve_with_context(
            &ResolutionContext {
                vertical_writing_mode: false,
                element_font_size: font_size,
                parent_font_size: font_size,
                root_font_size: font_size,
                containing_block_size: PhysicalSize::new(containing, containing),
                element_size: None,
                viewport_size: PhysicalSize::new(page_size.width, page_size.height),
            },
            PropertyContext::Other,
        )
        .max(0.0)
    };
    let css = &styled_dom.css_property_cache.ptr.retained_author_css;
    for declaration in css.footnote_area_declarations() {
        let CssDeclaration::Static(property) = declaration else {
            continue;
        };
        match property {
            CssProperty::MarginTop(v) => {
                if let Some(v) = v.get_property() {
                    area.margin_top = resolve(&v.inner, page_size.width);
                }
            }
            CssProperty::PaddingTop(v) => {
                if let Some(v) = v.get_property() {
                    area.padding_top = resolve(&v.inner, page_size.width);
                }
            }
            CssProperty::PaddingBottom(v) => {
                if let Some(v) = v.get_property() {
                    area.padding_bottom = resolve(&v.inner, page_size.width);
                }
            }
            CssProperty::PaddingLeft(v) => {
                if let Some(v) = v.get_property() {
                    area.padding_left = resolve(&v.inner, page_size.width);
                }
            }
            CssProperty::PaddingRight(v) => {
                if let Some(v) = v.get_property() {
                    area.padding_right = resolve(&v.inner, page_size.width);
                }
            }
            CssProperty::MaxHeight(v) => {
                area.max_height = v.get_property().map(|v| resolve(&v.inner, page_size.height));
            }
            CssProperty::BorderTopWidth(v) => rule.width = Some(*v),
            CssProperty::BorderTopStyle(v) => rule.style = Some(*v),
            CssProperty::BorderTopColor(v) => rule.color = Some(*v),
            _ => {}
        }
    }
    let drawn = rule
        .style
        .and_then(|s| s.get_property().copied())
        .is_some_and(|s| !matches!(s.inner, BorderStyle::None | BorderStyle::Hidden));
    area.border_top = if drawn {
        rule.width
            .and_then(|w| w.get_property().copied())
            .map_or_else(
                || resolve(&LayoutBorderTopWidth::default().inner, page_size.width),
                |w| resolve(&w.inner, page_size.width),
            )
    } else {
        0.0
    };
    if !drawn {
        rule = FootnoteRule::default();
    }
    (area, rule)
}

/// The footnotes of a paged canvas and which display items are theirs.
///
/// Made from ONE display list ([`collect_footnotes`]); `owners` index into
/// that list's items.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Footnotes {
    /// The notes, by the top of their anchor (then in document order).
    pub notes: Vec<Footnote>,
    /// Per display item: the index in `notes` of the note it belongs to.
    /// Empty for a canvas without notes.
    owners: Vec<Option<u32>>,
    /// Per note: the first and the last display item that is its.
    ranges: Vec<Option<(usize, usize)>>,
    /// Per note: the block ranges it cannot be cut in - its lines and its
    /// images - from its margin box's top, ascending and not overlapping.
    unbreakable: Vec<Vec<(f32, f32)>>,
    /// The box of the footnote area ([`footnote_area`]).
    pub area: FootnoteArea,
    /// The rule above the notes.
    pub rule: FootnoteRule,
}

/// A part of a footnote that is drawn on one page: the note (its index in
/// [`Footnotes::notes`]) from `from` to `to` below its margin box's top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteFragment {
    pub note: usize,
    pub from: f32,
    pub to: f32,
}

impl NoteFragment {
    #[must_use]
    pub fn height(&self) -> f32 {
        (self.to - self.from).max(0.0)
    }
}

/// A footnote that goes on on the next page: the note and how much of it
/// the pages before have drawn (0 = it starts there).
pub type NoteRest = (usize, f32);

/// How much of a page that also carries flow its footnote area may take at
/// the most, as a share of the page's content height.
///
/// CSS Generated Content for Paged Media, "Size of the footnote area": "The
/// max-height property on the footnote area limits the size of this area,
/// unless the page contains only footnotes [...]. Since it is undesirable
/// for a page to consist only of footnotes, user agents may set a default
/// max-height value on the footnote area." The value is a choice of this
/// engine. It is also what lets a note longer than a page go on: every page
/// keeps a fifth of its height for the flow, so the pages keep coming.
pub const FOOTNOTE_AREA_MAX_SHARE: f32 = 0.8;

impl Footnotes {
    /// Whether the canvas has no footnotes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// The note the display item `item_index` belongs to.
    #[must_use]
    pub fn owner(&self, item_index: usize) -> Option<usize> {
        self.owners
            .get(item_index)
            .copied()
            .flatten()
            .map(|n| n as usize)
    }

    /// The first and the last display item of note `note_index`: all its
    /// items lie between them (with items of others, in paint order).
    #[must_use]
    pub fn item_range(&self, note_index: usize) -> Option<(usize, usize)> {
        self.ranges.get(note_index).copied().flatten()
    }

    /// The notes of the page whose flow is the canvas from `start` to `end`
    /// (the last page: `end` = `f32::INFINITY`), in document order - the
    /// order they are stacked in. A note belongs to the page its anchor's
    /// top is on; an anchor exactly on a break is on the page after it, as
    /// content at a break is (`page_breaks::page_of_y`).
    #[must_use]
    pub fn on_page(&self, start: f32, end: f32) -> Vec<(usize, &Footnote)> {
        let mut on_page: Vec<(usize, &Footnote)> = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.anchor_top >= start && n.anchor_top < end)
            .collect();
        on_page.sort_by_key(|(_, n)| n.node);
        on_page
    }

    /// Where the note `note` can be cut at the latest, at or before `limit`
    /// below its margin box's top, having been drawn down to `from`: between
    /// two of its lines, never inside one (nor inside an image). `from`
    /// when not even its next line fits; its height when all of it does.
    #[must_use]
    pub fn cut_before(&self, note: usize, from: f32, limit: f32) -> f32 {
        let Some(n) = self.notes.get(note) else {
            return from;
        };
        if limit >= n.height - EPS {
            return n.height;
        }
        let mut cut = limit;
        if let Some(ranges) = self.unbreakable.get(note) {
            // The lines are ascending: the one `limit` falls in is cut
            // before; one above it may reach into that one's top.
            for &(top, bottom) in ranges.iter().rev() {
                if cut > top + EPS && cut < bottom - EPS {
                    cut = top;
                }
            }
        }
        if cut > from + EPS {
            cut
        } else {
            from
        }
    }

    /// The end of the line (or image) of `note` that starts at or after
    /// `from`: what a page takes of a note when nothing of it fits and it
    /// has to go on (a page of footnotes only).
    fn next_piece_end(&self, note: usize, from: f32) -> f32 {
        let height = self.notes.get(note).map_or(from, |n| n.height);
        self.unbreakable
            .get(note)
            .and_then(|ranges| ranges.iter().find(|(_, bottom)| *bottom > from + EPS))
            .map_or(height, |(_, bottom)| bottom.min(height))
    }

    /// Fills a footnote area that has `room` with the notes of `queue`, in
    /// order: each one whole when it fits, else cut between two of its lines
    /// ([`Self::cut_before`]) - and then it and everything after it goes on
    /// on the next page, which the returned rest is the queue of. `forced`
    /// (a page of footnotes only): a first piece that does not fit is taken
    /// anyway, so the pages end.
    #[must_use]
    pub fn fill_area(
        &self,
        queue: &[NoteRest],
        mut room: f32,
        forced: bool,
    ) -> (Vec<NoteFragment>, Vec<NoteRest>) {
        let mut placed: Vec<NoteFragment> = Vec::new();
        let mut rest: Vec<NoteRest> = Vec::new();
        for &(note, from) in queue {
            let Some(n) = self.notes.get(note) else {
                continue;
            };
            if !rest.is_empty() {
                rest.push((note, from));
                continue;
            }
            let mut to = self.cut_before(note, from, from + room.max(0.0));
            if to <= from + EPS && forced && placed.is_empty() {
                to = self.next_piece_end(note, from);
            }
            if to > from + EPS {
                placed.push(NoteFragment { note, from, to });
                room -= to - from;
            }
            if to < n.height - EPS {
                rest.push((note, to.max(from)));
            }
        }
        (placed, rest)
    }

    /// The parts of footnotes a page draws: what the pages before left over
    /// (`rest`, updated to what THIS page leaves over), then the page's own
    /// notes - those anchored in its flow from `start` to `end` - in
    /// document order, in the room its flow leaves of its `height`.
    ///
    /// A page with flow gives its footnote area no more than
    /// [`FOOTNOTE_AREA_MAX_SHARE`] of its height; a page without (`start`
    /// = `end`, after the end of the document) all of it.
    #[must_use]
    pub fn fragments_for_page(
        &self,
        start: f32,
        end: f32,
        flow_height: f32,
        height: f32,
        rest: &mut Vec<NoteRest>,
    ) -> Vec<NoteFragment> {
        let mut queue = core::mem::take(rest);
        let only_notes = flow_height <= EPS;
        if !only_notes {
            queue.extend(self.on_page(start, end).into_iter().map(|(i, _)| (i, 0.0)));
        }
        let room = if only_notes {
            height - self.area.chrome()
        } else {
            (height - flow_height - self.area.chrome()).min(self.area.content_max(height))
        };
        let (placed, left) = self.fill_area(&queue, room, only_notes);
        *rest = left;
        placed
    }

    /// How far the flow of the page that starts at `start` may run when the
    /// page is `height` tall, with the next forced break at `until` and the
    /// footnotes the pages before left over in `rest` (updated to what this
    /// page leaves over).
    ///
    /// First come the notes that go on from the page before, as much of
    /// them as the footnote area may hold ([`FOOTNOTE_AREA_MAX_SHARE`]).
    /// Then the page's own notes are tried in the order of their anchors. A
    /// note whose anchor starts where the flow still is takes its height
    /// from the flow, if its line then still ends above the notes - and so
    /// do the lines of the notes taken before it. The first note that does
    /// not fit ends the page before its line: line and note go to the next
    /// page together (`footnote-policy: line`, this user agent's `auto`).
    ///
    /// A note that fits NO page whole under its line is not moved, it is
    /// SPLIT: it takes the lines of it that fit under its line, and goes on
    /// in the footnote area of the next page. The draft has no rule for
    /// this ("We need an algorithm for laying out footnotes"); under `auto`
    /// "the user agent chooses how to render footnotes, and may place the
    /// footnote body on a later page than the footnote reference". Choices:
    /// a note that fits a page is never split; a split note starts on the
    /// page of its line, where the whole footnote area is free under that
    /// line - else the line goes to the next page first; where the area is
    /// full of what goes on from the pages before, the first line of a page
    /// keeps its place and its note starts on the next page.
    #[must_use]
    pub fn room_for_page(
        &self,
        start: f32,
        height: f32,
        until: Option<f32>,
        rest: &mut Vec<NoteRest>,
    ) -> PageRoom {
        let bottom = start + height;
        // The most the notes may take, and what the area takes with them
        // (its margin, rule and padding - on a page with notes only).
        let area_max = self.area.content_max(height);
        let chrome = self.area.chrome();
        let taken = |notes: f32| self.area.taken(notes);
        let (placed, mut left) = self.fill_area(&core::mem::take(rest), area_max, false);
        let mut reserved: f32 = placed.iter().map(NoteFragment::height).sum();
        // The lowest line end of the notes taken so far.
        let mut lines_end = start;
        let mut room = PageRoom {
            flow_end: bottom,
            ends_before_a_line: false,
        };
        for (index, note) in self.notes.iter().enumerate() {
            if note.anchor_top < start - EPS || until.is_some_and(|u| note.anchor_top >= u) {
                continue;
            }
            if note.anchor_top >= bottom - taken(reserved) - EPS {
                break; // this note and those after it start below the flow
            }
            let first_line = note.anchor_top <= start + FIRST_LINE;
            let line_end = lines_end.max(note.anchor_bottom);
            let line_height = note.anchor_bottom - note.anchor_top;
            let fits_here = left.is_empty()
                && line_end <= bottom - taken(reserved + note.height) + EPS
                && reserved + note.height <= area_max + EPS;
            let fits_a_page = note.height <= area_max + EPS
                && line_height + note.height + chrome <= height + EPS;
            if fits_here {
                reserved += note.height;
                lines_end = line_end;
                continue;
            }
            // How much of it fits under its line, cut between two lines.
            // It starts where the whole footnote area is free under its
            // line (the columns of a multi-column container are planned by
            // that: `multicol::plan_row`); the first line of a page cannot
            // move, there it takes what is under it.
            let part = if left.is_empty() && !fits_a_page {
                let space = if first_line {
                    (bottom - chrome - reserved - line_end).min(area_max - reserved)
                } else if line_end <= bottom - chrome - area_max + EPS {
                    area_max - reserved
                } else {
                    0.0
                };
                self.cut_before(index, 0.0, space.max(0.0))
            } else {
                0.0
            };
            if part > EPS {
                reserved += part;
                lines_end = line_end;
                left.push((index, part));
            } else if first_line {
                // Its line cannot move further up: the note starts on the
                // next page.
                lines_end = line_end;
                left.push((index, 0.0));
            } else {
                room = PageRoom {
                    flow_end: note.anchor_top,
                    ends_before_a_line: true,
                };
                break;
            }
        }
        *rest = left;
        if !room.ends_before_a_line {
            // Never above a line whose note is on the page.
            room.flow_end = (bottom - taken(reserved)).max(lines_end);
        }
        room
    }
}

/// Every footnote box of the layout in `tree`: its layout index and where
/// it is parked and anchored, in no particular order. Empty when the layout
/// was not a paged one with footnotes.
#[must_use]
pub(crate) fn footnote_boxes(
    tree: &LayoutTree,
    positions: &PositionVec,
    styled_dom: &StyledDom,
) -> Vec<(usize, Footnote)> {
    let mut found: Vec<(usize, Footnote)> = Vec::new();
    if !styled_dom.css_property_cache.ptr.paged_footnotes.get() {
        return found;
    }
    // The lines of each paragraph with footnotes, listed once for all of them.
    let mut places = crate::solver3::fc::TreeStaticPlaces::default();
    for (dom_id, indices) in &tree.dom_to_layout {
        if !is_footnote(styled_dom, dom_id) {
            continue;
        }
        let Some(index) = indices.first().map(|i| i.index()) else {
            continue;
        };
        let (Some(node), Some(pos)) = (
            tree.get(LayoutNodeId::new(index)),
            crate::solver3::pos_get(positions, index),
        ) else {
            continue;
        };
        let bp = node.box_props.unpack();
        let size = node.used_size.unwrap_or_default();
        let (mut anchor_top, mut anchor_bottom) = (pos.y, pos.y);
        // The inline formatting context it was taken out of: its parent's,
        // or the one its parent's inline boxes (`<em>`, `<a>`) stand in.
        if let Some(parent) = crate::solver3::fc::inline_context_of(styled_dom, tree, index) {
            let place = places.place(tree, parent, index);
            if let (Some(place), Some(parent_node), Some(parent_pos)) = (
                place,
                tree.get(LayoutNodeId::new(parent)),
                crate::solver3::pos_get(positions, parent),
            ) {
                let pbp = parent_node.box_props.unpack();
                let content_top = parent_pos.y + pbp.border.top + pbp.padding.top;
                anchor_top = content_top + place.line_top;
                anchor_bottom = content_top + place.line_bottom;
            }
        }
        found.push((
            index,
            Footnote {
                node: dom_id,
                top: pos.y - bp.margin.top,
                height: (bp.margin.top + size.height + bp.margin.bottom).max(0.0),
                anchor_top,
                anchor_bottom: anchor_bottom.max(anchor_top),
            },
        ));
    }
    found
}

/// The footnotes of the canvas `display_list` was made from.
///
/// A note's anchor is the line its static position is on when it was taken
/// out of an inline formatting context (`fc::ifc::TreeStaticPlaces`),
/// else the block offset it is parked at.
#[must_use]
pub fn collect_footnotes(
    display_list: &DisplayList,
    tree: &LayoutTree,
    positions: &PositionVec,
    styled_dom: &StyledDom,
    page_size: LogicalSize,
) -> Footnotes {
    let mut found = footnote_boxes(tree, positions, styled_dom);
    if found.is_empty() {
        return Footnotes::default();
    }
    found.sort_by(|(_, a), (_, b)| {
        a.anchor_top
            .total_cmp(&b.anchor_top)
            .then(a.node.cmp(&b.node))
    });
    let by_layout: BTreeMap<usize, u32> = found
        .iter()
        .enumerate()
        .map(|(n, (index, _))| (*index, u32::try_from(n).unwrap_or(u32::MAX)))
        .collect();
    let by_dom: BTreeMap<NodeId, u32> = found
        .iter()
        .enumerate()
        .map(|(n, (_, note))| (note.node, u32::try_from(n).unwrap_or(u32::MAX)))
        .collect();

    // Which note a layout box / a DOM node is in: the nearest one above it.
    let mut layout_owner: BTreeMap<usize, Option<u32>> = BTreeMap::new();
    let mut owner_of_layout = |index: usize| -> Option<u32> {
        if let Some(hit) = layout_owner.get(&index) {
            return *hit;
        }
        let mut current = Some(index);
        let mut owner = None;
        // At most one step per box: a malformed parent chain cannot loop.
        for _ in 0..=tree.nodes.len() {
            let Some(i) = current else { break };
            if let Some(n) = by_layout.get(&i) {
                owner = Some(*n);
                break;
            }
            current = tree.get(LayoutNodeId::new(i)).and_then(|n| n.parent);
        }
        layout_owner.insert(index, owner);
        owner
    };
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut dom_owner: BTreeMap<NodeId, Option<u32>> = BTreeMap::new();
    let mut owner_of_dom = |node: NodeId| -> Option<u32> {
        if let Some(hit) = dom_owner.get(&node) {
            return *hit;
        }
        let mut current = Some(node);
        let mut owner = None;
        for _ in 0..=hierarchy.len() {
            let Some(n) = current else { break };
            if let Some(o) = by_dom.get(&n) {
                owner = Some(*o);
                break;
            }
            current = hierarchy.get(n).and_then(|h| h.parent_id());
        }
        dom_owner.insert(node, owner);
        owner
    };

    // An item belongs to the note its DOM node is in; an item without a
    // node (text of an anonymous box, a clip or stacking marker) to the
    // note the layout box it was painted for is in.
    let owners: Vec<Option<u32>> = (0..display_list.items.len())
        .map(|i| {
            match display_list.node_mapping.get(i).copied().flatten() {
                Some(node) => owner_of_dom(node),
                None => display_list
                    .layout_node_mapping
                    .get(i)
                    .copied()
                    .flatten()
                    .and_then(|(index, _)| owner_of_layout(index)),
            }
        })
        .collect();
    let mut ranges: Vec<Option<(usize, usize)>> = vec![None; found.len()];
    for (i, owner) in owners.iter().enumerate() {
        if let Some(range) = owner.and_then(|o| ranges.get_mut(o as usize)) {
            *range = Some(range.map_or((i, i), |(first, _)| (first, i)));
        }
    }

    // Where a note cannot be cut: inside one of its lines or one of its
    // images. The lines are those of the inline formatting contexts inside
    // the note, each as tall as the items on it (the line boxes themselves
    // are not kept by the text layout; a text item's `line_box` in the
    // display list is its whole paragraph).
    let mut unbreakable: Vec<Vec<(f32, f32)>> = vec![Vec::new(); found.len()];
    for index in 0..tree.nodes.len() {
        let has_lines = tree
            .warm(LayoutNodeId::new(index))
            .is_some_and(|w| w.inline_layout_result.is_some());
        if !has_lines {
            continue;
        }
        let Some(o) = owner_of_layout(index).map(|o| o as usize) else {
            continue;
        };
        let (Some(node), Some(pos), Some(layout), Some((_, note))) = (
            tree.get(LayoutNodeId::new(index)),
            crate::solver3::pos_get(positions, index),
            tree.materialized_inline_layout_for_node(index),
            found.get(o),
        ) else {
            continue;
        };
        let bp = node.box_props.unpack();
        let content_top = pos.y + bp.border.top + bp.padding.top - note.top;
        let mut lines: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
        for item in &layout.items {
            let height = item.item.bounds().height;
            if height.is_nan() || height <= 0.0 {
                continue;
            }
            let (top, bottom) = (
                content_top + item.position.y,
                content_top + item.position.y + height,
            );
            lines
                .entry(item.line_index)
                .and_modify(|(t, b)| {
                    *t = t.min(top);
                    *b = b.max(bottom);
                })
                .or_insert((top, bottom));
        }
        unbreakable[o].extend(lines.into_values());
    }
    for (i, owner) in owners.iter().enumerate() {
        let Some(o) = owner.map(|o| o as usize) else {
            continue;
        };
        let item = &display_list.items[i];
        if !matches!(
            item,
            crate::solver3::display_list::DisplayListItem::Image { .. }
        ) {
            continue;
        }
        if let (Some(rect), Some((_, note))) = (item.bounds(), found.get(o)) {
            if rect.size.height > 0.0 {
                let top = rect.origin.y - note.top;
                unbreakable[o].push((top, top + rect.size.height));
            }
        }
    }
    for ranges in &mut unbreakable {
        ranges.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f32, f32)> = Vec::new();
        for &(top, bottom) in ranges.iter() {
            match merged.last_mut() {
                Some((_, last_bottom)) if top < *last_bottom - EPS => {
                    *last_bottom = last_bottom.max(bottom);
                }
                _ => merged.push((top, bottom)),
            }
        }
        *ranges = merged;
    }

    let (area, rule) = footnote_area(styled_dom, page_size);
    Footnotes {
        notes: found.into_iter().map(|(_, note)| note).collect(),
        owners,
        ranges,
        unbreakable,
        area,
        rule,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn note(node: usize, anchor: (f32, f32), height: f32) -> Footnote {
        Footnote {
            node: NodeId::new(node),
            top: anchor.0,
            height,
            anchor_top: anchor.0,
            anchor_bottom: anchor.1,
        }
    }

    fn notes(notes: Vec<Footnote>) -> Footnotes {
        Footnotes {
            unbreakable: vec![Vec::new(); notes.len()],
            notes,
            owners: Vec::new(),
            ranges: Vec::new(),
            area: FootnoteArea::default(),
            rule: FootnoteRule::default(),
        }
    }

    #[test]
    fn a_page_without_an_anchor_keeps_its_height() {
        let f = notes(vec![note(1, (250.0, 270.0), 30.0)]);
        assert_eq!(
            f.room_for_page(0.0, 200.0, None, &mut Vec::new()),
            PageRoom {
                flow_end: 200.0,
                ends_before_a_line: false
            }
        );
    }

    #[test]
    fn the_notes_of_a_page_take_their_heights_from_its_flow() {
        let f = notes(vec![
            note(1, (20.0, 40.0), 30.0),
            note(2, (60.0, 80.0), 25.0),
        ]);
        assert_eq!(f.room_for_page(0.0, 200.0, None, &mut Vec::new()).flow_end, 145.0);
    }

    #[test]
    fn a_line_whose_note_does_not_fit_under_it_ends_the_page_before_it() {
        // The line 150..170 fits a 200px page, but not above its 40px note.
        let f = notes(vec![note(1, (150.0, 170.0), 40.0)]);
        assert_eq!(
            f.room_for_page(0.0, 200.0, None, &mut Vec::new()),
            PageRoom {
                flow_end: 150.0,
                ends_before_a_line: true
            }
        );
        // On the next page it is the first line: it fits there.
        assert_eq!(f.room_for_page(150.0, 200.0, None, &mut Vec::new()).flow_end, 310.0);
    }

    #[test]
    fn a_note_that_would_push_an_earlier_notes_line_off_does_not_fit() {
        // The first note's line ends at 150; with both notes the flow would
        // end at 140.
        let f = notes(vec![
            note(1, (130.0, 150.0), 30.0),
            note(2, (135.0, 145.0), 30.0),
        ]);
        let room = f.room_for_page(0.0, 200.0, None, &mut Vec::new());
        assert_eq!((room.flow_end, room.ends_before_a_line), (135.0, true));
    }

    /// A note of `lines` 20px lines anchored in the line `anchor`.
    fn note_of_lines(node: usize, anchor: (f32, f32), lines: usize) -> Footnotes {
        #[allow(clippy::cast_precision_loss)] // a line count
        let height = 20.0 * lines as f32;
        #[allow(clippy::cast_precision_loss)]
        let ranges = (0..lines)
            .map(|i| (20.0 * i as f32, 20.0 * (i + 1) as f32))
            .collect();
        Footnotes {
            notes: vec![note(node, anchor, height)],
            owners: Vec::new(),
            ranges: Vec::new(),
            unbreakable: vec![ranges],
            area: FootnoteArea::default(),
            rule: FootnoteRule::default(),
        }
    }

    #[test]
    fn a_note_that_fits_no_page_is_split_between_its_lines_and_goes_on() {
        // Fifteen 20px lines (300px) under a line at 0..20 of a 200px page.
        // The footnote area may take 160px: eight lines. The other seven go
        // on on the next page, which they leave 60px of flow.
        let f = note_of_lines(1, (0.0, 20.0), 15);
        let mut rest = Vec::new();
        let room = f.room_for_page(0.0, 200.0, None, &mut rest);
        assert_eq!((room.flow_end, room.ends_before_a_line), (40.0, false));
        assert_eq!(rest, vec![(0, 160.0)]);
        let room = f.room_for_page(40.0, 200.0, None, &mut rest);
        assert_eq!(room.flow_end, 100.0);
        assert!(rest.is_empty());
    }

    #[test]
    fn a_note_is_not_cut_inside_a_line() {
        // 150px are free under the line at 0..50: seven lines, not seven
        // and a half.
        let f = note_of_lines(1, (0.0, 50.0), 15);
        let mut rest = Vec::new();
        let room = f.room_for_page(0.0, 200.0, None, &mut rest);
        assert_eq!(room.flow_end, 60.0);
        assert_eq!(rest, vec![(0, 140.0)]);
    }

    #[test]
    fn a_note_that_fits_a_page_is_moved_with_its_line_not_split() {
        // Seven lines (140px) do not fit under a line at 100..120 - but
        // they fit a page: the page ends before the line.
        let f = note_of_lines(1, (100.0, 120.0), 7);
        let mut rest = Vec::new();
        let room = f.room_for_page(0.0, 200.0, None, &mut rest);
        assert_eq!((room.flow_end, room.ends_before_a_line), (100.0, true));
        assert!(rest.is_empty());
        let room = f.room_for_page(100.0, 200.0, None, &mut rest);
        assert_eq!((room.flow_end, room.ends_before_a_line), (160.0, false));
        assert!(rest.is_empty());
    }

    #[test]
    fn what_goes_on_from_the_page_before_is_drawn_before_the_pages_own_notes() {
        let mut f = note_of_lines(1, (0.0, 20.0), 15);
        f.notes.push(note(2, (60.0, 80.0), 20.0));
        f.unbreakable.push(vec![(0.0, 20.0)]);
        let mut rest = vec![(0, 160.0)];
        // The page's flow ends at 80: 200px less the 140 + 20 of its notes.
        let fragments = f.fragments_for_page(40.0, 80.0, 40.0, 200.0, &mut rest);
        assert_eq!(
            fragments,
            vec![
                NoteFragment { note: 0, from: 160.0, to: 300.0 },
                NoteFragment { note: 1, from: 0.0, to: 20.0 },
            ]
        );
        assert!(rest.is_empty());
    }

    #[test]
    fn a_page_of_footnotes_only_takes_what_is_left_over() {
        // After the end of the document: no flow, the whole page.
        let f = note_of_lines(1, (0.0, 20.0), 30);
        let mut rest = vec![(0, 160.0)];
        let fragments = f.fragments_for_page(500.0, 500.0, 0.0, 200.0, &mut rest);
        assert_eq!(fragments, vec![NoteFragment { note: 0, from: 160.0, to: 360.0 }]);
        assert_eq!(rest, vec![(0, 360.0)]);
    }

    #[test]
    fn anchors_past_the_next_forced_break_are_not_this_pages() {
        let f = notes(vec![note(1, (120.0, 140.0), 30.0)]);
        assert_eq!(f.room_for_page(0.0, 200.0, Some(100.0), &mut Vec::new()).flow_end, 200.0);
    }

    #[test]
    fn a_pages_notes_are_stacked_in_document_order() {
        // In two columns the right column's first anchor is above the left
        // column's last one, and after it in the document.
        let f = notes(vec![note(7, (10.0, 30.0), 20.0), note(3, (90.0, 110.0), 20.0)]);
        let order: Vec<usize> = f
            .on_page(0.0, 200.0)
            .into_iter()
            .map(|(_, n)| n.node.index())
            .collect();
        assert_eq!(order, vec![3, 7]);
        assert!(f.on_page(200.0, f32::INFINITY).is_empty());
    }
}
