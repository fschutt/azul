use azul_css::env::{MapEnv, NoEnv};

use super::*;

/// The settings an environment with the one variable `name` set to `value` asks for.
fn with(name: &str, value: &str) -> TextSettings {
    TextSettings::from_lookup(&MapEnv::new().with(name, value))
}

#[test]
fn the_default_is_what_an_empty_environment_gives() {
    assert_eq!(TextSettings::default(), TextSettings::from_lookup(&NoEnv));
}

#[test]
fn the_default_is_the_shipped_behaviour() {
    let d = TextSettings::default();
    assert_eq!(
        d.hinting,
        cfg!(not(target_os = "macos")),
        "hinting is off on macOS and on elsewhere"
    );
    assert!(d.hint_light, "light hinting is on");
    assert!(d.subpixel, "glyphs are placed at sub-pixel positions");
    assert!(!d.trace_paginate);
    assert!(!d.trace_text);
    assert!(!d.trace_font_resolve);
    assert!(!d.trace_font_gc);
    assert!(!d.trace_font_fallback);
    assert!(!d.skip_restamp);
}

#[test]
fn a_trace_or_the_negative_control_is_on_when_its_variable_is_set_to_anything() {
    for value in ["1", "", "0"] {
        assert_eq!(
            with("AZ_PAGINATE_TRACE", value),
            TextSettings {
                trace_paginate: true,
                ..TextSettings::default()
            },
            "AZ_PAGINATE_TRACE={value:?}"
        );
        assert_eq!(
            with("TEXTDBG", value),
            TextSettings {
                trace_text: true,
                ..TextSettings::default()
            },
            "TEXTDBG={value:?}"
        );
        assert_eq!(
            with("AZ_FAST_RESOLVE_DEBUG", value),
            TextSettings {
                trace_font_resolve: true,
                ..TextSettings::default()
            },
            "AZ_FAST_RESOLVE_DEBUG={value:?}"
        );
        assert_eq!(
            with("AZ_FONT_GC_TRACE", value),
            TextSettings {
                trace_font_gc: true,
                ..TextSettings::default()
            },
            "AZ_FONT_GC_TRACE={value:?}"
        );
        assert_eq!(
            with("AZ_FONT_FALLBACK_DEBUG", value),
            TextSettings {
                trace_font_fallback: true,
                ..TextSettings::default()
            },
            "AZ_FONT_FALLBACK_DEBUG={value:?}"
        );
        assert_eq!(
            with("AZ_T2_SKIP_RESTAMP", value),
            TextSettings {
                skip_restamp: true,
                ..TextSettings::default()
            },
            "AZ_T2_SKIP_RESTAMP={value:?}"
        );
    }
}

#[test]
fn the_glyph_switches_are_off_for_zero_or_false_in_any_case_and_on_for_anything_else() {
    for value in ["0", "false", "FALSE", "False"] {
        assert!(!with("AZ_TEXT_HINTING", value).hinting, "AZ_TEXT_HINTING={value:?}");
        assert!(!with("AZ_HINT_LIGHT", value).hint_light, "AZ_HINT_LIGHT={value:?}");
        assert!(!with("AZ_TEXT_SUBPIXEL", value).subpixel, "AZ_TEXT_SUBPIXEL={value:?}");
    }
    for value in ["1", "true", "yes", "", "00", "off"] {
        assert!(with("AZ_TEXT_HINTING", value).hinting, "AZ_TEXT_HINTING={value:?}");
        assert!(with("AZ_HINT_LIGHT", value).hint_light, "AZ_HINT_LIGHT={value:?}");
        assert!(with("AZ_TEXT_SUBPIXEL", value).subpixel, "AZ_TEXT_SUBPIXEL={value:?}");
    }
}

#[test]
fn a_glyph_switch_touches_no_other_field() {
    assert_eq!(
        with("AZ_HINT_LIGHT", "0"),
        TextSettings {
            hint_light: false,
            ..TextSettings::default()
        }
    );
    assert_eq!(
        with("AZ_TEXT_SUBPIXEL", "0"),
        TextSettings {
            subpixel: false,
            ..TextSettings::default()
        }
    );
    assert_eq!(
        with("AZ_TEXT_HINTING", "0"),
        TextSettings {
            hinting: false,
            ..TextSettings::default()
        }
    );
}

#[test]
fn a_set_hinting_switch_beats_the_platform_default_both_ways() {
    assert!(with("AZ_TEXT_HINTING", "1").hinting, "forced on, macOS included");
    assert!(!with("AZ_TEXT_HINTING", "0").hinting, "forced off, elsewhere included");
}
