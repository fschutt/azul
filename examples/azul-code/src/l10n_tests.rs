//! AzCode's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label. (The Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azcodes_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azcode"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azcodes_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azcode-menu-file"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azcode() {
    crate::l10n::in_english();
    assert_eq!(t("azcode-menu-file"), "File");
    set_locale("de-DE");
    assert_eq!(t("azcode-menu-file"), "Datei");
    set_locale("en-US");
}

/// AzCode's notices are said in the window's language, the drive's own words inside them.
#[test]
fn the_search_and_sample_notices_are_said_in_the_windows_language() {
    crate::l10n::in_english();
    assert_eq!(crate::commands::search_failed("denied"), "The folder could not be searched: denied");
    set_locale("de-DE");
    assert_eq!(
        crate::commands::search_failed("denied"),
        "Der Ordner konnte nicht durchsucht werden: denied"
    );
    assert_eq!(
        crate::commands::sample_failed("full"),
        "Das Beispiel konnte nicht geschrieben werden: full"
    );
    set_locale("en-US");
}

/// The words the plain modules name by a key of their own (`name()`: the keys of the phone's key row, the kinds of
/// change, the severities, the task sources, the panel's tabs) are in both resources: a screen reader says them.
#[test]
fn the_words_the_plain_modules_name_by_key_are_in_english_and_german() {
    use crate::{
        app::PanelTab,
        diagnostics::Severity,
        git::ChangeKind,
        keyrow::KEYS,
        panel::tab_name,
        tasks::Source,
    };
    let mut names: Vec<&str> = KEYS.iter().map(|k| k.name).collect();
    names.extend([Severity::Error, Severity::Warning, Severity::Info].map(Severity::name));
    names.extend(
        [
            ChangeKind::Modified,
            ChangeKind::Added,
            ChangeKind::Deleted,
            ChangeKind::Renamed,
            ChangeKind::Copied,
            ChangeKind::Untracked,
            ChangeKind::Conflicted,
        ]
        .map(ChangeKind::name),
    );
    names.extend([Source::Cargo, Source::Npm, Source::Make].map(Source::name));
    names.extend([PanelTab::Terminal, PanelTab::Problems, PanelTab::Output].map(tab_name));
    for name in names {
        for (tag, text) in [("en", EN), ("de", DE)] {
            assert!(text.contains(&format!("\n{name} = ")), "{tag} lacks {name}");
        }
    }
}
