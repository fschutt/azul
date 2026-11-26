//! A button is a touch target on a phone: at least 48 dp on Android and 44 pt on iOS in each direction.
//!
//! A desktop button is 29 px tall (6 px of padding round a 14 px line) and an icon-only one 29 x 41: the zoom
//! buttons of AzMaps measured 41 x 29 dp on the Android emulator (QA run 45778714528), which a finger misses
//! (Material's touch target is 48 dp, Apple's 44 pt). The floor is a property of the button the themes put on
//! it for those two platforms only: a desktop button keeps its size, a link keeps the height of its line, and
//! a button that owns its whole style (a ribbon's key) sizes itself.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{
    system::{defaults, SystemStyle},
    AzString,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        button::{Button, ButtonType},
        text_input::TextInput,
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The size of the button `button` makes in a 411 x 914 window of `style`.
fn size_of(style: SystemStyle, button: Button) -> (f32, f32) {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(style));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(411.0, 914.0);
    lw.current_window_state = ws.clone();
    let dom = Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column;")
        .with_child(button.dom());
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");

    let lr = &lw.layout_results[&DomId::ROOT_ID];
    let node = (0..lr.styled_dom.node_data.as_ref().len())
        .map(NodeId::new)
        .find(|n| {
            lr.styled_dom.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == "__azul-native-button"))
        })
        .expect("a button node");
    let idx = *lr.layout_tree.dom_to_layout[&node].first().expect("laid out");
    let size = lr.layout_tree.get(idx).expect("layout node").used_size.expect("sized");
    (size.width, size.height)
}

fn words(theme: UiTheme) -> Button {
    Button::create(AzString::from("OK")).with_theme(theme)
}

fn icon_only(theme: UiTheme) -> Button {
    Button::create(AzString::from(""))
        .with_icon(AzString::from("add"))
        .with_theme(theme)
}

const THEMES: [UiTheme; 2] = [UiTheme::Flat, UiTheme::Flora];

#[test]
fn a_button_is_48_dp_in_each_direction_on_android_in_every_theme() {
    for theme in THEMES {
        for (what, button) in [("with words", words(theme)), ("icon only", icon_only(theme))] {
            let (w, h) = size_of(defaults::android_material_light(), button);
            assert!(w >= 47.9 && h >= 47.9, "{theme:?} {what}: {w} x {h} dp on Android");
        }
    }
}

#[test]
fn a_button_is_44_pt_in_each_direction_on_ios_in_every_theme() {
    for theme in THEMES {
        for (what, button) in [("with words", words(theme)), ("icon only", icon_only(theme))] {
            let (w, h) = size_of(defaults::ios_light(), button);
            assert!(w >= 43.9 && h >= 43.9, "{theme:?} {what}: {w} x {h} pt on iOS");
        }
    }
}

#[test]
fn a_desktop_button_keeps_its_size() {
    for theme in THEMES {
        let (_, h) = size_of(defaults::macos_modern_light(), words(theme));
        assert!(h < 40.0, "{theme:?}: a desktop button does not grow to a finger: {h}");
        let (_, h) = size_of(defaults::windows_11_light(), icon_only(theme));
        assert!(h < 40.0, "{theme:?}: nor an icon-only one: {h}");
    }
}

#[test]
fn a_link_keeps_the_height_of_its_line_on_a_phone() {
    for theme in THEMES {
        let link = Button::with_type(AzString::from("Terms"), ButtonType::Link).with_theme(theme);
        let (_, h) = size_of(defaults::android_material_light(), link);
        assert!(h < 40.0, "{theme:?}: a link is text on its line: {h}");
    }
}

/// The size of the text field `field` makes in a 411 x 914 window of `style`.
fn field_size(style: SystemStyle, field: TextInput) -> (f32, f32) {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(style));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(411.0, 914.0);
    lw.current_window_state = ws.clone();
    let dom = Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column;")
        .with_child(field.dom().with_id("field".into()));
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    let lr = &lw.layout_results[&DomId::ROOT_ID];
    let node = (0..lr.styled_dom.node_data.as_ref().len())
        .map(NodeId::new)
        .find(|n| {
            lr.styled_dom.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_id(), Some(s) if s == "field"))
        })
        .expect("the field's node");
    let idx = *lr.layout_tree.dom_to_layout[&node].first().expect("laid out");
    let size = lr.layout_tree.get(idx).expect("layout node").used_size.expect("sized");
    (size.width, size.height)
}

#[test]
fn a_text_field_is_as_tall_as_a_finger_on_a_phone_and_a_desktop_field_is_not() {
    let field = || TextInput::create().with_text(AzString::from("hello")).with_placeholder(AzString::from("From"));
    let (_, android) = field_size(defaults::android_material_light(), field());
    assert!(android >= 47.9, "48 dp on Android: {android}");
    let (_, ios) = field_size(defaults::ios_light(), field());
    assert!(ios >= 43.9, "44 pt on iOS: {ios}");
    let (_, desktop) = field_size(defaults::macos_modern_light(), field());
    assert!(desktop < 40.0, "a desktop field keeps its size: {desktop}");
}
