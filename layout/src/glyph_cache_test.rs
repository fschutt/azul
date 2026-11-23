use super::*;
use crate::settings::TextSettings;

#[test]
fn a_new_cache_draws_under_the_default_text_settings() {
    let cache = GlyphCache::new();
    assert_eq!(cache.text_settings(), TextSettings::default());
    assert_eq!(cache.hinting(), TextSettings::default().hinting);
}

#[test]
fn text_settings_are_taken_once_and_the_change_is_reported() {
    let mut cache = GlyphCache::new();
    let light_off = TextSettings {
        hint_light: false,
        ..TextSettings::default()
    };
    assert!(
        !cache.set_text_settings(TextSettings::default()),
        "the settings it holds are no change"
    );
    assert!(cache.set_text_settings(light_off));
    assert_eq!(cache.text_settings(), light_off);
    assert!(!cache.set_text_settings(light_off));
}

#[test]
fn hinting_follows_the_text_settings() {
    let mut cache = GlyphCache::new();
    let _ = cache.set_text_settings(TextSettings {
        hinting: true,
        ..TextSettings::default()
    });
    assert!(cache.hinting());
    let _ = cache.set_text_settings(TextSettings {
        hinting: false,
        ..TextSettings::default()
    });
    assert!(!cache.hinting());
}
