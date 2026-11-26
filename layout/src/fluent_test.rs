//! The tags a phone delivers, against the bundles an app registers.
//!
//! An app registers its words as `en` and `de` (`AppConfig::fluent_locales`); Android reports
//! `en-US`, `en_US` or `de-DE`, a phone in Brazil `pt-BR`, a device that has not been set up
//! nothing at all (an empty tag, `und`). Whatever arrives, the lookup has to end in a bundle: the
//! language's, and English for a language the app has no words for.

use super::{canonical_tag_case, FluentLocalizerHandle, FmtArgVec};

/// The localizer `FluentLocalizerHandle::from_locale_sources` makes of an app's `en` and `de`.
fn the_apps_localizer() -> FluentLocalizerHandle {
    let localizer = FluentLocalizerHandle::create("en-US", &[]);
    assert!(localizer.add_resource("en", "from = From\n"));
    assert!(localizer.add_resource("de", "from = Von\n"));
    localizer
}

fn say(localizer: &FluentLocalizerHandle, tag: &str) -> String {
    localizer
        .translate(tag.into(), "from".into(), FmtArgVec::new())
        .as_str()
        .to_string()
}

#[test]
fn the_tags_android_delivers_find_the_apps_bundles_and_the_rest_end_in_english() {
    let app = the_apps_localizer();
    for tag in ["en-US", "en_US", "en", "en-GB", "EN-us", "en_us"] {
        assert_eq!(say(&app, tag), "From", "{tag:?}");
    }
    for tag in ["de-DE", "de_DE", "de", "de-AT", "de_CH", "DE-de", "de_de", "de-DE-1996"] {
        assert_eq!(say(&app, tag), "Von", "{tag:?}");
    }
    // a language the app has no words for, and what names none: English, never the key
    for tag in ["pt-BR", "pt_BR", "zh-Hans-CN", "xx", "zz-ZZ", "", "und", "C", "POSIX"] {
        assert_eq!(say(&app, tag), "From", "{tag:?}");
    }
}

#[test]
fn a_regional_bundle_is_found_whatever_case_and_separator_the_request_spells() {
    let localizer = FluentLocalizerHandle::create("en-US", &[]);
    assert!(localizer.add_resource("en-US", "from = From\n"));
    assert!(localizer.add_resource("de-DE", "from = Von\n"));
    assert!(localizer.add_resource("zh-Hans-CN", "from = 从\n"));
    for tag in ["de-DE", "de_DE", "de-de", "DE-DE", "de_de"] {
        assert_eq!(say(&localizer, tag), "Von", "{tag:?}");
    }
    for tag in ["zh-Hans-CN", "zh_Hans_CN", "zh-hans-cn", "ZH-HANS-CN"] {
        assert_eq!(say(&localizer, tag), "从", "{tag:?}");
    }
    // no bundle of the language at all: the default locale's
    assert_eq!(say(&localizer, "fr_FR"), "From");
}

#[test]
fn a_key_without_a_message_is_told_as_the_key_in_every_tag() {
    let app = the_apps_localizer();
    for tag in ["en-US", "de_DE", "", "und"] {
        let said = app
            .translate(tag.into(), "no-such-key".into(), FmtArgVec::new())
            .as_str()
            .to_string();
        assert_eq!(said, "no-such-key", "{tag:?}");
    }
}

#[test]
fn a_tags_case_is_the_canonical_one() {
    assert_eq!(canonical_tag_case("DE-de"), "de-DE");
    assert_eq!(canonical_tag_case("zh-HANS-cn"), "zh-Hans-CN");
    assert_eq!(canonical_tag_case("en"), "en");
    assert_eq!(canonical_tag_case("es-419"), "es-419");
    assert_eq!(canonical_tag_case("de-DE-1996"), "de-DE-1996");
    assert_eq!(canonical_tag_case(""), "");
    assert_eq!(canonical_tag_case("EN-Üs"), "en-Üs", "no panic on what is no letter");
}
