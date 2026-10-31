//! The accessibility tree says which language the app's text is in.
//!
//! A screen reader picks its voice by the language the platform tree names:
//! VoiceOver by the text language attribute, Narrator by the UIA culture,
//! Orca by the AT-SPI "language" text attribute. accesskit inherits a node's
//! language from its ancestors, so the window root carries the app's
//! language, and a language switch re-publishes it. Without it German text
//! is read by an American voice.
#![cfg(feature = "a11y")]

use accesskit_consumer::{common_filter, Tree};
use azul_core::{
    dom::Dom, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn window() -> LayoutWindow {
    let mut dom = Dom::create_body().with_child(
        Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "Ordner hinzufuegen",
        )),
    );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
    lw
}

#[test]
fn the_window_root_carries_the_apps_language_and_every_element_inherits_it() {
    let mut lw = window();
    let _ = lw.set_locale("de-DE");
    lw.update_a11y_tree();
    let language = lw.active_language().id.as_str().to_string();
    assert!(language.starts_with("de"), "the window speaks German now: {language:?}");

    let update = lw.a11y_manager.full_tree();
    let root = update
        .nodes
        .iter()
        .find(|(id, _)| *id == lw.a11y_manager.root_id)
        .map(|(_, n)| n.clone())
        .expect("the tree has its window root");
    assert_eq!(
        root.language(),
        Some(language.as_str()),
        "the window root names the app's language"
    );

    let tree = Tree::new(update, true);
    let state = tree.state();
    let paragraph = state
        .root()
        .filtered_children(common_filter)
        .flat_map(|body| body.filtered_children(common_filter).collect::<Vec<_>>())
        .next()
        .expect("the body holds the paragraph");
    assert_eq!(
        paragraph.language(),
        Some(language.as_str()),
        "an element inherits the language of the window it is in"
    );
}

#[test]
fn a_language_switch_republishes_the_language_live() {
    let mut lw = window();
    let _ = lw.set_locale("de-DE");
    lw.update_a11y_tree();
    let _ = lw.a11y_manager.take_pending();

    let _ = lw.set_locale("en-US");
    lw.update_a11y_tree();
    let english = lw.active_language().id.as_str().to_string();
    let patch = lw
        .a11y_manager
        .take_pending()
        .expect("a language switch publishes what changed");
    let root = patch
        .nodes
        .iter()
        .find(|(id, _)| *id == lw.a11y_manager.root_id)
        .map(|(_, n)| n.clone())
        .expect("the switch re-publishes the window root");
    assert_eq!(root.language(), Some(english.as_str()));
}
