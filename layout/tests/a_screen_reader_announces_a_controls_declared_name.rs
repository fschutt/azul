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

/// A switch is announced as a switch ("Wi-Fi, switch, on"), not as a check
/// box: accesskit has the role (VoiceOver's AXSwitch subrole).
#[cfg(feature = "widgets")]
#[test]
fn a_switch_is_announced_as_a_switch_with_its_state() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let update = lay_out(
        &mut lw,
        Dom::create_body().with_child(
            azul_layout::widgets::switch::Switch::create(true)
                .with_accessibility_name("Wi-Fi")
                .dom(),
        ),
    );
    let switch = update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some("Wi-Fi"))
        .map(|(_, n)| n.clone())
        .expect("the switch is in the tree");
    assert_eq!(switch.role(), accesskit::Role::Switch);
    assert_eq!(switch.toggled(), Some(accesskit::Toggled::True));
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

/// A button showing "Save" with the title "Saves the file", a button that
/// shows nothing and has the title "Print", and an image with the alt text
/// "Company logo".
fn titled_controls() -> Dom {
    use azul_core::dom::{AccessibilityInfo, AccessibilityRole, AttributeType};
    Dom::create_body()
        .with_child(
            Dom::create_button_no_a11y(AzString::from("Save"))
                .with_attribute(AttributeType::Title("Saves the file".into())),
        )
        .with_child(
            Dom::create_div()
                .with_tab_index(TabIndex::Auto)
                .with_accessibility_info(AccessibilityInfo {
                    role: AccessibilityRole::PushButton,
                    ..AccessibilityInfo::default()
                })
                .with_attribute(AttributeType::Title("Print".into())),
        )
        .with_child(
            Dom::create_div()
                .with_accessibility_info(AccessibilityInfo {
                    role: AccessibilityRole::Graphic,
                    ..AccessibilityInfo::default()
                })
                .with_attribute(AttributeType::Alt("Company logo".into())),
        )
}

/// A `title` is a tooltip. It names a control only when nothing else does
/// (no declared name, no text of its own) and otherwise describes it, and a
/// name is never read a second time as the description. An image is named
/// by its `alt` text.
#[test]
fn a_title_names_a_control_only_when_nothing_else_does() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let update = lay_out(&mut lw, titled_controls());
    let read = |role: accesskit::Role| -> Vec<(Option<String>, Option<String>)> {
        update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == role)
            .map(|(_, n)| (n.label().map(str::to_string), n.description().map(str::to_string)))
            .collect()
    };
    assert_eq!(
        read(accesskit::Role::Button),
        vec![
            (Some("Save".to_string()), Some("Saves the file".to_string())),
            (Some("Print".to_string()), None),
        ],
        "the text names the button and the title describes it; a title that names a control does \
         not describe it too"
    );
    assert_eq!(
        read(accesskit::Role::Image),
        vec![(Some("Company logo".to_string()), None)],
        "an image is named by its alt text"
    );
}

/// The iOS and Android shells read their own element list. It names a
/// control the way the accesskit tree does, so VoiceOver on a phone says
/// the same as VoiceOver on a Mac.
#[test]
fn the_phone_element_list_names_controls_the_way_the_tree_does() {
    use azul_core::dom::AccessibilityRole;
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let _ = lay_out(&mut lw, titled_controls());
    let snapshot = lw.build_a11y_snapshot();
    let read = |role: AccessibilityRole| -> Vec<(String, Option<String>)> {
        snapshot
            .elements
            .iter()
            .filter(|e| e.role == role)
            .map(|e| (e.label.clone(), e.description.clone()))
            .collect()
    };
    assert_eq!(
        read(AccessibilityRole::PushButton),
        vec![
            ("Save".to_string(), Some("Saves the file".to_string())),
            ("Print".to_string(), None),
        ]
    );
    assert_eq!(read(AccessibilityRole::Graphic), vec![("Company logo".to_string(), None)]);
}

/// The phone element list reads a name given as a translation key in the
/// app's language, as the tree does.
#[cfg(feature = "fluent")]
#[test]
fn the_phone_element_list_reads_a_declared_name_translated() {
    use azul_core::{
        resources::AppConfig,
        window::{AzStringPair, StringPairVec},
    };

    let mut config = AppConfig::default();
    config.fluent_locales = StringPairVec::from_vec(vec![
        AzStringPair::create("en".into(), "az-save = Save\n".into()),
        AzStringPair::create("de".into(), "az-save = Speichern\n".into()),
    ]);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.set_app_localization(&config);
    let _ = lw.set_locale("de");
    let _ = lay_out(
        &mut lw,
        Dom::create_body().with_child(
            Dom::create_button_no_a11y(AzString::from("\u{e161}"))
                .with_accessibility_name(AzString::tr("az-save")),
        ),
    );
    let snapshot = lw.build_a11y_snapshot();
    assert!(
        snapshot.elements.iter().any(|e| e.label == "Speichern"),
        "the button is read by its translated name, not the key: {:?}",
        snapshot.elements.iter().map(|e| e.label.as_str()).collect::<Vec<_>>()
    );
}

/// A tooltip describes its anchor: a screen reader says "Save, button,
/// Saves the file". The tip is drawn on hover only and is never read as a
/// stray text after the button, shown or not, on any platform.
#[cfg(feature = "widgets")]
#[test]
fn a_tooltip_describes_its_anchor_and_is_not_read_on_its_own() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let update = lay_out(
        &mut lw,
        Dom::create_body().with_child(
            azul_layout::widgets::tooltip::Tooltip::new(
                Dom::create_button_no_a11y(AzString::from("Save")),
                AzString::from("Saves the file"),
            )
            .dom(),
        ),
    );
    let button = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == accesskit::Role::Button)
        .map(|(_, n)| n.clone())
        .expect("the button is in the tree");
    assert_eq!(button.label(), Some("Save"));
    assert_eq!(button.description(), Some("Saves the file"));

    let parent_of: std::collections::BTreeMap<accesskit::NodeId, accesskit::NodeId> = update
        .nodes
        .iter()
        .flat_map(|(id, n)| n.children().iter().map(move |child| (*child, *id)))
        .collect();
    let hidden_at = |start: accesskit::NodeId| -> bool {
        let mut current = Some(start);
        while let Some(id) = current {
            if update.nodes.iter().any(|(i, n)| *i == id && n.is_hidden()) {
                return true;
            }
            current = parent_of.get(&id).copied();
        }
        false
    };
    for (id, node) in &update.nodes {
        if node.label() == Some("Saves the file") {
            assert!(hidden_at(*id), "the tip's text is read on its own: {node:?}");
        }
    }

    let snapshot = lw.build_a11y_snapshot();
    assert!(
        snapshot.elements.iter().all(|e| e.label != "Saves the file"),
        "the phone element list reads the tip on its own"
    );
}
