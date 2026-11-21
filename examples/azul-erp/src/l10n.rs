//! AzERP's words through azul's localization (doc/guide/en/architecture/localization.md;
//! azul-appkit's `l10n` has the calls): `resources/en.ftl` and `resources/de.ftl` - every key
//! the source names is in both (`l10n_tests.rs`) - given to the engine at the start
//! ([`register`]) after appkit's. The engine's locale picks the language: the system's, or
//! Settings > General > Language (`--language`).
//!
//! The views' JSON names its labels by the ERP's keys (`fields.book_value.label`): each is the
//! message `azerp-label-<key>` ([`view_label`]), the English table `views/en.json` where a
//! thread has no AzERP words.

use azul::prelude::AppConfig;
use azul_appkit::l10n;

/// AzERP's words in English.
pub(crate) const EN: &str = include_str!("../resources/en.ftl");
/// AzERP's words in German.
pub(crate) const DE: &str = include_str!("../resources/de.ftl");

/// AzERP's resources per language.
const RESOURCES: [(&str, &str); 2] = [("en", EN), ("de", DE)];

/// Every resource the engine gets, per language: appkit's, then AzERP's.
#[must_use]
pub(crate) fn sources() -> Vec<(String, String)> {
    l10n::sources(&RESOURCES)
}

/// Gives the engine AzERP's resources (where the app config is built).
pub(crate) fn register(config: &mut AppConfig) {
    l10n::register(config, &RESOURCES);
}

/// The message `key` in the language of the layout pass; `english` on a thread without AzERP's
/// words (a test of the model, which reads English).
#[must_use]
pub(crate) fn or_english(key: &str, english: &str) -> String {
    let said = l10n::t(key);
    if said == key {
        english.to_string()
    } else {
        said
    }
}

/// The key a label of the views' JSON is said by: `fields.book_value.label` ->
/// `azerp-label-fields-book-value-label`.
#[must_use]
pub(crate) fn view_label_key(key: &str) -> String {
    l10n::named_key(crate::ABOUT.name, "label", key)
}

/// A label of the views' JSON in the language of the layout pass, `english` (the English
/// table's, or the key made readable) without its message.
#[must_use]
pub(crate) fn view_label(key: &str, english: &str) -> String {
    or_english(&view_label_key(key), english)
}

/// The words of AzERP in English on this thread (a test of words the window says).
#[cfg(test)]
pub(crate) fn in_english() {
    l10n::keep(&sources());
    l10n::set_locale("en-US");
}
