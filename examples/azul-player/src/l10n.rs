//! AzPlayer's words through azul's localization (doc/guide/en/architecture/localization.md;
//! azul-appkit's `l10n` has the calls): `resources/en.ftl` and `resources/de.ftl` - every key
//! the source names is in both (`l10n_tests.rs`) - given to the engine at the start
//! ([`register`]) after appkit's. The engine's locale picks the language: the system's, or
//! settings > general > language (`--language`).

use azul::prelude::AppConfig;
use azul_appkit::l10n;

/// AzPlayer's words in English.
pub(crate) const EN: &str = include_str!("../resources/en.ftl");
/// AzPlayer's words in German.
pub(crate) const DE: &str = include_str!("../resources/de.ftl");

/// AzPlayer's resources per language.
const RESOURCES: [(&str, &str); 2] = [("en", EN), ("de", DE)];

/// Every resource the engine gets, per language: appkit's, then AzPlayer's.
#[must_use]
pub(crate) fn sources() -> Vec<(String, String)> {
    l10n::sources(&RESOURCES)
}

/// Gives the engine AzPlayer's resources (where the app config is built).
pub(crate) fn register(config: &mut AppConfig) {
    l10n::register(config, &RESOURCES);
}

/// AzPlayer's word for a thing it keys by its English name (the strip's categories and items,
/// the pages and their views, the settings' categories and buttons, the libraries: their
/// markers, ids and scripts stay English): the message `azplayer-word-<name>`, the name as it
/// is without one.
#[must_use]
pub(crate) fn word(name: &str) -> String {
    l10n::named(crate::app::ABOUT.name, "word", name)
}

/// The words of AzPlayer in English on this thread (a test of words the window says).
#[cfg(test)]
pub(crate) fn in_english() {
    l10n::keep(&sources());
    l10n::set_locale("en-US");
}
