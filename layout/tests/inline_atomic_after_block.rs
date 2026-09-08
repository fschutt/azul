//! An atomic inline (`display: inline-flex`) that follows a block in the same
//! container — the shape of every hello-world in the repo: a `<p>` counter and
//! a button under it.
//!
//! Two things were wrong there, both visible in the frontpage screenshots:
//!
//! 1. The button sat 12.8 px lower on the FIRST layout pass than on every pass
//!    after it. 12.8 px is `0.8 * 16 px`, the strut ascent of the container's
//!    own font: the atomic inline reported no baseline the first time round, so
//!    the line box aligned its top edge to the strut's baseline instead of its
//!    own. Any redraw (clicking the button, i.e. `Update::RefreshDom`) then
//!    moved it up — which is what left a ghost of the old button on screen.
//!
//! 2. The line box holding it was only as tall as the strut (18.4 px), not as
//!    tall as the button (42.5 px), so the container's height was ~24 px short
//!    and the button hung out of its own parent.
//!
//! Both are asserted here against the geometry a browser produces for the same
//! markup: the box is positioned once, identically on every pass, and the
//! container is tall enough to hold it.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
};
use azul_layout::{
    font::loading::build_font_cache,
    font_traits::{FontManager, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{paged_layout::layout_document_paged_with_config, pagination::FakePageConfig},
    text3::default::PathLoader,
    xml::DomXmlExt,
    Solver3LayoutCache,
};
use std::collections::{BTreeMap, HashMap};

/// The hello-world shape: a 32 px paragraph, then an inline-level button.
const HTML: &str = r#"
<html>
    <head>
        <style>
            .counter { font-size: 32px; }
            .btn {
                display: inline-flex;
                flex-direction: row;
                align-items: center;
                justify-content: center;
                font-size: 13px;
                padding: 6px 12px;
                border: 1px solid #0d6efd;
            }
        </style>
    </head>
    <body>
        <p class="counter">5</p>
        <div class="btn"><p>Increase counter</p></div>
    </body>
</html>
"#;

struct Env {
    font_manager: FontManager<azul_css::props::basic::FontRef>,
}

/// One node's box after a pass: where it ended up and how big it is.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Box2D {
    y: f32,
    height: f32,
}

impl Env {
    fn new() -> Self {
        let fc_cache = build_font_cache();
        Self {
            font_manager: FontManager::new(fc_cache).expect("font manager"),
        }
    }

    fn fresh_cache() -> Solver3LayoutCache {
        Solver3LayoutCache {
            tree: None,
            calculated_positions: Vec::new(),
            viewport: None,
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
            counters: HashMap::new(),
            float_cache: HashMap::new(),
            cache_map: Default::default(),
            previous_positions: Vec::new(),
            cached_display_list: None,
            prev_dom_ptr: 0,
            prev_viewport: LogicalRect {
                origin: LogicalPosition::zero(),
                size: LogicalSize::zero(),
            },
            ..Default::default()
        }
    }

    /// Lays `HTML` out into `cache`; running it twice on the same cache is the
    /// relayout an `Update::RefreshDom` performs.
    fn layout(&mut self, cache: &mut Solver3LayoutCache, w: f32, h: f32) {
        let styled_dom = Dom::from_xml_string(HTML);
        let mut text_cache = TextLayoutCache::new();
        let content_size = LogicalSize::new(w, h);
        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: content_size,
        };
        let loader = PathLoader::new();
        let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
            loader.load_font_shared(bytes, index)
        };
        let mut debug_messages = Some(Vec::new());

        layout_document_paged_with_config(
            cache,
            &mut text_cache,
            FragmentationContext::new_paged(content_size),
            &styled_dom,
            viewport,
            &mut self.font_manager,
            &BTreeMap::new(),
            &mut debug_messages,
            None,
            &RendererResources::default(),
            azul_core::resources::IdNamespace(0),
            DomId::ROOT_ID,
            font_loader,
            FakePageConfig::new(),
            &azul_core::resources::ImageCache::default(),
            azul_core::task::GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
            false,
        )
        .expect("layout should succeed");
    }
}

/// The DOM node carrying `class`, found by walking the styled DOM.
fn dom_node_with_class(class: &str) -> NodeId {
    let styled_dom = Dom::from_xml_string(HTML);
    let node_data = styled_dom.node_data.as_container();
    for (id, data) in node_data.internal.iter().enumerate() {
        if data
            .get_ids_and_classes()
            .iter()
            .any(|c| c.as_class().map_or(false, |c| c.as_str() == class))
        {
            return NodeId::new(id);
        }
    }
    panic!("no node with class {class:?} in the test markup");
}

/// The layout box of the node carrying `class`, out of a laid-out cache.
fn box_of(cache: &Solver3LayoutCache, class: &str) -> Box2D {
    let dom_id = dom_node_with_class(class);
    let tree = cache.tree.as_ref().expect("layout tree");
    for (index, node) in tree.nodes.iter().enumerate() {
        if node.dom_node_id == Some(dom_id) {
            let pos = cache
                .calculated_positions
                .get(index)
                .copied()
                .unwrap_or_else(|| panic!("node {index} ({class}) has no calculated position"));
            let size = node
                .used_size
                .unwrap_or_else(|| panic!("node {index} ({class}) has no used size"));
            return Box2D {
                y: pos.y,
                height: size.height,
            };
        }
    }
    panic!("class {class:?} has no layout node");
}

/// The container the two boxes live in: `<body>`, the parent of `.counter`.
fn body_box(cache: &Solver3LayoutCache) -> Box2D {
    let tree = cache.tree.as_ref().expect("layout tree");
    let counter_dom = dom_node_with_class("counter");
    let counter_index = tree
        .nodes
        .iter()
        .position(|n| n.dom_node_id == Some(counter_dom))
        .expect(".counter has no layout node");
    let parent = tree.nodes[counter_index]
        .parent
        .expect(".counter has no parent");
    Box2D {
        y: cache
            .calculated_positions
            .get(parent)
            .copied()
            .expect("body has no calculated position")
            .y,
        height: tree.nodes[parent]
            .used_size
            .expect("body has no used size")
            .height,
    }
}

#[test]
fn atomic_inline_after_a_block_does_not_move_between_passes() {
    let mut env = Env::new();
    let mut cache = Env::fresh_cache();

    env.layout(&mut cache, 400.0, 300.0);
    let first = box_of(&cache, "btn");

    // A second pass over the same cache: what `Update::RefreshDom` does when
    // the counter is clicked. Nothing in the markup changed, so nothing may
    // move.
    env.layout(&mut cache, 400.0, 300.0);
    let second = box_of(&cache, "btn");

    assert!(
        (first.y - second.y).abs() < 0.01,
        "the inline-flex box moved between two identical layout passes: y {} -> {} (Δ {:.2} px). \
         On screen that is the button jumping, and the vacated pixels staying painted.",
        first.y,
        second.y,
        second.y - first.y
    );
    assert!(
        (first.height - second.height).abs() < 0.01,
        "the inline-flex box changed height between two identical passes: {} -> {}",
        first.height,
        second.height
    );
}

#[test]
fn the_line_box_of_an_atomic_inline_is_as_tall_as_the_box() {
    let mut env = Env::new();
    let mut cache = Env::fresh_cache();
    env.layout(&mut cache, 400.0, 300.0);

    let btn = box_of(&cache, "btn");
    let body = body_box(&cache);

    // The button is the last thing in <body>, so <body> ends where the line
    // box holding it ends. A line box is at least as tall as the atomic inline
    // on it (CSS 2.2 s 10.8), so the button cannot stick out of its parent.
    assert!(
        body.y + body.height + 0.01 >= btn.y + btn.height,
        "body ends at {:.2} but its inline-flex child ends at {:.2}: the line box was sized from \
         the strut ({:.2} px) instead of from the box ({:.2} px)",
        body.y + body.height,
        btn.y + btn.height,
        body.y + body.height - btn.y,
        btn.height
    );
}

#[test]
fn an_atomic_inline_sits_directly_below_the_paragraph_margin() {
    let mut env = Env::new();
    let mut cache = Env::fresh_cache();
    env.layout(&mut cache, 400.0, 300.0);

    let counter = box_of(&cache, "counter");
    let btn = box_of(&cache, "btn");

    // `p { margin: 1em 0 }` at 32 px puts exactly 32 px between the paragraph's
    // border box and the line box below it. Nothing else may creep in: the
    // strut of the anonymous line box must not push the button down.
    let gap = btn.y - (counter.y + counter.height);
    assert!(
        (gap - 32.0).abs() < 0.6,
        "the gap between the paragraph and the inline-flex box is {gap:.2} px, expected 32 px \
         (the paragraph's 1em bottom margin)"
    );
}
