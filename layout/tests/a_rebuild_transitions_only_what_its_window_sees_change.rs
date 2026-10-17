//! A rebuild transitions only what the WINDOW sees change.
//!
//! `animation: all` turns every property a rebuild changes into a timed
//! transition (`LayoutWindow::begin_reconciliation`, the old tree governs).
//! The new tree's cascade was read for that diff BEFORE the window's
//! dynamic-selector context was installed on it (the layout funnel installs
//! it later), so it answered for no window: the light UA table, no
//! `prefers-color-scheme: dark` block, no `system:` palette. In a dark window
//! every mode-dependent value therefore "changed" on every rebuild and
//! started a transition toward its light value.
//!
//! Found by the /e2e corpus against AzPaint (dark mode, HEADLESS6
//! 2026-10-03): e2e/css-animation-transition changes only the width and got
//! two transitions - the second one walking the inherited text colour from
//! #e8e8e8 toward black.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    task::Instant,
    window::DarkLightMode,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body > sidebar, the sidebar `width` wide; dark text on light, light text
/// on dark. The first page also declares `animation: all 1s linear`.
fn page(width: f32, animated: bool) -> Dom {
    let animation = if animated {
        "animation: all 1s linear;"
    } else {
        ""
    };
    Dom::create_body()
        .with_css("margin: 0;")
        .with_child(Dom::create_div().with_css(&format!(
            "width: {width}px; height: 100px; color: #000000; {animation} \
         @media (prefers-color-scheme: dark) {{ color: #ffffff; }}"
        )))
}

#[test]
fn a_rebuild_transitions_only_what_its_window_sees_change() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    ws.mode = DarkLightMode::Dark;
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page(200.0, true)),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the first page lays out");
    assert!(lw.css_transitions.is_empty(), "harness: nothing moves yet");

    // The rebuild changes the width only; the window stays dark.
    let mut next = StyledDom::create_from_dom(page(40.0, false));
    let _pending = lw.begin_reconciliation(DomId::ROOT_ID, &mut next, Instant::now());

    let transitioned: Vec<&str> = lw
        .css_transitions
        .iter()
        .map(|t| t.prop_type.to_str())
        .collect();
    assert_eq!(
        transitioned,
        vec!["width"],
        "only the width changed in this dark window; the text colour is #ffffff before and after"
    );
}
