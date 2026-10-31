//! AzCalculator's words through azul's localization (doc/guide/en/architecture/localization.md;
//! azul-appkit's `l10n` has the calls): `resources/en.ftl` and `resources/de.ftl` - every key
//! the source names is in both (`l10n_tests.rs`) - given to the engine at the start
//! ([`register`]) after appkit's. The engine's locale picks the language: the system's, or
//! Settings > General > Language (`--language`).

use azul::prelude::AppConfig;
use azul_appkit::l10n;

/// AzCalculator's words in English.
pub(crate) const EN: &str = include_str!("../resources/en.ftl");
/// AzCalculator's words in German.
pub(crate) const DE: &str = include_str!("../resources/de.ftl");

/// AzCalculator's resources per language.
const RESOURCES: [(&str, &str); 2] = [("en", EN), ("de", DE)];

/// Every resource the engine gets, per language: appkit's, then AzCalculator's.
#[must_use]
pub(crate) fn sources() -> Vec<(String, String)> {
    l10n::sources(&RESOURCES)
}

/// Gives the engine AzCalculator's resources (where the app config is built).
pub(crate) fn register(config: &mut AppConfig) {
    l10n::register(config, &RESOURCES);
}

/// AzCalculator's word for a thing it names in English (a unit, a category of units): the
/// message `azcalculator-<what>-<name>` in the language of the layout pass, the name as it is
/// without one. The English name stays the thing's id.
#[must_use]
pub(crate) fn word(what: &str, name: &str) -> String {
    l10n::named(crate::ui::ABOUT.name, what, name)
}

/// The words of AzCalculator in English on this thread (a test of words the window says).
#[cfg(test)]
pub(crate) fn in_english() {
    l10n::keep(&sources());
    l10n::set_locale("en-US");
}
