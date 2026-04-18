//! Regression tests for the root box-sizing / border-box fix in
//! `layout_flex_grid` (azul commit `d5c601de6`).
//!
//! Two bugs that surfaced on calc.c once it got `padding:14px` +
//! `height:100%` on its flex root:
//!
//! 1. `constraints.available_size` (content-box) was fed into taffy's
//!    `known_dimensions`, which taffy interprets as border-box. Net
//!    effect: content area came out 2× padding too narrow ("asymmetric
//!    padding on the right").
//!
//! 2. `calculate_used_size_for_node` inflates CSS-default content-box
//!    sizing to border-box by adding padding+border. For a root with
//!    `height:100%`, this yields border-box = viewport + 2×padding,
//!    so the root overflows the viewport vertically by the padding
//!    sum (and the overlay-scrollbar logic reserves a right-side
//!    gutter as a side effect).
//!
//! Fix: auto-apply `box-sizing: border-box` for the root element
//! (a CSS-reset pattern). Pull effective dimensions from
//! `node.used_size` (border-box) instead of re-interpreting
//! `available_size` (content-box) as border-box, and skip padding
//! inflation for explicit root dimensions.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn create_layout_window() -> LayoutWindow {
    let font_cache = FcFontCache::build();
    LayoutWindow::new(font_cache).unwrap()
}

fn create_window_state(width: f32, height: f32) -> FullWindowState {
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    window_state
}

fn layout_dom(dom: Dom, css_str: &str, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut layout_window = create_layout_window();
    let window_state = create_window_state(width, height);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    layout_window
}

fn root_id() -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    }
}

fn node_id(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// The exact calc.c reproduction: flex-column root with `height:100%`
/// and `padding:14px`. Without the fix, the root's border-box ends up
/// `viewport + 2×padding` vertically, overflowing the viewport by 28px.
#[test]
fn test_root_height_percent_plus_padding_does_not_overflow_viewport() {
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(Dom::create_div().with_ids_and_classes(
            vec![IdOrClass::Class("child".into())].into(),
        ));

    let css = r#"
        .root {
            height: 100%;
            display: flex;
            flex-direction: column;
            padding: 14px;
        }
        .child {
            flex-grow: 1;
        }
    "#;

    let vw = 320.0;
    let vh = 480.0;
    let lw = layout_dom(dom, css, vw, vh);

    let root_rect = lw
        .get_node_layout_rect(root_id())
        .expect("root layout rect");

    // Expected: root.border-box == viewport (auto box-sizing: border-box on root).
    // Without the fix: root.border-box = viewport + 2*14 = 508, overflowing
    // the viewport vertically by 28px.
    assert!(
        (root_rect.size.height - vh).abs() < 1.0,
        "Root height ({}) should match viewport ({}) — bug inflates it to \
         viewport + 2*padding = {}.",
        root_rect.size.height,
        vh,
        vh + 28.0,
    );
    assert!(
        (root_rect.size.width - vw).abs() < 1.0,
        "Root width ({}) should match viewport ({}), got {}.",
        root_rect.size.width,
        vw,
        root_rect.size.width,
    );
}

/// The child of a padded flex-column root should occupy the content-box:
/// (vw - 2*padding, vh - 2*padding) at offset (padding, padding).
///
/// Without the fix, two things go wrong:
/// - The root overflows the viewport by 28px vertically (bug B above),
///   so the child's bottom edge ends up at vh + padding, past the
///   viewport.
/// - The content area handed to taffy is 2× padding too narrow
///   (bug A — `available_size` was content-box but fed in as border-box),
///   so the child ends up (vw - 4*padding) wide instead of (vw - 2*padding).
#[test]
fn test_flex_child_fills_content_box_without_double_padding_subtraction() {
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(Dom::create_div().with_ids_and_classes(
            vec![IdOrClass::Class("child".into())].into(),
        ));

    let css = r#"
        .root {
            height: 100%;
            display: flex;
            flex-direction: column;
            padding: 14px;
        }
        .child {
            flex-grow: 1;
        }
    "#;

    let vw = 320.0;
    let vh = 480.0;
    let pad = 14.0;
    let lw = layout_dom(dom, css, vw, vh);

    let child_rect = lw
        .get_node_layout_rect(node_id(1))
        .expect("child layout rect");

    let expected_w = vw - 2.0 * pad;
    let expected_h = vh - 2.0 * pad;

    assert!(
        (child_rect.size.width - expected_w).abs() < 1.0,
        "Child width should be vw - 2*padding = {} (viewport {} minus 2×{} \
         padding). Got {}. Bug A (content-box fed in as border-box) \
         produces vw - 4*padding = {}.",
        expected_w,
        vw,
        pad,
        child_rect.size.width,
        vw - 4.0 * pad,
    );
    assert!(
        (child_rect.size.height - expected_h).abs() < 1.0,
        "Child height should be vh - 2*padding = {}. Got {}. \
         Bug B (root overflows viewport) would let the child extend to {} \
         (viewport + padding).",
        expected_h,
        child_rect.size.height,
        vh - pad,
    );

    // Child bottom edge must NOT extend below viewport. This is the
    // most user-visible symptom of bug B on calc.c.
    let child_bottom = child_rect.origin.y + child_rect.size.height;
    assert!(
        child_bottom <= vh + 0.5,
        "Child bottom ({}) must not extend past viewport height ({}) — \
         this is the calc.c y-overflow symptom.",
        child_bottom,
        vh,
    );
}

/// Symmetry: padding must be equal on left and right after layout.
/// The asymmetric-padding symptom on calc.c was the right-hand gutter
/// visibly larger than the left (content area was 2×padding too narrow,
/// so with `flex-grow:1` the child was left-anchored and a gap appeared
/// on the right).
#[test]
fn test_flex_child_has_symmetric_horizontal_padding_gutters() {
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(Dom::create_div().with_ids_and_classes(
            vec![IdOrClass::Class("child".into())].into(),
        ));

    let css = r#"
        .root {
            height: 100%;
            display: flex;
            flex-direction: column;
            padding: 14px;
        }
        .child {
            flex-grow: 1;
        }
    "#;

    let vw = 320.0;
    let vh = 480.0;
    let pad = 14.0;
    let lw = layout_dom(dom, css, vw, vh);

    let child_rect = lw
        .get_node_layout_rect(node_id(1))
        .expect("child layout rect");

    let left_gutter = child_rect.origin.x;
    let right_gutter = vw - (child_rect.origin.x + child_rect.size.width);

    assert!(
        (left_gutter - pad).abs() < 1.0,
        "Left gutter should be padding ({}), got {}",
        pad,
        left_gutter,
    );
    assert!(
        (right_gutter - pad).abs() < 1.0,
        "Right gutter should be padding ({}), got {}. \
         Asymmetric right gutter (> left) is the calc.c symptom.",
        pad,
        right_gutter,
    );
    assert!(
        (left_gutter - right_gutter).abs() < 1.0,
        "Left gutter ({}) and right gutter ({}) must be symmetric.",
        left_gutter,
        right_gutter,
    );
}

/// Explicit pixel height + padding on root should also stay in viewport
/// (the border-box interpretation applies equally to explicit dimensions).
#[test]
fn test_root_explicit_height_plus_padding_treated_as_border_box() {
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into());

    let css = r#"
        .root {
            height: 400px;
            display: flex;
            flex-direction: column;
            padding: 14px;
        }
    "#;

    let vw = 320.0;
    let vh = 480.0;
    let lw = layout_dom(dom, css, vw, vh);

    let root_rect = lw
        .get_node_layout_rect(root_id())
        .expect("root layout rect");

    // With the fix: explicit height on root is treated as border-box
    // (auto box-sizing:border-box for root), so 400 remains 400.
    // Without the fix: 400 + 2*14 = 428.
    assert!(
        (root_rect.size.height - 400.0).abs() < 1.0,
        "Explicit root height (400) should be treated as border-box. \
         Got {}, bug inflates it to 400 + 2*padding = 428.",
        root_rect.size.height,
    );
}
