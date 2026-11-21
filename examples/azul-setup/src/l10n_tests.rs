//! AzSetup's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse, and a language switch changes what the wizard says.
//! No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{DE, EN};

#[test]
fn every_key_of_azsetup_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azsetup"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_wizard_speaks_the_windows_language() {
    crate::l10n::in_english();
    assert_eq!(t("azsetup-welcome-title"), "Welcome to the AzOffice Setup Wizard");
    assert_eq!(t("azsetup-next"), "Next >");
    set_locale("de-DE");
    assert_eq!(t("azsetup-welcome-title"), "Willkommen beim AzOffice-Setup-Assistenten");
    assert_eq!(t("azsetup-next"), "Weiter >");
    set_locale("en-US");
}
