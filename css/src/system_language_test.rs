//! The language a phone reports, as the tag the engine translates and formats for.
//!
//! Neither phone shell read the language at all (the mobile style defaults say `en-US` for every
//! phone), so a German phone showed English. The shells now hand what the platform delivers -
//! Android's `Configuration` (`en` and `US`, or `Locale.toLanguageTag()`), iOS's
//! `NSLocale.preferredLanguages` and `localeIdentifier` - to `SystemLanguage::from_platform_tag`,
//! which says the tag the translation lookup finds a bundle for.

use super::SystemLanguage;

fn tag(os: &str) -> Option<String> {
    SystemLanguage::from_platform_tag(os, &[]).map(|language| language.id.as_str().to_string())
}

#[test]
fn the_tags_android_delivers_are_bcp_47_tags() {
    // `Configuration.getLocales().get(0)` joined from its language and country, `toLanguageTag()`,
    // and `Locale.toString()`.
    assert_eq!(tag("en-US").as_deref(), Some("en-US"));
    assert_eq!(tag("en_US").as_deref(), Some("en-US"));
    assert_eq!(tag("de-DE").as_deref(), Some("de-DE"));
    assert_eq!(tag("de_DE").as_deref(), Some("de-DE"));
    assert_eq!(tag("pt-BR").as_deref(), Some("pt-BR"));
    assert_eq!(tag("pt_BR").as_deref(), Some("pt-BR"));
    // a language without a country (the NDK has none for a region-less setting)
    assert_eq!(tag("de").as_deref(), Some("de"));
    assert_eq!(tag("fil-PH").as_deref(), Some("fil-PH"), "three letters");
    assert_eq!(tag("es-419").as_deref(), Some("es-419"), "a region of digits");
}

#[test]
fn what_names_no_language_is_none_and_the_caller_keeps_its_own() {
    assert_eq!(tag(""), None);
    assert_eq!(tag("   "), None);
    assert_eq!(tag("und"), None);
    assert_eq!(tag("und-US"), None);
    assert_eq!(tag("C"), None);
    assert_eq!(tag("POSIX"), None);
    assert_eq!(tag("x"), None);
    assert_eq!(tag("-"), None);
    assert_eq!(tag("e3"), None);
    assert_eq!(tag("english"), None);
}

#[test]
fn the_case_and_the_separators_are_fixed() {
    assert_eq!(tag("EN-us").as_deref(), Some("en-US"));
    assert_eq!(tag("de-de").as_deref(), Some("de-DE"));
    assert_eq!(tag(" de_DE ").as_deref(), Some("de-DE"));
    assert_eq!(tag("zh-hans-cn").as_deref(), Some("zh-Hans-CN"));
    assert_eq!(tag("sr-LATN-rs").as_deref(), Some("sr-Latn-RS"));
}

#[test]
fn the_codes_java_still_reports_are_the_current_ones() {
    // Hebrew, Indonesian, Yiddish and Javanese: `Locale.getLanguage()` and the NDK's
    // `AConfiguration_getLanguage` say `iw`, `in`, `ji` and `jw`.
    assert_eq!(tag("iw-IL").as_deref(), Some("he-IL"));
    assert_eq!(tag("iw").as_deref(), Some("he"));
    assert_eq!(tag("in-ID").as_deref(), Some("id-ID"));
    assert_eq!(tag("ji").as_deref(), Some("yi"));
    assert_eq!(tag("jw").as_deref(), Some("jv"));
}

#[test]
fn the_identifiers_ios_and_posix_deliver_are_cleaned() {
    // `NSLocale.localeIdentifier`, `preferredLanguages`, a POSIX name.
    assert_eq!(tag("en_US@calendar=gregorian").as_deref(), Some("en-US"));
    assert_eq!(tag("en_US@rg=dezzzz").as_deref(), Some("en-US"));
    assert_eq!(tag("zh-Hans-CN").as_deref(), Some("zh-Hans-CN"));
    assert_eq!(tag("zh-Hant-TW").as_deref(), Some("zh-Hant-TW"));
    assert_eq!(tag("de_DE.UTF-8").as_deref(), Some("de-DE"));
    assert_eq!(tag("en-US-u-ca-gregory-nu-latn").as_deref(), Some("en-US"), "an extension is no language");
    assert_eq!(tag("en-GB-x-private").as_deref(), Some("en-GB"));
    assert_eq!(tag("de-DE-1996").as_deref(), Some("de-DE-1996"), "a variant stays");
}

#[test]
fn a_right_to_left_language_is_told_from_the_apps_known_ones() {
    let known = [SystemLanguage::new("en-US", false), SystemLanguage::new("he-IL", true)];
    // The old Java code of Hebrew, as Android delivers it, is right-to-left.
    let hebrew = SystemLanguage::from_platform_tag("iw-IL", &known).expect("a language");
    assert_eq!(hebrew, SystemLanguage::new("he-IL", true));
    let german = SystemLanguage::from_platform_tag("de_DE", &known).expect("a language");
    assert_eq!(german, SystemLanguage::new("de-DE", false));
}
