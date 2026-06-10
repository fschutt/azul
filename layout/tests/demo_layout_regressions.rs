//! Regression tests for layout bugs reproduced from the shipped demos
//! (2026-06-10 demo audit). The MINIMAL versions of both bugs pass (see
//! image_flex_grow.rs and the first iteration of this file), so these tests
//! replicate the demos' REAL structure:
//!
//! 1. azul-maps: header (status text + button row) lays out at HEIGHT 0 in the
//!    live app — the map fills the whole window, the toolbar is invisible and
//!    unclickable (verified live: a 7×5 xdotool click grid over the header band
//!    only ever hit the map; the AT-SPI tree has the buttons but no pixels do).
//!    The real sibling is a MapWidget (VirtualView) — an empty-div stand-in
//!    does NOT reproduce, so the widget is used here.
//! 2. azul-paint: the canvas `<img>` (callback image, no intrinsic size) with
//!    `flex-grow: 1; position: relative; overflow: hidden` lays out 316px wide
//!    instead of stretching to the body width. Paint has a window menubar, so
//!    on Linux/KDE the engine wraps the user DOM via inject_software_menubar:
//!    Html [ menubar, user_body ] — replicated here, since the bare body
//!    version does NOT reproduce.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    menu::{Menu, MenuItem, StringMenuItem},
    resources::{ImageRef, RawImageFormat, RendererResources},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::map::{MapTileLayer, MapViewport, MapWidget},
    widgets::menubar::build_menubar_dom,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn layout_dom(dom: Dom, css_str: &str, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
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
    if let Some(msgs) = &debug_messages {
        for m in msgs.iter() {
            let s = format!("{:?}", m);
            if !s.contains("BoxProps") {
                println!("[layout-debug] {}", s);
            }
        }
    }
    layout_window
}

fn node_id(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// An image with NO intrinsic size (like a callback image): 0×0.
fn sizeless_image() -> ImageRef {
    ImageRef::null_image(0, 0, RawImageFormat::RGBA8, Vec::new())
}

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

/// azul-maps body: ROOT column → header (flex row: status text + nested
/// button row) → map container (flex-grow: 1) holding the REAL MapWidget.
/// The header must get its content height (~37px+), not 0.
#[test]
fn maps_header_must_not_collapse_to_zero_height() {
    let mut button_row = Dom::create_div().with_ids_and_classes(class("btnrow"));
    for label in ["←", "→", "↑", "↓", "+", "−", "Recentre", "Locate", "Clear pins"] {
        button_row = button_row.with_child(
            Dom::create_div()
                .with_ids_and_classes(class("btn"))
                .with_child(Dom::create_text(label)),
        );
    }

    let header = Dom::create_div()
        .with_ids_and_classes(class("header"))
        .with_child(Dom::create_text("AzulMaps — centre 37.0000°, -122.0000° · zoom 2.0"))
        .with_child(button_row);

    // The REAL widget, like examples/azul-maps does it (an empty-div
    // stand-in does not reproduce the collapse).
    let map = MapWidget::create(MapTileLayer::default())
        .with_viewport(MapViewport::default())
        .dom();
    let map_container = Dom::create_div()
        .with_ids_and_classes(class("mapbox"))
        .with_child(map);

    let dom = Dom::create_body()
        .with_ids_and_classes(class("root"))
        .with_child(header)
        .with_child(map_container);

    // Verbatim from examples/azul-maps/src/lib.rs (ROOT / HEADER / BTN /
    // MAP_CONTAINER consts).
    let css = r#"
        .root { display: flex; flex-direction: column; height: 100%; }
        .header { background: #2b2b2b; color: white;
            display: flex; padding: 10px 16px; flex-direction: row; align-items: center;
            justify-content: space-between; font-family: sans-serif;
            font-size: 14px; flex-shrink: 0; }
        .btnrow { display: flex; flex-direction: row; }
        .btn { background: #4a90e2; color: white;
            padding: 6px 12px; border-radius: 4px; cursor: pointer;
            margin-left: 6px; font-size: 13px; }
        .mapbox { flex-grow: 1; position: relative;
            background: #cbd2d8; overflow: hidden; }
    "#;

    let lw = layout_dom(dom, css, 640.0, 480.0);

    // Node order (depth-first): 0 body, 1 header, 2 status text, 3 btnrow,
    // 4..21 buttons (9 × div+text), 22 mapbox, 23.. map widget subtree.
    for n in 0..30 {
        println!("node {:2} -> {:?}", n, lw.get_node_layout_rect(node_id(n)));
    }
    let header_rect = lw.get_node_layout_rect(node_id(1)).expect("header rect");
    let map_rect = lw.get_node_layout_rect(node_id(22)).expect("mapbox rect");
    println!("header = {:?}", header_rect);
    println!("mapbox = {:?}", map_rect);

    // Text ~14px + buttons (~13px + 12px padding) + 20px header padding ⇒ ≥ 30px.
    assert!(
        header_rect.size.height > 25.0,
        "maps header collapsed: height = {} (expected ≥ ~37px; live bug: the \
         header is invisible and the map starts at y=0)",
        header_rect.size.height
    );
    assert!(
        map_rect.origin.y >= header_rect.size.height - 1.0,
        "map must start BELOW the header: map.y = {}, header.height = {}",
        map_rect.origin.y,
        header_rect.size.height
    );
}

/// azul-paint with the engine's software-menubar wrapper (paint sets a window
/// menubar, so on Linux/KDE inject_software_menubar wraps the user DOM):
/// Html [ menubar, Body(ROOT column) [ header, canvas ] ].
/// The canvas must stretch to the full body width (~640), not 316px.
#[test]
fn paint_canvas_with_menubar_wrapper_must_stretch_full_width() {
    let header = Dom::create_div()
        .with_ids_and_classes(class("header"))
        .with_child(Dom::create_text("AzulPaint  ·  0 strokes  ·  Effect: Metaballs"));

    let canvas = Dom::create_image(sizeless_image()).with_ids_and_classes(class("canvas"));

    let body = Dom::create_body()
        .with_ids_and_classes(class("root"))
        .with_child(header)
        .with_child(canvas);

    // The same File/Edit/View menubar azul-paint declares.
    let menu = Menu::create(
        vec![
            MenuItem::String(StringMenuItem::create("File".into())),
            MenuItem::String(StringMenuItem::create("Edit".into())),
            MenuItem::String(StringMenuItem::create("View".into())),
        ]
        .into(),
    );
    let menubar = build_menubar_dom(&menu);
    let menubar_nodes = menubar.estimated_total_children + 1;

    // Mirror dll inject_software_menubar: Html root, menubar first.
    let dom = Dom::create_html()
        .with_children(vec![menubar, body].into());

    // Verbatim from examples/azul-paint/src/lib.rs (ROOT / HEADER / CANVAS).
    let css = r#"
        .root { display: flex; flex-direction: column; height: 100%; }
        .header { display: flex; background: #2b2b2b; color: white; padding: 12px 20px;
            flex-direction: row; align-items: center; font-family: sans-serif;
            font-size: 16px; }
        .canvas { flex-grow: 1; position: relative; overflow: hidden; }
    "#;

    let lw = layout_dom(dom, css, 640.0, 480.0);

    // 0 html, 1..=menubar_nodes menubar subtree, then body, header, text, canvas.
    let body_idx = 1 + menubar_nodes;
    let header_idx = body_idx + 1;
    let canvas_idx = body_idx + 3;
    let body_rect = lw.get_node_layout_rect(node_id(body_idx)).expect("body rect");
    let header_rect = lw.get_node_layout_rect(node_id(header_idx)).expect("header rect");
    let canvas_rect = lw.get_node_layout_rect(node_id(canvas_idx)).expect("canvas rect");
    println!("menubar_nodes = {menubar_nodes}");
    println!("body   = {:?}", body_rect);
    println!("header = {:?}", header_rect);
    println!("canvas = {:?}", canvas_rect);

    assert!(
        canvas_rect.size.width > 600.0,
        "canvas width = {} (expected ~624–640 from cross-axis stretch; live bug \
         lays it out 316px wide)",
        canvas_rect.size.width
    );
    assert!(
        canvas_rect.size.height > 300.0,
        "canvas height = {} (expected to fill the body below the header via \
         flex-grow; got a collapsed/diminished height)",
        canvas_rect.size.height
    );
}
