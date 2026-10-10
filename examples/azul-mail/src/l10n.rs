//! AzMail's words through azul's localization (doc/guide/en/architecture/localization.md;
//! azul-appkit's `l10n` has the calls): `resources/en.ftl` and `resources/de.ftl` - every key
//! the source names is in both (`l10n_tests.rs`) - and azcloud-kit's error table with them,
//! given to the engine at the start ([`register`]). The engine's locale picks the language
//! (the system's, or Options > General > Language).

use azcloud_kit::user_errors::{fluent_source, Lang};
use azul::prelude::AppConfig;
use azul_appkit::l10n;

/// AzMail's words in English.
pub(crate) const EN: &str = include_str!("../resources/en.ftl");
/// AzMail's words in German.
pub(crate) const DE: &str = include_str!("../resources/de.ftl");

/// AzMail's resources per language: its own words, then azcloud-kit's error table, the Azlin
/// Bridge's settings and a ban's words.
fn resources() -> Vec<(&'static str, String)> {
    Lang::ALL
        .iter()
        .map(|lang| {
            let own = match lang {
                Lang::En => EN,
                Lang::De => DE,
            };
            let kit = format!(
                "{}\n{}\n{}",
                fluent_source(*lang),
                azcloud_kit::bridge::fluent_source(*lang),
                azcloud_kit::token::ban_fluent_source(*lang)
            );
            (lang.tag(), format!("{own}\n{kit}"))
        })
        .collect()
}

/// Every resource the engine gets, per language: appkit's, AzMail's, the error table.
#[must_use]
pub(crate) fn sources() -> Vec<(String, String)> {
    let own = resources();
    let pairs: Vec<(&str, &str)> = own.iter().map(|(tag, s)| (*tag, s.as_str())).collect();
    l10n::sources(&pairs)
}

/// Gives the engine AzMail's resources (where the app config is built).
pub(crate) fn register(config: &mut AppConfig) {
    let own = resources();
    let pairs: Vec<(&str, &str)> = own.iter().map(|(tag, s)| (*tag, s.as_str())).collect();
    l10n::register(config, &pairs);
}

/// The words of AzMail in English on this thread (a test of words the window says).
#[cfg(test)]
pub(crate) fn in_english() {
    l10n::keep(&sources());
    l10n::set_locale("en-US");
}
