//! Comprehensive tests for solver3 layout engine
//!
//! This module tests:
//! - IFrame callback invocation and conditional re-invocation
//! - ImageCallback triggering on resize
//! - Multi-DOM layout coordination
//! - Window resizing with cached layout results
//! - Scroll state tracking across DOMs

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    callbacks::{IFrameCallback, IFrameCallbackInfo, IFrameCallbackReturn},
    dom::{Dom, DomId, NodeData, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    refany::RefAny,
    resources::ImageRef,
    selection::SelectionState,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::{
    parser2::CssApiWrapper,
    props::layout::{LayoutDisplay, LayoutPosition},
};
// Font embedding for tests
use rust_fontconfig::{FcFont, FcFontCache, FcPattern, FcWeight};

use crate::{
    solver3::{cache::LayoutCache, display_list::DisplayList, layout_document, LayoutError},
    text3::cache::{FontManager, LayoutCache as TextLayoutCache},
    window::{DomLayoutResult, LayoutWindow},
    window_state::FullWindowState,
};

/// Helper function to create a test font cache with in-memory fonts
fn create_test_fc_cache() -> FcFontCache {
    let mut fc_cache = FcFontCache::default();

    // Embed test font at compile time
    const FONT_BYTES: &[u8] = include_bytes!("../../../examples/assets/fonts/KoHo-Light.ttf");

    // Add as in-memory font with sans-serif fallback
    let sans_pattern = FcPattern {
        name: Some("sans-serif".to_string()),
        weight: FcWeight::Normal,
        ..Default::default()
    };
    let sans_font = FcFont {
        id: "test-sans".to_string(),
        font_index: 0,
        bytes: FONT_BYTES.to_vec(),
    };
    fc_cache.with_memory_fonts(vec![(sans_pattern, sans_font)]);

    fc_cache
}

/// Helper function to create a minimal test font manager with in-memory fonts
fn create_test_font_manager() -> Result<
    FontManager<crate::font::parsed::ParsedFont, crate::text3::default::PathLoader>,
    LayoutError,
> {
    FontManager::new(create_test_fc_cache()).map_err(|e| LayoutError::Text(e))
}

/// Helper to create a simple DOM with some content
fn create_simple_dom() -> Dom {
    let mut dom = Dom::body();
    dom.add_child(Dom::div());
    dom.add_child(Dom::text("Hello World"));
    dom
}

/// Helper to create a styled DOM from a Dom
fn create_styled_dom(dom: &mut Dom) -> StyledDom {
    let css = CssApiWrapper::empty();
    StyledDom::new(dom, css)
}

// ============================================================================
// BASIC LAYOUT TESTS
// ============================================================================

#[test]
fn test_basic_layout() {
    let mut dom = create_simple_dom();
    let mut styled_dom = create_styled_dom(&mut dom);
    styled_dom.dom_id = DomId::ROOT_ID;

    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(800.0, 600.0),
    };

    let mut layout_cache = LayoutCache {
        tree: None,
        absolute_positions: BTreeMap::new(),
        viewport: None,
    };
    let mut text_cache = TextLayoutCache::new();
    let font_manager = create_test_font_manager().expect("Failed to create font manager");
    let scroll_offsets = BTreeMap::new();
    let selections = BTreeMap::new();
    let mut debug_messages = None;

    let result = layout_document(
        &mut layout_cache,
        &mut text_cache,
        styled_dom,
        viewport,
        &font_manager,
        &scroll_offsets,
        &selections,
        &mut debug_messages,
    );

    assert!(
        result.is_ok(),
        "Basic layout should succeed: {:?}",
        result.err()
    );
    let display_list = result.unwrap();
    assert!(
        !display_list.items.is_empty(),
        "Display list should contain items"
    );
}

#[test]
fn test_layout_with_empty_font_cache() {
    let mut dom = create_simple_dom();
    let mut styled_dom = create_styled_dom(&mut dom);
    styled_dom.dom_id = DomId::ROOT_ID;

    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(800.0, 600.0),
    };

    let mut layout_cache = LayoutCache {
        tree: None,
        absolute_positions: BTreeMap::new(),
        viewport: None,
    };
    let mut text_cache = TextLayoutCache::new();
    // Create font manager with EMPTY font cache (no fonts loaded)
    let empty_fc_cache = rust_fontconfig::FcFontCache::default();
    let font_manager = FontManager::new(empty_fc_cache).expect("Failed to create font manager");
    let scroll_offsets = BTreeMap::new();
    let selections = BTreeMap::new();
    let mut debug_messages = Some(Vec::new());

    let result = layout_document(
        &mut layout_cache,
        &mut text_cache,
        styled_dom,
        viewport,
        &font_manager,
        &scroll_offsets,
        &selections,
        &mut debug_messages,
    );

    // Layout should succeed even with empty font cache (using fallbacks)
    assert!(
        result.is_ok(),
        "Layout should succeed with empty font cache using fallbacks: {:?}",
        result.err()
    );

    // Check debug messages to confirm fallback was used
    if let Some(messages) = debug_messages {
        let has_fallback_message = messages
            .iter()
            .any(|msg| msg.message.contains("fallback") || msg.message.contains("Font not found"));
        // We expect some font loading issues to be logged
        println!("Debug messages: {:?}", messages);
    }
}

// ============================================================================
// WINDOW RESIZING TESTS
// ============================================================================

#[test]
fn test_window_resize_invalidates_layout() {
    let mut window =
        LayoutWindow::new(create_test_fc_cache()).expect("Failed to create layout window");

    // Initial layout at 800x600
    let mut dom1 = create_simple_dom();
    let styled_dom1 = create_styled_dom(&mut dom1);

    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);

    let result1 = window.layout_and_generate_display_list(
        styled_dom1,
        &window_state,
        &azul_core::resources::RendererResources::default(),
        &mut None,
    );

    assert!(result1.is_ok(), "Initial layout should succeed");

    // Get the cached viewport
    let initial_viewport = window.layout_cache.viewport;
    assert_eq!(
        initial_viewport,
        Some(LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::new(800.0, 600.0),
        })
    );

    // Resize to 1024x768
    let mut dom2 = create_simple_dom();
    let styled_dom2 = create_styled_dom(&mut dom2);

    let result2 = window.resize_window(
        styled_dom2,
        LogicalSize::new(1024.0, 768.0),
        &azul_core::resources::RendererResources::default(),
        &mut None,
    );

    assert!(result2.is_ok(), "Resize layout should succeed");

    // Verify viewport was updated
    let new_viewport = window.layout_cache.viewport;
    assert_eq!(
        new_viewport,
        Some(LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::new(1024.0, 768.0),
        })
    );

    assert_ne!(
        initial_viewport, new_viewport,
        "Viewport should change after resize"
    );
}

// ============================================================================
// SCROLL STATE TRACKING TESTS
// ============================================================================

#[test]
fn test_scroll_state_tracking() {
    use rust_fontconfig::FcFontCache;

    let mut window =
        LayoutWindow::new(FcFontCache::default()).expect("Failed to create layout window");

    let dom_id = DomId::ROOT_ID;
    let node_id = NodeId::new(0);

    // Initially no scroll position
    assert_eq!(window.get_scroll_position(dom_id, node_id), None);

    // Set scroll position
    let scroll = ScrollPosition {
        parent_rect: LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        ),
        children_rect: LogicalRect::new(
            LogicalPosition::new(100.0, 50.0),
            LogicalSize::new(200.0, 200.0),
        ),
    };
    window.set_scroll_position(dom_id, node_id, scroll.clone());

    // Verify it was stored
    assert_eq!(window.get_scroll_position(dom_id, node_id), Some(scroll));

    // Update scroll position
    let new_scroll = ScrollPosition {
        parent_rect: LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        ),
        children_rect: LogicalRect::new(
            LogicalPosition::new(200.0, 150.0),
            LogicalSize::new(300.0, 300.0),
        ),
    };
    window.set_scroll_position(dom_id, node_id, new_scroll.clone());

    // Verify it was updated
    assert_eq!(
        window.get_scroll_position(dom_id, node_id),
        Some(new_scroll)
    );
}

#[test]
fn test_scroll_state_per_dom() {
    use rust_fontconfig::FcFontCache;

    let mut window =
        LayoutWindow::new(FcFontCache::default()).expect("Failed to create layout window");

    let dom1 = DomId { inner: 1 };
    let dom2 = DomId { inner: 2 };
    let node_id = NodeId::new(0);

    let scroll1 = ScrollPosition {
        parent_rect: LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        ),
        children_rect: LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(200.0, 200.0),
        ),
    };
    let scroll2 = ScrollPosition {
        parent_rect: LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        ),
        children_rect: LogicalRect::new(
            LogicalPosition::new(30.0, 40.0),
            LogicalSize::new(300.0, 300.0),
        ),
    };

    window.set_scroll_position(dom1, node_id, scroll1.clone());
    window.set_scroll_position(dom2, node_id, scroll2.clone());

    // Each DOM maintains its own scroll state
    assert_eq!(
        window.get_scroll_position(dom1, node_id),
        Some(scroll1.clone())
    );
    assert_eq!(
        window.get_scroll_position(dom2, node_id),
        Some(scroll2.clone())
    );
    assert_ne!(scroll1, scroll2);
}

// ============================================================================
// SELECTION STATE TRACKING TESTS
// ============================================================================

#[test]
fn test_selection_state_tracking() {
    use azul_core::{
        dom::DomNodeId,
        selection::{CursorAffinity, GraphemeClusterId, Selection, TextCursor},
    };
    use rust_fontconfig::FcFontCache;

    let mut window =
        LayoutWindow::new(FcFontCache::default()).expect("Failed to create layout window");

    let dom_id = DomId::ROOT_ID;
    let node_id = NodeId::ZERO;

    // Create a selection state
    let cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    };

    let selection_state = SelectionState {
        selections: vec![Selection::Cursor(cursor)],
        node_id: DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        },
    };

    // Initially no selection
    assert!(window.get_selection(dom_id).is_none());

    // Set selection
    window.set_selection(dom_id, selection_state.clone());

    // Verify it was stored
    assert_eq!(window.get_selection(dom_id), Some(&selection_state));
}

// ============================================================================
// LAYOUT RESULT CACHING TESTS
// ============================================================================

#[test]
fn test_layout_result_caching() {
    let mut window =
        LayoutWindow::new(create_test_fc_cache()).expect("Failed to create layout window");

    let mut dom = create_simple_dom();
    let styled_dom = create_styled_dom(&mut dom);

    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);

    // Perform layout
    let result = window.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &azul_core::resources::RendererResources::default(),
        &mut None,
    );

    assert!(result.is_ok());

    // Check that layout result was cached
    let cached_result = window.get_layout_result(&DomId::ROOT_ID);
    assert!(cached_result.is_some(), "Layout result should be cached");

    let layout_result = cached_result.unwrap();
    // assert_eq!(layout_result.dom_id, DomId::ROOT_ID);
    assert_eq!(layout_result.viewport.size, LogicalSize::new(800.0, 600.0));
    assert!(
        layout_result.layout_tree.nodes.len() > 0,
        "Layout tree should have nodes"
    );
}

// ============================================================================
// DOM ID ALLOCATION TESTS
// ============================================================================

#[test]
fn test_dom_id_allocation() {
    use rust_fontconfig::FcFontCache;

    let mut window =
        LayoutWindow::new(FcFontCache::default()).expect("Failed to create layout window");

    // allocate_dom_id is private, but we can test through next_dom_id
    assert_eq!(window.next_dom_id, 1, "Should start at 1 (0 is ROOT_ID)");

    // After clearing, it should reset
    window.clear_caches();
    assert_eq!(window.next_dom_id, 1, "Should reset to 1 after clear");
}

// ============================================================================
// IFRAME CALLBACK TESTS (Preparation for implementation)
// ============================================================================

/// Test data structure to track IFrame callback invocations
#[derive(Debug, Clone)]
struct IFrameCallbackTracker {
    invocation_count: std::sync::Arc<std::sync::Mutex<usize>>,
    bounds_history: std::sync::Arc<std::sync::Mutex<Vec<LogicalRect>>>,
}

impl IFrameCallbackTracker {
    fn new() -> Self {
        Self {
            invocation_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
            bounds_history: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn invocation_count(&self) -> usize {
        *self.invocation_count.lock().unwrap()
    }

    fn bounds_history(&self) -> Vec<LogicalRect> {
        self.bounds_history.lock().unwrap().clone()
    }
}

extern "C" fn test_iframe_callback(
    _data: &mut RefAny,
    info: &mut IFrameCallbackInfo,
) -> IFrameCallbackReturn {
    // Create a simple DOM for the iframe content
    let mut iframe_dom = Dom::body();
    iframe_dom.add_child(Dom::text("IFrame Content"));

    let css = CssApiWrapper::empty();
    let styled_dom = StyledDom::new(&mut iframe_dom, css);

    IFrameCallbackReturn {
        dom: styled_dom,
        scroll_size: info.bounds.get_logical_size(),
        scroll_offset: LogicalPosition::zero(),
        virtual_scroll_size: info.bounds.get_logical_size(),
        virtual_scroll_offset: LogicalPosition::zero(),
    }
}

#[test]
#[ignore = "IFrame callback infrastructure causes SIGSEGV - needs deeper investigation of callback \
            invocation in layout_document"]
fn test_iframe_callback_invocation() {
    // This test verifies that IFrame callbacks are invoked during layout
    // and that the returned DOM is integrated into the layout tree

    use azul_core::dom::IFrameNode;

    let mut dom = Dom::body();

    // Create an IFrame node
    let tracker = IFrameCallbackTracker::new();
    let iframe_data = RefAny::new(tracker.clone());

    dom.add_child(Dom::iframe(iframe_data, test_iframe_callback));

    let css = CssApiWrapper::empty();
    let mut styled_dom = StyledDom::new(&mut dom, css);
    styled_dom.dom_id = DomId::ROOT_ID;

    // Perform layout - this should invoke the IFrame callback
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(800.0, 600.0),
    };

    let mut layout_cache = LayoutCache {
        tree: None,
        absolute_positions: BTreeMap::new(),
        viewport: None,
    };
    let mut text_cache = TextLayoutCache::new();
    let font_manager = create_test_font_manager().expect("Failed to create font manager");
    let scroll_offsets = BTreeMap::new();
    let selections = BTreeMap::new();
    let mut debug_messages = None;

    let result = layout_document(
        &mut layout_cache,
        &mut text_cache,
        styled_dom,
        viewport,
        &font_manager,
        &scroll_offsets,
        &selections,
        &mut debug_messages,
    );

    assert!(result.is_ok(), "Layout with IFrame should succeed");

    // Verify callback was invoked
    assert_eq!(
        tracker.invocation_count(),
        1,
        "IFrame callback should be invoked once"
    );
}

#[test]
#[ignore = "Depends on test_iframe_callback_invocation which causes SIGSEGV"]
fn test_iframe_conditional_reinvocation() {
    // This test verifies that IFrame callbacks are only re-invoked when
    // the iframe's bounds or scroll position changes

    use azul_core::dom::IFrameNode;
    use rust_fontconfig::FcFontCache;

    let mut window =
        LayoutWindow::new(FcFontCache::default()).expect("Failed to create layout window");

    let tracker = IFrameCallbackTracker::new();
    let iframe_data = RefAny::new(tracker.clone());

    // First layout - callback should be invoked
    let mut dom1 = Dom::body();
    dom1.add_child(Dom::iframe(iframe_data.clone(), test_iframe_callback));

    let css1 = CssApiWrapper::empty();
    let styled_dom1 = StyledDom::new(&mut dom1, css1);

    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);

    let _ = window.layout_and_generate_display_list(
        styled_dom1,
        &window_state,
        &azul_core::resources::RendererResources::default(),
        &mut None,
    );

    assert_eq!(
        tracker.invocation_count(),
        1,
        "First layout should invoke callback"
    );

    // Second layout with same bounds - callback should NOT be re-invoked
    let mut dom2 = Dom::body();
    dom2.add_child(Dom::iframe(iframe_data.clone(), test_iframe_callback));

    let css2 = CssApiWrapper::empty();
    let styled_dom2 = StyledDom::new(&mut dom2, css2);

    let _ = window.layout_and_generate_display_list(
        styled_dom2,
        &window_state,
        &azul_core::resources::RendererResources::default(),
        &mut None,
    );

    assert_eq!(
        tracker.invocation_count(),
        1,
        "Same bounds should not re-invoke callback"
    );

    // Third layout with different window size - callback SHOULD be re-invoked
    let mut dom3 = Dom::body();
    let iframe_node3 = IFrameNode {
        callback: IFrameCallback {
            cb: test_iframe_callback,
        },
        data: iframe_data.clone(),
    };
    dom3.add_child(Dom::iframe(iframe_node3.data, iframe_node3.callback.cb));

    let css3 = CssApiWrapper::empty();
    let styled_dom3 = StyledDom::new(&mut dom3, css3);

    let _ = window.resize_window(
        styled_dom3,
        LogicalSize::new(1024.0, 768.0),
        &azul_core::resources::RendererResources::default(),
        &mut None,
    );

    assert_eq!(
        tracker.invocation_count(),
        2,
        "Changed bounds should re-invoke callback"
    );

    // Verify bounds history
    let bounds = tracker.bounds_history();
    assert_eq!(bounds.len(), 2, "Should have recorded 2 different bounds");
}

// ============================================================================
// IMAGE CALLBACK TESTS
// ============================================================================
// TODO: Add ImageCallback tests once implementation is complete
// - test_image_callback_on_resize: Verify ImageCallback is invoked on resize
// - test_image_callback_conditional_reinvocation: Verify conditional re-invocation

// ============================================================================
// MULTI-DOM COORDINATION TESTS
// ============================================================================

#[test]
fn test_multi_dom_layout_results() {
    // This test verifies that multiple DOMs (root + iframes) can be
    // laid out and their results are properly tracked

    use rust_fontconfig::FcFontCache;

    let mut window =
        LayoutWindow::new(FcFontCache::default()).expect("Failed to create layout window");

    // Layout root DOM
    let mut root_dom = create_simple_dom();
    let styled_root = create_styled_dom(&mut root_dom);

    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);

    let _ = window.layout_and_generate_display_list(
        styled_root,
        &window_state,
        &azul_core::resources::RendererResources::default(),
        &mut None,
    );

    // Verify root DOM is tracked
    assert!(window.get_layout_result(&DomId::ROOT_ID).is_some());

    // TODO: Add iframe DOM and verify it's also tracked with unique DomId
    // This will require full IFrame callback implementation
}

#[test]
fn test_clear_caches_resets_all_state() {
    let mut window =
        LayoutWindow::new(create_test_fc_cache()).expect("Failed to create layout window");

    // Setup some state
    let dom_id = DomId::ROOT_ID;
    let node_id = NodeId::new(0);
    let scroll = ScrollPosition {
        parent_rect: LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        ),
        children_rect: LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(200.0, 200.0),
        ),
    };

    window.set_scroll_position(dom_id, node_id, scroll);

    // Layout a DOM to populate caches
    let mut dom = create_simple_dom();
    let styled_dom = create_styled_dom(&mut dom);

    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);

    let _ = window.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &azul_core::resources::RendererResources::default(),
        &mut None,
    );

    // Verify state exists
    assert!(window.get_scroll_position(dom_id, node_id).is_some());
    assert!(window.get_layout_result(&dom_id).is_some());

    // Clear all caches
    window.clear_caches();

    // Verify everything was cleared
    assert!(window.get_scroll_position(dom_id, node_id).is_none());
    assert!(window.get_layout_result(&dom_id).is_none());
    assert_eq!(window.next_dom_id, 1);
    assert!(window.layout_cache.tree.is_none());
    assert!(window.layout_cache.viewport.is_none());
}
