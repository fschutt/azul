//! AzKeys' words through azul's localization (doc/guide/en/architecture/localization.md;
//! azul-appkit's `l10n` has the calls): `resources/en.ftl` and `resources/de.ftl` - every key
//! the source names is in both (`l10n_tests.rs`) - given to the engine at the start
//! ([`register`]) after appkit's. The engine's locale picks the language: the system's, or
//! Settings > General > Language (`--language`).
//!
//! The model keeps its English names (the search, the exported audit report); the window says
//! them with the words below, in the language of the layout pass.

use azul::prelude::AppConfig;
use azul_appkit::l10n::{self, t, t_args, Arg};

use crate::audit::Problem;
use crate::generator::{Mode, Strength};
use crate::vault::Kind;

/// AzKeys' words in English.
pub(crate) const EN: &str = include_str!("../resources/en.ftl");
/// AzKeys' words in German.
pub(crate) const DE: &str = include_str!("../resources/de.ftl");

/// AzKeys' resources per language.
const RESOURCES: [(&str, &str); 2] = [("en", EN), ("de", DE)];

/// Every resource the engine gets, per language: appkit's, then AzKeys'.
#[must_use]
pub(crate) fn sources() -> Vec<(String, String)> {
    l10n::sources(&RESOURCES)
}

/// Gives the engine AzKeys' resources (where the app config is built).
pub(crate) fn register(config: &mut AppConfig) {
    l10n::register(config, &RESOURCES);
}

/// The words of AzKeys in English on this thread (a test of words the window says).
#[cfg(test)]
pub(crate) fn in_english() {
    l10n::keep(&sources());
    l10n::set_locale("en-US");
}

/// The navigation's name of a kind's items ("Logins").
#[must_use]
pub(crate) fn kinds(kind: Kind) -> String {
    t(match kind {
        Kind::Login => "azkeys-kinds-login",
        Kind::Card => "azkeys-kinds-card",
        Kind::Note => "azkeys-kinds-note",
        Kind::Identity => "azkeys-kinds-identity",
        Kind::SshKey => "azkeys-kinds-ssh-key",
    })
}

/// One item of a kind, under the item's title ("login").
#[must_use]
pub(crate) fn kind(kind: Kind) -> String {
    t(match kind {
        Kind::Login => "azkeys-kind-login",
        Kind::Card => "azkeys-kind-card",
        Kind::Note => "azkeys-kind-note",
        Kind::Identity => "azkeys-kind-identity",
        Kind::SshKey => "azkeys-kind-ssh-key",
    })
}

/// The kind switch's choice of a new item ("Login").
#[must_use]
pub(crate) fn kind_choice(kind: Kind) -> &'static str {
    match kind {
        Kind::Login => "azkeys-kind-choice-login",
        Kind::Card => "azkeys-kind-choice-card",
        Kind::Note => "azkeys-kind-choice-note",
        Kind::Identity => "azkeys-kind-choice-identity",
        Kind::SshKey => "azkeys-kind-choice-ssh-key",
    }
}

/// The edit form's heading: a new item of the kind or one being edited ("New login"); a
/// message each, as German's words change with the kind.
#[must_use]
pub(crate) fn form_title(kind: Kind, new: bool) -> String {
    t(match (kind, new) {
        (Kind::Login, true) => "azkeys-new-login",
        (Kind::Card, true) => "azkeys-new-card",
        (Kind::Note, true) => "azkeys-new-note",
        (Kind::Identity, true) => "azkeys-new-identity",
        (Kind::SshKey, true) => "azkeys-new-ssh-key",
        (Kind::Login, false) => "azkeys-edit-login",
        (Kind::Card, false) => "azkeys-edit-card",
        (Kind::Note, false) => "azkeys-edit-note",
        (Kind::Identity, false) => "azkeys-edit-identity",
        (Kind::SshKey, false) => "azkeys-edit-ssh-key",
    })
}

/// A count of items of a kind in the import preview ("3 logins").
#[must_use]
pub(crate) fn kind_count(kind: Kind, count: usize) -> String {
    let key = match kind {
        Kind::Login => "azkeys-count-login",
        Kind::Card => "azkeys-count-card",
        Kind::Note => "azkeys-count-note",
        Kind::Identity => "azkeys-count-identity",
        Kind::SshKey => "azkeys-count-ssh-key",
    };
    t_args(key, &[("count", Arg::from(count))])
}

/// A password's strength in words ("strong").
#[must_use]
pub(crate) fn strength(strength: Strength) -> String {
    t(match strength {
        Strength::VeryWeak => "azkeys-strength-very-weak",
        Strength::Weak => "azkeys-strength-weak",
        Strength::Medium => "azkeys-strength-medium",
        Strength::Strong => "azkeys-strength-strong",
        Strength::VeryStrong => "azkeys-strength-very-strong",
    })
}

/// A strength with its estimate ("strong (~72 bits)").
#[must_use]
pub(crate) fn strength_bits(bits: f64) -> String {
    t_args(
        "azkeys-strength-bits",
        &[
            ("strength", Arg::from(strength(Strength::of_bits(bits)))),
            ("bits", Arg::from(format!("{bits:.0}"))),
        ],
    )
}

/// The generator's choice of what it makes.
#[must_use]
pub(crate) fn mode(mode: Mode) -> &'static str {
    match mode {
        Mode::Password => "azkeys-mode-password",
        Mode::Passphrase => "azkeys-mode-passphrase",
        Mode::Pin => "azkeys-mode-pin",
    }
}

/// The audit table's words for a problem.
#[must_use]
pub(crate) fn problem(problem: Problem) -> String {
    match problem {
        Problem::Weak => t("azkeys-problem-weak"),
        Problem::Reused(n) => t_args("azkeys-problem-reused", &[("count", Arg::from(n))]),
        Problem::Old => t("azkeys-problem-old"),
        Problem::NoTwoFactor => t("azkeys-problem-no-two-factor"),
    }
}
