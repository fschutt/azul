//! A screen reader reads what the window renders, and nothing else.
//!
//! A subtree with `display: none` generates no box: it is not on screen, so
//! a browser leaves it out of the accessibility tree as well. A closed popup,
//! a hidden tab panel and a collapsed menu are all `display: none` in their
//! window. A text node that holds only white space is layout glue, not
//! content: announcing it reads an empty element.
#![cfg(feature = "a11y")]

use azul_core::{
    dom::{Dom, IdOrClass},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { width: 600px; font-size: 14px; }
    .gone { display: none; }
    .row { display: flex; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(text: &str) -> Dom {
    Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text))
}

fn tree_of(mut dom: Dom) -> accesskit::TreeUpdate {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 400.0);
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
    lw.update_a11y_tree();
    lw.a11y_manager.full_tree()
}

fn labels(update: &accesskit::TreeUpdate) -> Vec<String> {
    update
        .nodes
        .iter()
        .filter(|(id, _)| id.0 != 0)
        .filter_map(|(_, n)| n.label().map(str::to_string))
        .collect()
}

#[test]
fn a_subtree_with_display_none_is_not_in_the_accessibility_tree() {
    let update = tree_of(
        Dom::create_body()
            .with_child(para("Shown"))
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("gone"))
                    .with_child(para("Secret"))
                    .with_child(Dom::create_button_no_a11y("Delete everything".into())),
            )
            .with_child(para("Also shown")),
    );
    let labels = labels(&update);
    assert!(
        labels.iter().any(|l| l == "Shown") && labels.iter().any(|l| l == "Also shown"),
        "the rendered paragraphs are read: {labels:?}"
    );
    assert!(
        !labels
            .iter()
            .any(|l| l == "Secret" || l == "Delete everything"),
        "nothing of a display:none subtree is read: {labels:?}"
    );
}

#[test]
fn a_white_space_text_node_is_not_announced() {
    let update = tree_of(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("row"))
                .with_child(para("left"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("\n    "))
                .with_child(para("right")),
        ),
    );
    let blank: Vec<_> = update
        .nodes
        .iter()
        .filter(|(id, _)| id.0 != 0)
        .filter(|(_, n)| n.role() == accesskit::Role::Label)
        .filter(|(_, n)| n.label().is_none_or(|l| l.trim().is_empty()))
        .map(|(id, n)| (*id, n.label().map(str::to_string)))
        .collect();
    assert!(
        blank.is_empty(),
        "a text node of white space only is not an element a screen reader stops on: {blank:?}"
    );
}
