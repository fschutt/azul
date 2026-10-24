//! AzTasks' words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label; the language is `--language` for a run, else the settings file's (Options
//! > Appearance > Language). No window.

use std::path::Path;

use azul_appkit::{
    args::LanguagePref,
    l10n::{set_locale, t},
    l10n_check::check,
    settings::AppSettings,
};

use crate::{
    appearance,
    args::Args,
    l10n::{sources, DE, EN},
};

#[test]
fn every_key_of_aztasks_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["aztasks"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_aztasks_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "aztasks-tasks"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_aztasks() {
    crate::l10n::in_english();
    assert_eq!(t("aztasks-tasks"), "Tasks");
    set_locale("de-DE");
    assert_eq!(t("aztasks-tasks"), "Aufgaben");
    set_locale("en-US");
}

#[test]
fn the_language_is_the_switchs_for_a_run_else_the_settings_files() {
    let args = Args::parse(["--language", "de"]).unwrap();
    assert_eq!(args.language, Some(LanguagePref::German));
    assert!(Args::parse(["--language", "klingon"]).is_err());
    let mut saved = AppSettings::default();
    assert_eq!(
        appearance::language(&Args::default(), &saved),
        LanguagePref::System
    );
    saved.set_language(LanguagePref::English);
    assert_eq!(
        appearance::language(&Args::default(), &saved),
        LanguagePref::English
    );
    assert_eq!(
        appearance::language(&args, &saved),
        LanguagePref::German,
        "the switch wins"
    );
}
