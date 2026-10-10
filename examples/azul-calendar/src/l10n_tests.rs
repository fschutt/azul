//! AzCalendar's words in azul's localization: every key its source (and azul-calendar-core's)
//! names is in resources/en.ftl and resources/de.ftl and both parse; the engine gets them with
//! appkit's; a language switch changes a label; the Language setting is a switch and a line of
//! the settings file. No window.

use std::path::Path;

use azul_appkit::{
    args::LanguagePref,
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::{
    args::Args,
    l10n::{sources, DE, EN},
    settings,
};

#[test]
fn every_key_of_azcalendars_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems: Vec<String> = [manifest.join("src"), manifest.join("../azul-calendar-core/src")]
        .iter()
        .flat_map(|dir| check(dir, &["azcalendar"], EN, DE))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azcalendars_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azcalendar-tab-home"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azcalendar() {
    crate::l10n::in_english();
    assert_eq!(t("azcalendar-tab-home"), "Home");
    set_locale("de-DE");
    assert_eq!(t("azcalendar-tab-home"), "Start");
    set_locale("en-US");
}

/// `--language` picks the run's language; Options > Language keeps it in the settings file.
#[test]
fn the_language_is_a_switch_and_a_line_of_the_settings_file() {
    let args = Args::parse(["--language", "de"]).unwrap();
    assert_eq!(args.language, Some(LanguagePref::German));
    assert!(Args::parse(["--language", "klingon"]).is_err());
    let text = settings::with_line("", &settings::language_line(LanguagePref::English));
    assert_eq!(settings::language(&text), LanguagePref::English);
    assert_eq!(settings::language(""), LanguagePref::System, "the system's by default");
}
