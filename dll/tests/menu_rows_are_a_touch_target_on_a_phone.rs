//! A row of a popup menu is a touch target on a phone: 48 dp on Android and 44 pt on iOS.
//!
//! A drop-down's choices and a context menu are popup menus (`menu_renderer`), and their rows were as tall as a
//! desktop's (about 28 px): choosing a language in the settings of an app with a finger was a matter of luck. The
//! content box of an item grows so that its border box is the touch target; a separator and the frame keep their
//! desktop padding, and a desktop's menu is as it was.

use azul::desktop::menu_renderer::MenuMetrics;
use azul_css::system::defaults;

#[test]
fn an_item_is_48_dp_on_android_and_44_pt_on_ios() {
    let android = MenuMetrics::from_system_style(&defaults::android_material_light());
    assert!(android.item_height >= 47.9, "48 dp on Android: {}", android.item_height);
    let ios = MenuMetrics::from_system_style(&defaults::ios_light());
    assert!(ios.item_height >= 43.9, "44 pt on iOS: {}", ios.item_height);
}

#[test]
fn a_desktop_menu_is_as_it_was_and_the_separators_and_the_frame_keep_their_padding_on_a_phone() {
    for style in [defaults::macos_modern_light(), defaults::windows_11_light()] {
        let desktop = MenuMetrics::from_system_style(&style);
        assert!(desktop.item_height < 40.0, "a desktop item keeps its size: {}", desktop.item_height);
    }
    let android = MenuMetrics::from_system_style(&defaults::android_material_light());
    assert!(android.separator_height < 24.0, "a separator is a rule and its margins: {}", android.separator_height);
    assert!(android.frame_pad_v < 12.0, "the frame's padding is the desktop's: {}", android.frame_pad_v);
}
