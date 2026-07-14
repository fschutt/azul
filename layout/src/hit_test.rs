//! Hit-testing logic for layout windows
//!
//! This module handles determining which DOM nodes are under the mouse cursor
//! and resolving the cursor icon based on CSS cursor properties.
//!
//! ## Cursor Resolution Algorithm
//!
//! WebRender returns hit-test results in **front-to-back** order:
//! - `depth = 0` is the frontmost/topmost element (closest to the user)
//! - Higher depth values are further back in the z-order
//!
//! The algorithm finds the **frontmost** node that has an explicit CSS `cursor`
//! property set. If no node has a cursor property, we check if the node has
//! text children and use their cursor property (typically `cursor:text`).
//!
//! ## Design Principles
//!
//! 1. **Frontmost priority**: The node closest to the user (lowest depth) takes
//!    precedence. This matches browser behavior where a button's cursor:pointer
//!    overrides any parent's cursor setting.
//!
//! 2. **Text-child inheritance**: Text nodes are inline and don't get hit-test areas.
//!    Their container inherits the text node's cursor if the container has no explicit
//!    cursor property. This shows I-beam cursor over text containers.
//!
//! 3. **Explicit cursor wins**: If a container has an explicit cursor property
//!    (like `cursor:pointer` on a button), it overrides any text-child cursor.

// Re-export FullHitTest for use by other layout modules
pub use azul_core::hit_test::FullHitTest;
use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    hit_test::{HitTest, HitTestItem},
    window::MouseCursorType,
};
use azul_css::props::style::StyleCursor;

use crate::window::LayoutWindow;

/// Result of cursor type hit-testing, determines which mouse cursor to display
#[derive(Copy, Debug, Clone, Default, PartialEq, Eq)]
pub struct CursorTypeHitTest {
    /// The node that has a non-default cursor property (if any)
    pub cursor_node: Option<(DomId, NodeId)>,
    /// The mouse cursor type to display
    pub cursor_icon: MouseCursorType,
}

impl CursorTypeHitTest {
    /// Create a new cursor type hit-test from a full hit-test and layout window.
    ///
    /// Finds the frontmost (lowest depth) node with a cursor property by checking
    /// `cursor_hit_test_nodes` (text runs) and `regular_hit_test_nodes` (DOM nodes).
    pub fn new(hit_test: &FullHitTest, layout_window: &LayoutWindow) -> Self {
        use azul_core::hit_test::CursorType;
        
        let mut cursor_node = None;
        let mut cursor_icon = MouseCursorType::Default;
        // Start with MAX so any node with a cursor property will be selected
        let mut best_depth: u32 = u32::MAX;

        // Iterate through all hovered nodes across all DOMs
        for (dom_id, hit_nodes) in &hit_test.hovered_nodes {
            // Get the layout result for this DOM
            let Some(layout_result) = layout_window.get_layout_result(dom_id) else {
                continue;
            };

            let styled_dom = &layout_result.styled_dom;
            let node_data_container = styled_dom.node_data.as_container();
            let styled_nodes = styled_dom.styled_nodes.as_container();

            // Check cursor_hit_test_nodes (direct text run hits with cursor
            // type encoded in the tag, no CSS lookup needed)
            for (node_id, cursor_hit) in &hit_nodes.cursor_hit_test_nodes {
                let node_depth = cursor_hit.hit_depth;
                
                // Only consider if it's in front of our current best
                if node_depth >= best_depth {
                    continue;
                }
                
                // Convert CursorType to MouseCursorType
                let mouse_cursor = translate_cursor_type(cursor_hit.cursor_type);
                
                // Only use this cursor if it's not the default
                // (allows containers behind text to show their cursor if text has default)
                if mouse_cursor != MouseCursorType::Default {
                    cursor_node = Some((*dom_id, *node_id));
                    cursor_icon = mouse_cursor;
                    best_depth = node_depth;
                }
            }

            // Check regular_hit_test_nodes (DOM nodes with CSS cursor property)
            for (node_id, hit_item) in &hit_nodes.regular_hit_test_nodes {
                let node_depth = hit_item.hit_depth;

                // Only consider this node if it's in front of our current best
                if node_depth >= best_depth {
                    continue;
                }

                // CHECKED access: hit-test results can reference a PREVIOUS
                // generation of a VirtualView child DOM — the child is rebuilt
                // in place with fresh (possibly fewer) NodeIds while the hover
                // state / CPU hit-tester still hold last frame's ids (e.g.
                // panning the MapWidget shrinks the tile grid). Blind indexing
                // panicked here ("len is 25 but the index is 27"); a stale id is
                // skipped instead — the next pointer move re-hit-tests against
                // the fresh tree.
                let (Some(node_data), Some(styled_node)) = (
                    node_data_container.get(*node_id),
                    styled_nodes.get(*node_id),
                ) else {
                    continue;
                };

                // Query the CSS cursor property for this node
                let cursor_prop = styled_dom.get_css_property_cache().get_cursor(
                    node_data,
                    node_id,
                    &styled_node.styled_node_state,
                );
                
                // If this node has an explicit cursor property, use it
                if let Some(cursor_prop) = cursor_prop {
                    let css_cursor = cursor_prop.get_property().copied().unwrap_or_default();
                    cursor_node = Some((*dom_id, *node_id));
                    cursor_icon = translate_cursor(css_cursor);
                    best_depth = node_depth;
                } else {
                    // No explicit `cursor`: editable text (contenteditable / a
                    // <textarea>) defaults to the I-beam, like browsers — so a
                    // multi-line textarea shows the text cursor on hover even
                    // without `cursor: text` in CSS. (A single-line input already
                    // gets the I-beam from its text-run cursor tag / explicit CSS;
                    // this makes them consistent. Does NOT affect the text_input
                    // widget, which sets cursor:text explicitly and so takes the
                    // branch above.)
                    if node_data.is_contenteditable()
                        || matches!(node_data.node_type, azul_core::dom::NodeType::TextArea)
                    {
                        cursor_node = Some((*dom_id, *node_id));
                        cursor_icon = MouseCursorType::Text;
                        best_depth = node_depth;
                    }
                }
            }
        }

        Self {
            cursor_node,
            cursor_icon,
        }
    }
}

/// Translate `CursorType` (from hit-test tag) to `MouseCursorType`
const fn translate_cursor_type(cursor_type: azul_core::hit_test::CursorType) -> MouseCursorType {
    use azul_core::hit_test::CursorType;
    
    match cursor_type {
        CursorType::Default => MouseCursorType::Default,
        CursorType::Pointer => MouseCursorType::Hand,
        CursorType::Text => MouseCursorType::Text,
        CursorType::Crosshair => MouseCursorType::Crosshair,
        CursorType::Move => MouseCursorType::Move,
        CursorType::NotAllowed => MouseCursorType::NotAllowed,
        CursorType::Grab => MouseCursorType::Grab,
        CursorType::Grabbing => MouseCursorType::Grabbing,
        CursorType::EResize => MouseCursorType::EResize,
        CursorType::WResize => MouseCursorType::WResize,
        CursorType::NResize => MouseCursorType::NResize,
        CursorType::SResize => MouseCursorType::SResize,
        CursorType::EwResize => MouseCursorType::EwResize,
        CursorType::NsResize => MouseCursorType::NsResize,
        CursorType::NeswResize => MouseCursorType::NeswResize,
        CursorType::NwseResize => MouseCursorType::NwseResize,
        CursorType::ColResize => MouseCursorType::ColResize,
        CursorType::RowResize => MouseCursorType::RowResize,
        CursorType::Wait => MouseCursorType::Wait,
        CursorType::Help => MouseCursorType::Help,
        CursorType::Progress => MouseCursorType::Progress,
    }
}

/// Translate CSS cursor value to `MouseCursorType`
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
const fn translate_cursor(cursor: StyleCursor) -> MouseCursorType {
    use azul_css::props::style::effects::StyleCursor;

    match cursor {
        StyleCursor::Default => MouseCursorType::Default,
        StyleCursor::Crosshair => MouseCursorType::Crosshair,
        StyleCursor::Pointer => MouseCursorType::Hand,
        StyleCursor::Move => MouseCursorType::Move,
        StyleCursor::Text => MouseCursorType::Text,
        StyleCursor::Wait => MouseCursorType::Wait,
        StyleCursor::Help => MouseCursorType::Help,
        StyleCursor::Progress => MouseCursorType::Progress,
        StyleCursor::ContextMenu => MouseCursorType::ContextMenu,
        StyleCursor::Cell => MouseCursorType::Cell,
        StyleCursor::VerticalText => MouseCursorType::VerticalText,
        StyleCursor::Alias => MouseCursorType::Alias,
        StyleCursor::Copy => MouseCursorType::Copy,
        StyleCursor::Grab => MouseCursorType::Grab,
        StyleCursor::Grabbing => MouseCursorType::Grabbing,
        StyleCursor::AllScroll => MouseCursorType::AllScroll,
        StyleCursor::ZoomIn => MouseCursorType::ZoomIn,
        StyleCursor::ZoomOut => MouseCursorType::ZoomOut,
        StyleCursor::EResize => MouseCursorType::EResize,
        StyleCursor::NResize => MouseCursorType::NResize,
        StyleCursor::SResize => MouseCursorType::SResize,
        StyleCursor::SeResize => MouseCursorType::SeResize,
        StyleCursor::WResize => MouseCursorType::WResize,
        StyleCursor::EwResize => MouseCursorType::EwResize,
        StyleCursor::NsResize => MouseCursorType::NsResize,
        StyleCursor::NeswResize => MouseCursorType::NeswResize,
        StyleCursor::NwseResize => MouseCursorType::NwseResize,
        StyleCursor::ColResize => MouseCursorType::ColResize,
        StyleCursor::RowResize => MouseCursorType::RowResize,
        StyleCursor::Unset => MouseCursorType::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::dom::DomNodeId;
    use azul_core::dom::OptionDomNodeId;

    #[test]
    fn test_full_hit_test_empty() {
        let hit_test = FullHitTest::empty(None);
        assert!(hit_test.is_empty());
        assert!(hit_test.focused_node.is_none());
    }

    #[test]
    fn test_full_hit_test_with_focused_node() {
        let focused = DomNodeId {
            dom: DomId { inner: 0 },
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                NodeId::new(5),
            )),
        };
        let hit_test = FullHitTest::empty(Some(focused));
        assert!(hit_test.is_empty()); // No hovered nodes
        assert_eq!(
            hit_test.focused_node,
            OptionDomNodeId::Some(DomNodeId {
                dom: DomId { inner: 0 },
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    NodeId::new(5),
                )),
            })
        );
    }

    #[test]
    fn test_cursor_type_hit_test_default() {
        let cursor_test = CursorTypeHitTest::default();
        assert_eq!(cursor_test.cursor_icon, MouseCursorType::Default);
        assert!(cursor_test.cursor_node.is_none());
    }

    #[test]
    fn test_translate_cursor_mapping() {
        use azul_css::props::style::effects::StyleCursor;

        assert_eq!(
            translate_cursor(StyleCursor::Default),
            MouseCursorType::Default
        );
        assert_eq!(
            translate_cursor(StyleCursor::Pointer),
            MouseCursorType::Hand
        );
        assert_eq!(translate_cursor(StyleCursor::Text), MouseCursorType::Text);
        assert_eq!(translate_cursor(StyleCursor::Move), MouseCursorType::Move);
        assert_eq!(
            translate_cursor(StyleCursor::Crosshair),
            MouseCursorType::Crosshair
        );
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::collections::{BTreeMap, HashMap};

    use azul_core::{
        dom::{Dom, NodeData, NodeType},
        geom::{LogicalPosition, LogicalRect},
        hit_test::{CursorHitTestItem, CursorType},
        styled_dom::StyledDom,
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    use crate::{
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::DomLayoutResult,
    };

    // ------------------------------------------------------------------
    // Fixture
    // ------------------------------------------------------------------

    // Flatten indices of `fixture_dom()` (pre-order, body first).
    const BODY: usize = 0;
    /// explicit `cursor: pointer`
    const POINTER_DIV: usize = 1;
    /// explicit `cursor: crosshair`
    const CROSSHAIR_DIV: usize = 2;
    /// no cursor property at all
    const PLAIN_DIV: usize = 3;
    /// no cursor property, but `contenteditable`
    const EDITABLE_DIV: usize = 4;
    /// UA stylesheet gives it `cursor: text`
    const TEXTAREA: usize = 5;
    /// explicit `cursor: default`
    const DEFAULT_CURSOR_DIV: usize = 6;
    /// UA stylesheet gives it `cursor: pointer`
    const BUTTON: usize = 7;
    /// One past the last node — every id >= this is stale.
    const NODE_COUNT: usize = 8;

    /// A DOM covering every branch of `CursorTypeHitTest::new`: explicit CSS
    /// cursors, a UA-stylesheet cursor, a node with no cursor at all, and the
    /// two "editable text implies I-beam" node kinds. All cursor-carrying nodes
    /// are *leaves* and *siblings*, so CSS inheritance (cursor is an inherited
    /// property) can never make one of them bleed into another.
    fn fixture_dom() -> StyledDom {
        let dom = Dom::create_body()
            .with_child(Dom::create_div().with_css("cursor: pointer;"))
            .with_child(Dom::create_div().with_css("cursor: crosshair;"))
            .with_child(Dom::create_div())
            .with_child(Dom::create_div().with_contenteditable(true))
            .with_child(Dom::create_node(NodeType::TextArea))
            .with_child(Dom::create_div().with_css("cursor: default;"))
            .with_child(Dom::create_from_data(NodeData::create_button_no_a11y()));
        StyledDom::create_from_dom(dom)
    }

    /// A `DomLayoutResult` with an *empty* layout tree: `CursorTypeHitTest::new`
    /// only ever reads `styled_dom`, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: DisplayList::default(),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// A window holding the fixture under every one of `dom_ids`.
    fn window_with(dom_ids: &[DomId]) -> LayoutWindow {
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        for dom_id in dom_ids {
            lw.layout_results.insert(*dom_id, layout_result(fixture_dom()));
        }
        lw
    }

    fn dom(inner: usize) -> DomId {
        DomId { inner }
    }

    /// `(node index, hit depth)` regular hits + `(node index, cursor type, hit
    /// depth)` text-run hits, grouped per DOM.
    type DomHits<'a> = (DomId, &'a [(usize, u32)], &'a [(usize, CursorType, u32)]);

    fn make_hit_test(entries: &[DomHits<'_>]) -> FullHitTest {
        let mut full = FullHitTest::empty(None);
        for (dom_id, regular, cursors) in entries {
            let mut ht = HitTest::empty();
            for (idx, depth) in *regular {
                ht.regular_hit_test_nodes.insert(
                    NodeId::new(*idx),
                    HitTestItem {
                        point_in_viewport: LogicalPosition::zero(),
                        point_relative_to_item: LogicalPosition::zero(),
                        is_focusable: false,
                        is_virtual_view_hit: None,
                        hit_depth: *depth,
                    },
                );
            }
            for (idx, cursor_type, depth) in *cursors {
                ht.cursor_hit_test_nodes.insert(
                    NodeId::new(*idx),
                    CursorHitTestItem {
                        cursor_type: *cursor_type,
                        hit_depth: *depth,
                        point_in_viewport: LogicalPosition::zero(),
                    },
                );
            }
            full.hovered_nodes.insert(*dom_id, ht);
        }
        full
    }

    /// Hit-test the fixture (loaded as the root DOM) and return the resolved cursor.
    fn resolve(regular: &[(usize, u32)], cursors: &[(usize, CursorType, u32)]) -> CursorTypeHitTest {
        let lw = window_with(&[DomId::ROOT_ID]);
        let hit = make_hit_test(&[(DomId::ROOT_ID, regular, cursors)]);
        CursorTypeHitTest::new(&hit, &lw)
    }

    fn root_node(idx: usize) -> Option<(DomId, NodeId)> {
        Some((DomId::ROOT_ID, NodeId::new(idx)))
    }

    /// The one invariant every result must satisfy: no node selected => no cursor.
    fn assert_invariant(result: &CursorTypeHitTest) {
        if result.cursor_node.is_none() {
            assert_eq!(
                result.cursor_icon,
                MouseCursorType::Default,
                "cursor_node == None must imply the default icon"
            );
        }
    }

    const ALL_CURSOR_TYPES: [CursorType; 21] = [
        CursorType::Default,
        CursorType::Pointer,
        CursorType::Text,
        CursorType::Crosshair,
        CursorType::Move,
        CursorType::NotAllowed,
        CursorType::Grab,
        CursorType::Grabbing,
        CursorType::EResize,
        CursorType::WResize,
        CursorType::NResize,
        CursorType::SResize,
        CursorType::EwResize,
        CursorType::NsResize,
        CursorType::NeswResize,
        CursorType::NwseResize,
        CursorType::ColResize,
        CursorType::RowResize,
        CursorType::Wait,
        CursorType::Help,
        CursorType::Progress,
    ];

    const ALL_STYLE_CURSORS: [StyleCursor; 30] = [
        StyleCursor::Alias,
        StyleCursor::AllScroll,
        StyleCursor::Cell,
        StyleCursor::ColResize,
        StyleCursor::ContextMenu,
        StyleCursor::Copy,
        StyleCursor::Crosshair,
        StyleCursor::Default,
        StyleCursor::EResize,
        StyleCursor::EwResize,
        StyleCursor::Grab,
        StyleCursor::Grabbing,
        StyleCursor::Help,
        StyleCursor::Move,
        StyleCursor::NResize,
        StyleCursor::NsResize,
        StyleCursor::NeswResize,
        StyleCursor::NwseResize,
        StyleCursor::Pointer,
        StyleCursor::Progress,
        StyleCursor::RowResize,
        StyleCursor::SResize,
        StyleCursor::SeResize,
        StyleCursor::Text,
        StyleCursor::Unset,
        StyleCursor::VerticalText,
        StyleCursor::WResize,
        StyleCursor::Wait,
        StyleCursor::ZoomIn,
        StyleCursor::ZoomOut,
    ];

    // ==================================================================
    // Fixture preconditions — if these fail, every assertion below is
    // meaningless, so they are asserted separately and up front.
    // ==================================================================

    #[test]
    fn fixture_node_ids_and_cursor_properties_are_what_the_tests_assume() {
        let styled = fixture_dom();
        let node_data = styled.node_data.as_container();
        assert_eq!(
            node_data.internal.len(),
            NODE_COUNT,
            "fixture flatten order changed — the node index constants are stale"
        );
        assert!(matches!(node_data.internal[BODY].node_type, NodeType::Body));
        assert!(node_data.internal[EDITABLE_DIV].is_contenteditable());
        assert!(matches!(
            node_data.internal[TEXTAREA].node_type,
            NodeType::TextArea
        ));
        assert!(matches!(
            node_data.internal[BUTTON].node_type,
            NodeType::Button
        ));

        // The CSS cascade must actually deliver `cursor` to the property cache,
        // otherwise CursorTypeHitTest can never see it.
        let styled_nodes = styled.styled_nodes.as_container();
        let cache = styled.get_css_property_cache();
        let cursor_of = |idx: usize| {
            let nid = NodeId::new(idx);
            cache
                .get_cursor(
                    &node_data.internal[idx],
                    &nid,
                    &styled_nodes.internal[idx].styled_node_state,
                )
                .and_then(|p| p.get_property().copied())
        };
        assert_eq!(cursor_of(POINTER_DIV), Some(StyleCursor::Pointer));
        assert_eq!(cursor_of(CROSSHAIR_DIV), Some(StyleCursor::Crosshair));
        assert_eq!(cursor_of(DEFAULT_CURSOR_DIV), Some(StyleCursor::Default));
        assert_eq!(
            cursor_of(PLAIN_DIV),
            None,
            "a plain div must have no cursor property, or the contenteditable / \
             text-child fallbacks are unreachable"
        );
        assert_eq!(
            cursor_of(EDITABLE_DIV),
            None,
            "contenteditable must NOT get a cursor from CSS — the I-beam has to \
             come from the node-data fallback branch"
        );
    }

    // ==================================================================
    // translate_cursor_type — total, injective, u8-saturating
    // ==================================================================

    #[test]
    fn translate_cursor_type_maps_every_variant_to_its_documented_icon() {
        let expected = [
            (CursorType::Default, MouseCursorType::Default),
            (CursorType::Pointer, MouseCursorType::Hand),
            (CursorType::Text, MouseCursorType::Text),
            (CursorType::Crosshair, MouseCursorType::Crosshair),
            (CursorType::Move, MouseCursorType::Move),
            (CursorType::NotAllowed, MouseCursorType::NotAllowed),
            (CursorType::Grab, MouseCursorType::Grab),
            (CursorType::Grabbing, MouseCursorType::Grabbing),
            (CursorType::EResize, MouseCursorType::EResize),
            (CursorType::WResize, MouseCursorType::WResize),
            (CursorType::NResize, MouseCursorType::NResize),
            (CursorType::SResize, MouseCursorType::SResize),
            (CursorType::EwResize, MouseCursorType::EwResize),
            (CursorType::NsResize, MouseCursorType::NsResize),
            (CursorType::NeswResize, MouseCursorType::NeswResize),
            (CursorType::NwseResize, MouseCursorType::NwseResize),
            (CursorType::ColResize, MouseCursorType::ColResize),
            (CursorType::RowResize, MouseCursorType::RowResize),
            (CursorType::Wait, MouseCursorType::Wait),
            (CursorType::Help, MouseCursorType::Help),
            (CursorType::Progress, MouseCursorType::Progress),
        ];
        assert_eq!(expected.len(), ALL_CURSOR_TYPES.len());
        for (input, want) in expected {
            assert_eq!(translate_cursor_type(input), want, "{input:?}");
        }
    }

    #[test]
    fn translate_cursor_type_is_injective_so_no_hit_tag_is_aliased() {
        // Two distinct hit-test tags must never collapse onto the same icon:
        // `CursorTypeHitTest::new` treats `MouseCursorType::Default` as "no
        // cursor here", so an accidental alias onto Default would silently drop
        // a text run's cursor.
        let mut seen = std::collections::BTreeSet::new();
        for ct in ALL_CURSOR_TYPES {
            assert!(
                seen.insert(translate_cursor_type(ct)),
                "{ct:?} aliases an icon already produced by another CursorType"
            );
        }
        assert_eq!(seen.len(), ALL_CURSOR_TYPES.len());
    }

    #[test]
    fn only_cursor_type_default_translates_to_the_default_icon() {
        for ct in ALL_CURSOR_TYPES {
            let is_default_icon = translate_cursor_type(ct) == MouseCursorType::Default;
            assert_eq!(
                is_default_icon,
                ct == CursorType::Default,
                "{ct:?}: a non-Default CursorType that maps to the Default icon \
                 would be silently skipped by CursorTypeHitTest::new"
            );
        }
    }

    #[test]
    fn cursor_type_round_trips_through_its_u8_discriminant() {
        for ct in ALL_CURSOR_TYPES {
            assert_eq!(CursorType::from_u8(ct as u8), ct, "{ct:?}");
        }
    }

    #[test]
    fn every_u8_tag_byte_decodes_and_translates_without_panicking() {
        // The cursor type is carried in the low byte of a WebRender ItemTag, so
        // any of the 256 byte values can reach `from_u8` from a stale/corrupt
        // display list. Out-of-range bytes must saturate to Default, not panic.
        for byte in 0u8..=u8::MAX {
            let ct = CursorType::from_u8(byte);
            let icon = translate_cursor_type(ct);
            if byte > 20 {
                assert_eq!(ct, CursorType::Default, "byte {byte} should saturate");
                assert_eq!(icon, MouseCursorType::Default, "byte {byte}");
            } else {
                assert_eq!(ct as u8, byte, "byte {byte} must decode to itself");
            }
        }
    }

    // ==================================================================
    // translate_cursor — total over StyleCursor
    // ==================================================================

    #[test]
    fn translate_cursor_maps_every_style_cursor_to_its_documented_icon() {
        let expected = [
            (StyleCursor::Alias, MouseCursorType::Alias),
            (StyleCursor::AllScroll, MouseCursorType::AllScroll),
            (StyleCursor::Cell, MouseCursorType::Cell),
            (StyleCursor::ColResize, MouseCursorType::ColResize),
            (StyleCursor::ContextMenu, MouseCursorType::ContextMenu),
            (StyleCursor::Copy, MouseCursorType::Copy),
            (StyleCursor::Crosshair, MouseCursorType::Crosshair),
            (StyleCursor::Default, MouseCursorType::Default),
            (StyleCursor::EResize, MouseCursorType::EResize),
            (StyleCursor::EwResize, MouseCursorType::EwResize),
            (StyleCursor::Grab, MouseCursorType::Grab),
            (StyleCursor::Grabbing, MouseCursorType::Grabbing),
            (StyleCursor::Help, MouseCursorType::Help),
            (StyleCursor::Move, MouseCursorType::Move),
            (StyleCursor::NResize, MouseCursorType::NResize),
            (StyleCursor::NsResize, MouseCursorType::NsResize),
            (StyleCursor::NeswResize, MouseCursorType::NeswResize),
            (StyleCursor::NwseResize, MouseCursorType::NwseResize),
            (StyleCursor::Pointer, MouseCursorType::Hand),
            (StyleCursor::Progress, MouseCursorType::Progress),
            (StyleCursor::RowResize, MouseCursorType::RowResize),
            (StyleCursor::SResize, MouseCursorType::SResize),
            (StyleCursor::SeResize, MouseCursorType::SeResize),
            (StyleCursor::Text, MouseCursorType::Text),
            (StyleCursor::Unset, MouseCursorType::Default),
            (StyleCursor::VerticalText, MouseCursorType::VerticalText),
            (StyleCursor::WResize, MouseCursorType::WResize),
            (StyleCursor::Wait, MouseCursorType::Wait),
            (StyleCursor::ZoomIn, MouseCursorType::ZoomIn),
            (StyleCursor::ZoomOut, MouseCursorType::ZoomOut),
        ];
        assert_eq!(expected.len(), ALL_STYLE_CURSORS.len());
        for (input, want) in expected {
            assert_eq!(translate_cursor(input), want, "{input:?}");
        }
    }

    #[test]
    fn default_and_unset_are_the_only_style_cursors_that_yield_the_default_icon() {
        for sc in ALL_STYLE_CURSORS {
            let is_default_icon = translate_cursor(sc) == MouseCursorType::Default;
            let expected = matches!(sc, StyleCursor::Default | StyleCursor::Unset);
            assert_eq!(is_default_icon, expected, "{sc:?}");
        }
    }

    #[test]
    fn the_two_translators_agree_on_the_cursors_they_both_understand() {
        // A text run's cursor comes from the hit-test tag (CursorType) while its
        // container's comes from CSS (StyleCursor). If the two tables disagreed,
        // the icon would flicker depending on which one won the depth race.
        let shared = [
            (CursorType::Default, StyleCursor::Default),
            (CursorType::Pointer, StyleCursor::Pointer),
            (CursorType::Text, StyleCursor::Text),
            (CursorType::Crosshair, StyleCursor::Crosshair),
            (CursorType::Move, StyleCursor::Move),
            (CursorType::Grab, StyleCursor::Grab),
            (CursorType::Grabbing, StyleCursor::Grabbing),
            (CursorType::EResize, StyleCursor::EResize),
            (CursorType::WResize, StyleCursor::WResize),
            (CursorType::NResize, StyleCursor::NResize),
            (CursorType::SResize, StyleCursor::SResize),
            (CursorType::EwResize, StyleCursor::EwResize),
            (CursorType::NsResize, StyleCursor::NsResize),
            (CursorType::NeswResize, StyleCursor::NeswResize),
            (CursorType::NwseResize, StyleCursor::NwseResize),
            (CursorType::ColResize, StyleCursor::ColResize),
            (CursorType::RowResize, StyleCursor::RowResize),
            (CursorType::Wait, StyleCursor::Wait),
            (CursorType::Help, StyleCursor::Help),
            (CursorType::Progress, StyleCursor::Progress),
        ];
        for (ct, sc) in shared {
            assert_eq!(
                translate_cursor_type(ct),
                translate_cursor(sc),
                "{ct:?} / {sc:?} disagree"
            );
        }
    }

    // ==================================================================
    // CursorTypeHitTest::new — degenerate inputs
    // ==================================================================

    #[test]
    fn empty_hit_test_against_an_empty_window_yields_the_default_cursor() {
        let lw = LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        let result = CursorTypeHitTest::new(&FullHitTest::empty(None), &lw);
        assert_eq!(result, CursorTypeHitTest::default());
        assert_invariant(&result);
    }

    #[test]
    fn hits_against_a_dom_without_a_layout_result_are_skipped() {
        // A hovered DomId with no layout result (child DOM torn down between
        // frames) must be skipped, not indexed into.
        let lw = window_with(&[DomId::ROOT_ID]);
        let hit = make_hit_test(&[
            (dom(1), &[(POINTER_DIV, 0)], &[]),
            (dom(usize::MAX), &[(BUTTON, 0)], &[(PLAIN_DIV, CursorType::Text, 0)]),
        ]);
        let result = CursorTypeHitTest::new(&hit, &lw);
        assert_eq!(result.cursor_node, None);
        assert_eq!(result.cursor_icon, MouseCursorType::Default);
        assert_invariant(&result);
    }

    #[test]
    fn stale_node_ids_past_the_end_of_the_dom_are_skipped_instead_of_panicking() {
        // Regression: a VirtualView child rebuilt with fewer nodes leaves the
        // hover state holding last frame's (larger) NodeIds. Blind indexing
        // panicked with "len is 25 but the index is 27".
        let stale = [
            (NODE_COUNT, 0u32),
            (NODE_COUNT + 1, 1),
            (9_999, 2),
            (usize::MAX, 3),
        ];
        let result = resolve(&stale, &[]);
        assert_eq!(result.cursor_node, None);
        assert_eq!(result.cursor_icon, MouseCursorType::Default);
        assert_invariant(&result);
    }

    #[test]
    fn a_live_node_behind_a_stale_one_still_resolves() {
        // The stale id is frontmost (depth 0) but must not consume the search:
        // the live pointer div behind it still gets to set the cursor.
        let result = resolve(&[(usize::MAX, 0), (POINTER_DIV, 1)], &[]);
        assert_eq!(result.cursor_node, root_node(POINTER_DIV));
        assert_eq!(result.cursor_icon, MouseCursorType::Hand);
    }

    #[test]
    fn a_stale_node_id_in_a_text_run_hit_is_reported_verbatim() {
        // cursor_hit_test_nodes are NOT bounds-checked against the DOM (the
        // cursor type is carried in the tag, so no node lookup happens). A stale
        // id therefore surfaces as `cursor_node` — harmless for the icon, but
        // callers must not assume `cursor_node` indexes a live node.
        let result = resolve(&[], &[(usize::MAX, CursorType::Text, 0)]);
        assert_eq!(result.cursor_icon, MouseCursorType::Text);
        assert_eq!(result.cursor_node, root_node(usize::MAX));
    }

    #[test]
    fn a_thousand_hits_with_extreme_depths_do_not_panic() {
        let mut regular = Vec::new();
        let mut cursors = Vec::new();
        for i in 0..1000usize {
            // Mostly stale ids, alternating extreme depths.
            let depth = match i % 4 {
                0 => 0,
                1 => u32::MAX,
                2 => u32::MAX - 1,
                _ => i as u32,
            };
            regular.push((i, depth));
            cursors.push((i, CursorType::from_u8((i % 256) as u8), depth));
        }
        let result = resolve(&regular, &cursors);
        assert_invariant(&result);
        // Whatever wins the depth race, the resolution must be a decision and
        // not a crash — and it must be stable across runs.
        assert_eq!(result, resolve(&regular, &cursors));
    }

    // ==================================================================
    // CursorTypeHitTest::new — depth resolution
    // ==================================================================

    #[test]
    fn the_frontmost_node_wins_regardless_of_iteration_order() {
        // Lower NodeId iterated first, but the deeper one must lose either way.
        let front_is_last = resolve(&[(POINTER_DIV, 5), (CROSSHAIR_DIV, 0)], &[]);
        assert_eq!(front_is_last.cursor_node, root_node(CROSSHAIR_DIV));
        assert_eq!(front_is_last.cursor_icon, MouseCursorType::Crosshair);

        let front_is_first = resolve(&[(POINTER_DIV, 0), (CROSSHAIR_DIV, 5)], &[]);
        assert_eq!(front_is_first.cursor_node, root_node(POINTER_DIV));
        assert_eq!(front_is_first.cursor_icon, MouseCursorType::Hand);
    }

    #[test]
    fn on_a_depth_tie_the_first_iterated_node_keeps_the_cursor() {
        // The guard is `node_depth >= best_depth`, so a tie never replaces the
        // incumbent: with equal depths the lowest NodeId (BTreeMap order) wins.
        let result = resolve(&[(POINTER_DIV, 3), (CROSSHAIR_DIV, 3)], &[]);
        assert_eq!(result.cursor_node, root_node(POINTER_DIV));
        assert_eq!(result.cursor_icon, MouseCursorType::Hand);
    }

    #[test]
    fn a_hit_at_depth_u32_max_can_never_be_selected() {
        // BOUNDARY: best_depth is seeded with u32::MAX and the guard is `>=`, so
        // depth == u32::MAX is unreachable while u32::MAX - 1 is fine. Depths
        // that large are not producible by the real hit-tester, but the seed is
        // an in-band sentinel — worth pinning down.
        let at_max = resolve(&[(POINTER_DIV, u32::MAX)], &[]);
        assert_eq!(at_max.cursor_node, None);
        assert_eq!(at_max.cursor_icon, MouseCursorType::Default);
        assert_invariant(&at_max);

        let below_max = resolve(&[(POINTER_DIV, u32::MAX - 1)], &[]);
        assert_eq!(below_max.cursor_node, root_node(POINTER_DIV));
        assert_eq!(below_max.cursor_icon, MouseCursorType::Hand);
    }

    #[test]
    fn a_text_run_cursor_shadows_a_regular_node_at_the_same_depth() {
        // cursor_hit_test_nodes are processed first, so at equal depth the text
        // run's tag wins and the CSS cursor behind it never applies.
        let result = resolve(&[(POINTER_DIV, 2)], &[(PLAIN_DIV, CursorType::Text, 2)]);
        assert_eq!(result.cursor_icon, MouseCursorType::Text);
        assert_eq!(result.cursor_node, root_node(PLAIN_DIV));
    }

    #[test]
    fn a_regular_node_strictly_in_front_of_a_text_run_wins() {
        let result = resolve(&[(POINTER_DIV, 1)], &[(PLAIN_DIV, CursorType::Text, 2)]);
        assert_eq!(result.cursor_icon, MouseCursorType::Hand);
        assert_eq!(result.cursor_node, root_node(POINTER_DIV));
    }

    #[test]
    fn a_default_text_run_cursor_does_not_shadow_the_container_behind_it() {
        // A frontmost text run tagged `CursorType::Default` must neither set the
        // icon nor lower best_depth — otherwise the button behind it (depth 5)
        // would lose its cursor:pointer.
        let result = resolve(&[(POINTER_DIV, 5)], &[(PLAIN_DIV, CursorType::Default, 0)]);
        assert_eq!(result.cursor_icon, MouseCursorType::Hand);
        assert_eq!(result.cursor_node, root_node(POINTER_DIV));
    }

    // ==================================================================
    // CursorTypeHitTest::new — per-node cursor sources
    // ==================================================================

    #[test]
    fn a_node_without_any_cursor_property_leaves_the_cursor_unset() {
        let result = resolve(&[(BODY, 1), (PLAIN_DIV, 0)], &[]);
        assert_eq!(result.cursor_node, None);
        assert_eq!(result.cursor_icon, MouseCursorType::Default);
        assert_invariant(&result);
    }

    #[test]
    fn a_contenteditable_node_gets_the_ibeam_without_any_css() {
        let result = resolve(&[(EDITABLE_DIV, 0)], &[]);
        assert_eq!(result.cursor_node, root_node(EDITABLE_DIV));
        assert_eq!(result.cursor_icon, MouseCursorType::Text);
    }

    #[test]
    fn a_textarea_gets_the_ibeam() {
        let result = resolve(&[(TEXTAREA, 0)], &[]);
        assert_eq!(result.cursor_node, root_node(TEXTAREA));
        assert_eq!(result.cursor_icon, MouseCursorType::Text);
    }

    #[test]
    fn a_button_gets_the_hand_from_the_ua_stylesheet() {
        let result = resolve(&[(BUTTON, 0)], &[]);
        assert_eq!(result.cursor_node, root_node(BUTTON));
        assert_eq!(result.cursor_icon, MouseCursorType::Hand);
    }

    #[test]
    fn an_explicit_cursor_default_is_recorded_and_shadows_nodes_behind_it() {
        // Contradicts the doc comment ("The node that has a NON-DEFAULT cursor
        // property"): an explicit `cursor: default` sets cursor_node = Some(..)
        // with the Default icon. Behaviourally it is still right — the frontmost
        // explicit cursor must win — but consumers cannot rely on
        // `cursor_node.is_some()` meaning "non-default cursor".
        let alone = resolve(&[(DEFAULT_CURSOR_DIV, 0)], &[]);
        assert_eq!(alone.cursor_node, root_node(DEFAULT_CURSOR_DIV));
        assert_eq!(alone.cursor_icon, MouseCursorType::Default);

        let over_pointer = resolve(&[(DEFAULT_CURSOR_DIV, 0), (POINTER_DIV, 1)], &[]);
        assert_eq!(over_pointer.cursor_node, root_node(DEFAULT_CURSOR_DIV));
        assert_eq!(over_pointer.cursor_icon, MouseCursorType::Default);
    }

    #[test]
    fn an_editable_node_in_front_of_a_pointer_node_still_wins() {
        let result = resolve(&[(EDITABLE_DIV, 0), (BUTTON, 1)], &[]);
        assert_eq!(result.cursor_node, root_node(EDITABLE_DIV));
        assert_eq!(result.cursor_icon, MouseCursorType::Text);
    }

    // ==================================================================
    // CursorTypeHitTest::new — multiple DOMs
    // ==================================================================

    #[test]
    fn the_frontmost_dom_wins_not_the_lowest_dom_id() {
        let lw = window_with(&[DomId::ROOT_ID, dom(1)]);

        // Frontmost hit lives in the *second* DOM.
        let hit = make_hit_test(&[
            (DomId::ROOT_ID, &[(POINTER_DIV, 4)], &[]),
            (dom(1), &[(CROSSHAIR_DIV, 0)], &[]),
        ]);
        let result = CursorTypeHitTest::new(&hit, &lw);
        assert_eq!(result.cursor_node, Some((dom(1), NodeId::new(CROSSHAIR_DIV))));
        assert_eq!(result.cursor_icon, MouseCursorType::Crosshair);

        // ... and in the *first* DOM: the later DOM must not clobber it.
        let hit = make_hit_test(&[
            (DomId::ROOT_ID, &[(CROSSHAIR_DIV, 0)], &[]),
            (dom(1), &[(POINTER_DIV, 4)], &[]),
        ]);
        let result = CursorTypeHitTest::new(&hit, &lw);
        assert_eq!(result.cursor_node, root_node(CROSSHAIR_DIV));
        assert_eq!(result.cursor_icon, MouseCursorType::Crosshair);
    }

    #[test]
    fn a_dom_with_a_layout_result_is_used_even_when_a_sibling_dom_is_missing() {
        let lw = window_with(&[dom(2)]);
        let hit = make_hit_test(&[
            (DomId::ROOT_ID, &[(POINTER_DIV, 0)], &[]),
            (dom(2), &[(BUTTON, 7)], &[]),
        ]);
        let result = CursorTypeHitTest::new(&hit, &lw);
        assert_eq!(result.cursor_node, Some((dom(2), NodeId::new(BUTTON))));
        assert_eq!(result.cursor_icon, MouseCursorType::Hand);
    }

    #[test]
    fn the_result_is_a_pure_function_of_the_hit_test() {
        // Same input twice => same output (no interior mutation of the window).
        let lw = window_with(&[DomId::ROOT_ID]);
        let hit = make_hit_test(&[(
            DomId::ROOT_ID,
            &[(POINTER_DIV, 3), (BUTTON, 9)],
            &[(TEXTAREA, CursorType::Text, 4)],
        )]);
        let first = CursorTypeHitTest::new(&hit, &lw);
        let second = CursorTypeHitTest::new(&hit, &lw);
        assert_eq!(first, second);
        assert_eq!(first.cursor_icon, MouseCursorType::Hand);
    }
}
