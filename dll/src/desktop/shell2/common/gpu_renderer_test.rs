//! Tests of the parts of `gpu_renderer` that need no GL: which text WebRender draws for each setting of the app.

use azul_core::resources::TextAntialiasMode as Mode;
use azul_layout::cpurender::{TextAa, TEXT_AA_DEFAULT};

use super::*;

#[test]
fn the_platform_default_is_lcd_text_on_a_desktop_and_grayscale_on_a_phone() {
    assert_eq!(
        webrender_text_aa(Mode::PlatformDefault, false),
        TextRendering { aa: true, subpixel: true }
    );
    assert_eq!(
        webrender_text_aa(Mode::PlatformDefault, true),
        TextRendering { aa: true, subpixel: false }
    );
}

/// What an app asks for outright is the same on every platform: `AZ_TEXT_AA=lcd` gives LCD text on a phone's GPU too.
#[test]
fn the_other_modes_mean_the_same_on_every_platform() {
    for phone in [false, true] {
        assert_eq!(webrender_text_aa(Mode::Lcd, phone), TextRendering { aa: true, subpixel: true });
        assert_eq!(webrender_text_aa(Mode::LcdLegacy, phone), TextRendering { aa: true, subpixel: true });
        assert_eq!(webrender_text_aa(Mode::Grayscale, phone), TextRendering { aa: true, subpixel: false });
        assert_eq!(webrender_text_aa(Mode::Aliased, phone), TextRendering { aa: false, subpixel: false });
    }
}

#[test]
fn the_default_is_the_platform_default_of_this_target() {
    assert_eq!(PHONE, cfg!(any(target_os = "android", target_os = "ios")));
    assert_eq!(TextRendering::default(), webrender_text_aa(Mode::PlatformDefault, PHONE));
}

/// The two renderers of a window show one kind of text when nobody chose: the GPU's default is the CPU renderer's.
#[test]
fn the_two_renderers_default_to_the_same_kind_of_text() {
    assert_eq!(TextRendering::default().subpixel, TEXT_AA_DEFAULT == TextAa::Lcd);
}
