//! A `<label>` names the control it is for, the way HTML associates them: a
//! label `for` the control's id, or the label the control sits in. A screen
//! reader then says "E-mail, edit text" on the field, not just "edit text"
//! with the label read as an unrelated text before it. A name the control
//! declares itself still wins.
#![cfg(feature = "a11y")]

use azul_core::{
    dom::{AccessibilityInfo, AccessibilityRole, Dom, NodeType, TabIndex},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn lay_out(lw: &mut LayoutWindow, mut dom: Dom) -> accesskit::TreeUpdate {
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
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
    lw.update_a11y_tree();
    lw.a11y_manager.full_tree()
}

/// A focusable control with `role` and no name of its own.
fn control(role: AccessibilityRole) -> Dom {
    Dom::create_div()
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role,
            ..AccessibilityInfo::default()
        })
}

fn label_of(update: &accesskit::TreeUpdate, role: accesskit::Role) -> Vec<Option<String>> {
    update
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == role)
        .map(|(_, n)| n.label().map(str::to_string))
        .collect()
}

#[test]
fn a_label_for_a_controls_id_names_the_control() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let update = lay_out(
        &mut lw,
        Dom::create_body()
            .with_child(Dom::create_label_no_a11y("email".into(), "E-mail".into()))
            .with_child(control(AccessibilityRole::Text).with_id("email".into())),
    );
    assert_eq!(
        label_of(&update, accesskit::Role::TextInput),
        vec![Some("E-mail".to_string())],
        "the field is named by the label for its id"
    );
}

#[test]
fn a_label_around_a_control_names_it_with_its_own_text() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let update = lay_out(
        &mut lw,
        Dom::create_body().with_child(
            Dom::create_node(NodeType::Label)
                .with_child(control(AccessibilityRole::CheckButton))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "Remember me",
                )),
        ),
    );
    assert_eq!(
        label_of(&update, accesskit::Role::CheckBox),
        vec![Some("Remember me".to_string())],
        "the check box is named by the label it sits in"
    );
}

#[test]
fn a_declared_name_wins_over_the_label() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let update = lay_out(
        &mut lw,
        Dom::create_body()
            .with_child(Dom::create_label_no_a11y("q".into(), "Search".into()))
            .with_child(
                control(AccessibilityRole::Text)
                    .with_id("q".into())
                    .with_accessibility_name(AzString::from("Search the archive")),
            ),
    );
    assert_eq!(
        label_of(&update, accesskit::Role::TextInput),
        vec![Some("Search the archive".to_string())]
    );
}

/// The phone element list (iOS / Android) names the control the same way.
#[test]
fn the_phone_element_list_names_a_labelled_control() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let _ = lay_out(
        &mut lw,
        Dom::create_body()
            .with_child(Dom::create_label_no_a11y("email".into(), "E-mail".into()))
            .with_child(control(AccessibilityRole::Text).with_id("email".into())),
    );
    let snapshot = lw.build_a11y_snapshot();
    let field = snapshot
        .elements
        .iter()
        .find(|e| e.role == AccessibilityRole::Text)
        .expect("the field is in the element list");
    assert_eq!(field.label, "E-mail");
}
