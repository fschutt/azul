//! AzVideoCut's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label; the edits AzVideoCut names in English have their words. (The
//! Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{named, named_key, set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azvideocut_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azvideocut"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azvideocut_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azvideocut-export-now"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azvideocut() {
    crate::l10n::in_english();
    assert_eq!(t("azvideocut-export-now"), "Export now");
    set_locale("de-DE");
    assert_eq!(t("azvideocut-export-now"), "Jetzt exportieren");
    set_locale("en-US");
}


/// The undo history keeps an edit by its English name (the scripts read it too); the status bar
/// and Undo / Redo say it with `azvideocut-edit-<name>`: every one is checked here.
#[test]
fn every_edit_name_of_azvideocut_has_its_words() {
    let app = crate::ABOUT.name;
    let edits = [
        "Insert",
        "Overwrite",
        "Razor",
        "Lift",
        "Ripple delete",
        "Move",
        "Trim",
        "Ripple trim",
        "Slip",
        "Effects",
        "Transition",
        "Enable",
        "Track visibility",
        "Track lock",
    ];
    for name in edits {
        let key = named_key(app, "edit", name);
        for (tag, source) in [("en", EN), ("de", DE)] {
            assert!(source.contains(&format!("\n{key} = ")), "{tag} lacks {key}");
        }
    }
    crate::l10n::in_english();
    assert_eq!(named(app, "edit", "Ripple delete"), "Ripple delete");
    set_locale("de-DE");
    assert_eq!(named(app, "edit", "Ripple delete"), "Löschen und Lücke schließen");
    set_locale("en-US");
}
