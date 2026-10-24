//! A VirtualView's callback is handed the window's light / dark mode and its
//! frame (`VirtualViewCallbackInfo::window_mode`, `window_frame`) and may build
//! its DOM from them: the map picks its tiles' look from the mode, the window
//! controls draw "maximize" or "restore" from the frame. So a view renders
//! again when either moves, like a view whose box changed size, and it is
//! handed the mode its window's cascade resolves - the app's pin included -
//! never the desktop's alone.

use std::sync::{Arc, Mutex};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
    window::{DarkLightMode, OptionDarkLightMode, WindowFrame},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// What each render of the view was handed.
type Seen = Arc<Mutex<Vec<(DarkLightMode, WindowFrame)>>>;

struct ViewData {
    seen: Seen,
}

extern "C" fn render_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    if let Some(view) = data.downcast_ref::<ViewData>() {
        view.seen
            .lock()
            .expect("the log")
            .push((info.window_mode, info.window_frame));
    }
    let size = info.bounds.logical_size;
    let dom = Dom::create_div_with_text("text in the view").with_css("width: 100%; height: 20px;");
    let rect = LogicalRect::new(LogicalPosition::zero(), size);
    VirtualViewReturn::with_dom(dom, rect, rect)
}

fn host_page(seen: &Seen) -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_virtual_view(
            RefAny::new(ViewData {
                seen: Arc::clone(seen),
            }),
            VirtualViewCallback::create(render_view),
        )
        .with_css("width: 200px; height: 20px;"),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 400.0);
    ws.mode = DarkLightMode::Light;
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
    .expect("the host lays out");
}

/// Lay the host out again from its retained DOM - what a mode switch that
/// does not rebuild the DOM does (a restyle).
fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    lay_out(lw, result.styled_dom);
}

fn renders(seen: &Seen) -> Vec<(DarkLightMode, WindowFrame)> {
    seen.lock().expect("the log").clone()
}

#[test]
fn a_virtual_view_renders_again_when_its_window_turns_dark() {
    let seen: Seen = Arc::default();
    let mut lw = window();
    lay_out(&mut lw, host_page(&seen));
    assert_eq!(
        renders(&seen),
        vec![(DarkLightMode::Light, WindowFrame::Normal)],
        "harness: one render, in light"
    );

    lw.current_window_state.mode = DarkLightMode::Dark;
    relayout(&mut lw);
    assert_eq!(
        renders(&seen).last(),
        Some(&(DarkLightMode::Dark, WindowFrame::Normal)),
        "the window turned dark - the view must render again, handed dark"
    );
    let after_switch = renders(&seen).len();
    assert_eq!(after_switch, 2, "one render for the switch");

    relayout(&mut lw);
    assert_eq!(
        renders(&seen).len(),
        after_switch,
        "nothing moved since: the view keeps what it rendered"
    );
}

#[test]
fn a_virtual_view_renders_again_when_its_window_is_maximized_at_the_same_size() {
    let seen: Seen = Arc::default();
    let mut lw = window();
    lay_out(&mut lw, host_page(&seen));
    lw.current_window_state.flags.frame = WindowFrame::Maximized;
    relayout(&mut lw);
    assert_eq!(
        renders(&seen),
        vec![
            (DarkLightMode::Light, WindowFrame::Normal),
            (DarkLightMode::Light, WindowFrame::Maximized),
        ],
        "the frame moved - a view drawing maximize or restore must render again"
    );
}

#[test]
fn a_virtual_view_is_handed_the_mode_the_app_pins_not_the_desktops() {
    let seen: Seen = Arc::default();
    let mut lw = window();
    // The desktop is light; the app pins dark, which is what the window's
    // cascade resolves (`LayoutWindow::dynamic_selector_context`).
    lw.mode = OptionDarkLightMode::Some(DarkLightMode::Dark);
    lay_out(&mut lw, host_page(&seen));
    assert_eq!(
        renders(&seen),
        vec![(DarkLightMode::Dark, WindowFrame::Normal)],
        "the view must be handed the mode the window shows"
    );
}
