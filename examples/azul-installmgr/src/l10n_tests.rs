//! AzInstallManager's words in azul's localization: every key its source names is in
//! resources/en.ftl and resources/de.ftl and both parse; the engine gets them with appkit's; a
//! language switch changes a label. No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t, t_args, Arg},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azinstallmgr_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azinstallmgr"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azinstallmgr_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-update-banner", "azinstallmgr-check"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azinstallmgr() {
    crate::l10n::in_english();
    assert_eq!(t("azinstallmgr-check"), "Check for updates");
    assert_eq!(
        t_args("azinstallmgr-update-all", &[("count", Arg::from(3_usize))]),
        "Update all (3)"
    );
    set_locale("de-DE");
    assert_eq!(t("azinstallmgr-check"), "Nach Updates suchen");
    assert_eq!(
        t_args("azinstallmgr-update-all", &[("count", Arg::from(0_usize))]),
        "Alle aktualisieren"
    );
    set_locale("en-US");
}
