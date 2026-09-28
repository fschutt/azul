//! Which clip and scroll frames a laid-out box is painted in.
//!
//! Six places used to answer "which ancestors' scroll offsets move this box"
//! with six rules: the display list (`scroll | auto`), the CPU hit tester and
//! `node_rect_to_screen` (layout ancestors with a scroll id), the scroll
//! manager's bar tracks (the same set, registered nodes only),
//! `LayoutWindow::accumulated_scroll` (layout ancestors with ANY scroll state)
//! and `ScrollManager::find_scroll_parent` (DOM ancestors with a state). They
//! agreed only while every offset sat on a `scroll | auto` box and the DOM
//! chain was the layout chain; an offset on an `overflow: hidden` box was
//! hit-tested scrolled and painted unscrolled.
//!
//! A [`ScrollChain`] is the one answer: the boxes above a laid-out node whose
//! clip or scroll frame the node is painted in, outermost first - the order
//! the display list nests them in - each marked with whether its offset moves
//! what is painted inside it. The display list pushes exactly these frames
//! around the node, and the hit tester, the scroll manager and the text paths
//! add back exactly these offsets.

use std::collections::HashMap;

use azul_core::{dom::NodeId, spaces::Inclusivity, styled_dom::StyledDom};

use crate::solver3::{
    getters::{get_overflow_x, get_overflow_y},
    layout_tree::{LayoutNodeId, LayoutTree},
};

/// One box above a laid-out node whose clip or scroll frame the node is
/// painted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScrollChainLink {
    /// The box's index in its dom's layout tree.
    pub layout_index: LayoutNodeId,
    /// The box's DOM node: the key its scroll offset is stored under in the
    /// `ScrollManager`.
    pub node: NodeId,
    /// Does the box's scroll offset move what is painted inside it?
    ///
    /// True for every box the display list opens a `PushScrollFrame` for:
    /// the boxes with a scroll id (`DomLayoutResult::scroll_ids`), except a
    /// `VirtualView`, whose child dom is moved by the `VirtualView` item
    /// instead. False for a box that only clips (`overflow: clip`, an
    /// `overflow: hidden` box whose content fits) - an offset stored on such
    /// a box moves nothing on screen, so nothing may add it back either.
    pub moves_content: bool,
}

/// The boxes above a laid-out node whose clip or scroll frame it is painted
/// in, outermost first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrollChain {
    /// Outermost first: the order the display list nests the frames in.
    pub links: Vec<ScrollChainLink>,
}

impl ScrollChain {
    /// The chain of the box at `index`.
    ///
    /// [`Inclusivity::AncestorsOnly`] is where the node's BOX is painted: a
    /// scroll container's own offset moves its content, never its own border
    /// box. [`Inclusivity::SelfAndAncestors`] is where its CONTENT is
    /// painted: the same chain, plus the node itself when it opens a frame of
    /// its own.
    ///
    /// Walks one anchor per level ([`box_anchor`]), so it costs the depth of
    /// the tree; [`ScrollChains`] computes every node's chain at once.
    #[must_use]
    pub fn of(
        tree: &LayoutTree,
        styled_dom: &StyledDom,
        scroll_ids: &HashMap<LayoutNodeId, u64>,
        index: LayoutNodeId,
        inclusivity: Inclusivity,
    ) -> Self {
        let mut links = Vec::new();
        if inclusivity.includes_self() {
            links.extend(chain_link(tree, styled_dom, scroll_ids, index));
        }
        let mut cur = index;
        // No acyclic anchor walk is longer than the tree: a malformed parent
        // link must not spin.
        let mut budget = tree.nodes.len();
        while let Some((anchor, in_its_content)) = box_anchor(tree, styled_dom, cur) {
            if budget == 0 {
                break;
            }
            budget -= 1;
            if in_its_content {
                links.extend(chain_link(tree, styled_dom, scroll_ids, anchor));
            }
            cur = anchor;
        }
        links.reverse();
        Self { links }
    }

    /// The links whose offsets move painted content - the ones every
    /// "where is this on screen" answer adds back.
    pub fn scrolling(&self) -> impl Iterator<Item = &ScrollChainLink> + '_ {
        self.links.iter().filter(|link| link.moves_content)
    }

    /// Is the box at `index` one of the links?
    #[must_use]
    pub fn contains(&self, index: LayoutNodeId) -> bool {
        self.links.iter().any(|link| link.layout_index == index)
    }
}

/// Every laid-out node's box chain (the [`Inclusivity::AncestorsOnly`]
/// [`ScrollChain`]) computed at once, for the consumers that need all of
/// them - the CPU hit tester and the scroll registration. Linear in the tree:
/// a node's chain is its anchor's chain plus at most one link, so the chains
/// are stored as a trie and a node keeps only the id of its entry.
#[derive(Debug, Clone)]
pub struct ScrollChains {
    /// Entry `k` is the chain `entries[k].0` plus `entries[k].1`. Entry 0
    /// is the empty chain.
    entries: Vec<(u32, Option<ScrollChainLink>)>,
    /// Per layout node: the entry of its box chain.
    box_of: Vec<u32>,
}

impl ScrollChains {
    /// The empty chain's id.
    pub const EMPTY: u32 = 0;

    /// Every node's box chain in `tree`.
    #[must_use]
    pub fn compute(
        tree: &LayoutTree,
        styled_dom: &StyledDom,
        scroll_ids: &HashMap<LayoutNodeId, u64>,
    ) -> Self {
        const UNSET: u32 = u32::MAX;
        let n = tree.nodes.len();
        let anchors: Vec<Option<(LayoutNodeId, bool)>> = (0..n)
            .map(|i| box_anchor(tree, styled_dom, LayoutNodeId::new(i)))
            .collect();
        let mut entries: Vec<(u32, Option<ScrollChainLink>)> = vec![(Self::EMPTY, None)];
        let mut interned: HashMap<(u32, usize), u32> = HashMap::new();
        let mut box_of: Vec<u32> = vec![UNSET; n];
        // A node's CONTENT chain: its box chain, plus itself when it is a
        // link. Memoised - every child of a box asks for it.
        let mut content_of: Vec<u32> = vec![UNSET; n];
        let mut path: Vec<usize> = Vec::new();
        for start in 0..n {
            if box_of[start] != UNSET {
                continue;
            }
            // Walk up the anchors to one whose chain is known (or the root),
            // then resolve downwards: every anchor is an ancestor, so this
            // visits each node once however the tree is numbered.
            path.clear();
            let mut cur = start;
            loop {
                path.push(cur);
                if path.len() > n {
                    break; // a malformed (cyclic) parent chain
                }
                match anchors[cur] {
                    Some((anchor, _)) if anchor.index() < n && box_of[anchor.index()] == UNSET => {
                        cur = anchor.index();
                    }
                    _ => break,
                }
            }
            for &idx in path.iter().rev() {
                if box_of[idx] != UNSET {
                    continue;
                }
                let chain = match anchors[idx] {
                    Some((anchor, in_its_content)) if anchor.index() < n => {
                        let a = anchor.index();
                        if box_of[a] == UNSET {
                            Self::EMPTY // cycle guard tripped; degrade gracefully
                        } else if in_its_content {
                            if content_of[a] == UNSET {
                                content_of[a] = match chain_link(tree, styled_dom, scroll_ids, anchor)
                                {
                                    Some(link) => {
                                        let parent = box_of[a];
                                        *interned.entry((parent, a)).or_insert_with(|| {
                                            let id = u32::try_from(entries.len())
                                                .unwrap_or(u32::MAX);
                                            entries.push((parent, Some(link)));
                                            id
                                        })
                                    }
                                    None => box_of[a],
                                };
                            }
                            content_of[a]
                        } else {
                            box_of[a]
                        }
                    }
                    _ => Self::EMPTY,
                };
                box_of[idx] = chain;
            }
        }
        Self { entries, box_of }
    }

    /// The id of the box chain of the node at `index` (see
    /// [`Self::chain`]); two nodes painted in the same frames share it.
    #[must_use]
    pub fn box_chain_id(&self, index: LayoutNodeId) -> u32 {
        self.box_of
            .get(index.index())
            .copied()
            .filter(|id| (*id as usize) < self.entries.len())
            .unwrap_or(Self::EMPTY)
    }

    /// The chain an id names, outermost first.
    #[must_use]
    pub fn chain(&self, id: u32) -> ScrollChain {
        let mut links = Vec::new();
        let mut cur = id;
        // Every entry's parent is an older entry, so this terminates; the
        // budget only guards a corrupt table.
        let mut budget = self.entries.len();
        while let Some((parent, link)) = self.entries.get(cur as usize) {
            if budget == 0 {
                break;
            }
            budget -= 1;
            match link {
                Some(link) => links.push(*link),
                None => break, // the empty chain
            }
            cur = *parent;
        }
        links.reverse();
        ScrollChain { links }
    }

    /// The box chain of the node at `index` - the same answer
    /// [`ScrollChain::of`] gives with [`Inclusivity::AncestorsOnly`].
    #[must_use]
    pub fn box_chain(&self, index: LayoutNodeId) -> ScrollChain {
        self.chain(self.box_chain_id(index))
    }
}

/// Where the box at `index` is painted: inside the CONTENT of the returned
/// box (`true`) - its clip and scroll frame included - or beside it, in the
/// frames the returned box's own box is painted in (`false`). `None` at the
/// root.
///
/// A box sits in its parent's content.
pub(crate) fn box_anchor(
    tree: &LayoutTree,
    _styled_dom: &StyledDom,
    index: LayoutNodeId,
) -> Option<(LayoutNodeId, bool)> {
    let parent = tree.get(index)?.parent?;
    Some((LayoutNodeId::new(parent), true))
}

/// The link the box at `index` adds to the chains of what is painted in its
/// content, if any: a box that clips on either axis, or one the display
/// list opens a scroll frame for (every box with a scroll id - the viewport's
/// root among them, whose own `overflow: visible` clips nothing).
#[must_use]
pub fn chain_link(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    scroll_ids: &HashMap<LayoutNodeId, u64>,
    index: LayoutNodeId,
) -> Option<ScrollChainLink> {
    let node = tree.get(index)?.dom_node_id?;
    let has_scroll_id = scroll_ids.contains_key(&index);
    let state = styled_dom
        .styled_nodes
        .as_container()
        .get(node)
        .map(|n| n.styled_node_state)
        .unwrap_or_default();
    let clips = get_overflow_x(styled_dom, node, &state).is_clipped()
        || get_overflow_y(styled_dom, node, &state).is_clipped();
    if !clips && !has_scroll_id {
        return None;
    }
    Some(ScrollChainLink {
        layout_index: index,
        node,
        moves_content: has_scroll_id && !is_virtual_view(styled_dom, node),
    })
}

/// Does the display list open a `PushScrollFrame` for the box at `index`?
/// Every box with a scroll id does, except a `VirtualView` (see
/// [`ScrollChainLink::moves_content`]).
#[must_use]
pub fn opens_scroll_frame(
    styled_dom: &StyledDom,
    scroll_ids: &HashMap<LayoutNodeId, u64>,
    index: LayoutNodeId,
    node: NodeId,
) -> bool {
    scroll_ids.contains_key(&index) && !is_virtual_view(styled_dom, node)
}

fn is_virtual_view(styled_dom: &StyledDom, node: NodeId) -> bool {
    styled_dom
        .node_data
        .as_container()
        .get(node)
        .is_some_and(|nd| matches!(nd.get_node_type(), azul_core::dom::NodeType::VirtualView))
}

#[cfg(test)]
mod tests {
    use azul_core::{
        dom::{Dom, DomId},
        geom::LogicalSize,
        resources::RendererResources,
        spaces::Inclusivity,
        styled_dom::StyledDom,
    };
    use rust_fontconfig::FcFontCache;

    use super::{ScrollChain, ScrollChains};
    use crate::{
        callbacks::ExternalSystemCallbacks, solver3::layout_tree::LayoutNodeId,
        window::LayoutWindow, window_state::FullWindowState,
    };

    fn laid_out(dom: Dom) -> LayoutWindow {
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(400.0, 300.0);
        lw.current_window_state = ws.clone();
        lw.layout_and_generate_display_list(
            StyledDom::create_from_dom(dom),
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the fixture lays out");
        lw
    }

    /// `body > outer(auto) > [inner(auto) > leaf, clip(clip) > leaf2]`.
    fn nested() -> Dom {
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("width: 200px; height: 100px; overflow: auto;")
                .with_child(
                    Dom::create_div()
                        .with_css("width: 150px; height: 50px; overflow: auto;")
                        .with_child(Dom::create_div().with_css("height: 400px;")),
                )
                .with_child(
                    Dom::create_div()
                        .with_css("width: 150px; height: 50px; overflow: clip;")
                        .with_child(Dom::create_div().with_css("height: 400px;")),
                ),
        )
    }

    fn layout_index(lw: &LayoutWindow, node: usize) -> LayoutNodeId {
        *lw.layout_results[&DomId::ROOT_ID]
            .layout_tree
            .dom_to_layout
            .get(&azul_core::dom::NodeId::new(node))
            .and_then(|v| v.first())
            .expect("the fixture node is laid out")
    }

    #[test]
    fn a_box_is_painted_in_the_frames_of_the_boxes_above_it_outermost_first() {
        let lw = laid_out(nested());
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        // 0 body, 1 outer, 2 inner, 3 leaf, 4 clip, 5 leaf2
        let chain = ScrollChain::of(
            &lr.layout_tree,
            &lr.styled_dom,
            &lr.scroll_ids,
            layout_index(&lw, 3),
            Inclusivity::AncestorsOnly,
        );
        let nodes: Vec<usize> = chain.links.iter().map(|l| l.node.index()).collect();
        assert_eq!(nodes, vec![1, 2], "outer, then inner: {chain:?}");
        assert!(chain.links.iter().all(|l| l.moves_content), "{chain:?}");

        // A box that only clips is a link that moves nothing.
        let chain = ScrollChain::of(
            &lr.layout_tree,
            &lr.styled_dom,
            &lr.scroll_ids,
            layout_index(&lw, 5),
            Inclusivity::AncestorsOnly,
        );
        let nodes: Vec<(usize, bool)> = chain
            .links
            .iter()
            .map(|l| (l.node.index(), l.moves_content))
            .collect();
        assert_eq!(nodes, vec![(1, true), (4, false)], "{chain:?}");

        // Self-inclusive adds the box's own frame; ancestors-only never does.
        let own = ScrollChain::of(
            &lr.layout_tree,
            &lr.styled_dom,
            &lr.scroll_ids,
            layout_index(&lw, 2),
            Inclusivity::SelfAndAncestors,
        );
        let nodes: Vec<usize> = own.links.iter().map(|l| l.node.index()).collect();
        assert_eq!(nodes, vec![1, 2], "{own:?}");
    }

    /// The bulk table and the per-node walk are one rule: every consumer
    /// reads the same chain whichever of the two it asks.
    #[test]
    fn every_nodes_chain_in_the_table_is_the_chain_its_own_walk_finds() {
        let lw = laid_out(nested());
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        let chains = ScrollChains::compute(&lr.layout_tree, &lr.styled_dom, &lr.scroll_ids);
        for i in 0..lr.layout_tree.nodes.len() {
            let index = LayoutNodeId::new(i);
            assert_eq!(
                chains.box_chain(index),
                ScrollChain::of(
                    &lr.layout_tree,
                    &lr.styled_dom,
                    &lr.scroll_ids,
                    index,
                    Inclusivity::AncestorsOnly,
                ),
                "layout node {i}"
            );
        }
        assert!(chains.box_chain(LayoutNodeId::new(9_999)).links.is_empty());
    }
}
