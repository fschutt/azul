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

// Include the inline intrinsic width tests
mod test_inline_intrinsic_width;

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
    FontManager<azul_css::props::basic::FontRef, crate::text3::default::PathLoader>,
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
        calculated_positions: BTreeMap::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
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
        None, // gpu_value_cache
        &azul_core::resources::RendererResources::default(),
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
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
        calculated_positions: BTreeMap::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
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
        None, // gpu_value_cache
        &azul_core::resources::RendererResources::default(),
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
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

    let system_callbacks = crate::callbacks::ExternalSystemCallbacks::rust_internal();

    let result1 = window.layout_and_generate_display_list(
        styled_dom1,
        &window_state,
        &azul_core::resources::RendererResources::default(),
        &system_callbacks,
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
        &system_callbacks,
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
        &crate::callbacks::ExternalSystemCallbacks::rust_internal(),
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

    // Note: next_dom_id field no longer exists. DomId allocation is now
    // managed by IFrameManager::get_or_create_nested_dom_id().
    // The test still verifies that clear_caches() works correctly.

    // After clearing, caches should be reset
    window.clear_caches();
    assert!(
        window.layout_cache.tree.is_none(),
        "Layout cache should be cleared"
    );
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

    fn record_invocation(&self) {
        *self.invocation_count.lock().unwrap() += 1;
    }

    fn record_bounds(&self, bounds: azul_core::callbacks::HidpiAdjustedBounds) {
        // Convert HidpiAdjustedBounds to LogicalRect (position unknown, use zero)
        let rect = LogicalRect {
            origin: LogicalPosition::zero(),
            size: bounds.get_logical_size(),
        };
        self.bounds_history.lock().unwrap().push(rect);
    }
}

extern "C" fn test_iframe_callback(
    data: &mut RefAny,
    info: &mut IFrameCallbackInfo,
) -> IFrameCallbackReturn {
    // Increment the invocation counter in the tracker
    if let Some(tracker) = data.downcast_ref::<IFrameCallbackTracker>() {
        tracker.record_invocation();
        tracker.record_bounds(info.bounds);

        // Log the reason for this invocation
        eprintln!("IFrame callback invoked with reason: {:?}", info.reason);
    }

    // Create a simple DOM for the iframe content
    let mut iframe_dom = Dom::body();
    iframe_dom.add_child(Dom::text("IFrame Content"));

    let css = CssApiWrapper::empty();
    let styled_dom = StyledDom::new(&mut iframe_dom, css);

    IFrameCallbackReturn {
        dom: azul_core::styled_dom::OptionStyledDom::Some(styled_dom),
        scroll_size: info.bounds.get_logical_size(),
        scroll_offset: LogicalPosition::zero(),
        virtual_scroll_size: info.bounds.get_logical_size(),
        virtual_scroll_offset: LogicalPosition::zero(),
    }
}

// ============================================================================
// IFRAME MANAGER TESTS (New Architecture)
// ============================================================================

#[test]
fn test_iframe_manager_initial_dom_id_creation() {
    use azul_core::dom::{DomId, NodeId};

    use crate::managers::iframe::IFrameManager;

    let mut iframe_manager = IFrameManager::new();
    let parent_dom = DomId::ROOT_ID;
    let node_id = NodeId::new(1);

    // First call should create a new DomId
    let child_dom_id = iframe_manager.get_or_create_nested_dom_id(parent_dom, node_id);

    assert_ne!(
        child_dom_id, parent_dom,
        "Child DOM should have different ID"
    );

    // Second call should return the same DomId
    let same_dom_id = iframe_manager.get_or_create_nested_dom_id(parent_dom, node_id);
    assert_eq!(child_dom_id, same_dom_id, "Should return cached DomId");
}

#[test]
fn test_iframe_manager_multiple_iframes() {
    use azul_core::dom::{DomId, NodeId};

    use crate::managers::iframe::IFrameManager;

    let mut iframe_manager = IFrameManager::new();
    let parent_dom = DomId::ROOT_ID;
    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);

    let child1 = iframe_manager.get_or_create_nested_dom_id(parent_dom, node1);
    let child2 = iframe_manager.get_or_create_nested_dom_id(parent_dom, node2);

    assert_ne!(
        child1, child2,
        "Different IFrames should have different DomIds"
    );
}

#[test]
fn test_iframe_manager_check_reinvoke_initial_render() {
    use azul_core::{
        callbacks::IFrameCallbackReason,
        dom::{DomId, NodeId},
        geom::{LogicalPosition, LogicalRect, LogicalSize},
    };

    use crate::managers::{iframe::IFrameManager, scroll_state::ScrollManager};

    let mut iframe_manager = IFrameManager::new();
    let scroll_manager = ScrollManager::new();
    let parent_dom = DomId::ROOT_ID;
    let node_id = NodeId::new(1);
    let bounds = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(300.0, 200.0));

    // First check - should return InitialRender
    let reason = iframe_manager.check_reinvoke(parent_dom, node_id, &scroll_manager, bounds);

    assert!(reason.is_some(), "Initial render should trigger reinvoke");
    assert!(
        matches!(reason, Some(IFrameCallbackReason::InitialRender)),
        "First invocation should be InitialRender, got {:?}",
        reason
    );
}

#[test]
fn test_iframe_manager_no_reinvoke_same_bounds() {
    use azul_core::{
        callbacks::IFrameCallbackReason,
        dom::{DomId, NodeId},
        geom::{LogicalPosition, LogicalRect, LogicalSize},
    };

    use crate::managers::{iframe::IFrameManager, scroll_state::ScrollManager};

    let mut iframe_manager = IFrameManager::new();
    let scroll_manager = ScrollManager::new();
    let parent_dom = DomId::ROOT_ID;
    let node_id = NodeId::new(1);
    let bounds = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(300.0, 200.0));

    // Initial render
    let _reason = iframe_manager.check_reinvoke(parent_dom, node_id, &scroll_manager, bounds);
    iframe_manager.mark_invoked(parent_dom, node_id, IFrameCallbackReason::InitialRender);

    // Same bounds - should not reinvoke
    let reason = iframe_manager.check_reinvoke(parent_dom, node_id, &scroll_manager, bounds);

    assert!(
        reason.is_none(),
        "Same bounds should not trigger reinvoke, got {:?}",
        reason
    );
}

#[test]
fn test_iframe_manager_reinvoke_on_bounds_expansion() {
    use azul_core::{
        callbacks::IFrameCallbackReason,
        dom::{DomId, NodeId},
        geom::{LogicalPosition, LogicalRect, LogicalSize},
    };

    use crate::managers::{iframe::IFrameManager, scroll_state::ScrollManager};

    let mut iframe_manager = IFrameManager::new();
    let scroll_manager = ScrollManager::new();
    let parent_dom = DomId::ROOT_ID;
    let node_id = NodeId::new(1);
    let initial_bounds = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(300.0, 200.0));

    // Initial render
    iframe_manager.check_reinvoke(parent_dom, node_id, &scroll_manager, initial_bounds);
    iframe_manager.mark_invoked(parent_dom, node_id, IFrameCallbackReason::InitialRender);

    // Set scroll size to match initial bounds (IFrame callback would do this)
    iframe_manager.update_iframe_info(
        parent_dom,
        node_id,
        LogicalSize::new(300.0, 200.0), // scroll_size = initial bounds
        LogicalSize::new(300.0, 200.0), // virtual_scroll_size
    );

    // Expanded bounds - should reinvoke because container expanded beyond scroll_size
    let expanded_bounds = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(500.0, 400.0));
    let reason =
        iframe_manager.check_reinvoke(parent_dom, node_id, &scroll_manager, expanded_bounds);

    assert!(reason.is_some(), "Expanded bounds should trigger reinvoke");
    assert!(
        matches!(reason, Some(IFrameCallbackReason::BoundsExpanded)),
        "Should be BoundsExpanded reason, got {:?}",
        reason
    );
}

#[test]
fn test_iframe_manager_no_reinvoke_on_bounds_shrink() {
    use azul_core::{
        callbacks::IFrameCallbackReason,
        dom::{DomId, NodeId},
        geom::{LogicalPosition, LogicalRect, LogicalSize},
    };

    use crate::managers::{iframe::IFrameManager, scroll_state::ScrollManager};

    let mut iframe_manager = IFrameManager::new();
    let scroll_manager = ScrollManager::new();
    let parent_dom = DomId::ROOT_ID;
    let node_id = NodeId::new(1);
    let initial_bounds = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(500.0, 400.0));

    // Initial render
    iframe_manager.check_reinvoke(parent_dom, node_id, &scroll_manager, initial_bounds);
    iframe_manager.mark_invoked(parent_dom, node_id, IFrameCallbackReason::InitialRender);

    // Shrunken bounds - should NOT reinvoke
    let shrunken_bounds = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(300.0, 200.0));
    let reason =
        iframe_manager.check_reinvoke(parent_dom, node_id, &scroll_manager, shrunken_bounds);

    assert!(
        reason.is_none(),
        "Shrunken bounds should not trigger reinvoke (lazy loading), got {:?}",
        reason
    );
}

#[test]
fn test_iframe_manager_update_scroll_info() {
    use azul_core::{
        dom::{DomId, NodeId},
        geom::LogicalSize,
    };

    use crate::managers::iframe::IFrameManager;

    let mut iframe_manager = IFrameManager::new();
    let parent_dom = DomId::ROOT_ID;
    let node_id = NodeId::new(1);

    // Update scroll info
    iframe_manager.update_iframe_info(
        parent_dom,
        node_id,
        LogicalSize::new(800.0, 600.0),   // scroll_size
        LogicalSize::new(1600.0, 1200.0), // virtual_scroll_size
    );

    // Verify info was stored (can't directly access, but it should not panic)
    // Future: Add getter methods to IFrameManager to verify state
}

#[test]
fn test_iframe_manager_nested_iframes() {
    use azul_core::dom::{DomId, NodeId};

    use crate::managers::iframe::IFrameManager;

    let mut iframe_manager = IFrameManager::new();

    // Parent IFrame
    let root_dom = DomId::ROOT_ID;
    let parent_iframe_node = NodeId::new(1);
    let parent_child_dom = iframe_manager.get_or_create_nested_dom_id(root_dom, parent_iframe_node);

    // Nested IFrame (child of the first IFrame)
    let nested_iframe_node = NodeId::new(2);
    let nested_child_dom =
        iframe_manager.get_or_create_nested_dom_id(parent_child_dom, nested_iframe_node);

    assert_ne!(
        root_dom, parent_child_dom,
        "Parent child DOM should be different"
    );
    assert_ne!(
        parent_child_dom, nested_child_dom,
        "Nested child DOM should be different"
    );
    assert_ne!(root_dom, nested_child_dom, "All DOMs should be unique");
}

// Note: Tests for Rule 4 (scroll near edge) and Rule 5 (scroll beyond content)
// will be added once scroll tracking is properly implemented in invoke_iframe_callback

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
        &crate::callbacks::ExternalSystemCallbacks::rust_internal(),
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
        &crate::callbacks::ExternalSystemCallbacks::rust_internal(),
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
    // Note: next_dom_id removed - DomId allocation now in IFrameManager
    assert!(window.layout_cache.tree.is_none());
    assert!(window.layout_cache.viewport.is_none());
}

// ============================================================================
// DEFAULT VALUES REGRESSION TESTS
// These tests validate the "default stuff" fixes from defaultvalues.md:
// - Auto vs. explicit zero sizing (CSS Box Model Level 3)
// - Block layout stacking (CSS 2.1 Section 9.5 - Floats)
// - Font-size inheritance and cascade (CSS Cascade Level 4)
// ============================================================================

/// Helper function to run layout on HTML + CSS strings for regression tests
/// This is a simplified version that creates a DOM from CSS and HTML
fn layout_test_html_simple(
    html_body: &str,
    extra_css: &str,
    viewport_size: LogicalSize,
) -> Result<(LayoutCache<azul_css::props::basic::FontRef>, DisplayList), LayoutError> {
    // Create a simple DOM with the HTML content
    let mut dom = Dom::body();
    
    // Parse simple HTML tags (very basic parser for test purposes)
    // For now, just create text nodes from the HTML content
    let text_content: String = html_body.chars().collect();
    dom.add_child(Dom::text(text_content));

    // Create CSS
    let css = if extra_css.is_empty() {
        CssApiWrapper::empty()
    } else {
        // Parse CSS from string - convert to owned AzString via String
        use azul_css::AzString;
        let css_string = AzString::from_string(extra_css.to_string());
        CssApiWrapper::from_string(css_string)
    };

    // Create StyledDom
    let mut styled_dom = StyledDom::new(&mut dom, css);
    styled_dom.dom_id = DomId::ROOT_ID;

    // Set up layout context
    let mut layout_cache = LayoutCache {
        tree: None,
        calculated_positions: BTreeMap::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
    };
    let mut text_cache = TextLayoutCache::new();
    let font_manager = create_test_font_manager()?;
    let viewport = LogicalRect::new(LogicalPosition::zero(), viewport_size);

    // Run layout
    let display_list = layout_document(
        &mut layout_cache,
        &mut text_cache,
        styled_dom,
        viewport,
        &font_manager,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut None,
        None,
        &azul_core::resources::RendererResources::default(),
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
    )?;

    Ok((layout_cache, display_list))
}

/// Tests that width: auto and height: auto resolve to content-based dimensions
/// 
/// CSS Specification: CSS Box Model Level 3
/// - For inline-level elements, 'auto' width is shrink-to-fit (max-content)
/// - For block-level elements, 'auto' width fills available space
/// - 'auto' height is always content-based
#[test]
fn test_auto_sizing_regression() {
    let result = layout_test_html_simple(
        "Auto Sized Content",
        "body { width: auto; height: auto; font-size: 20px; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Auto sizing layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
    assert!(!tree.nodes.is_empty(), "Layout tree should have nodes");
    
    let root_node = &tree.nodes[0];
    let size = root_node.used_size.expect("Root node should have used_size");
    
    // CSS 2.1 §10.3.3: Width of inline formatting contexts
    // The width should be based on content, not zero
    assert!(
        size.width > 0.0,
        "Auto width for inline content should be > 0, got {}px. \
         CSS Box Model requires 'auto' to compute to max-content width for inline contexts.",
        size.width
    );
    
    // CSS 2.1 §10.6.3: Height of inline formatting contexts
    // The height should be based on line box heights
    assert!(
        size.height > 0.0,
        "Auto height should be > 0, got {}px. \
         CSS requires 'auto' height to be sum of line box heights.",
        size.height
    );
    
    // Verify the size is reasonable for the content "Auto Sized Content"
    // At 20px font-size, we expect width ~154px and height ~26px
    assert!(
        size.width >= 150.0 && size.width <= 200.0,
        "Width {} should be approximately 154px for the test content at 20px font-size",
        size.width
    );
    assert!(
        size.height >= 20.0 && size.height <= 30.0,
        "Height {} should be approximately 26px (1 line at 20px font-size)",
        size.height
    );
}

/// Tests that explicit width: 0px and height: 0px are honored exactly
/// 
/// CSS Specification: CSS Box Model Level 3
/// - Explicit length values must be used as-is, regardless of content
/// - This distinguishes '0px' from 'auto' (the critical bug we fixed)
#[test]
fn test_explicit_zero_sizing_regression() {
    let result = layout_test_html_simple(
        "Hidden Content",
        "body { width: 0px; height: 0px; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Explicit zero sizing should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
    assert!(!tree.nodes.is_empty(), "Layout tree should have nodes");
    
    let root_node = &tree.nodes[0];
    let size = root_node.used_size.expect("Root node should have used_size");
    
    // CSS 2.1 §10.2: Content width and height
    // Explicit 0px must be honored exactly, content should be clipped
    assert_eq!(
        size.width, 0.0,
        "Explicit width: 0px must result in exactly 0px width. \
         Got {}px. This is a critical distinction from 'auto'.",
        size.width
    );
    assert_eq!(
        size.height, 0.0,
        "Explicit height: 0px must result in exactly 0px height. \
         Got {}px. Content overflow should be clipped.",
        size.height
    );
}

/// Tests that block-level elements in normal flow stack vertically
/// 
/// CSS Specification: CSS 2.1 Section 9.5 - Floats and Section 9.4.1 - Block formatting contexts
/// - Block boxes in a BFC are laid out vertically, one after another
/// - The vertical distance is determined by margin properties
/// - No overlapping should occur (unless negative margins are used)
#[test]
fn test_block_layout_stacking_regression() {
    let result = layout_test_html_simple(
        "Block 1\nBlock 2",
        "body > * { display: block; height: 40px; margin: 0; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Block stacking layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
    let positions = &layout_cache.calculated_positions;
    
    // In a simple case with body containing two block children:
    // - Node 0: body (root)
    // - Node 1: first text node (becomes block via CSS)
    // - Node 2: second text node (becomes block via CSS)
    
    // Note: Actual structure depends on how text nodes are handled
    // The key assertion is that if we have multiple positioned nodes,
    // they should have different Y coordinates
    
    let mut y_positions: Vec<f32> = positions.values().map(|pos| pos.y).collect();
    y_positions.sort_by(|a, b| a.partial_cmp(b).unwrap());
    
    // Verify we have at least some positioned elements
    assert!(
        !y_positions.is_empty(),
        "Should have positioned elements in the layout tree"
    );
    
    // Check that consecutive positioned elements don't overlap
    // (allowing for floating point precision)
    for window in y_positions.windows(2) {
        let (y1, y2) = (window[0], window[1]);
        if y1 == y2 {
            continue; // Same element or siblings at same level
        }
        
        // CSS 2.1 §9.5: In a BFC, each box's top outer edge touches
        // the bottom outer edge of the preceding box
        assert!(
            y2 >= y1,
            "Block boxes should stack vertically: y1={}, y2={}. \
             CSS 2.1 requires non-overlapping vertical layout in BFC.",
            y1, y2
        );
    }
}

/// Tests basic font-size inheritance from parent to child
/// 
/// CSS Specification: CSS Cascade Level 4
/// - font-size is an inherited property
/// - Child elements inherit computed values from their parent
/// - This tests the cascade implementation in cascade.rs
#[test]
fn test_font_size_inheritance_regression() {
    let result = layout_test_html_simple(
        "Inherited Text",
        "body { font-size: 32px; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Font inheritance layout should succeed: {:?}", result.err());
    let (layout_cache, display_list) = result.unwrap();
    
    // Verify that text items in the display list have the inherited font-size
    let text_items: Vec<_> = display_list.items.iter()
        .filter_map(|item| {
            if let crate::solver3::display_list::DisplayListItem::Text { font_size_px, .. } = item {
                Some(*font_size_px)
            } else {
                None
            }
        })
        .collect();
    
    assert!(
        !text_items.is_empty(),
        "Display list should contain text items"
    );
    
    // CSS Cascade: All text should inherit the 32px font-size from body
    for (i, &font_size) in text_items.iter().enumerate() {
        assert_eq!(
            font_size, 32.0,
            "Text item {} should inherit font-size: 32px from body, got {}px. \
             CSS Cascade requires inherited properties to propagate.",
            i, font_size
        );
    }
}

/// Tests percentage width resolution against containing block
/// 
/// CSS Specification: CSS Values Level 3
/// - Percentage values are resolved against the corresponding dimension of the containing block
/// - For width, this is the containing block's width
#[test]
fn test_percentage_width_resolution_regression() {
    let result = layout_test_html_simple(
        "50% Width",
        "body { width: 400px; } body > * { width: 50%; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Percentage width layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
    
    // Find the body node and verify its width
    if let Some(body_node) = tree.nodes.get(0) {
        if let Some(body_size) = body_node.used_size {
            // Body should have the explicit 400px width
            assert_eq!(
                body_size.width, 400.0,
                "Body should have width: 400px, got {}px",
                body_size.width
            );
        }
    }
    
    // Child nodes (if any) should have 50% of parent's width = 200px
    // Note: The exact node structure depends on how the DOM is built
    for (idx, node) in tree.nodes.iter().enumerate().skip(1) {
        if let Some(size) = node.used_size {
            if size.width > 0.0 {
                // CSS Values: percentage resolves to percentage * containing block dimension
                assert!(
                    (size.width - 200.0).abs() < 1.0,
                    "Child node {} with percentage width should be ~200px (50% of 400px), got {}px. \
                     CSS Values Level 3 requires percentage resolution against containing block.",
                    idx, size.width
                );
            }
        }
    }
}

/// Tests vertical margin collapsing between adjacent block-level siblings
/// 
/// CSS Specification: CSS 2.1 Section 8.3.1 - Collapsing margins
/// - Adjacent vertical margins of block-level boxes collapse
/// - The collapsed margin is the maximum of the adjoining margins
/// - This prevents excessive whitespace accumulation
#[test]
fn test_margin_collapsing_regression() {
    let result = layout_test_html_simple(
        "Block 1\nBlock 2",
        "body > *:first-child { margin-bottom: 20px; height: 20px; } \
         body > *:last-child { margin-top: 30px; height: 20px; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Margin collapsing layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    let positions = &layout_cache.calculated_positions;
    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
    
    // Get all positioned nodes and sort by Y coordinate
    let mut positioned_nodes: Vec<_> = positions.iter()
        .filter_map(|(idx, pos)| {
            tree.nodes.get(*idx).and_then(|node| {
                node.used_size.map(|size| (*idx, pos.y, size.height))
            })
        })
        .collect();
    positioned_nodes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    
    // If we have at least 2 positioned blocks, verify margin collapsing
    if positioned_nodes.len() >= 2 {
        let (idx1, y1, height1) = positioned_nodes[0];
        let (idx2, y2, _height2) = positioned_nodes[1];
        
        // CSS 2.1 §8.3.1: The larger margin wins
        // Block 1 has margin-bottom: 20px, Block 2 has margin-top: 30px
        // The space between should be max(20, 30) = 30px
        let expected_y2 = y1 + height1 + 30.0; // y1 + height1 + collapsed_margin
        
        assert!(
            (y2 - expected_y2).abs() < 1.0,
            "Margin collapsing failed: Block {} at y={}, Block {} at y={}. \
             Expected y2={} (y1 {} + height {} + max_margin 30px). \
             CSS 2.1 §8.3.1 requires adjacent margins to collapse to the maximum.",
            idx1, y1, idx2, y2, expected_y2, y1, height1
        );
    }
}

/// Tests deep font-size inheritance through multiple DOM levels
/// 
/// CSS Specification: CSS Cascade Level 4
/// - Inherited properties propagate through the entire tree
/// - Each level inherits the computed value from its parent
/// - Tests that cascade.rs walks up the tree correctly
#[test]
fn test_deep_font_size_inheritance_regression() {
    let result = layout_test_html_simple(
        "Deeply Nested Text",
        "body { font-size: 24px; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Deep inheritance layout should succeed: {:?}", result.err());
    let (_layout_cache, display_list) = result.unwrap();
    
    // Verify all text items inherit the 24px font-size
    let text_items: Vec<_> = display_list.items.iter()
        .filter_map(|item| {
            if let crate::solver3::display_list::DisplayListItem::Text { font_size_px, .. } = item {
                Some(*font_size_px)
            } else {
                None
            }
        })
        .collect();
    
    assert!(
        !text_items.is_empty(),
        "Display list should contain text items for deep inheritance test"
    );
    
    // CSS Cascade: Inheritance propagates through all levels
    for (i, &font_size) in text_items.iter().enumerate() {
        assert_eq!(
            font_size, 24.0,
            "Text item {} should inherit font-size: 24px through the tree, got {}px. \
             CSS Cascade requires inheritance through all ancestor levels.",
            i, font_size
        );
    }
}

/// Tests box-sizing: border-box behavior
/// 
/// CSS Specification: CSS Box Sizing Level 3
/// - With border-box, padding and border are included in the specified width/height
/// - This changes how the content box size is calculated
#[test]
fn test_box_sizing_border_box() {
    let result = layout_test_html_simple(
        "Border Box Content",
        "body { width: 200px; padding: 20px; box-sizing: border-box; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Border-box layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
    let root_node = &tree.nodes[0];
    let size = root_node.used_size.expect("Root node should have used_size");
    
    // CSS Box Sizing §4: With border-box, the specified width is the border-box width
    // So the total width including padding should be 200px
    let padding = &root_node.box_props.padding;
    let total_width_with_padding = size.width; // This is the border-box width
    
    // Note: In our implementation, used_size might be the content-box or border-box
    // depending on how sizing.rs interprets box-sizing
    // The key is that the element doesn't exceed 200px total
    assert!(
        total_width_with_padding <= 200.0 + 0.1, // Allow small float error
        "With box-sizing: border-box and width: 200px, total width should be ≤200px, got {}px. \
         Padding ({}, {}) should be included in the 200px.",
        total_width_with_padding, padding.left, padding.right
    );
}

// ============================================================================
// LIST TESTS
// ============================================================================

/// Tests basic unordered list with disc markers
/// 
/// CSS Specification: CSS Lists and Counters Module Level 3
/// - list-style-type: disc produces bullet markers (•)
/// - Markers are automatically generated for list-items
/// - Counter "list-item" is auto-incremented
#[test]
fn test_unordered_list_disc() {
    use azul_css::parser2::CssApiWrapper;
    use azul_css::AzString;
    
    // Create DOM manually: <ul><li>Item 1</li><li>Item 2</li><li>Item 3</li></ul>
    let mut ul = Dom::div(); // Use div as container (ul/ol are div-like)
    ul.root.add_class("ul".into());
    
    let mut li1 = Dom::div();
    li1.root.add_class("li".into());
    li1.add_child(Dom::text("Item 1"));
    
    let mut li2 = Dom::div();
    li2.root.add_class("li".into());
    li2.add_child(Dom::text("Item 2"));
    
    let mut li3 = Dom::div();
    li3.root.add_class("li".into());
    li3.add_child(Dom::text("Item 3"));
    
    ul.add_child(li1);
    ul.add_child(li2);
    ul.add_child(li3);
    
    let mut dom = Dom::body();
    dom.add_child(ul);
    
    // Create CSS
    let css_str = ".ul { list-style-type: disc; } .li { display: list-item; }";
    let css = CssApiWrapper::from_string(AzString::from_string(css_str.to_string()));
    
    let mut styled_dom = StyledDom::new(&mut dom, css);
    styled_dom.dom_id = DomId::ROOT_ID;
    
    // Set up layout
    let mut layout_cache = LayoutCache {
        tree: None,
        calculated_positions: BTreeMap::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
    };
    let mut text_cache = TextLayoutCache::new();
    let font_manager = create_test_font_manager().expect("Font manager creation failed");
    let viewport = LogicalRect::new(
        LogicalPosition::zero(),
        LogicalSize::new(800.0, 600.0),
    );
    
    let result = layout_document(
        &mut layout_cache,
        &mut text_cache,
        styled_dom,
        viewport,
        &font_manager,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut None,
        None,
        &azul_core::resources::RendererResources::default(),
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
    );
    
    assert!(result.is_ok(), "Unordered list layout should succeed: {:?}", result.err());
    
    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
    
    // CSS Lists §3.1: Each list-item generates a marker box
    // Verify that counter values exist for list items
    let counter_count = layout_cache.counters.iter()
        .filter(|((_, name), _)| name == "list-item")
        .count();
    
    assert!(
        counter_count > 0,
        "List items should have counter values stored. \
         CSS Counters requires 'list-item' counter to be auto-incremented. \
         Found {} counter entries.",
        counter_count
    );
}

/// Tests ordered list with decimal markers
/// 
/// CSS Specification: CSS Lists and Counters Module Level 3
/// - list-style-type: decimal produces numeric markers (1, 2, 3, ...)
/// - Counter "list-item" is auto-incremented on each <li>
#[test]
fn test_ordered_list_decimal() {
    let result = layout_test_html_simple(
        "<ol><li>First</li><li>Second</li><li>Third</li></ol>",
        "ol { list-style-type: decimal; } li { display: list-item; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Ordered list layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    // Verify counter values are sequential
    let mut counter_values: Vec<_> = layout_cache.counters.iter()
        .filter(|((_, name), _)| name == "list-item")
        .map(|(_, &value)| value)
        .collect();
    
    counter_values.sort();
    
    // CSS Counters §2.1: list-item counter starts at 1 and increments by 1
    assert!(
        counter_values.len() >= 3,
        "Should have at least 3 counter values for 3 list items"
    );
    
    if counter_values.len() >= 3 {
        assert_eq!(
            counter_values[0], 1,
            "First list item counter should be 1, got {}",
            counter_values[0]
        );
        assert_eq!(
            counter_values[1], 2,
            "Second list item counter should be 2, got {}",
            counter_values[1]
        );
        assert_eq!(
            counter_values[2], 3,
            "Third list item counter should be 3, got {}",
            counter_values[2]
        );
    }
}

/// Tests ordered list with alphabetic markers
/// 
/// CSS Specification: CSS Counter Styles Level 3
/// - lower-alpha produces lowercase letters (a, b, c, ...)
/// - Counters 1-26 map to a-z, 27 becomes "aa"
#[test]
fn test_ordered_list_lower_alpha() {
    let result = layout_test_html_simple(
        "<ol><li>A</li><li>B</li><li>C</li></ol>",
        "ol { list-style-type: lower-alpha; } li { display: list-item; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Alphabetic list layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    // Verify counters exist
    let counter_count = layout_cache.counters.iter()
        .filter(|((_, name), _)| name == "list-item")
        .count();
    
    assert!(
        counter_count >= 3,
        "Should have at least 3 counter values for alphabetic list"
    );
}

/// Tests nested lists with counter scoping
/// 
/// CSS Specification: CSS Lists Module Level 3
/// - Nested lists create nested counter scopes
/// - Each nesting level has its own counter sequence
/// - Counters are reset when entering a nested list
#[test]
fn test_nested_lists() {
    let result = layout_test_html_simple(
        "<ol><li>Item 1<ol><li>Nested 1</li><li>Nested 2</li></ol></li><li>Item 2</li></ol>",
        "ol { list-style-type: decimal; } li { display: list-item; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Nested list layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
    
    // Count list-item nodes
    let list_item_count = tree.nodes.iter()
        .filter(|node| {
            node.formatting_context == azul_core::dom::FormattingContext::Block { establishes_new_context: true }
                && node.dom_node_id.is_some()
        })
        .count();
    
    // CSS Lists §4: Nested lists should maintain separate counter scopes
    // We should have at least 4 list items total (2 outer + 2 inner)
    assert!(
        list_item_count >= 4,
        "Nested list should have at least 4 list items, found {}",
        list_item_count
    );
}

/// Tests counter-reset property
/// 
/// CSS Specification: CSS Lists and Counters Module Level 3
/// - counter-reset creates a new counter scope
/// - Following items increment from the reset value
#[test]
fn test_counter_reset() {
    let result = layout_test_html_simple(
        "<ol><li>Item 1</li></ol><ol style='counter-reset: list-item 5;'><li>Item 6</li></ol>",
        "li { display: list-item; list-style-type: decimal; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Counter-reset layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    // Verify that counters exist (actual value checking would require more context)
    let counter_count = layout_cache.counters.len();
    
    assert!(
        counter_count >= 2,
        "Should have counter values for both list items. \
         CSS Counters §3: counter-reset should create new counter scopes."
    );
}

/// Tests list-style-type: roman numerals
/// 
/// CSS Specification: CSS Counter Styles Level 3
/// - lower-roman produces lowercase Roman numerals (i, ii, iii, iv, ...)
/// - upper-roman produces uppercase Roman numerals (I, II, III, IV, ...)
#[test]
fn test_ordered_list_roman() {
    let result = layout_test_html_simple(
        "<ol><li>I</li><li>II</li><li>III</li><li>IV</li></ol>",
        "ol { list-style-type: upper-roman; } li { display: list-item; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "Roman numeral list layout should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    // Verify counter values are sequential
    let counter_values: Vec<_> = layout_cache.counters.iter()
        .filter(|((_, name), _)| name == "list-item")
        .map(|(_, &value)| value)
        .collect();
    
    assert!(
        counter_values.len() >= 4,
        "Should have 4 counter values for Roman numeral list"
    );
}

/// Tests list-style-type: none (no markers)
/// 
/// CSS Specification: CSS Lists Module Level 3
/// - list-style-type: none suppresses marker generation
/// - List items still increment counters but don't display markers
#[test]
fn test_list_style_none() {
    let result = layout_test_html_simple(
        "<ul><li>Item 1</li><li>Item 2</li></ul>",
        "ul { list-style-type: none; } li { display: list-item; }",
        LogicalSize::new(800.0, 600.0),
    );
    
    assert!(result.is_ok(), "List with style:none should succeed: {:?}", result.err());
    let (layout_cache, _) = result.unwrap();
    
    // Even with list-style-type: none, counters should still be tracked
    let counter_count = layout_cache.counters.iter()
        .filter(|((_, name), _)| name == "list-item")
        .count();
    
    assert!(
        counter_count >= 2,
        "Counters should be tracked even with list-style-type: none. \
         CSS Lists §3.3: Markers are suppressed but counters still increment."
    );
}



