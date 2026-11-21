//! AzShow's words through azul's localization (doc/guide/en/architecture/localization.md;
//! azul-appkit's `l10n` has the calls): `resources/en.ftl` and `resources/de.ftl` - every key
//! the source names is in both (`l10n_tests.rs`) - given to the engine at the start
//! ([`register`]) after appkit's. The engine's locale picks the language: the system's, or
//! Settings > General > Language (`--language`).

use azul::prelude::AppConfig;
use azul_appkit::l10n;

/// AzShow's words in English.
pub(crate) const EN: &str = include_str!("../resources/en.ftl");
/// AzShow's words in German.
pub(crate) const DE: &str = include_str!("../resources/de.ftl");

/// AzShow's resources per language.
const RESOURCES: [(&str, &str); 2] = [("en", EN), ("de", DE)];

/// Every resource the engine gets, per language: appkit's, then AzShow's.
#[must_use]
pub(crate) fn sources() -> Vec<(String, String)> {
    l10n::sources(&RESOURCES)
}

/// Gives the engine AzShow's resources (where the app config is built).
pub(crate) fn register(config: &mut AppConfig) {
    l10n::register(config, &RESOURCES);
}

/// The words of AzShow in English on this thread (a test of words the window says).
#[cfg(test)]
pub(crate) fn in_english() {
    l10n::keep(&sources());
    l10n::set_locale("en-US");
}

/// AzShow's word for a thing it names in English (a view, a layout, a shape, an effect, a
/// transition, a slide size, a backstage page): `azshow-<what>-<name>` in the language of the
/// layout pass, the name as it is without one. The English name stays the thing's id (the
/// stdout markers and the logs print it).
#[must_use]
pub(crate) fn word(what: &str, name: &str) -> String {
    l10n::named(crate::ABOUT.name, what, name)
}
