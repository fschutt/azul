//! AzPasswordManager's words through azul's localization (doc/guide/en/architecture/localization.md;
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

/// AzPasswordManager's words in English.
pub(crate) const EN: &str = include_str!("../resources/en.ftl");
/// AzPasswordManager's words in German.
pub(crate) const DE: &str = include_str!("../resources/de.ftl");

/// AzPasswordManager's resources per language.
const RESOURCES: [(&str, &str); 2] = [("en", EN), ("de", DE)];

/// Every resource the engine gets, per language: appkit's, then AzPasswordManager's.
#[must_use]
pub(crate) fn sources() -> Vec<(String, String)> {
    l10n::sources(&RESOURCES)
}

/// Gives the engine AzPasswordManager's resources (where the app config is built).
pub(crate) fn register(config: &mut AppConfig) {
    l10n::register(config, &RESOURCES);
}

/// The words of AzPasswordManager in English on this thread (a test of words the window says).
#[cfg(test)]
pub(crate) fn in_english() {
    l10n::keep(&sources());
    l10n::set_locale("en-US");
}

/// The navigation's name of a kind's items ("Logins").
#[must_use]
pub(crate) fn kinds(kind: Kind) -> String {
    t(match kind {
        Kind::Login => "azpasswordmanager-kinds-login",
        Kind::Card => "azpasswordmanager-kinds-card",
        Kind::Note => "azpasswordmanager-kinds-note",
        Kind::Identity => "azpasswordmanager-kinds-identity",
        Kind::SshKey => "azpasswordmanager-kinds-ssh-key",
    })
}

/// One item of a kind, under the item's title ("login").
#[must_use]
pub(crate) fn kind(kind: Kind) -> String {
    t(match kind {
        Kind::Login => "azpasswordmanager-kind-login",
        Kind::Card => "azpasswordmanager-kind-card",
        Kind::Note => "azpasswordmanager-kind-note",
        Kind::Identity => "azpasswordmanager-kind-identity",
        Kind::SshKey => "azpasswordmanager-kind-ssh-key",
    })
}

/// The kind switch's choice of a new item ("Login").
#[must_use]
pub(crate) fn kind_choice(kind: Kind) -> &'static str {
    match kind {
        Kind::Login => "azpasswordmanager-kind-choice-login",
        Kind::Card => "azpasswordmanager-kind-choice-card",
        Kind::Note => "azpasswordmanager-kind-choice-note",
        Kind::Identity => "azpasswordmanager-kind-choice-identity",
        Kind::SshKey => "azpasswordmanager-kind-choice-ssh-key",
    }
}

/// The edit form's heading: a new item of the kind or one being edited ("New login"); a
/// message each, as German's words change with the kind.
#[must_use]
pub(crate) fn form_title(kind: Kind, new: bool) -> String {
    t(match (kind, new) {
        (Kind::Login, true) => "azpasswordmanager-new-login",
        (Kind::Card, true) => "azpasswordmanager-new-card",
        (Kind::Note, true) => "azpasswordmanager-new-note",
        (Kind::Identity, true) => "azpasswordmanager-new-identity",
        (Kind::SshKey, true) => "azpasswordmanager-new-ssh-key",
        (Kind::Login, false) => "azpasswordmanager-edit-login",
        (Kind::Card, false) => "azpasswordmanager-edit-card",
        (Kind::Note, false) => "azpasswordmanager-edit-note",
        (Kind::Identity, false) => "azpasswordmanager-edit-identity",
        (Kind::SshKey, false) => "azpasswordmanager-edit-ssh-key",
    })
}

/// A count of items of a kind in the import preview ("3 logins").
#[must_use]
pub(crate) fn kind_count(kind: Kind, count: usize) -> String {
    let key = match kind {
        Kind::Login => "azpasswordmanager-count-login",
        Kind::Card => "azpasswordmanager-count-card",
        Kind::Note => "azpasswordmanager-count-note",
        Kind::Identity => "azpasswordmanager-count-identity",
        Kind::SshKey => "azpasswordmanager-count-ssh-key",
    };
    t_args(key, &[("count", Arg::from(count))])
}

/// A password's strength in words ("strong").
#[must_use]
pub(crate) fn strength(strength: Strength) -> String {
    t(match strength {
        Strength::VeryWeak => "azpasswordmanager-strength-very-weak",
        Strength::Weak => "azpasswordmanager-strength-weak",
        Strength::Medium => "azpasswordmanager-strength-medium",
        Strength::Strong => "azpasswordmanager-strength-strong",
        Strength::VeryStrong => "azpasswordmanager-strength-very-strong",
    })
}

/// A strength with its estimate ("strong (~72 bits)").
#[must_use]
pub(crate) fn strength_bits(bits: f64) -> String {
    t_args(
        "azpasswordmanager-strength-bits",
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
        Mode::Password => "azpasswordmanager-mode-password",
        Mode::Passphrase => "azpasswordmanager-mode-passphrase",
        Mode::Pin => "azpasswordmanager-mode-pin",
    }
}

/// The audit table's words for a problem.
#[must_use]
pub(crate) fn problem(problem: Problem) -> String {
    match problem {
        Problem::Weak => t("azpasswordmanager-problem-weak"),
        Problem::Reused(n) => t_args("azpasswordmanager-problem-reused", &[("count", Arg::from(n))]),
        Problem::Old => t("azpasswordmanager-problem-old"),
        Problem::NoTwoFactor => t("azpasswordmanager-problem-no-two-factor"),
    }
}
