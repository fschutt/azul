//! AzShow's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label. (The Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azshow_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azshow"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azshow_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azshow-tab-home"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azshow() {
    crate::l10n::in_english();
    assert_eq!(t("azshow-tab-home"), "HOME");
    set_locale("de-DE");
    assert_eq!(t("azshow-tab-home"), "START");
    set_locale("en-US");
}

/// The things AzShow names in English (`crate::l10n::word`): no key in the source names their
/// words, so the scanner above does not see them - every one is in both resources.
#[test]
fn every_view_layout_shape_effect_transition_size_page_and_role_has_its_words() {
    use crate::{
        app::{BackstagePage, View},
        model::{AnimationEffect, LayoutKind, PlaceholderRole, ShapeKind, SlideSize, TransitionKind},
    };
    let mut named: Vec<(&str, String)> = Vec::new();
    named.extend(View::ALL.iter().map(|v| ("view", v.label().to_string())));
    named.extend(BackstagePage::NAV.iter().map(|p| ("backstage", p.label().to_string())));
    named.extend(LayoutKind::ALL.iter().map(|l| ("layout", l.label().to_string())));
    named.extend(ShapeKind::ALL.iter().map(|k| ("shape", k.label().to_string())));
    named.extend(AnimationEffect::ALL.iter().map(|e| ("effect", e.label().to_string())));
    named.extend(TransitionKind::ALL.iter().map(|k| ("transition", k.label().to_string())));
    named.extend([SlideSize::Wide, SlideSize::Standard].iter().map(|z| ("size", z.label().to_string())));
    let roles = [
        PlaceholderRole::Title,
        PlaceholderRole::Subtitle,
        PlaceholderRole::Body,
        PlaceholderRole::Body2,
        PlaceholderRole::Heading,
        PlaceholderRole::Heading2,
    ];
    named.extend(roles.iter().map(|r| ("role", format!("{r:?}"))));
    for (what, name) in &named {
        let key = azul_appkit::l10n::named_key(crate::ABOUT.name, what, name);
        for (tag, source) in [("en", EN), ("de", DE)] {
            assert!(source.contains(&format!("\n{key} = ")), "{tag} lacks {key} ({name})");
        }
    }
    crate::l10n::in_english();
    assert_eq!(crate::l10n::word("view", "Slide Sorter"), "Slide Sorter");
    set_locale("de-DE");
    assert_eq!(crate::l10n::word("view", "Slide Sorter"), "Foliensortierung");
    set_locale("en-US");
}
