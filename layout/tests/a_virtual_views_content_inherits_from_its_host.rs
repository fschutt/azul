//! A `VirtualView`'s content inherits from the node that hosts it.
//!
//! A view is a virtualized part of the SAME document - unlike an iframe - so
//! what its host computes for an inherited property (`font-family`,
//! `font-size`, `color`, `user-select`, `cursor`, `direction`,
//! `line-height`, ...) is what the root of the view's DOM inherits (CSS
//! Cascade 4 s7: an element inherits its parent's computed value; the host is
//! the parent of the view's root). The view's DOM was cascaded in a scope of
//! its own instead (`LayoutWindow::style_user_dom_in_scope`), as if it were a
//! document of its own: its text came out in the engine's default serif, in
//! the document root's UA colour, selectable - whatever the page around it
//! said (AzMusic's track tables, AzNews' article table, 2026-10-07).
//!
//! The child's own declarations still win over what it inherits, and a view
//! the relayout KEEPS (its host node unchanged) follows its host when the
//! host's values change - a host colour change, a light/dark switch.
//!
//! Not compiled by the author (house rule). Expected RED before the fix:
//! every assertion on the view's text reads the engine defaults.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
    window::DarkLightMode,
};
use azul_css::{
    props::{
        basic::{color::ColorU, font::StyleFontFamily},
        style::effects::StyleCursor,
    },
    system::SystemFontType,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::getters::{get_element_font_size, get_used_text_color, is_text_selectable},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// What the view's callback renders: a bare `<p>` with text (`p_css` on the
/// `<p>`, if any), counting its invocations.
struct ViewData {
    p_css: &'static str,
    calls: Arc<AtomicUsize>,
}

extern "C" fn render_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let p_css = match data.downcast_ref::<ViewData>() {
        Some(view) => {
            view.calls.fetch_add(1, Ordering::SeqCst);
            view.p_css
        }
        None => "",
    };
    let mut p = Dom::create_p_with_text("Track 01 - 3:41");
    if !p_css.is_empty() {
        p = p.with_css(p_css);
    }
    let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
    VirtualViewReturn::with_dom(p, rect, rect)
}

fn view(p_css: &'static str, calls: &Arc<AtomicUsize>) -> RefAny {
    RefAny::new(ViewData {
        p_css,
        calls: Arc::clone(calls),
    })
}

/// A page whose `host_css` container holds the view (`view_data`).
fn page(host_css: &str, view_data: &RefAny) -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div().with_css(host_css).with_child(
            Dom::create_virtual_view(view_data.clone(), VirtualViewCallback::create(render_view))
                .with_css("width: 300px; height: 40px;"),
        ),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws;
    lw
}

fn lay_out(lw: &mut LayoutWindow, styled_dom: StyledDom) {
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

/// Lay the page out again from its retained DOM, as an animation frame or a
/// paint-only light/dark switch does.
fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    lay_out(lw, result.styled_dom);
}

/// The view's DOM: `<p>` is node 0, its text node 1.
fn view_dom(lw: &LayoutWindow) -> &StyledDom {
    let child = lw
        .layout_results
        .keys()
        .copied()
        .find(|d| *d != DomId::ROOT_ID)
        .expect("harness: the view's DOM is laid out");
    &lw.layout_results.get(&child).expect("laid out").styled_dom
}

const P: NodeId = NodeId::new(0);
const TEXT: NodeId = NodeId::new(1);

fn state(sd: &StyledDom, node: NodeId) -> StyledNodeState {
    sd.styled_nodes.as_container()[node].styled_node_state
}

fn colour(sd: &StyledDom, node: NodeId) -> ColorU {
    get_used_text_color(sd, node, &state(sd, node))
}

/// The node's `font-family` as the cascade answers it (`computed_values`)
/// and as the text layout reads it (the compact cache's family hash).
fn font_families(
    sd: &StyledDom,
    node: NodeId,
) -> (Option<Vec<StyleFontFamily>>, Option<Vec<StyleFontFamily>>) {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let cascaded = cache
        .get_font_family(node_data, &node, &state(sd, node))
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.as_slice().to_vec());
    let laid_out = cache.compact_cache.as_ref().and_then(|cc| {
        let hash = cc.tier2b_text[node.index()].font_family_hash;
        cc.font_hash_to_families
            .get(&hash)
            .map(|v| v.as_slice().to_vec())
    });
    (cascaded, laid_out)
}

fn cursor(sd: &StyledDom, node: NodeId) -> Option<StyleCursor> {
    let node_data = &sd.node_data.as_container()[node];
    sd.get_css_property_cache()
        .get_cursor(node_data, &node, &state(sd, node))
        .and_then(|v| v.get_property().copied())
}

const HOST: &str = "font-family: system:ui; font-size: 13px; user-select: none; \
                    cursor: default; color: #123456;";

fn rgb(r: u8, g: u8, b: u8) -> ColorU {
    ColorU { r, g, b, a: 255 }
}

#[test]
fn a_virtual_views_text_inherits_its_hosts_font_colour_and_selectability() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window();
    lay_out(&mut lw, page(HOST, &view("", &calls)));
    let sd = view_dom(&lw);

    let system_ui = vec![StyleFontFamily::SystemType(SystemFontType::Ui)];
    assert_eq!(
        font_families(sd, TEXT),
        (Some(system_ui.clone()), Some(system_ui)),
        "the view's text must take its host's `font-family: system:ui` (cascade, text layout) - \
         not the engine's default serif"
    );
    assert_eq!(
        get_element_font_size(sd, TEXT, &state(sd, TEXT)),
        13.0,
        "the view's text must take its host's 13px"
    );
    assert_eq!(
        colour(sd, TEXT),
        rgb(0x12, 0x34, 0x56),
        "the view's text must paint in its host's colour, not the document root's UA colour"
    );
    assert!(
        !is_text_selectable(sd, TEXT, &state(sd, TEXT)),
        "the host says `user-select: none` - the view's text must not be selectable"
    );
    assert_eq!(
        cursor(sd, TEXT),
        Some(StyleCursor::Default),
        "the host's `cursor: default` reaches the view's text over a text node's UA I-beam, as \
         it does a text node under the host itself"
    );
}

#[test]
fn a_virtual_views_own_declarations_win_over_what_its_host_hands_down() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window();
    lay_out(
        &mut lw,
        page(HOST, &view("color: #00aa00; user-select: text;", &calls)),
    );
    let sd = view_dom(&lw);

    assert_eq!(
        colour(sd, P),
        rgb(0x00, 0xaa, 0x00),
        "the <p>'s own `color` beats the colour its host hands down"
    );
    assert_eq!(colour(sd, TEXT), rgb(0x00, 0xaa, 0x00), "and its text inherits it");
    assert!(
        is_text_selectable(sd, TEXT, &state(sd, TEXT)),
        "the <p>'s own `user-select: text` beats its host's `none`"
    );
    let system_ui = vec![StyleFontFamily::SystemType(SystemFontType::Ui)];
    assert_eq!(
        font_families(sd, TEXT).0,
        Some(system_ui),
        "what the <p> does not declare still comes from the host"
    );
}

#[test]
fn a_host_colour_change_reaches_the_view_a_relayout_keeps() {
    let calls = Arc::new(AtomicUsize::new(0));
    let data = view("", &calls);
    let mut lw = window();
    lay_out(&mut lw, page(HOST, &data));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "harness: the view rendered once");
    assert_eq!(colour(view_dom(&lw), TEXT), rgb(0x12, 0x34, 0x56));

    // The same view node (callback and dataset instance) under a host that
    // changed its colour: the relayout keeps the view's DOM...
    lay_out(
        &mut lw,
        page(
            "font-family: system:ui; font-size: 13px; user-select: none; cursor: default; \
             color: #654321;",
            &data,
        ),
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "harness: the host node is unchanged - the relayout keeps the view"
    );
    // ...and the kept DOM follows its host.
    assert_eq!(
        colour(view_dom(&lw), TEXT),
        rgb(0x65, 0x43, 0x21),
        "the kept view's text must follow its host's new colour"
    );
}

#[test]
fn a_light_dark_switch_reaches_the_view_a_relayout_keeps() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window();
    lw.current_window_state.mode = DarkLightMode::Light;
    // A host that sets no colour: its text takes the document's UA colour,
    // which follows the mode.
    lay_out(
        &mut lw,
        page("font-family: system:ui;", &view("", &calls)),
    );
    let host_colour = |lw: &LayoutWindow| {
        let root = &lw.layout_results.get(&DomId::ROOT_ID).expect("laid out").styled_dom;
        // body > div > view: the view is node 2.
        colour(root, NodeId::new(2))
    };
    let light = host_colour(&lw);
    assert_eq!(
        colour(view_dom(&lw), TEXT),
        light,
        "the view's text takes its host's (light) colour"
    );

    // A paint-only switch: the app's layout() does not read the mode, so the
    // retained DOM is laid out again under the new context.
    lw.current_window_state.mode = DarkLightMode::Dark;
    relayout(&mut lw);
    let dark = host_colour(&lw);
    assert_ne!(light, dark, "harness: the host's UA colour follows the mode");
    assert_eq!(
        colour(view_dom(&lw), TEXT),
        dark,
        "after a light/dark switch the view's text must follow its host into the dark colour"
    );
}
