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
//! `footnote-policy` (same draft): this user agent's `auto` is the behaviour
//! of `line` - a line whose note does not fit under it on the page goes to
//! the next page together with the note. The property itself is not
//! implemented.
//!
//! NOT in this version:
//! - `::footnote-call` and `::footnote-marker` and the `footnote` counter: the author writes the
//!   call mark in the text and the mark in the note;
//! - `@page { @footnote { ... } }` (the footnote area cannot be styled or moved), `footnote-display`,
//!   `footnote-policy`;
//! - a note is never split: one taller than what its page leaves is drawn from the end of the
//!   page's flow and runs past the bottom edge;
//! - the margins of two notes stacked on a page do not collapse;
//! - a page sequence with pages of different heights is not tested with notes.

use std::collections::BTreeMap;

use azul_core::{dom::NodeId, styled_dom::StyledDom};

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
}

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

    /// How far the flow of the page that starts at `start` may run when the
    /// page is `height` tall, with the next forced break at `until`.
    ///
    /// The notes are tried in the order of their anchors. A note whose
    /// anchor starts where the flow still is takes its height from the
    /// flow, if its line then still ends above the notes - and so do the
    /// lines of the notes taken before it. The first note that does not fit
    /// ends the page before its line: line and note go to the next page
    /// together (`footnote-policy: line`, this user agent's `auto`).
    ///
    /// A note that does not fit under the FIRST line of a page can fit on
    /// no page: it stays with its line, and is drawn from the end of the
    /// page's flow past the bottom edge (a limit of this implementation:
    /// notes are not split).
    #[must_use]
    pub fn room_for_page(&self, start: f32, height: f32, until: Option<f32>) -> PageRoom {
        let bottom = start + height;
        let mut reserved = 0.0_f32;
        // The lowest line end of the notes taken so far.
        let mut lines_end = start;
        for note in &self.notes {
            if note.anchor_top < start - EPS || until.is_some_and(|u| note.anchor_top >= u) {
                continue;
            }
            if note.anchor_top >= bottom - reserved - EPS {
                break; // this note and those after it start below the flow
            }
            let line_end = lines_end.max(note.anchor_bottom);
            if line_end <= bottom - reserved - note.height + EPS {
                reserved += note.height;
                lines_end = line_end;
            } else if note.anchor_top <= start + FIRST_LINE {
                reserved += note.height;
                lines_end = line_end;
            } else {
                return PageRoom {
                    flow_end: note.anchor_top,
                    ends_before_a_line: true,
                };
            }
        }
        PageRoom {
            // Never above a line whose note is on the page.
            flow_end: (bottom - reserved).max(lines_end),
            ends_before_a_line: false,
        }
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
    for (&dom_id, indices) in &tree.dom_to_layout {
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
            let place = crate::solver3::fc::inline_static_place_in_tree(tree, parent, index);
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
/// out of an inline formatting context (`fc::ifc::inline_static_place_in_tree`),
/// else the block offset it is parked at.
#[must_use]
pub fn collect_footnotes(
    display_list: &DisplayList,
    tree: &LayoutTree,
    positions: &PositionVec,
    styled_dom: &StyledDom,
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

    Footnotes {
        notes: found.into_iter().map(|(_, note)| note).collect(),
        owners,
        ranges,
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
            notes,
            owners: Vec::new(),
            ranges: Vec::new(),
        }
    }

    #[test]
    fn a_page_without_an_anchor_keeps_its_height() {
        let f = notes(vec![note(1, (250.0, 270.0), 30.0)]);
        assert_eq!(
            f.room_for_page(0.0, 200.0, None),
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
        assert_eq!(f.room_for_page(0.0, 200.0, None).flow_end, 145.0);
    }

    #[test]
    fn a_line_whose_note_does_not_fit_under_it_ends_the_page_before_it() {
        // The line 150..170 fits a 200px page, but not above its 40px note.
        let f = notes(vec![note(1, (150.0, 170.0), 40.0)]);
        assert_eq!(
            f.room_for_page(0.0, 200.0, None),
            PageRoom {
                flow_end: 150.0,
                ends_before_a_line: true
            }
        );
        // On the next page it is the first line: it fits there.
        assert_eq!(f.room_for_page(150.0, 200.0, None).flow_end, 310.0);
    }

    #[test]
    fn a_note_that_would_push_an_earlier_notes_line_off_does_not_fit() {
        // The first note's line ends at 150; with both notes the flow would
        // end at 140.
        let f = notes(vec![
            note(1, (130.0, 150.0), 30.0),
            note(2, (135.0, 145.0), 30.0),
        ]);
        let room = f.room_for_page(0.0, 200.0, None);
        assert_eq!((room.flow_end, room.ends_before_a_line), (135.0, true));
    }

    #[test]
    fn a_note_taller_than_the_page_stays_with_the_first_line() {
        let f = notes(vec![note(1, (0.0, 20.0), 500.0)]);
        let room = f.room_for_page(0.0, 200.0, None);
        assert_eq!((room.flow_end, room.ends_before_a_line), (20.0, false));
    }

    #[test]
    fn anchors_past_the_next_forced_break_are_not_this_pages() {
        let f = notes(vec![note(1, (120.0, 140.0), 30.0)]);
        assert_eq!(f.room_for_page(0.0, 200.0, Some(100.0)).flow_end, 200.0);
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
