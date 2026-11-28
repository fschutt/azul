//! The typed accessibility preferences: what each word means, what the categories scale to, how an
//! override layers on what the platform reported, and what the environment pins.

use alloc::string::ToString;

use super::*;
use crate::env::{MapEnv, NoEnv};

#[test]
fn an_option_nobody_answered_is_unknown_and_never_off() {
    let prefs = AccessibilityPreferences::default();
    assert!(prefs.is_empty());
    assert_eq!(prefs.bold_text, SettingState::Unknown);
    assert!(!prefs.bold_text.is_on());
    assert!(!prefs.bold_text.is_off(), "Unknown is not Off: the platform did not answer");
    assert!(!prefs.bold_text.is_known());
    assert_eq!(prefs.contrast, ContrastPreference::Unknown);
    assert_eq!(prefs.text_scale, OptionF32::None);
    assert_eq!(prefs.effective_text_scale(), 1.0, "unknown text size is the default size");
}

#[test]
fn a_setting_state_reads_the_words_every_tool_uses() {
    for on in ["1", "true", "on", "YES", "enabled", "active", "reduce", "more", "bold", "inverted"] {
        assert_eq!(SettingState::parse(on), Some(SettingState::On), "{on}");
    }
    for off in ["0", "false", "off", "No", "disabled", "none", "no-preference", "normal"] {
        assert_eq!(SettingState::parse(off), Some(SettingState::Off), "{off}");
    }
    for unknown in ["", "unknown", "auto", "default", "unset", "  "] {
        assert_eq!(SettingState::parse(unknown), Some(SettingState::Unknown), "{unknown:?}");
    }
    assert_eq!(SettingState::parse("maybe"), None);
    assert_eq!(SettingState::from_bool(true), SettingState::On);
    assert_eq!(SettingState::from_bool(false), SettingState::Off);
    assert_eq!(SettingState::On.or(SettingState::Off), SettingState::On);
    assert_eq!(SettingState::Unknown.or(SettingState::Off), SettingState::Off);
    assert_eq!(SettingState::Off.or(SettingState::On), SettingState::Off, "an answer is kept");
    assert_eq!(SettingState::On.as_condition(), BoolCondition::True);
    assert_eq!(SettingState::Off.as_condition(), BoolCondition::False);
    assert_eq!(SettingState::Unknown.as_condition(), BoolCondition::False);
}

#[test]
fn the_dynamic_type_categories_scale_by_their_body_size() {
    // Apple's table: Body text at each category, against the default (Large, 17 pt).
    let expected: [(TextSizeCategory, u8); 12] = [
        (TextSizeCategory::ExtraSmall, 14),
        (TextSizeCategory::Small, 15),
        (TextSizeCategory::Medium, 16),
        (TextSizeCategory::Large, 17),
        (TextSizeCategory::ExtraLarge, 19),
        (TextSizeCategory::ExtraExtraLarge, 21),
        (TextSizeCategory::ExtraExtraExtraLarge, 23),
        (TextSizeCategory::AccessibilityMedium, 28),
        (TextSizeCategory::AccessibilityLarge, 33),
        (TextSizeCategory::AccessibilityExtraLarge, 40),
        (TextSizeCategory::AccessibilityExtraExtraLarge, 47),
        (TextSizeCategory::AccessibilityExtraExtraExtraLarge, 53),
    ];
    for (category, points) in expected {
        assert_eq!(category.body_points(), Some(points), "{category:?}");
        let factor = category.scale_factor().expect("a known category has a factor");
        assert!((factor - f32::from(points) / 17.0).abs() < 1e-6, "{category:?} {factor}");
    }
    assert_eq!(TextSizeCategory::Large.scale_factor(), Some(1.0), "the default size is 1.0");
    let ax5 = TextSizeCategory::AccessibilityExtraExtraExtraLarge.scale_factor().unwrap();
    assert!((ax5 - 3.1176).abs() < 1e-3, "AX5 is about 3.12: {ax5}");
    let xs = TextSizeCategory::ExtraSmall.scale_factor().unwrap();
    assert!((xs - 0.8235).abs() < 1e-3, "the smallest is about 0.82: {xs}");
    assert_eq!(TextSizeCategory::Unknown.scale_factor(), None);
    assert!(TextSizeCategory::AccessibilityMedium.is_accessibility());
    assert!(!TextSizeCategory::ExtraExtraExtraLarge.is_accessibility());
    // The table grows with every step.
    let mut previous = 0.0_f32;
    for category in TextSizeCategory::ALL {
        let factor = category.scale_factor().unwrap();
        assert!(factor > previous, "{category:?}");
        previous = factor;
    }
}

#[test]
fn a_category_is_read_from_every_spelling_it_has() {
    use TextSizeCategory as C;
    // xcrun simctl ui <device> content_size
    for category in C::ALL {
        let name = category.simctl_name().unwrap();
        assert_eq!(C::parse(name), Some(category), "simctl name {name}");
    }
    // UIKit constants and the raw strings preferredContentSizeCategory answers.
    assert_eq!(C::parse("UIContentSizeCategoryExtraSmall"), Some(C::ExtraSmall));
    assert_eq!(C::parse("UICTContentSizeCategoryXS"), Some(C::ExtraSmall));
    assert_eq!(C::parse("UICTContentSizeCategoryS"), Some(C::Small));
    assert_eq!(C::parse("UICTContentSizeCategoryM"), Some(C::Medium));
    assert_eq!(C::parse("UICTContentSizeCategoryL"), Some(C::Large));
    assert_eq!(C::parse("UICTContentSizeCategoryXL"), Some(C::ExtraLarge));
    assert_eq!(C::parse("UICTContentSizeCategoryXXL"), Some(C::ExtraExtraLarge));
    assert_eq!(C::parse("UICTContentSizeCategoryXXXL"), Some(C::ExtraExtraExtraLarge));
    assert_eq!(C::parse("UICTContentSizeCategoryAccessibilityM"), Some(C::AccessibilityMedium));
    assert_eq!(C::parse("UICTContentSizeCategoryAccessibilityL"), Some(C::AccessibilityLarge));
    assert_eq!(C::parse("UICTContentSizeCategoryAccessibilityXL"), Some(C::AccessibilityExtraLarge));
    assert_eq!(
        C::parse("UICTContentSizeCategoryAccessibilityXXL"),
        Some(C::AccessibilityExtraExtraLarge)
    );
    assert_eq!(
        C::parse("UICTContentSizeCategoryAccessibilityXXXL"),
        Some(C::AccessibilityExtraExtraExtraLarge)
    );
    assert_eq!(
        C::parse("UIContentSizeCategoryAccessibilityExtraExtraExtraLarge"),
        Some(C::AccessibilityExtraExtraExtraLarge)
    );
    // short forms of the e2e ops
    assert_eq!(C::parse("ax1"), Some(C::AccessibilityMedium));
    assert_eq!(C::parse("AX5"), Some(C::AccessibilityExtraExtraExtraLarge));
    assert_eq!(C::parse("xxxl"), Some(C::ExtraExtraExtraLarge));
    assert_eq!(C::parse("extra_extra_large"), Some(C::ExtraExtraLarge));
    assert_eq!(
        C::parse("UICTContentSizeCategoryUnspecified"),
        Some(C::Unknown),
        "the system's own word for no preference is no size"
    );
    assert_eq!(C::parse("unspecified"), Some(C::Unknown));
    assert_eq!(C::parse("huge"), None);
    assert_eq!(C::parse("1.5"), None, "a factor is not a category");
}

#[test]
fn the_nearest_category_of_a_factor() {
    use TextSizeCategory as C;
    assert_eq!(C::nearest(1.0), C::Large);
    assert_eq!(C::nearest(0.85), C::ExtraSmall, "0.82 is nearer to 0.85 than 0.88");
    assert_eq!(C::nearest(0.88), C::Small);
    assert_eq!(C::nearest(0.1), C::ExtraSmall);
    assert_eq!(C::nearest(1.3), C::ExtraExtraExtraLarge, "Android's largest step, 1.3");
    assert_eq!(C::nearest(2.0), C::AccessibilityLarge);
    assert_eq!(C::nearest(3.0), C::AccessibilityExtraExtraExtraLarge);
    assert_eq!(C::nearest(40.0), C::AccessibilityExtraExtraExtraLarge, "a larger factor is the largest");
    assert_eq!(C::nearest(f32::NAN), C::Unknown);
    assert_eq!(C::nearest(f32::INFINITY), C::Unknown);
}

#[test]
fn a_text_scale_is_capped_with_a_diagnostic_and_never_crashes() {
    assert_eq!(clamp_text_scale(1.0), Some((1.0, false)));
    assert_eq!(clamp_text_scale(1.3), Some((1.3, false)));
    assert_eq!(clamp_text_scale(0.1), Some((MIN_TEXT_SCALE, true)));
    assert_eq!(clamp_text_scale(100.0), Some((MAX_TEXT_SCALE, true)));
    assert_eq!(clamp_text_scale(f32::INFINITY), Some((MAX_TEXT_SCALE, true)));
    assert_eq!(clamp_text_scale(f32::NAN), None, "not a number: no scale at all");

    let mut prefs = AccessibilityPreferences::default();
    assert!(prefs.set_text_scale(1.5).is_none(), "in range: no diagnostic");
    assert_eq!(prefs.effective_text_scale(), 1.5);
    assert!(
        matches!(
            prefs.text_size_category,
            TextSizeCategory::ExtraExtraExtraLarge | TextSizeCategory::AccessibilityMedium
        ),
        "1.5 sits between XXXL and AX1: {:?}",
        prefs.text_size_category
    );
    let note = prefs.set_text_scale(99.0).expect("capped");
    assert!(note.contains("99") && note.contains("4"), "{note}");
    assert_eq!(prefs.effective_text_scale(), MAX_TEXT_SCALE);
    let note = prefs.set_text_scale(f32::NAN).expect("not a number");
    assert!(note.contains("not a number"), "{note}");
    assert_eq!(prefs.text_scale, OptionF32::None);
    assert_eq!(prefs.effective_text_scale(), 1.0);

    // Whatever a platform reports, the engine reads a scale in range.
    for reported in [f32::NEG_INFINITY, -3.0, 0.0, 1e-9, 1e9, f32::MAX] {
        let prefs = AccessibilityPreferences {
            text_scale: OptionF32::Some(reported),
            ..Default::default()
        };
        let scale = prefs.effective_text_scale();
        assert!((MIN_TEXT_SCALE..=MAX_TEXT_SCALE).contains(&scale), "{reported} -> {scale}");
    }
}

#[test]
fn the_text_scale_is_active_only_off_the_default() {
    let mut prefs = AccessibilityPreferences::default();
    assert!(!prefs.text_scale_is_active());
    prefs.set_text_scale(1.0);
    assert!(!prefs.text_scale_is_active());
    prefs.set_text_scale(1.3);
    assert!(prefs.text_scale_is_active());
    prefs.set_text_scale(0.85);
    assert!(prefs.text_scale_is_active(), "smaller text is a scale too");
}

#[test]
fn an_override_replaces_what_it_knows_and_leaves_the_rest() {
    let mut detected = AccessibilityPreferences::default();
    detected.set_text_scale(1.15);
    detected.bold_text = SettingState::Off;
    detected.contrast = ContrastPreference::NoPreference;
    detected.reduced_motion = SettingState::On;
    detected.long_press_ms = OptionU32::Some(400);

    let mut pin = AccessibilityPreferences::default();
    pin.set_text_scale(2.0);
    pin.bold_text = SettingState::On;
    pin.contrast = ContrastPreference::More;

    let effective = detected.overlaid(&pin);
    assert_eq!(effective.effective_text_scale(), 2.0, "the pin's scale");
    assert_eq!(effective.bold_text, SettingState::On, "the pin's bold text");
    assert_eq!(effective.contrast, ContrastPreference::More);
    assert_eq!(effective.reduced_motion, SettingState::On, "unpinned: what the system said");
    assert_eq!(effective.long_press_ms, OptionU32::Some(400), "unpinned numbers stay too");
    // an empty override is the identity, and the override of an empty set is the override
    assert_eq!(detected.overlaid(&AccessibilityPreferences::default()), detected);
    assert_eq!(AccessibilityPreferences::default().overlaid(&pin), pin);
    // an explicit Off in the pin is an answer: it replaces an On
    let mut off = AccessibilityPreferences::default();
    off.reduced_motion = SettingState::Off;
    assert_eq!(detected.overlaid(&off).reduced_motion, SettingState::Off);
}

#[test]
fn the_derived_questions_follow_the_options() {
    let mut prefs = AccessibilityPreferences::default();
    assert!(!prefs.reduces_motion());
    assert!(!prefs.high_contrast());
    assert_eq!(prefs.bold_weight_step(), 0);

    prefs.animation_scale = OptionF32::Some(0.0);
    assert!(prefs.reduces_motion(), "the system's animation speed of zero is reduced motion");
    prefs.animation_scale = OptionF32::Some(1.0);
    assert!(!prefs.reduces_motion());
    prefs.reduced_motion = SettingState::On;
    assert!(prefs.reduces_motion());

    prefs.contrast = ContrastPreference::More;
    assert!(prefs.high_contrast());
    prefs.contrast = ContrastPreference::NoPreference;
    assert!(!prefs.high_contrast());
    prefs.forced_colors = SettingState::On;
    assert!(prefs.high_contrast(), "a contrast theme is high contrast");

    prefs.bold_text = SettingState::On;
    assert_eq!(prefs.bold_weight_step(), BOLD_TEXT_WEIGHT_STEP, "only 'on': the engine's step");
    prefs.bold_weight_adjustment = OptionI16::Some(300);
    assert_eq!(prefs.bold_weight_step(), 300, "the platform's own adjustment");
    prefs.bold_weight_adjustment = OptionI16::Some(5000);
    assert_eq!(prefs.bold_weight_step(), 1000, "capped");
    prefs.bold_weight_adjustment = OptionI16::Some(0);
    assert_eq!(prefs.bold_weight_step(), BOLD_TEXT_WEIGHT_STEP, "0 with bold on: the platform did not say");
    prefs.bold_text = SettingState::Off;
    prefs.bold_weight_adjustment = OptionI16::None;
    assert_eq!(prefs.bold_weight_step(), 0);
}

#[test]
fn every_field_is_set_from_text_by_its_own_name() {
    for field in PREFERENCE_FIELDS {
        let mut prefs = AccessibilityPreferences::default();
        let value = match field {
            "text_scale" => "1.5",
            "text_size_category" => "accessibility-large",
            "contrast" => "more",
            "bold_weight_adjustment" => "300",
            "animation_scale" | "pointer_scale" => "2",
            "long_press_ms" | "action_timeout_ms" => "1500",
            _ => "on",
        };
        let result = prefs.set_from_text(field, value);
        assert!(result.is_ok(), "{field}: {result:?}");
        assert!(!prefs.is_empty(), "{field} set nothing");
        // and "unknown" puts it back
        let back = prefs.set_from_text(field, "unknown");
        assert!(back.is_ok(), "{field}: {back:?}");
        assert!(prefs.is_empty(), "{field} did not reset: {prefs:?}");
        // the e2e key is the same name in any case
        let mut upper = AccessibilityPreferences::default();
        assert!(upper.set_from_text(&field.to_ascii_uppercase(), value).is_ok(), "{field}");
        assert_eq!(upper, {
            let mut again = AccessibilityPreferences::default();
            again.set_from_text(field, value).unwrap();
            again
        });
    }
    let mut prefs = AccessibilityPreferences::default();
    let err = prefs.set_from_text("sparkles", "on").expect_err("no such option");
    assert!(err.contains("sparkles") && err.contains("text_scale"), "{err}");
    assert!(prefs.is_empty());
}

#[test]
fn a_bad_value_leaves_the_field_as_it_was() {
    let mut prefs = AccessibilityPreferences::default();
    prefs.set_from_text("bold_text", "on").unwrap();
    assert!(prefs.set_from_text("bold_text", "perhaps").is_err());
    assert_eq!(prefs.bold_text, SettingState::On);
    assert!(prefs.set_from_text("text_scale", "huge").is_err());
    assert!(prefs.set_from_text("text_scale", "nan").is_err());
    assert_eq!(prefs.text_scale, OptionF32::None);
    assert!(prefs.set_from_text("long_press_ms", "-4").is_err());
    assert!(prefs.set_from_text("animation_scale", "-1").is_err());
    assert!(prefs.set_from_text("contrast", "loud").is_err());
    // numbers are capped, not refused
    prefs.set_from_text("long_press_ms", "99999999").unwrap();
    assert_eq!(prefs.long_press_ms, OptionU32::Some(MAX_TIMEOUT_MS));
    prefs.set_from_text("animation_scale", "500").unwrap();
    assert_eq!(prefs.animation_scale, OptionF32::Some(MAX_ANIMATION_SCALE));
    let note = prefs.set_from_text("text_scale", "7").unwrap();
    assert!(note.is_some(), "a scale past the range is capped with a diagnostic");
    assert_eq!(prefs.effective_text_scale(), MAX_TEXT_SCALE);
}

#[test]
fn the_environment_pins_what_it_names() {
    let (empty, notes) = AccessibilityPreferences::from_lookup(&NoEnv);
    assert!(empty.is_empty());
    assert!(notes.is_empty());

    let env = MapEnv::new()
        .with("AZ_A11Y_TEXT_SCALE", "AX5")
        .with("AZ_A11Y_BOLD_TEXT", "on")
        .with("AZ_A11Y_CONTRAST", "more")
        .with("AZ_A11Y_REDUCED_MOTION", "1")
        .with("AZ_A11Y_REDUCED_TRANSPARENCY", "off")
        .with("AZ_A11Y_LONG_PRESS_MS", "1500")
        .with("AZ_MODE", "dark");
    let (pin, notes) = AccessibilityPreferences::from_lookup(&env);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(pin.text_size_category, TextSizeCategory::AccessibilityExtraExtraExtraLarge);
    assert!((pin.effective_text_scale() - 3.1176).abs() < 1e-3);
    assert_eq!(pin.bold_text, SettingState::On);
    assert_eq!(pin.contrast, ContrastPreference::More);
    assert_eq!(pin.reduced_motion, SettingState::On);
    assert_eq!(pin.reduced_transparency, SettingState::Off, "an explicit off is a pin");
    assert_eq!(pin.long_press_ms, OptionU32::Some(1500));
    assert_eq!(pin.screen_reader, SettingState::Unknown, "not named: not pinned");

    let env = MapEnv::new()
        .with("AZ_A11Y_BOLD_TEXT", "maybe")
        .with("AZ_A11Y_TEXT_SCALE", "50");
    let (pin, notes) = AccessibilityPreferences::from_lookup(&env);
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("AZ_A11Y_BOLD_TEXT") && n.contains("maybe")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("AZ_A11Y_TEXT_SCALE")), "{notes:?}");
    assert_eq!(pin.bold_text, SettingState::Unknown, "a bad value pins nothing");
    assert_eq!(pin.effective_text_scale(), MAX_TEXT_SCALE, "a scale out of range is capped");
}

#[test]
fn the_json_names_every_option_once() {
    let mut prefs = AccessibilityPreferences::default();
    prefs.set_text_scale(1.3);
    prefs.bold_text = SettingState::On;
    prefs.contrast = ContrastPreference::More;
    prefs.long_press_ms = OptionU32::Some(1000);
    let json = prefs.to_json();
    assert!(json.starts_with('{') && json.ends_with('}'), "{json}");
    for field in PREFERENCE_FIELDS {
        let key = alloc::format!("\"{field}\":");
        assert_eq!(json.matches(key.as_str()).count(), 1, "{field} in {json}");
    }
    assert!(json.contains("\"text_scale\": 1.300"), "{json}");
    assert!(json.contains("\"bold_text\": \"on\""), "{json}");
    assert!(json.contains("\"contrast\": \"more\""), "{json}");
    assert!(json.contains("\"long_press_ms\": 1000"), "{json}");
    assert!(json.contains("\"screen_reader\": \"unknown\""), "{json}");
    assert!(json.contains("\"pointer_scale\": null"), "{json}");
    assert_eq!(json.to_string(), prefs.to_json(), "deterministic");
}

// ---- the stylesheet's conditions ------------------------------------------------------------------------------------

use crate::dynamic_selector::{DynamicSelector, DynamicSelectorContext};

fn condition(key: &str, value: &str) -> Option<AccessibilityCondition> {
    AccessibilityCondition::from_media_feature(key, value)
}

fn context_with(preferences: AccessibilityPreferences) -> DynamicSelectorContext {
    DynamicSelectorContext {
        preferences,
        ..DynamicSelectorContext::default()
    }
}

#[test]
fn the_media_features_of_the_preferences_are_read() {
    use AccessibilityCondition as C;
    assert_eq!(condition("min-text-scale", "1.3"), Some(C::TextScaleAtLeast(1300)));
    assert_eq!(condition("max-text-scale", "1"), Some(C::TextScaleAtMost(1000)));
    assert_eq!(condition("min-text-scale", "x"), None);
    assert_eq!(condition("min-text-scale", "-1"), None);
    assert_eq!(condition("min-text-scale", "NaN"), None);
    assert_eq!(condition("forced-colors", "active"), Some(C::ForcedColors(BoolCondition::True)));
    assert_eq!(condition("forced-colors", "none"), Some(C::ForcedColors(BoolCondition::False)));
    assert_eq!(condition("forced-colors", "sometimes"), None);
    assert_eq!(
        condition("prefers-reduced-transparency", "reduce"),
        Some(C::ReducedTransparency(BoolCondition::True))
    );
    assert_eq!(
        condition("prefers-reduced-transparency", "no-preference"),
        Some(C::ReducedTransparency(BoolCondition::False))
    );
    assert_eq!(condition("inverted-colors", "inverted"), Some(C::InvertedColors(BoolCondition::True)));
    assert_eq!(condition("inverted-colors", "none"), Some(C::InvertedColors(BoolCondition::False)));
    assert_eq!(condition("prefers-bold-text", "bold"), Some(C::BoldText(BoolCondition::True)));
    assert_eq!(condition("PREFERS-BOLD-TEXT", "no-preference"), Some(C::BoldText(BoolCondition::False)));
    assert_eq!(
        condition("differentiate-without-color", "active"),
        Some(C::DifferentiateWithoutColor(BoolCondition::True))
    );
    assert_eq!(condition("button-shapes", "active"), Some(C::ButtonShapes(BoolCondition::True)));
    assert_eq!(condition("on-off-labels", "active"), Some(C::OnOffLabels(BoolCondition::True)));
    assert_eq!(condition("screen-reader", "active"), Some(C::ScreenReader(BoolCondition::True)));
    assert_eq!(condition("prefers-cross-fade", "reduce"), Some(C::CrossFade(BoolCondition::True)));
    assert_eq!(condition("keyboard-navigation", "active"), Some(C::KeyboardNavigation(BoolCondition::True)));
    // the contrast feature: `less` and `custom` are here, `more` has its own selector
    assert_eq!(condition("prefers-contrast", "less"), Some(C::Contrast(ContrastPreference::Less)));
    assert_eq!(
        condition("prefers-contrast", "no-preference"),
        Some(C::Contrast(ContrastPreference::NoPreference))
    );
    assert_eq!(condition("prefers-contrast", "custom"), Some(C::ForcedColors(BoolCondition::True)));
    assert_eq!(condition("prefers-contrast", "more"), None);
    assert_eq!(condition("prefers-reduced-motion", "reduce"), None, "its own selector");
    assert_eq!(condition("orientation", "portrait"), None);
}

#[test]
fn a_condition_holds_for_the_preferences_it_names() {
    use AccessibilityCondition as C;
    let mut preferences = AccessibilityPreferences::default();
    preferences.set_text_scale(1.3);
    preferences.bold_text = SettingState::On;
    preferences.contrast = ContrastPreference::Less;
    preferences.reduced_transparency = SettingState::Off;
    let ctx = context_with(preferences);

    assert!(DynamicSelector::Accessibility(C::TextScaleAtLeast(1300)).matches(&ctx), "1.3 is at least 1.3");
    assert!(DynamicSelector::Accessibility(C::TextScaleAtLeast(1000)).matches(&ctx));
    assert!(!DynamicSelector::Accessibility(C::TextScaleAtLeast(1500)).matches(&ctx));
    assert!(DynamicSelector::Accessibility(C::TextScaleAtMost(1300)).matches(&ctx));
    assert!(!DynamicSelector::Accessibility(C::TextScaleAtMost(1000)).matches(&ctx));
    assert!(DynamicSelector::Accessibility(C::BoldText(BoolCondition::True)).matches(&ctx));
    assert!(!DynamicSelector::Accessibility(C::BoldText(BoolCondition::False)).matches(&ctx));
    assert!(DynamicSelector::Accessibility(C::Contrast(ContrastPreference::Less)).matches(&ctx));
    assert!(!DynamicSelector::Accessibility(C::Contrast(ContrastPreference::More)).matches(&ctx));
    assert!(!DynamicSelector::Accessibility(C::Contrast(ContrastPreference::NoPreference)).matches(&ctx));
    assert!(DynamicSelector::Accessibility(C::ReducedTransparency(BoolCondition::False)).matches(&ctx));
    assert!(!DynamicSelector::Accessibility(C::ReducedTransparency(BoolCondition::True)).matches(&ctx));

    // an unknown option is the default look: not "on", and "no preference"
    let unknown = context_with(AccessibilityPreferences::default());
    assert!(!DynamicSelector::Accessibility(C::BoldText(BoolCondition::True)).matches(&unknown));
    assert!(DynamicSelector::Accessibility(C::BoldText(BoolCondition::False)).matches(&unknown));
    assert!(DynamicSelector::Accessibility(C::Contrast(ContrastPreference::NoPreference)).matches(&unknown));
    assert!(DynamicSelector::Accessibility(C::TextScaleAtMost(1000)).matches(&unknown));
    assert!(DynamicSelector::Accessibility(C::TextScaleAtLeast(1000)).matches(&unknown));

    // a contrast theme is more contrast for `prefers-contrast: more`
    let mut forced = AccessibilityPreferences::default();
    forced.forced_colors = SettingState::On;
    assert!(DynamicSelector::Accessibility(C::Contrast(ContrastPreference::More)).matches(&context_with(forced)));
    assert!(DynamicSelector::Accessibility(C::ForcedColors(BoolCondition::True)).matches(&context_with(forced)));
}

#[test]
fn a_changed_preference_is_a_new_context() {
    let a = context_with(AccessibilityPreferences::default());
    let mut preferences = AccessibilityPreferences::default();
    preferences.bold_text = SettingState::On;
    let b = context_with(preferences);
    assert_ne!(a, b, "the cascade epoch must move when a preference does");
    assert_eq!(a, context_with(AccessibilityPreferences::default()));
}

#[test]
fn motion_is_reduced_by_any_of_the_three_answers() {
    let mut ctx = DynamicSelectorContext::default();
    assert!(!ctx.motion_is_reduced());
    ctx.preferences.reduced_motion = SettingState::On;
    assert!(ctx.motion_is_reduced());
    let mut ctx = DynamicSelectorContext::default();
    ctx.preferences.animation_scale = OptionF32::Some(0.0);
    assert!(ctx.motion_is_reduced(), "the system's animation speed of zero");
    let mut ctx = DynamicSelectorContext::default();
    ctx.prefers_reduced_motion = BoolCondition::True;
    assert!(ctx.motion_is_reduced(), "the older condition (a test mock sets it)");
}

#[cfg(feature = "parser")]
#[test]
fn a_stylesheet_names_the_preferences_in_its_media_blocks() {
    let css = crate::css::Css::parse_inline(
        "@media (min-text-scale: 1.3) { color: blue; } @media (forced-colors: active) { color: red; } \
         @media (prefers-contrast: less) { color: green; }",
    );
    let conditions: alloc::vec::Vec<_> = css
        .rules
        .as_slice()
        .iter()
        .flat_map(|rule| rule.conditions.as_slice().to_vec())
        .collect();
    use AccessibilityCondition as C;
    for expected in [
        DynamicSelector::Accessibility(C::TextScaleAtLeast(1300)),
        DynamicSelector::Accessibility(C::ForcedColors(BoolCondition::True)),
        DynamicSelector::Accessibility(C::Contrast(ContrastPreference::Less)),
    ] {
        assert!(conditions.contains(&expected), "{expected:?} in {conditions:?}");
    }
}

// ---- the style ---------------------------------------------------------------------------------------------------

use crate::system::{AccessibilitySettings, SystemStyle};

#[test]
fn the_older_fields_mirror_the_typed_preferences() {
    let mut style = SystemStyle::default();
    assert_eq!(style.accessibility.text_scale_factor, 1.0, "the default is no scaling, not 0.0");
    let mut preferences = AccessibilityPreferences::default();
    preferences.set_text_scale(1.5);
    preferences.bold_text = SettingState::On;
    preferences.contrast = ContrastPreference::More;
    preferences.reduced_motion = SettingState::On;
    preferences.reduced_transparency = SettingState::On;
    preferences.screen_reader = SettingState::On;
    preferences.differentiate_without_color = SettingState::On;
    style.set_preferences(preferences);

    assert_eq!(style.text_scale(), 1.5);
    assert_eq!(style.accessibility.text_scale_factor, 1.5);
    assert!(style.accessibility.prefers_larger_text);
    assert!(style.accessibility.prefers_bold_text);
    assert!(style.accessibility.prefers_high_contrast);
    assert!(style.accessibility.prefers_reduced_motion);
    assert!(style.accessibility.prefers_reduced_transparency);
    assert!(style.accessibility.screen_reader_active);
    assert!(style.accessibility.differentiate_without_color);
    assert_eq!(style.prefers_reduced_motion, BoolCondition::True);
    assert_eq!(style.prefers_high_contrast, BoolCondition::True);
    assert!(!style.animation.animations_enabled);
    assert!(style.text_rendering.increased_contrast);

    // and back
    style.set_preferences(AccessibilityPreferences::default());
    assert_eq!(style.accessibility, AccessibilitySettings::default());
    assert_eq!(style.prefers_reduced_motion, BoolCondition::False);
    assert_eq!(style.prefers_high_contrast, BoolCondition::False);
}

#[test]
fn the_context_of_a_style_carries_its_preferences() {
    let mut style = SystemStyle::default();
    let mut preferences = AccessibilityPreferences::default();
    preferences.set_text_scale(2.0);
    style.set_preferences(preferences);
    let ctx = DynamicSelectorContext::from_system_style(&style);
    assert_eq!(ctx.preferences, preferences);
    assert_eq!(ctx.preferences.effective_text_scale(), 2.0);
}

#[test]
fn a_pin_is_laid_over_the_reported_preferences_and_changes_nothing_when_it_says_the_same() {
    use alloc::sync::Arc;
    let mut reported = AccessibilityPreferences::default();
    reported.set_text_scale(1.15);
    reported.bold_text = SettingState::Off;
    let mut style = SystemStyle::default();
    style.set_preferences(reported);
    let style = Arc::new(style);

    let none = SystemStyle::with_preferences_overlaid(Arc::clone(&style), &AccessibilityPreferences::default());
    assert!(Arc::ptr_eq(&none, &style), "an empty pin is the same style");
    let mut same = AccessibilityPreferences::default();
    same.bold_text = SettingState::Off;
    let same = SystemStyle::with_preferences_overlaid(Arc::clone(&style), &same);
    assert!(Arc::ptr_eq(&same, &style), "a pin that says what the system said changes nothing");

    let mut pin = AccessibilityPreferences::default();
    pin.set_text_scale(3.0);
    pin.bold_text = SettingState::On;
    let pinned = SystemStyle::with_preferences_overlaid(Arc::clone(&style), &pin);
    assert!(!Arc::ptr_eq(&pinned, &style));
    assert_eq!(pinned.text_scale(), 3.0);
    assert_eq!(pinned.preferences.bold_text, SettingState::On);
    assert!(pinned.accessibility.prefers_bold_text, "the mirrors follow the pin");
    assert_eq!(style.text_scale(), 1.15, "the reported style is untouched");
}

// ---- the poll of a platform without a notification -------------------------------------------------------------------

#[test]
fn a_poll_is_due_at_once_and_then_every_interval() {
    let mut poll = PreferencePoll::every(1000);
    assert!(poll.due(5), "the first ask is due");
    assert!(!poll.due(5));
    assert!(!poll.due(999), "not before the interval");
    assert!(poll.due(1005), "the interval after the last ask");
    assert!(!poll.due(1006));
    // asked again by a notification
    poll.ask_again_now();
    assert!(poll.due(1007));
    assert!(!poll.due(1008));
    // a clock that went back is not due until it catches up
    assert!(!poll.due(3));
    assert!(poll.due(5000));
    // an interval of zero is due at every ask
    let mut always = PreferencePoll::every(0);
    assert!(always.due(1) && always.due(1) && always.due(2));
}
