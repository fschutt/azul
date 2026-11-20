//! A callback's write of `transform` or `opacity` to a node the display list
//! already binds is a property value, not a display-list rebuild.
//!
//! azul binds a node's CSS `transform` and `opacity` to WebRender property
//! keys (`GpuValueCache`): both compositors read the value live from the cache
//! every frame, and a CSS animation or transition moves it without touching the
//! list. `set_css_property` did not use that: every write, whatever the
//! property, ended in `regenerate_display_list_for_dom` - which re-emits every
//! item of the DOM - and the `RestyleResult::gpu_only_changes` classification
//! it computed (`is_gpu_only_property`: opacity and transform) was dropped on
//! the floor (`let _restyle_result`). A graph editor that dragged its nodes
//! with `transform: translate()` rebuilt a 38 000-item list for every mouse
//! move.
//!
//! Now a write of nothing but those two to a node with a binding publishes the
//! value under the node's key and returns the Paint tier: no cascade rebuild,
//! no layout, no display-list regeneration. A node without a binding yet (its
//! first `transform`, an opacity layer that does not exist) changes the key
//! POPULATION, which only a build can show: it takes one rebuild, and every
//! later write is a value. A write of anything else along with them takes the
//! full path.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
    transform::ComputedTransform3D,
};
use azul_css::props::{
    basic::{ColorU, PercentageValue, PixelValue},
    property::CssProperty,
    style::{
        StyleBackgroundContent, StyleBackgroundContentVec, StyleOpacity, StyleTransform,
        StyleTransformTranslate2D,
    },
};
use crate::{
    callbacks::ExternalSystemCallbacks,
    cpurender::extract_gpu_values,
    overlay::{ContentChange, ContentDirtyTier},
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A page of three boxes: `moved` carries a transform, `plain` carries none,
/// `faded` is half transparent.
struct Page {
    lw: LayoutWindow,
    moved: NodeId,
    plain: NodeId,
    faded: NodeId,
}

fn node_with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == class))
        })
        .unwrap_or_else(|| panic!("a node of class {class}"))
}

fn page() -> Page {
    let mut dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_class("moved".into())
                .with_css("width: 80px; height: 24px; background: #ffcc00; transform: translate(10px, 5px);"),
        )
        .with_child(
            Dom::create_div()
                .with_class("plain".into())
                .with_css("width: 80px; height: 24px; background: #eeeeee;"),
        )
        .with_child(
            Dom::create_div()
                .with_class("faded".into())
                .with_css("width: 80px; height: 24px; background: #cc0000; opacity: 0.5;"),
        );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("the page lays out");
    let moved = node_with_class(&lw, "moved");
    let plain = node_with_class(&lw, "plain");
    let faded = node_with_class(&lw, "faded");
    Page {
        lw,
        moved,
        plain,
        faded,
    }
}

fn translate(x: f32, y: f32) -> CssProperty {
    CssProperty::transform(
        vec![StyleTransform::Translate(StyleTransformTranslate2D {
            x: PixelValue::px(x),
            y: PixelValue::px(y),
        })]
        .into(),
    )
}

fn opacity(percent: f32) -> CssProperty {
    CssProperty::opacity(StyleOpacity {
        inner: PercentageValue::new(percent),
    })
}

fn solid(c: ColorU) -> CssProperty {
    CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
        StyleBackgroundContent::Color(c),
    ]))
}

fn write(lw: &mut LayoutWindow, node: NodeId, props: Vec<CssProperty>, override_only: bool) -> ContentDirtyTier {
    lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: node,
        props,
        override_only,
    })
    .tier
}

/// The matrix the compositors read for `node`'s reference frame: the live
/// value under its key.
fn bound_transform(lw: &LayoutWindow, node: NodeId) -> Option<ComputedTransform3D> {
    lw.gpu_state_manager
        .get_cache(DomId::ROOT_ID)?
        .css_current_transform_values
        .get(&node)
        .copied()
}

/// The live opacity under `node`'s key.
fn bound_opacity(lw: &LayoutWindow, node: NodeId) -> Option<f32> {
    lw.gpu_state_manager
        .get_cache(DomId::ROOT_ID)?
        .current_opacity_values
        .get(&node)
        .copied()
}

/// Whether `node`'s live opacity is `want`, to the step the compact cache keeps
/// opacity in (1/254): a list built after a write reads it from there.
fn opacity_is(lw: &LayoutWindow, node: NodeId, want: f32) -> bool {
    bound_opacity(lw, node).is_some_and(|got| (got - want).abs() < 1.0 / 254.0)
}

/// The `(x, y)` translation of a matrix.
fn translation(m: &ComputedTransform3D) -> (f32, f32) {
    (m.m[3][0], m.m[3][1])
}

fn rebuilds(lw: &LayoutWindow) -> u32 {
    lw.frame_report.dl_rebuilds
}

fn layouts(lw: &LayoutWindow) -> u32 {
    lw.frame_report.layout_passes
}

/// What the compositors draw `node`'s reference frame with: the display list
/// names the frame by the node's transform key, and both compositors read the
/// key's LIVE value (`extract_gpu_values` is the CPU renderer's read,
/// `synchronize_gpu_values` WebRender's).
fn painted_translation(lw: &LayoutWindow, node: NodeId) -> Option<(f32, f32)> {
    let cache = lw.gpu_state_manager.get_cache(DomId::ROOT_ID)?;
    let key = *cache.css_transform_keys.get(&node)?;
    let dl = &lw.layout_results[&DomId::ROOT_ID].display_list;
    let named_by_the_list = dl.items.iter().any(|item| {
        matches!(item, DisplayListItem::PushReferenceFrame { transform_key, .. } if *transform_key == key)
    });
    assert!(named_by_the_list, "the display list opens a reference frame under the node's key");
    let (transforms, _) = extract_gpu_values(Some(cache), DomId::ROOT_ID);
    transforms.get(&key.id).map(|m| (m.m[3][0], m.m[3][1]))
}

#[test]
fn a_transform_written_to_a_bound_node_is_a_value_and_no_display_list() {
    let mut p = page();
    assert_eq!(
        bound_transform(&p.lw, p.moved).map(|m| translation(&m)),
        Some((10.0, 5.0)),
        "harness: the display list binds the node's reference frame"
    );
    p.lw.frame_report.reset_counters();

    let tier = write(&mut p.lw, p.moved, vec![translate(40.0, 8.0)], false);

    assert_eq!(tier, ContentDirtyTier::Paint, "a repaint, not a rebuild");
    assert_eq!(rebuilds(&p.lw), 0, "no display-list regeneration");
    assert_eq!(layouts(&p.lw), 0, "no layout");
    assert_eq!(
        bound_transform(&p.lw, p.moved).map(|m| translation(&m)),
        Some((40.0, 8.0)),
        "the value under the node's key moved"
    );
    assert_eq!(
        painted_translation(&p.lw, p.moved),
        Some((40.0, 8.0)),
        "the next frame - both compositors read the key's live value - draws it there"
    );
}

/// A drag writes the node's transform on every mouse move: each one is a value.
#[test]
fn every_write_of_a_drag_is_a_value() {
    let mut p = page();
    p.lw.frame_report.reset_counters();
    for step in 1..=50 {
        let tier = write(&mut p.lw, p.moved, vec![translate(10.0 + step as f32, 5.0)], false);
        assert_eq!(tier, ContentDirtyTier::Paint, "move {step}");
    }
    assert_eq!(rebuilds(&p.lw), 0, "fifty moves, no display-list regeneration");
    assert_eq!(layouts(&p.lw), 0);
    assert_eq!(
        bound_transform(&p.lw, p.moved).map(|m| translation(&m)),
        Some((60.0, 5.0))
    );
}

/// The override channel (`override_node_css_properties`, the animation path)
/// takes the same road.
#[test]
fn the_override_channel_publishes_a_bound_value_too() {
    let mut p = page();
    p.lw.frame_report.reset_counters();
    let tier = write(&mut p.lw, p.moved, vec![translate(22.0, 3.0)], true);
    assert_eq!(tier, ContentDirtyTier::Paint);
    assert_eq!(rebuilds(&p.lw), 0);
    assert_eq!(
        bound_transform(&p.lw, p.moved).map(|m| translation(&m)),
        Some((22.0, 3.0))
    );
}

/// A node without a reference frame changes the key population: ONE rebuild
/// makes the binding, and every later write is a value.
#[test]
fn a_node_without_a_binding_takes_one_rebuild_and_then_values() {
    let mut p = page();
    assert_eq!(bound_transform(&p.lw, p.plain), None, "harness: no binding yet");
    p.lw.frame_report.reset_counters();

    let first = write(&mut p.lw, p.plain, vec![translate(12.0, 0.0)], false);
    assert_eq!(first, ContentDirtyTier::RebuildDisplayList, "its first transform");
    assert_eq!(rebuilds(&p.lw), 1, "exactly one rebuild");
    assert_eq!(
        bound_transform(&p.lw, p.plain).map(|m| translation(&m)),
        Some((12.0, 0.0)),
        "the rebuild minted the binding"
    );

    let second = write(&mut p.lw, p.plain, vec![translate(30.0, 4.0)], false);
    assert_eq!(second, ContentDirtyTier::Paint, "now it has one");
    assert_eq!(rebuilds(&p.lw), 1, "still one");
    assert_eq!(
        bound_transform(&p.lw, p.plain).map(|m| translation(&m)),
        Some((30.0, 4.0))
    );
}

/// A transform along with a colour is not a GPU-only write: the full path.
#[test]
fn a_transform_with_a_colour_takes_the_full_path() {
    let mut p = page();
    p.lw.frame_report.reset_counters();
    let tier = write(
        &mut p.lw,
        p.moved,
        vec![translate(50.0, 9.0), solid(ColorU::new(0, 128, 0, 255))],
        false,
    );
    assert_eq!(tier, ContentDirtyTier::RebuildDisplayList);
    assert_eq!(rebuilds(&p.lw), 1, "a colour is in the list");
    assert_eq!(
        bound_transform(&p.lw, p.moved).map(|m| translation(&m)),
        Some((50.0, 9.0)),
        "and the transform still arrives"
    );
}

#[test]
fn an_opacity_written_to_a_node_with_a_layer_is_a_value_and_no_display_list() {
    let mut p = page();
    assert_eq!(bound_opacity(&p.lw, p.faded), Some(0.5), "harness: the layer is bound");
    p.lw.frame_report.reset_counters();

    let tier = write(&mut p.lw, p.faded, vec![opacity(25.0)], false);

    assert_eq!(tier, ContentDirtyTier::Paint);
    assert_eq!(rebuilds(&p.lw), 0, "no display-list regeneration");
    assert_eq!(layouts(&p.lw), 0);
    assert!(opacity_is(&p.lw, p.faded, 0.25), "the value under the key moved");
}

/// A node at full opacity has no layer: its first fade is a rebuild, the next
/// writes are values.
#[test]
fn an_opacity_written_to_a_node_without_a_layer_takes_one_rebuild() {
    let mut p = page();
    assert_eq!(bound_opacity(&p.lw, p.plain), None, "harness: no layer");
    p.lw.frame_report.reset_counters();

    let first = write(&mut p.lw, p.plain, vec![opacity(60.0)], false);
    assert_eq!(first, ContentDirtyTier::RebuildDisplayList);
    assert_eq!(rebuilds(&p.lw), 1);
    assert!(opacity_is(&p.lw, p.plain, 0.6), "the rebuild made the layer");

    let second = write(&mut p.lw, p.plain, vec![opacity(10.0)], false);
    assert_eq!(second, ContentDirtyTier::Paint);
    assert_eq!(rebuilds(&p.lw), 1, "still one");
    assert!(opacity_is(&p.lw, p.plain, 0.1));
}

/// A list built after a value write paints what the value says: the value is
/// in the cascade, not only in the cache.
#[test]
fn a_list_built_after_a_value_write_draws_the_written_value() {
    let mut p = page();
    let _ = write(&mut p.lw, p.moved, vec![translate(70.0, 2.0)], false);
    let _ = write(&mut p.lw, p.faded, vec![opacity(30.0)], false);
    p.lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    assert_eq!(
        bound_transform(&p.lw, p.moved).map(|m| translation(&m)),
        Some((70.0, 2.0))
    );
    assert!(opacity_is(&p.lw, p.faded, 0.3));
    assert_eq!(painted_translation(&p.lw, p.moved), Some((70.0, 2.0)));
}

/// The value the node already resolves to: nothing to paint.
#[test]
fn a_write_of_the_value_the_node_already_has_owes_no_frame() {
    let mut p = page();
    p.lw.frame_report.reset_counters();
    let tier = write(&mut p.lw, p.moved, vec![translate(10.0, 5.0)], false);
    assert_eq!(tier, ContentDirtyTier::Unchanged);
    assert_eq!(rebuilds(&p.lw), 0);
}

// ---- the restyle of a state flip (`:hover`, `:focus`) takes the same road ----

/// A page whose `:hover` rules move a transform and an opacity: `.card` has both
/// at rest (so the display list binds them), `.bare` has neither.
fn hover_page() -> LayoutWindow {
    let markup = "<style>\
        .card { width: 80px; height: 24px; background: #ffcc00; transform: translate(0px, 0px); opacity: 0.5; }\
        .card:hover { transform: translate(40px, 8px); opacity: 0.9; }\
        .bare { width: 80px; height: 24px; background: #eeeeee; }\
        .bare:hover { transform: translate(5px, 5px); }\
        </style>\
        <div class=\"card\"></div><div class=\"bare\"></div>";
    let parsed = crate::xml::parse_xml(markup).expect("parses");
    let styled = StyledDom::create_from_dom(crate::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

fn class_node(lw: &LayoutWindow, class: &str) -> NodeId {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class(class))
        .unwrap_or_else(|| panic!("a node of class {class}"))
}

/// The pointer enters (`true`) or leaves `node`: the restyle, as the event pass
/// runs it.
fn flip_hover(lw: &mut LayoutWindow, node: NodeId, enter: bool) -> azul_core::styled_dom::RestyleResult {
    let change = if enter {
        azul_core::styled_dom::HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: vec![node],
        }
    } else {
        azul_core::styled_dom::HoverChange {
            left_nodes: vec![node],
            entered_nodes: Vec::new(),
        }
    };
    lw.layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .restyle_on_state_change(None, Some(change), None)
}

/// REGRESSION: a `:hover` that moves nothing but a transform and an opacity is
/// classified GPU-only, and the shell answered it with a plain re-render with
/// nothing published: the new values never reached the keys the compositors
/// read, and the hover showed nothing until something else rebuilt the list.
#[test]
fn a_hover_that_moves_a_transform_and_an_opacity_publishes_them_and_rebuilds_nothing() {
    let mut lw = hover_page();
    let card = class_node(&lw, "card");
    assert_eq!(bound_transform(&lw, card).map(|m| translation(&m)), Some((0.0, 0.0)), "harness");
    assert_eq!(bound_opacity(&lw, card), Some(0.5), "harness");
    lw.frame_report.reset_counters();

    let entered = flip_hover(&mut lw, card, true);
    assert!(entered.gpu_only_changes, "harness: only transform and opacity changed");
    assert!(
        lw.publish_gpu_only_restyle(DomId::ROOT_ID, &entered.changed_nodes),
        "the card has both bindings: the values are published"
    );
    assert_eq!(rebuilds(&lw), 0, "no display-list regeneration");
    assert_eq!(layouts(&lw), 0);
    assert_eq!(
        bound_transform(&lw, card).map(|m| translation(&m)),
        Some((40.0, 8.0)),
        "the hovered transform is under the key"
    );
    assert!(opacity_is(&lw, card, 0.9));
    assert_eq!(painted_translation(&lw, card), Some((40.0, 8.0)), "and both compositors read it");

    let left = flip_hover(&mut lw, card, false);
    assert!(lw.publish_gpu_only_restyle(DomId::ROOT_ID, &left.changed_nodes));
    assert_eq!(bound_transform(&lw, card).map(|m| translation(&m)), Some((0.0, 0.0)));
    assert!(opacity_is(&lw, card, 0.5), "the pointer left: back at rest");
    assert_eq!(rebuilds(&lw), 0);
}

/// A node that has no reference frame at rest cannot take its hover transform
/// as a value: the key population changes, the caller rebuilds the list.
#[test]
fn a_hover_on_a_node_without_a_binding_is_left_to_a_rebuild() {
    let mut lw = hover_page();
    let bare = class_node(&lw, "bare");
    let entered = flip_hover(&mut lw, bare, true);
    assert!(entered.gpu_only_changes);
    assert!(
        !lw.publish_gpu_only_restyle(DomId::ROOT_ID, &entered.changed_nodes),
        "nothing to publish under: the shell rebuilds the list"
    );
}

// ---- a VirtualView's child DOM: the same road, in the child's own caches ----

/// A host holding a view whose child root carries an identity transform (what an app
/// does to a layer it will scale or move while a gesture runs: the identity gives it the
/// reference frame the later writes publish under).
extern "C" fn layer_view(
    _data: azul_core::refany::RefAny,
    info: azul_core::callbacks::VirtualViewCallbackInfo,
) -> azul_core::callbacks::VirtualViewReturn {
    let dom = Dom::create_div()
        .with_class("layer".into())
        .with_css("width: 200px; height: 100px; background: #3366cc; transform: translate(0px, 0px);");
    let rect = azul_core::geom::LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
    azul_core::callbacks::VirtualViewReturn::with_dom(dom, rect, rect)
}

#[test]
fn a_transform_written_to_a_view_childs_layer_is_a_value_too() {
    let mut dom = Dom::create_body().with_child(
        Dom::create_virtual_view(
            azul_core::refany::RefAny::new(()),
            azul_core::callbacks::VirtualViewCallback::create(layer_view),
        )
        .with_css("width: 300px; height: 200px;"),
    );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_new_generation(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let child = *lw
        .layout_results
        .keys()
        .find(|dom| **dom != DomId::ROOT_ID)
        .expect("the view's child DOM");
    let layer = {
        let nodes = lw.layout_results[&child].styled_dom.node_data.as_container();
        (0..nodes.len())
            .map(NodeId::new)
            .find(|n| nodes[*n].has_class("layer"))
            .expect("the layer")
    };
    let key_of = |lw: &LayoutWindow| {
        lw.gpu_state_manager
            .get_cache(child)
            .and_then(|cache| cache.css_current_transform_values.get(&layer).copied())
    };
    assert_eq!(key_of(&lw).map(|m| translation(&m)), Some((0.0, 0.0)), "harness: the layer is bound in the child's cache");
    lw.frame_report.reset_counters();

    let tier = lw
        .apply_content_change(ContentChange::NodeCss {
            dom_id: child,
            node_id: layer,
            props: vec![translate(30.0, 12.0)],
            override_only: false,
        })
        .tier;
    assert_eq!(tier, ContentDirtyTier::Paint, "a value in the child's own cache");
    assert_eq!(rebuilds(&lw), 0, "no display-list regeneration");
    assert_eq!(key_of(&lw).map(|m| translation(&m)), Some((30.0, 12.0)));
}
