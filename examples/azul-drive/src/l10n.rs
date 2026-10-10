//! AzDrive's words through azul's localization (doc/guide/en/architecture/localization.md;
//! azul-appkit's `l10n` has the calls): `resources/en.ftl` and `resources/de.ftl` - every key
//! the source names is in both (`l10n_tests.rs`) - and azcloud-kit's error table with them,
//! given to the engine at the start ([`register`]). The engine's locale picks the language
//! (the system's, or Options > General > Language).
//!
//! A storage or token server error is the table's message with its error ID ([`error_text`]);
//! one the user caused (no such file, a name that cannot be one) its own words.

use azcloud_kit::{
    user_errors::{fluent_source, Lang, ID_LABEL_MESSAGE},
    CloudError, TokenError, UserError,
};
use azul::prelude::AppConfig;
use azul_appkit::l10n::{self, Arg, Phrase, Text};
use azul_storage::DriveError;

/// AzDrive's words in English.
pub(crate) const EN: &str = include_str!("../resources/en.ftl");
/// AzDrive's words in German.
pub(crate) const DE: &str = include_str!("../resources/de.ftl");

/// AzDrive's resources per language: its own words, then azcloud-kit's error table and the Azlin
/// Bridge's settings.
fn resources() -> Vec<(&'static str, String)> {
    Lang::ALL
        .iter()
        .map(|lang| {
            let own = match lang {
                Lang::En => EN,
                Lang::De => DE,
            };
            let kit = format!(
                "{}\n{}",
                fluent_source(*lang),
                azcloud_kit::bridge::fluent_source(*lang)
            );
            (lang.tag(), format!("{own}\n{kit}"))
        })
        .collect()
}

/// Every resource the engine gets, per language: appkit's, AzDrive's, the error table.
#[must_use]
pub(crate) fn sources() -> Vec<(String, String)> {
    let own = resources();
    let pairs: Vec<(&str, &str)> = own.iter().map(|(tag, s)| (*tag, s.as_str())).collect();
    l10n::sources(&pairs)
}

/// Gives the engine AzDrive's resources (where the app config is built).
pub(crate) fn register(config: &mut AppConfig) {
    let own = resources();
    let pairs: Vec<(&str, &str)> = own.iter().map(|(tag, s)| (*tag, s.as_str())).collect();
    l10n::register(config, &pairs);
}

/// A storage or token server error as the table words it: its message, then its error ID.
#[must_use]
pub(crate) fn error_text(e: &UserError) -> Text {
    let mut phrase = Phrase::new(&e.message_id());
    for (name, value) in e.fluent_args() {
        phrase = phrase.arg(name, value);
    }
    let text = Text::from(phrase);
    match e.error_id() {
        Some(id) => text
            .then(" ")
            .then(Phrase::new(ID_LABEL_MESSAGE).arg("id", id)),
        None => text,
    }
}

/// A drive's error: the table's ([`error_text`]) for a storage or token server error, AzDrive's
/// words for one the user caused ([`own_error_text`]).
#[must_use]
pub(crate) fn drive_error_text(e: &DriveError) -> Text {
    UserError::from_drive_error(e).map_or_else(|| own_error_text(e), |u| error_text(&u))
}

/// An error the user caused, in AzDrive's words: the item's name, the file system's or the
/// settings' own words as they are.
fn own_error_text(e: &DriveError) -> Text {
    let said = match e {
        DriveError::NotFound { key } => {
            Phrase::new("azdrive-err-not-found").arg("name", azul_storage::key::last_segment(key))
        }
        // The reason as a word of the resources: said in the window's language when shown (a
        // worker thread makes this too), azul-storage's English for a reason they lack.
        DriveError::InvalidKey { key, reason } => Phrase::new("azdrive-err-invalid-name")
            .arg("name", key.as_str())
            .arg(
                "reason",
                Arg::word(&l10n::named_key("AzDrive", "reason", reason), reason),
            ),
        DriveError::InvalidRange { key } => {
            Phrase::new("azdrive-err-range").arg("name", azul_storage::key::last_segment(key))
        }
        DriveError::Io(why) => Phrase::new("azdrive-err-io").arg("detail", why.as_str()),
        DriveError::Unsupported(what) => {
            Phrase::new("azdrive-err-unsupported").arg("detail", what.as_str())
        }
        other => return Text::plain(other.to_string()),
    };
    said.into()
}

/// A token server's error ([`drive_error_text`]'s rule).
#[must_use]
pub(crate) fn token_error_text(e: &TokenError) -> Text {
    UserError::from_token_error(e).map_or_else(|| Text::plain(e.to_string()), |u| error_text(&u))
}

/// A worker thread's reason: a key of the resources (its own words), or an error's words as they
/// are.
#[must_use]
pub(crate) fn said(why: &str) -> Text {
    if l10n::is_key(why) {
        Text::key(why)
    } else {
        Text::plain(why)
    }
}

/// The words of AzDrive in English on this thread (a test of words the window says).
#[cfg(test)]
pub(crate) fn in_english() {
    l10n::keep(&sources());
    l10n::set_locale("en-US");
}

/// An Azlin call's error ([`drive_error_text`]'s rule).
#[must_use]
pub(crate) fn cloud_error_text(e: &CloudError) -> Text {
    UserError::from_cloud_error(e).map_or_else(|| Text::plain(e.to_string()), |u| error_text(&u))
}
