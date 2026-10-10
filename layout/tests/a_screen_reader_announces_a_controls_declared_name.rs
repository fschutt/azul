//! A screen reader announces a control by the name it declares, in the app's
//! language.
//!
//! The accessible name computation (ARIA's accname) puts a declared name -
//! `aria-label`, the API's `accessibility_name` - before the text inside the
//! control: an icon button names itself "Search" and its glyph is not read, a
//! button showing "OK" can say "Confirm the order". A name given as a
//! translation key (`AzString::tr`) is announced translated: the DOM's
//! translation pass only translates text nodes.
#![cfg(feature = "a11y")]

use azul_core::{
    dom::{Dom, TabIndex},
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

fn labels_of_role(update: &accesskit::TreeUpdate, role: accesskit::Role) -> Vec<String> {
    update
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == role)
        .filter_map(|(_, n)| n.label().map(str::to_string))
        .collect()
}

#[test]
fn a_declared_name_wins_over_the_text_inside_the_control() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let update = lay_out(
        &mut lw,
        Dom::create_body()
            .with_child(
                Dom::create_button_no_a11y(AzString::from("OK"))
                    .with_accessibility_name("Confirm the order"),
            )
            .with_child(
                // An icon font's glyph is the button's only text.
                Dom::create_button_no_a11y(AzString::from("\u{e8b6}"))
                    .with_accessibility_name("Search"),
            ),
    );
    assert_eq!(
        labels_of_role(&update, accesskit::Role::Button),
        vec!["Confirm the order".to_string(), "Search".into()],
        "a declared name is the control's name; the text inside it is not"
    );
}

#[cfg(feature = "fluent")]
#[test]
fn a_name_given_as_a_translation_key_is_announced_translated() {
    use azul_core::{
        resources::AppConfig,
        window::{AzStringPair, StringPairVec},
    };

    let mut config = AppConfig::default();
    config.fluent_locales = StringPairVec::from_vec(vec![
        AzStringPair::create("en".into(), "az-save = Save\naz-save-hint = Saves the file\n".into()),
        AzStringPair::create(
            "de".into(),
            "az-save = Speichern\naz-save-hint = Speichert die Datei\n".into(),
        ),
    ]);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.set_app_localization(&config);
    let _ = lw.set_locale("de");
    let mut info = azul_core::dom::AccessibilityInfo::default();
    info.accessibility_name = Some(AzString::tr("az-save")).into();
    info.description = Some(AzString::tr("az-save-hint")).into();
    info.role = azul_core::dom::AccessibilityRole::PushButton;
    let update = lay_out(
        &mut lw,
        Dom::create_body().with_child(
            Dom::create_div()
                .with_tab_index(TabIndex::Auto)
                .with_accessibility_info(info),
        ),
    );
    let button = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == accesskit::Role::Button)
        .map(|(_, n)| n.clone())
        .expect("the control is in the tree");
    assert_eq!(button.label(), Some("Speichern"), "the name is the translation, not the key");
    assert_eq!(button.description(), Some("Speichert die Datei"));

    let _ = lw.set_locale("en");
    lw.update_a11y_tree();
    let english = lw.a11y_manager.full_tree();
    let button = english
        .nodes
        .iter()
        .find(|(_, n)| n.role() == accesskit::Role::Button)
        .map(|(_, n)| n.clone())
        .expect("the control is in the tree");
    assert_eq!(button.label(), Some("Save"), "a language switch re-translates the name");
}
