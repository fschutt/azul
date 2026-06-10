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

/// Lay the SAME dom out `passes` times on one LayoutWindow (pass 2+ exercises
/// the incremental reconcile path the live apps hit on every configure /
/// RefreshDom — a cold-pass-only test misses incremental-only bugs).
fn layout_dom_n_passes(
    mut make_dom: impl FnMut() -> Dom,
    css_str: &str,
    width: f32,
    height: f32,
    passes: usize,
) -> LayoutWindow {
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();

    for pass in 0..passes {
        let (css, _) = azul_css::parser2::new_from_str(css_str);
        let mut dom = make_dom();
        let styled_dom = StyledDom::create(&mut dom, css);
        let mut debug_messages = Some(Vec::new());
        layout_window
            .layout_and_generate_display_list(
                styled_dom,
                &window_state,
                &renderer_resources,
                &system_callbacks,
                &mut debug_messages,
            )
            .unwrap_or_else(|e| panic!("layout pass {pass} failed: {e:?}"));
    }
    layout_window
}

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

// Verbatim from examples/azul-maps/src/lib.rs (ROOT / HEADER / BTN /
// MAP_CONTAINER consts).
const MAPS_CSS: &str = r#"
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

/// The azul-maps body: ROOT column → header (flex row: status text + nested
/// button row) → map container (flex-grow: 1) holding the REAL MapWidget.
fn maps_demo_dom() -> Dom {
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

    Dom::create_body()
        .with_ids_and_classes(class("root"))
        .with_child(header)
        .with_child(map_container)
}

/// The header must not only OCCUPY space — its items must actually be IN the
/// root display list (dark background rect + text runs above the map). The
/// live app laid the header out correctly yet painted nothing: rect-only
/// assertions miss display-list-generation drops.
#[test]
fn maps_header_items_present_in_display_list() {
    let lw = layout_dom(maps_demo_dom(), MAPS_CSS, 640.0, 480.0);
    let result = lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("root layout result");
    let items = &result.display_list.items;

    use azul_layout::solver3::display_list::DisplayListItem;
    let mut kinds: std::collections::BTreeMap<&'static str, usize> = Default::default();
    for it in items.iter() {
        let k = match it {
            DisplayListItem::Rect { .. } => "rect",
            DisplayListItem::Text { .. } => "text",
            DisplayListItem::TextLayout { .. } => "textlayout",
            DisplayListItem::VirtualView { .. } => "vview",
            DisplayListItem::VirtualViewPlaceholder { .. } => "vview_ph",
            _ => "other",
        };
        *kinds.entry(k).or_default() += 1;
    }
    println!("root DL: {} items, kinds = {:?}", items.len(), kinds);
    for (i, it) in items.iter().enumerate() {
        match it {
            DisplayListItem::Rect { bounds, color, .. } => println!(
                "  [{i:2}] Rect    rgb({},{},{}) {:?}", color.r, color.g, color.b, bounds.inner()
            ),
            DisplayListItem::VirtualView { bounds, .. } => println!(
                "  [{i:2}] VView   {:?}", bounds.inner()
            ),
            DisplayListItem::VirtualViewPlaceholder { bounds, .. } => println!(
                "  [{i:2}] VViewPh {:?}", bounds.inner()
            ),
            _ => {}
        }
    }

    // The dark header background (#2b2b2b) must be painted by the root DL.
    let header_bg = items.iter().any(|it| match it {
        DisplayListItem::Rect { color, .. } => {
            color.r == 0x2b && color.g == 0x2b && color.b == 0x2b
        }
        _ => false,
    });
    assert!(
        header_bg,
        "the root display list does not paint the header's #2b2b2b background \
         (items={}, kinds={:?}) — display-list generation dropped the header",
        items.len(),
        kinds
    );
    // And at least one text/textlayout item must exist (status line + buttons).
    assert!(
        kinds.get("text").copied().unwrap_or(0) + kinds.get("textlayout").copied().unwrap_or(0) > 0,
        "no text items in the root display list at all: kinds={:?}",
        kinds
    );
}

/// PIXEL-level test: rasterize the maps root display list through the SAME
/// layered-compositor path the CPU backends use (allocate_layers →
/// render_layers → composite_frame) and assert the header band actually comes
/// out dark. The display list is verified correct (header rect 624x70 @ (8,8))
/// yet the live app shows no header — so the drop is in the rasterizer.
#[test]
fn maps_render_paints_header_pixels() {
    use azul_layout::cpurender;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lw = layout_dom(maps_demo_dom(), MAPS_CSS, 640.0, 480.0);
    let root = lw.layout_results.get(&DomId::ROOT_ID).expect("root result");

    // The MapWidget's VirtualView child DOM(s), laid out during the pass.
    let mut vview_dls: BTreeMap<DomId, Arc<azul_layout::solver3::display_list::DisplayList>> =
        BTreeMap::new();
    for (id, r) in lw.layout_results.iter() {
        if *id != DomId::ROOT_ID {
            vview_dls.insert(*id, Arc::new(r.display_list.clone()));
        }
    }
    println!("vview child DLs: {:?}", vview_dls.keys().collect::<Vec<_>>());

    let dpi = 1.0f32;
    let renderer_resources = RendererResources::default();
    let mut glyph_cache = azul_layout::glyph_cache::GlyphCache::new();
    let render_state = cpurender::CpuRenderState::new(Default::default())
        .with_virtual_view_display_lists(vview_dls);

    // Replicate CpuBackend::render_frame's FULL-RENDER path exactly.
    let mut compositor = cpurender::CompositorState::new(640, 480);
    compositor.allocate_layers_from_display_list(&root.display_list, dpi);
    compositor
        .render_layers(
            &root.display_list,
            dpi,
            &renderer_resources,
            Some(&lw.font_manager),
            &mut glyph_cache,
            &render_state,
        )
        .expect("render_layers");
    let mut out = cpurender::AzulPixmap::new(640, 480).expect("pixmap");
    compositor.composite_frame(&mut out, dpi);

    // Dump the rendered frame for visual inspection on failure analysis.
    if let Ok(bytes) = out.encode_png() {
        let _ = std::fs::write("/tmp/maps-pixel-test.png", bytes);
    }

    let px = |x: usize, y: usize| -> (u8, u8, u8) {
        let i = (y * 640 + x) * 4;
        let d = out.data();
        (d[i], d[i + 1], d[i + 2])
    };
    // Sample a row across the header band at y=12 — inside the header's
    // padding rows, ABOVE the status text, so no anti-aliased glyph pixels
    // pollute the samples (y=40 hits text edges).
    let samples: Vec<(usize, (u8, u8, u8))> =
        [40usize, 150, 300, 450, 600].iter().map(|&x| (x, px(x, 12))).collect();
    println!("header-band pixels at y=12: {:?}", samples);

    // The header background is #2b2b2b (43,43,43); buttons are #4a90e2.
    let dark = |c: (u8, u8, u8)| {
        (c.0 as i32 - 43).abs() < 25 && (c.1 as i32 - 43).abs() < 25 && (c.2 as i32 - 43).abs() < 25
    };
    let blue = |c: (u8, u8, u8)| {
        (c.0 as i32 - 74).abs() < 25 && (c.1 as i32 - 144).abs() < 30 && (c.2 as i32 - 226).abs() < 30
    };
    assert!(
        dark(px(40, 12)) && dark(px(150, 12)),
        "header band is NOT painted dark at y=40: {:?} — the rasterizer drops \
         or overpaints the header (live bug: child PushClip replaced the \
         VirtualView composite clip)",
        samples
    );
    // Every sampled header-band pixel must be header chrome (dark bg, white
    // text, or blue button) — never the child's placeholder-tile grey, which
    // is what unclipped child overdraw paints.
    for (x, c) in &samples {
        let whiteish = c.0 > 230 && c.1 > 230 && c.2 > 230;
        assert!(
            dark(*c) || blue(*c) || whiteish,
            "header band pixel at x={x} is {:?} — child content overdrew the \
             header (clip escape)",
            c
        );
    }
}

/// MINIMAL reproduction of the live invisible-header bug, independent of the
/// MapWidget: a parent display list paints a dark header rect, then composites
/// a VirtualView child inside a PushClip. The CHILD's own display list begins
/// with a window-sized PushClip + background — exactly what a child DOM's root
/// produces. Before the fix, the child's PushClip REPLACED the composite clip
/// (instead of intersecting), and its bounds were not shifted into the
/// composite coordinate space, so the child's background painted over the
/// whole window — wiping the header (live: azul-maps' toolbar invisible).
#[test]
fn virtual_view_child_clip_cannot_escape_composite_bounds() {
    use azul_layout::cpurender;
    use azul_layout::solver3::display_list::{
        BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect,
    };
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
    use azul_css::props::basic::color::ColorU;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let rect = |x: f32, y: f32, w: f32, h: f32| -> WindowLogicalRect {
        LogicalRect {
            origin: LogicalPosition { x, y },
            size: LogicalSize { width: w, height: h },
        }
        .into()
    };
    let dark = ColorU { r: 43, g: 43, b: 43, a: 255 };
    let grey = ColorU { r: 231, g: 233, b: 236, a: 255 };

    let child_dom = DomId { inner: 1 };
    // Child DL: window-sized clip + background — a typical child root.
    let child_dl = DisplayList {
        items: vec![
            DisplayListItem::PushClip {
                bounds: rect(0.0, 0.0, 640.0, 480.0),
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::Rect {
                bounds: rect(0.0, 0.0, 640.0, 480.0),
                color: grey,
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::PopClip,
        ],
        ..Default::default()
    };
    // Parent DL: dark header strip, then the VirtualView clipped to the lower
    // region (y 78..488).
    let parent_dl = DisplayList {
        items: vec![
            DisplayListItem::Rect {
                bounds: rect(8.0, 8.0, 624.0, 70.0),
                color: dark,
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::PushClip {
                bounds: rect(8.0, 78.0, 624.0, 402.0),
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::VirtualView {
                child_dom_id: child_dom,
                bounds: rect(8.0, 78.0, 624.0, 402.0),
                clip_rect: rect(8.0, 78.0, 624.0, 402.0),
            },
            DisplayListItem::PopClip,
        ],
        ..Default::default()
    };

    let mut vview_dls = BTreeMap::new();
    vview_dls.insert(child_dom, Arc::new(child_dl));

    let renderer_resources = RendererResources::default();
    let mut glyph_cache = azul_layout::glyph_cache::GlyphCache::new();
    let render_state = cpurender::CpuRenderState::new(Default::default())
        .with_virtual_view_display_lists(vview_dls);

    let mut compositor = cpurender::CompositorState::new(640, 480);
    compositor.allocate_layers_from_display_list(&parent_dl, 1.0);
    compositor
        .render_layers(&parent_dl, 1.0, &renderer_resources, None, &mut glyph_cache, &render_state)
        .expect("render_layers");
    let mut out = cpurender::AzulPixmap::new(640, 480).expect("pixmap");
    compositor.composite_frame(&mut out, 1.0);

    let px = |x: usize, y: usize| -> (u8, u8, u8) {
        let i = (y * 640 + x) * 4;
        let d = out.data();
        (d[i], d[i + 1], d[i + 2])
    };
    println!("header (40,40)={:?} child-region (40,200)={:?}", px(40, 40), px(40, 200));
    assert_eq!(
        px(40, 40),
        (43, 43, 43),
        "the child's own PushClip escaped the VirtualView composite clip and \
         painted over the parent's header"
    );
    assert_eq!(
        px(40, 200),
        (231, 233, 236),
        "the child must still paint INSIDE the composite bounds"
    );
}

/// Cold pass: the header must get its content height (~37px+), not 0.
#[test]
fn maps_header_must_not_collapse_to_zero_height() {
    let lw = layout_dom(maps_demo_dom(), MAPS_CSS, 640.0, 480.0);

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

/// The demo attaches styles via `.with_css("...")` per node (the @scope
/// subtree path), NOT via a compiled stylesheet — replicate that exactly,
/// since the stylesheet-styled variants above pass while the live app fails.
fn maps_demo_dom_with_css() -> Dom {
    const HEADER: &str = "background: #2b2b2b; color: white; \
        display: flex; padding: 10px 16px; flex-direction: row; align-items: center; \
        justify-content: space-between; font-family: sans-serif; \
        font-size: 14px; flex-shrink: 0;";
    const BTN: &str = "background: #4a90e2; color: white; \
        padding: 6px 12px; border-radius: 4px; cursor: pointer; \
        margin-left: 6px; font-size: 13px;";
    const MAP_CONTAINER: &str = "flex-grow: 1; position: relative; \
        background: #cbd2d8; overflow: hidden;";
    const ROOT: &str = "display: flex; flex-direction: column; height: 100%;";

    let mut button_row = Dom::create_div().with_css("display: flex; flex-direction: row;");
    for label in ["←", "→", "↑", "↓", "+", "−", "Recentre", "Locate", "Clear pins"] {
        button_row = button_row
            .with_child(Dom::create_div().with_css(BTN).with_child(Dom::create_text(label)));
    }
    let header = Dom::create_div()
        .with_css(HEADER)
        .with_child(Dom::create_text("AzulMaps — centre 37.0000°, -122.0000° · zoom 2.0"))
        .with_child(button_row);
    let map = MapWidget::create(MapTileLayer::default())
        .with_viewport(MapViewport::default())
        .dom();
    let map_container = Dom::create_div().with_css(MAP_CONTAINER).with_child(map);
    Dom::create_body()
        .with_css(ROOT)
        .with_child(header)
        .with_child(map_container)
}

/// Same as the cold test but styled the way the LIVE demo styles itself
/// (per-node with_css). The live header is invisible while the
/// stylesheet-styled test passes — this pins down whether the with_css
/// cascade path is the missing trigger.
#[test]
fn maps_header_with_css_styling_must_not_collapse() {
    let lw = layout_dom(maps_demo_dom_with_css(), "", 640.0, 480.0);

    let header_rect = lw
        .get_node_layout_rect(node_id(1))
        .expect("header rect (with_css variant)");
    println!("with_css header = {:?}", header_rect);
    assert!(
        header_rect.size.height > 25.0,
        "maps header (with_css styling) collapsed: height = {}",
        header_rect.size.height
    );
}

/// Incremental passes: the live app re-lays out on every configure /
/// RefreshDom through the reconcile path — the header must survive THAT too
/// (the cold pass above passing while the live header is invisible means the
/// incremental path regressed separately).
#[test]
fn maps_header_survives_incremental_relayout() {
    let lw = layout_dom_n_passes(maps_demo_dom, MAPS_CSS, 640.0, 480.0, 3);

    let header_rect = lw
        .get_node_layout_rect(node_id(1))
        .expect("header rect after incremental relayout");
    println!("header after 3 passes = {:?}", header_rect);
    assert!(
        header_rect.size.height > 25.0,
        "maps header collapsed on INCREMENTAL relayout: height = {}",
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
