//! AzMeet's words through azul's localization (doc/guide/en/architecture/localization.md;
//! azul-appkit's `l10n` has the calls): `resources/en.ftl` and `resources/de.ftl` - every key
//! the source names is in both (`l10n_tests.rs`) - given to the engine at the start
//! ([`register`]) after appkit's. The engine's locale picks the language: the system's, or
//! Settings > General > Language (`--language`).

use azul::prelude::AppConfig;
use azul_appkit::{args::LanguagePref, l10n};

/// AzMeet's words in English.
pub(crate) const EN: &str = include_str!("../resources/en.ftl");
/// AzMeet's words in German.
pub(crate) const DE: &str = include_str!("../resources/de.ftl");

/// AzMeet's resources per language.
const RESOURCES: [(&str, &str); 2] = [("en", EN), ("de", DE)];

/// Every resource the engine gets, per language: appkit's, then AzMeet's.
#[must_use]
pub(crate) fn sources() -> Vec<(String, String)> {
    l10n::sources(&RESOURCES)
}

/// Gives the engine AzMeet's resources (where the app config is built).
pub(crate) fn register(config: &mut AppConfig) {
    l10n::register(config, &RESOURCES);
}

/// Keeps AzMeet's words on this thread and says them in `language` (the system's for
/// `System`): before the window and its kit exist - the meeting server's first check is said
/// at the start ([`register`] gives them to the engine with the window's config later).
pub(crate) fn speak(language: LanguagePref) {
    l10n::keep(&sources());
    if language != LanguagePref::System {
        l10n::set_locale(language.tag());
    }
}

/// AzMeet's word for a thing it names in English (a category of its settings, which is the
/// category's id too): `azmeet-<what>-<name>` in the language of the layout pass.
#[must_use]
pub(crate) fn word(what: &str, name: &str) -> String {
    l10n::named(crate::ABOUT.name, what, name)
}

/// The words of AzMeet in English on this thread (a test of words the window says).
#[cfg(test)]
pub(crate) fn in_english() {
    l10n::keep(&sources());
    l10n::set_locale("en-US");
}
