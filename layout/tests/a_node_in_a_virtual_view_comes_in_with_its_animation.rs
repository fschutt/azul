//! A node in a `VirtualView`'s content comes in with its `-azul-animation-in`.
//!
//! Only the root DOM's reconciliation started enter tracks, so a node of a view's child DOM
//! that declared one appeared at rest: a spinner inside a view stood still, a panel a view
//! renders did not fade in. A node that enters with the view's render starts its track
//! when the child DOM is laid out, is drawn from its first keyframe in that frame, and
//! keeps the window asking for frames while it runs.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

struct ViewData;

/// The view's content: a box that fades up from nothing as it comes in.
extern "C" fn render_view(_data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let (keyframes, _) = azul_css::parser2::new_from_str(
        "@keyframes rise { from { opacity: 0; } to { opacity: 1; } }",
    );
    let dom = Dom::create_div()
        .with_css("width: 200px; height: 100px;")
        .with_component_css(keyframes)
        .with_child(Dom::create_div().with_css(
            "width: 100px; height: 40px; background: #ff0000; -azul-animation-in: rise 300ms \
             linear;",
        ));
    let rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(200.0, 100.0));
    VirtualViewReturn::with_dom(dom, rect, rect)
}

fn host_page() -> StyledDom {
    let mut dom = Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_virtual_view(
            RefAny::new(ViewData),
            VirtualViewCallback::create(render_view),
        )
        .with_css("width: 200px; height: 100px;"),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

#[test]
fn a_node_in_a_virtual_view_comes_in_with_its_animation() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        host_page(),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the host lays out");

    let view = *lw
        .layout_results
        .keys()
        .find(|d| **d != DomId::ROOT_ID)
        .expect("harness: the view's child DOM was laid out");
    let cache = lw.gpu_state_manager.get_cache(view);
    let keyed: Vec<_> = cache
        .map(|c| {
            c.anim_opacity_keys
                .iter()
                .map(|(node, key)| (*node, *key))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        keyed.len(),
        1,
        "the box that came in with the view runs its entrance in the view's own channels"
    );
    let (node, key) = keyed[0];
    let value = cache.and_then(|c| c.anim_current_opacity_values.get(&node).copied());
    let list = &lw.layout_results[&view].display_list;
    let drawn_from_nothing = list.items.iter().any(|item| {
        matches!(
            item,
            DisplayListItem::PushOpacity { opacity_key: Some(k), .. } if *k == key
        )
    }) && value.is_some_and(|v| v < 0.05);
    assert!(
        drawn_from_nothing,
        "the view's frame draws the box at its first keyframe (opacity {value:?})"
    );
    assert!(
        lw.needs_animation_frame(),
        "the window keeps asking for frames while the entrance runs"
    );
}
