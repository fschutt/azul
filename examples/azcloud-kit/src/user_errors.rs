//! The errors users see (D33): ONE table from the codes the token server and the storage nodes
//! answer with - an Azlin node's `x-azlin-error` with its S3 error and `Retry-After`, the token
//! server's `{"error": ...}`, the client's own (no connection, a drive token gone, a damaged
//! object) - to a [`Class`], a [`Behaviour`] and a text in English (the source) and German, the
//! request ID shown as the error ID. Every app shows a storage or token server error through
//! [`UserError`]; nothing else words them.
//!
//! The texts are the app's to show as they are ([`UserError::message`]) or as a Fluent resource
//! ([`fluent_source`], one message `azlin-error-<code>` per row) for the engine's localization;
//! a language without its own texts falls back to English.

use std::fmt::Write as _;

use azlin_contract::{AzlinError, CheckoutRefusal};
use azul_storage::{DriveError, ServiceError};

use crate::{error::CloudError, token::TokenError};

/// What kind of trouble it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// It passes: try again (the user sees nothing for a while).
    Retry,
    /// This device must sign in to the drive again.
    ReAuth,
    /// The drive takes no writes until the user acts (full, unpaid); reads go on.
    ReadOnly,
    /// This request will not work: said once, per file where it is one.
    Fatal,
}

/// What the app does about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Again with backoff, the drive's other nodes (`failover`) first.
    RetryWithBackoff,
    /// Again after the `Retry-After` the node sent.
    RetryAfterPause,
    /// Queue it; go on when the connection is back.
    WaitUntilOnline,
    /// No more uploads to the drive; the queue is kept.
    StopUploads,
    /// No uploads; reads go on.
    ReadsOnly,
    /// Stop and ask to sign in to the drive again.
    SignInAgain,
    /// Fresh credentials with the drive token, then the call again - the drive does it by
    /// itself; no node is asked again with the old ones, every node refuses them.
    RefreshSession,
    /// Ask to unlock the drive on this device: its recovery code, or a join code from a device
    /// that holds its key.
    Unlock,
    /// This file is skipped.
    SkipFile,
    /// Both versions are kept.
    KeepBoth,
    /// Nothing to do.
    Nothing,
    /// Once more from another node, then the file is marked.
    RetryOnceThenMark,
    /// The drive moved: reconnect at the endpoint the answer names.
    FollowRedirect,
    /// As a busy service first, then as a damaged file; with the error ID for support.
    Report,
}

/// The codes, as `x-azlin-error` (and the client) name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Unavailable,
    Maintenance,
    /// No connection (DNS, TLS, refused) - the client's own.
    Network,
    QuotaExceeded,
    ReadOnlyUnpaid,
    CredentialsRevoked,
    /// The drive's credentials or token are gone on this device - the client's own.
    SignIn,
    /// The session's credentials ran out, or were made under a signing epoch that ended: the
    /// drive refreshes them with its drive token.
    SessionExpired,
    /// An encrypted drive this device holds no key for (yet) - the client's own.
    Locked,
    ObjectTooLarge,
    NameConflict,
    LinkBlocked,
    Integrity,
    DeletePaused,
    WrongBlock,
    /// A newer version of the app changed the drive's format (D43) - the client's own: the
    /// drive is read-only here until the app is updated.
    NewerFormat,
    /// Anything else (`internal`, a code this app does not know yet, an answer that makes no
    /// sense).
    Other,
    /// The drive is banned (ban contract v1): it takes no writes; its files can be read and
    /// copied until the ban's end.
    DriveBanned,
    /// A recovery-key lockdown of this computer waits for its 48-hour notice (D42): the drive is
    /// handed over when it ends - the token server's `lockdown_pending`.
    LockdownPending,
    /// A change of the drive's recovery keys, or a lockdown's cancel, needs a recovery key's
    /// signature (F12) - `recovery_key_required`.
    RecoveryKeyRequired,
    /// The drive's last recovery key stays - `last_recovery_key`.
    LastRecoveryKey,
    /// A recovery lookup's challenge ran out or is not the server's - `bad_challenge`.
    BadChallenge,
    /// This computer's clock is too far off the node's for a signed request (S3's
    /// `RequestTimeTooSkewed`) - the client's own: nothing works until the clock is set.
    ClockSkew,
    /// The service no longer speaks this app's contract (`client_too_old`): only an update
    /// helps.
    UpdateApp,
    /// Azlin Storage is not sold to the buyer's country (`blocked_country`): nothing can be
    /// bought, asking again does not help.
    BlockedCountry,
    /// The tier is not sold for the period ordered (`bad_months`).
    BadMonths,
    /// The tier is not sold with the payment method ordered (`bad_method`).
    BadMethod,
    /// The tier is sold out for now, or no longer sold (`tier_unavailable`, `tier_ended`).
    TierUnavailable,
    /// The payment method needs details of the payer first (`payer_required`,
    /// `payer_invalid`).
    PayerRequired,
    /// The checkout was paid already (`checkout_paid`): its drive comes, nothing to cancel.
    CheckoutPaid,
    /// A grant asked again whose first answer was used (`already_answered`): the request went
    /// through before, nothing more to do.
    AlreadyAnswered,
    /// Someone used the drive's recovery code: it takes no changes until the notice ends or a
    /// device of the owner cancels the lockdown (a node's `read_only_lockdown`).
    ReadOnlyLockdown,
    /// Azlin Storage made the drive read-only for now (`read_only_operator`).
    ReadOnlyOperator,
}

impl Code {
    /// Every code, in the table's order.
    pub const ALL: [Code; 33] = [
        Code::Unavailable,
        Code::Maintenance,
        Code::Network,
        Code::QuotaExceeded,
        Code::ReadOnlyUnpaid,
        Code::CredentialsRevoked,
        Code::SignIn,
        Code::SessionExpired,
        Code::Locked,
        Code::ObjectTooLarge,
        Code::NameConflict,
        Code::LinkBlocked,
        Code::Integrity,
        Code::DeletePaused,
        Code::WrongBlock,
        Code::NewerFormat,
        Code::Other,
        Code::DriveBanned,
        Code::LockdownPending,
        Code::RecoveryKeyRequired,
        Code::LastRecoveryKey,
        Code::BadChallenge,
        Code::ClockSkew,
        Code::UpdateApp,
        Code::BlockedCountry,
        Code::BadMonths,
        Code::BadMethod,
        Code::TierUnavailable,
        Code::PayerRequired,
        Code::CheckoutPaid,
        Code::AlreadyAnswered,
        Code::ReadOnlyLockdown,
        Code::ReadOnlyOperator,
    ];

    /// `quota_exceeded`, ...
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Code::Unavailable => "unavailable",
            Code::Maintenance => "maintenance",
            Code::Network => "network",
            Code::QuotaExceeded => "quota_exceeded",
            Code::ReadOnlyUnpaid => "read_only_unpaid",
            Code::CredentialsRevoked => "credentials_revoked",
            Code::SignIn => "sign_in",
            Code::SessionExpired => "session_expired",
            Code::Locked => "locked",
            Code::ObjectTooLarge => "object_too_large",
            Code::NameConflict => "name_conflict",
            Code::LinkBlocked => "link_blocked",
            Code::Integrity => "integrity",
            Code::DeletePaused => "delete_paused",
            Code::WrongBlock => "wrong_block",
            Code::NewerFormat => "newer_format",
            Code::LockdownPending => "lockdown_pending",
            Code::RecoveryKeyRequired => "recovery_key_required",
            Code::LastRecoveryKey => "last_recovery_key",
            Code::BadChallenge => "bad_challenge",
            Code::ClockSkew => "clock_skew",
            Code::UpdateApp => "update_app",
            Code::BlockedCountry => "blocked_country",
            Code::BadMonths => "bad_months",
            Code::BadMethod => "bad_method",
            Code::TierUnavailable => "tier_unavailable",
            Code::PayerRequired => "payer_required",
            Code::CheckoutPaid => "checkout_paid",
            Code::AlreadyAnswered => azlin_contract::ALREADY_ANSWERED,
            Code::ReadOnlyLockdown => "read_only_lockdown",
            Code::ReadOnlyOperator => "read_only_operator",
            Code::Other => "other",
            Code::DriveBanned => "drive_banned",
        }
    }

    /// The code named `text` (`internal` is [`Code::Other`]).
    #[must_use]
    pub fn parse(text: &str) -> Option<Code> {
        let text = text.trim();
        if text == "internal" {
            return Some(Code::Other);
        }
        Code::ALL.into_iter().find(|code| code.as_str() == text)
    }

    /// Its message in the Fluent resources ([`fluent_source`]): `azlin-error-<code>`, the
    /// code's underscores as hyphens (`azlin-error-read-only-unpaid`).
    #[must_use]
    pub fn message_id(self) -> String {
        format!("azlin-error-{}", self.as_str().replace('_', "-"))
    }

    /// Its row of [`ROWS`] (the row of [`Code::Other`] for one without a row).
    #[must_use]
    pub fn row(self) -> &'static Row {
        ROWS.iter()
            .find(|row| row.code == self)
            .or_else(|| ROWS.iter().find(|row| row.code == Code::Other))
            .unwrap_or(&ROWS[ROWS.len() - 1])
    }
}

/// Every code a service sends has its row: a new code in the contract does not compile
/// until it is given one here.
impl From<AzlinError> for Code {
    fn from(code: AzlinError) -> Code {
        match code {
            AzlinError::Unavailable => Code::Unavailable,
            AzlinError::Maintenance => Code::Maintenance,
            AzlinError::QuotaExceeded => Code::QuotaExceeded,
            AzlinError::ReadOnlyUnpaid => Code::ReadOnlyUnpaid,
            AzlinError::CredentialsRevoked => Code::CredentialsRevoked,
            AzlinError::ObjectTooLarge => Code::ObjectTooLarge,
            AzlinError::NameConflict => Code::NameConflict,
            AzlinError::LinkBlocked => Code::LinkBlocked,
            AzlinError::Integrity => Code::Integrity,
            AzlinError::DeletePaused => Code::DeletePaused,
            AzlinError::WrongBlock => Code::WrongBlock,
            AzlinError::Internal => Code::Other,
            AzlinError::DriveBanned => Code::DriveBanned,
            AzlinError::SessionExpired => Code::SessionExpired,
            AzlinError::ClockSkew => Code::ClockSkew,
            // Both are fixed by fresh credentials, as expired ones are.
            AzlinError::MapStale | AzlinError::CredentialsStale => Code::SessionExpired,
            AzlinError::LockdownPending => Code::LockdownPending,
            AzlinError::ClientTooOld => Code::UpdateApp,
            // A code from a newer node, or a broken one: said with what the node said.
            AzlinError::Unknown => Code::Other,
            AzlinError::RecoveryKeyRequired => Code::RecoveryKeyRequired,
            AzlinError::LastRecoveryKey => Code::LastRecoveryKey,
            AzlinError::BadChallenge => Code::BadChallenge,
            AzlinError::ReadOnlyLockdown => Code::ReadOnlyLockdown,
            AzlinError::ReadOnlyOperator => Code::ReadOnlyOperator,
        }
    }
}

/// Every way the token server refuses an order has its row: a new refusal in the contract does
/// not compile until it is given one here. The refusals no buyer can act on are said as what
/// went wrong ([`Code::Other`], with the server's sentence as the detail).
impl From<&CheckoutRefusal> for Code {
    fn from(refusal: &CheckoutRefusal) -> Code {
        match refusal {
            CheckoutRefusal::BlockedCountry => Code::BlockedCountry,
            CheckoutRefusal::BadMonths { .. } => Code::BadMonths,
            CheckoutRefusal::BadMethod => Code::BadMethod,
            CheckoutRefusal::TierUnavailable { .. } | CheckoutRefusal::TierEnded => {
                Code::TierUnavailable
            }
            CheckoutRefusal::PayerRequired | CheckoutRefusal::PayerInvalid => Code::PayerRequired,
            CheckoutRefusal::CheckoutPaid => Code::CheckoutPaid,
            // Asking again in a moment may work.
            CheckoutRefusal::ProviderUnavailable
            | CheckoutRefusal::NoPriceLadder
            | CheckoutRefusal::IssuerKeyMissing => Code::Unavailable,
            CheckoutRefusal::BadTier
            | CheckoutRefusal::BadCurrency
            | CheckoutRefusal::CurrencyEnded
            | CheckoutRefusal::PayerUnexpected
            | CheckoutRefusal::ConsentRequired
            | CheckoutRefusal::BadProvider
            | CheckoutRefusal::SurfaceUnavailable
            | CheckoutRefusal::CashUnavailable
            | CheckoutRefusal::ClaimKeyRequired
            | CheckoutRefusal::LockUsed
            | CheckoutRefusal::LockKeyWrong
            | CheckoutRefusal::LockTier
            | CheckoutRefusal::NoLock
            | CheckoutRefusal::NotPaid
            | CheckoutRefusal::CheckoutClosed
            | CheckoutRefusal::NoSuchCheckout
            | CheckoutRefusal::Unknown => Code::Other,
        }
    }
}

/// One row of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub code: Code,
    pub class: Class,
    pub behaviour: Behaviour,
    /// The English source text (`{detail}`: what went wrong, for [`Code::Other`]).
    pub en: &'static str,
    pub de: &'static str,
}

/// The Fluent message of the error ID's label (`Error ID: { $id }`), shown after an error's
/// message when it has an error ID ([`UserError::error_id`]).
pub const ID_LABEL_MESSAGE: &str = "azlin-id-label";

/// The table (D33), one row per [`Code`].
pub const ROWS: &[Row] = &[
    Row {
        code: Code::Unavailable,
        class: Class::Retry,
        behaviour: Behaviour::RetryWithBackoff,
        en: "We're having trouble reaching Azlin Storage - we'll keep trying.",
        de: "Azlin Storage ist gerade schwer zu erreichen - wir versuchen es weiter.",
    },
    Row {
        code: Code::Maintenance,
        class: Class::Retry,
        behaviour: Behaviour::RetryAfterPause,
        en: "Azlin Storage is being updated. Your files will sync in a few minutes.",
        de: "Azlin Storage wird gerade aktualisiert. Deine Dateien werden in ein paar Minuten \
             synchronisiert.",
    },
    Row {
        code: Code::Network,
        class: Class::Retry,
        behaviour: Behaviour::WaitUntilOnline,
        en: "Offline - changes will upload when you're back online.",
        de: "Offline - Änderungen werden hochgeladen, sobald du wieder online bist.",
    },
    Row {
        code: Code::QuotaExceeded,
        class: Class::ReadOnly,
        behaviour: Behaviour::StopUploads,
        en: "Your drive is full. Free up space or upgrade.",
        de: "Dein Laufwerk ist voll. Gib Speicherplatz frei oder wähle einen größeren Tarif.",
    },
    Row {
        code: Code::ReadOnlyUnpaid,
        class: Class::ReadOnly,
        behaviour: Behaviour::ReadsOnly,
        en: "Your paid period has ended. Your files are safe and readable - renew the drive to write again.",
        de: "Dein bezahlter Zeitraum ist abgelaufen. Deine Dateien sind sicher und lesbar - verlängere das Laufwerk, um wieder zu schreiben.",
    },
    Row {
        code: Code::CredentialsRevoked,
        class: Class::ReAuth,
        behaviour: Behaviour::SignInAgain,
        en: "This device was removed from the drive. Sign in again.",
        de: "Dieses Gerät wurde vom Laufwerk entfernt. Melde dich erneut an.",
    },
    Row {
        code: Code::SignIn,
        class: Class::ReAuth,
        behaviour: Behaviour::SignInAgain,
        en: "This device can no longer open the drive. Sign in to it again.",
        de: "Dieses Gerät kann das Laufwerk nicht mehr öffnen. Melde dich erneut an.",
    },
    Row {
        code: Code::SessionExpired,
        class: Class::Retry,
        behaviour: Behaviour::RefreshSession,
        en: "Renewing this device's access to the drive - syncing goes on in a moment.",
        de: "Der Zugang dieses Geräts zum Laufwerk wird erneuert - die Synchronisierung geht \
             gleich weiter.",
    },
    Row {
        code: Code::Locked,
        class: Class::ReAuth,
        behaviour: Behaviour::Unlock,
        en: "This drive is locked on this computer. Unlock it with its recovery code, or join it \
             from a device that has it.",
        de: "Dieses Laufwerk ist auf diesem Computer gesperrt. Entsperre es mit seinem \
             Wiederherstellungscode oder tritt ihm von einem Gerät aus bei, das es hat.",
    },
    Row {
        code: Code::ObjectTooLarge,
        class: Class::Fatal,
        behaviour: Behaviour::SkipFile,
        en: "This file is too large to upload (max 5 TB).",
        de: "Diese Datei ist zu groß zum Hochladen (höchstens 5 TB).",
    },
    Row {
        code: Code::NameConflict,
        class: Class::Fatal,
        behaviour: Behaviour::KeepBoth,
        en: "A newer version exists - kept both.",
        de: "Es gibt eine neuere Version - beide wurden behalten.",
    },
    Row {
        code: Code::LinkBlocked,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This link was disabled after a report.",
        de: "Dieser Link wurde nach einer Meldung deaktiviert.",
    },
    Row {
        code: Code::Integrity,
        class: Class::Fatal,
        behaviour: Behaviour::RetryOnceThenMark,
        en: "This file couldn't be read. We've noted the problem.",
        de: "Diese Datei konnte nicht gelesen werden. Wir haben das Problem vermerkt.",
    },
    Row {
        code: Code::DeletePaused,
        class: Class::Retry,
        behaviour: Behaviour::RetryAfterPause,
        en: "Deleting is paused on this drive for now - we'll try again later.",
        de: "Löschen ist auf diesem Laufwerk gerade angehalten - wir versuchen es später noch \
             einmal.",
    },
    Row {
        code: Code::WrongBlock,
        class: Class::Retry,
        behaviour: Behaviour::FollowRedirect,
        en: "Azlin Storage moved this drive - reconnecting.",
        de: "Azlin Storage hat dieses Laufwerk verschoben - die Verbindung wird neu aufgebaut.",
    },
    Row {
        code: Code::NewerFormat,
        class: Class::ReadOnly,
        behaviour: Behaviour::ReadsOnly,
        en: "A newer version of the app changed this drive. Update the app to sync it - until \
             then it stays as it is here.",
        de: "Eine neuere Version der App hat dieses Laufwerk geändert. Aktualisiere die App, um \
             es zu synchronisieren - bis dahin bleibt es hier, wie es ist.",
    },
    Row {
        code: Code::ClockSkew,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This computer's clock is off by more than 15 minutes. Set the right date and time, \
             and syncing goes on.",
        de: "Die Uhr dieses Computers geht um mehr als 15 Minuten falsch. Stelle Datum und \
             Uhrzeit richtig, dann geht die Synchronisierung weiter.",
    },
    Row {
        code: Code::UpdateApp,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This version of the app is too old for Azlin Storage. Update the app to go on - \
             your files are safe.",
        de: "Diese Version der App ist zu alt für Azlin Storage. Aktualisiere die App, um \
             weiterzumachen - deine Dateien sind sicher.",
    },
    Row {
        code: Code::Other,
        class: Class::Fatal,
        behaviour: Behaviour::Report,
        en: "Something went wrong: {detail}",
        de: "Etwas ist schiefgelaufen: {detail}",
    },
    Row {
        code: Code::DriveBanned,
        class: Class::ReadOnly,
        behaviour: Behaviour::ReadsOnly,
        en: "This drive was banned: nothing new can be uploaded. Its files can still be read and \
             copied to this computer until it closes.",
        de: "Dieses Laufwerk wurde gesperrt: Neues kann nicht hochgeladen werden. Seine Dateien \
             lassen sich noch lesen und auf diesen Computer kopieren, bis es geschlossen wird.",
    },
    Row {
        code: Code::LockdownPending,
        class: Class::Retry,
        behaviour: Behaviour::RetryAfterPause,
        en: "Your recovery is underway: this computer gets the drive when the 48-hour notice \
             ends. Your other devices can still stop it until then.",
        de: "Deine Wiederherstellung läuft: Dieser Computer bekommt das Laufwerk, wenn die \
             48-Stunden-Frist endet. Bis dahin können deine anderen Geräte sie noch stoppen.",
    },
    Row {
        code: Code::RecoveryKeyRequired,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This needs your recovery code: type it from your emergency kit.",
        de: "Dafür brauchst du deinen Wiederherstellungscode: Gib ihn aus deinem Notfall-Kit ein.",
    },
    Row {
        code: Code::LastRecoveryKey,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This is the drive's last recovery key. Add another one before you remove it.",
        de: "Das ist der letzte Wiederherstellungsschlüssel des Laufwerks. Füge zuerst einen \
             weiteren hinzu, bevor du ihn entfernst.",
    },
    Row {
        code: Code::BlockedCountry,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "Azlin Storage is not sold in your country yet. Nothing was charged.",
        de: "Azlin Storage wird in deinem Land noch nicht verkauft. Es wurde nichts berechnet.",
    },
    Row {
        code: Code::BadMonths,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This size is not sold for that period. Choose another period.",
        de: "Diese Größe wird für diesen Zeitraum nicht verkauft. Wähle einen anderen Zeitraum.",
    },
    Row {
        code: Code::BadMethod,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This size is not sold with that payment method. Choose another way to pay.",
        de: "Diese Größe wird mit dieser Zahlungsart nicht verkauft. Wähle eine andere \
             Zahlungsart.",
    },
    Row {
        code: Code::TierUnavailable,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This size is not available right now. Choose another size, or try again later.",
        de: "Diese Größe ist gerade nicht erhältlich. Wähle eine andere Größe oder versuche es \
             später noch einmal.",
    },
    Row {
        code: Code::PayerRequired,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This payment method needs a few details about you first. Fill them in and order \
             again.",
        de: "Diese Zahlungsart braucht zuerst ein paar Angaben zu dir. Fülle sie aus und \
             bestelle noch einmal.",
    },
    Row {
        code: Code::AlreadyAnswered,
        class: Class::Fatal,
        behaviour: Behaviour::Nothing,
        en: "This was already done: the earlier request went through.",
        de: "Das wurde bereits erledigt: Die frühere Anfrage ist durchgegangen.",
    },
    Row {
        code: Code::CheckoutPaid,
        class: Class::Retry,
        behaviour: Behaviour::Nothing,
        en: "This order is paid already: the drive arrives as soon as the payment is confirmed.",
        de: "Diese Bestellung ist schon bezahlt: Das Laufwerk kommt, sobald die Zahlung \
             bestätigt ist.",
    },
    Row {
        code: Code::ReadOnlyLockdown,
        class: Class::ReadOnly,
        behaviour: Behaviour::ReadsOnly,
        en: "Someone used this drive's recovery code: it takes no changes until the 48-hour \
             notice ends. If it wasn't you, cancel the lockdown with your recovery code - your \
             files are safe.",
        de: "Jemand hat den Wiederherstellungscode dieses Laufwerks benutzt: Es nimmt keine \
             Änderungen an, bis die 48-Stunden-Frist endet. Wenn du das nicht warst, brich die \
             Sperrung mit deinem Wiederherstellungscode ab - deine Dateien sind sicher.",
    },
    Row {
        code: Code::ReadOnlyOperator,
        class: Class::ReadOnly,
        behaviour: Behaviour::ReadsOnly,
        en: "Azlin Storage made this drive read-only for now. Your files are safe and readable; \
             syncing goes on when it takes changes again.",
        de: "Azlin Storage hat dieses Laufwerk vorübergehend schreibgeschützt. Deine Dateien \
             sind sicher und lesbar; die Synchronisierung geht weiter, sobald es wieder \
             Änderungen annimmt.",
    },
    Row {
        code: Code::BadChallenge,
        class: Class::Retry,
        behaviour: Behaviour::RetryWithBackoff,
        en: "The recovery request ran out. Try again.",
        de: "Die Wiederherstellungsanfrage ist abgelaufen. Versuche es noch einmal.",
    },
];

/// The languages with texts of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    De,
}

impl Lang {
    /// Every language with texts of its own.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::De];

    /// Its tag, under which an app gives the engine its Fluent resource ([`fluent_source`]):
    /// `en`, `de`.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
        }
    }

    /// The language of an OS locale (`de_DE.UTF-8`, `de-AT`, `de`): German for German, else
    /// English.
    #[must_use]
    pub fn from_locale(locale: &str) -> Lang {
        let primary = locale
            .trim()
            .split(['_', '-', '.', '@'])
            .next()
            .unwrap_or_default();
        if primary.eq_ignore_ascii_case("de") {
            Lang::De
        } else {
            Lang::En
        }
    }

    fn error_id(self) -> &'static str {
        match self {
            Lang::En => "Error ID",
            Lang::De => "Fehler-ID",
        }
    }
}

/// A storage or token server error as the user sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserError {
    pub code: Code,
    /// The node's `Retry-After`, in seconds.
    pub retry_after: Option<u64>,
    /// The request ID: the error ID support traces.
    pub request_id: Option<String>,
    /// What went wrong, as the service or the client said it (for logs, and [`Code::Other`]).
    pub detail: String,
}

impl UserError {
    fn new(code: Code, detail: impl Into<String>) -> UserError {
        UserError {
            code,
            retry_after: None,
            request_id: None,
            detail: detail.into(),
        }
    }

    /// A storage error; `None` for one the user caused or chose (no such file, a name that
    /// cannot be one, a local disk, a drive's settings), which say what they are themselves.
    #[must_use]
    pub fn from_drive_error(e: &DriveError) -> Option<UserError> {
        Some(match e {
            DriveError::Service(service) => UserError::from_service(service),
            DriveError::Transport(why) => UserError::new(Code::Network, why.clone()),
            DriveError::Denied { message } => UserError::new(Code::SignIn, message.clone()),
            DriveError::Locked { .. } => UserError::new(Code::Locked, e.to_string()),
            DriveError::Conflict { .. } => UserError::new(Code::NameConflict, e.to_string()),
            DriveError::Corrupt { .. } => UserError::new(Code::Integrity, e.to_string()),
            DriveError::Protocol(why) => UserError::new(Code::Other, why.clone()),
            DriveError::NotFound { .. }
            | DriveError::InvalidKey { .. }
            | DriveError::InvalidRange { .. }
            | DriveError::Io(_)
            | DriveError::Unsupported(_)
            | DriveError::InvalidConfig(_) => return None,
        })
    }

    /// An S3 error answer: its `x-azlin-error` (an unknown one is [`Code::Other`]); a service
    /// without Azlin codes by its status only - busy (503, 429) or [`Code::Other`].
    fn from_service(service: &ServiceError) -> UserError {
        let code = match service.azlin_error {
            Some(code) => Code::from(code),
            None if service.code == "RequestTimeTooSkewed" => Code::ClockSkew,
            None => busy_or_other(service.status),
        };
        UserError {
            code,
            retry_after: service.retry_after,
            request_id: service.request_id.clone(),
            detail: service.to_string(),
        }
    }

    /// A token server's refusal; `None` for a setting of this app (no token server, no id).
    #[must_use]
    pub fn from_token_error(e: &TokenError) -> Option<UserError> {
        Some(match e {
            TokenError::Connect(why) => UserError::new(Code::Network, why.clone()),
            TokenError::SignIn(why) => UserError::new(Code::CredentialsRevoked, why.clone()),
            // The token server's other codes are not in the contract yet: by their status.
            TokenError::Refused { status, code, .. } => {
                let known = AzlinError::parse(code)
                    .map(Code::from)
                    .or_else(|| CheckoutRefusal::parse(code).map(|r| Code::from(&r)))
                    .or_else(|| (code == azlin_contract::ALREADY_ANSWERED).then_some(Code::AlreadyAnswered))
                    .filter(|code| *code != Code::Other);
                UserError::new(known.unwrap_or_else(|| busy_or_other(*status)), e.to_string())
            }
            TokenError::Protocol(why) => UserError::new(Code::Other, why.clone()),
            TokenError::Config(_) => return None,
        })
    }

    /// The error under every context of `e` ([`UserError::from_drive_error`],
    /// [`UserError::from_token_error`]); `None` for the app's own.
    #[must_use]
    pub fn from_cloud_error(e: &CloudError) -> Option<UserError> {
        match e.root() {
            CloudError::Token(token) => UserError::from_token_error(token),
            CloudError::Drive(drive) => UserError::from_drive_error(drive),
            _ => None,
        }
    }

    #[must_use]
    pub fn class(&self) -> Class {
        self.code.row().class
    }

    #[must_use]
    pub fn behaviour(&self) -> Behaviour {
        self.code.row().behaviour
    }

    /// Whether it asks for a system notification: only an error the user must act on (sign in
    /// again, a full or unpaid drive) - a transient one stays in the app.
    #[must_use]
    pub fn notifies(&self) -> bool {
        matches!(self.class(), Class::ReAuth | Class::ReadOnly)
    }

    /// The text in `lang`, the error ID (the request ID) at its end when there is one - for
    /// a terminal; an app's window says [`Self::message_id`] with [`Self::fluent_args`] and
    /// the error ID through the engine's localization in the window's locale.
    #[must_use]
    pub fn message(&self, lang: Lang) -> String {
        let row = self.code.row();
        let text = match lang {
            Lang::En => row.en,
            Lang::De => row.de,
        };
        let mut message = text.replace("{detail}", self.detail.trim());
        if let Some(id) = self.error_id() {
            let _ = write!(message, " {}: {id}", lang.error_id());
        }
        message
    }

    /// Its message in the Fluent resources ([`Code::message_id`]).
    #[must_use]
    pub fn message_id(&self) -> String {
        self.code.message_id()
    }

    /// The arguments of its message: `detail` (what went wrong, for [`Code::Other`]).
    #[must_use]
    pub fn fluent_args(&self) -> Vec<(&'static str, String)> {
        vec![("detail", self.detail.trim().to_string())]
    }

    /// The error ID (the request ID support traces), when there is one: the `id` of
    /// [`ID_LABEL_MESSAGE`].
    #[must_use]
    pub fn error_id(&self) -> Option<&str> {
        self.request_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    }
}

/// A service without Azlin codes: busy (503, 429), too new for this app (426, "upgrade
/// required", also from a proxy that drops the body) or something else.
fn busy_or_other(status: u16) -> Code {
    match status {
        503 | 429 => Code::Unavailable,
        426 => Code::UpdateApp,
        _ => Code::Other,
    }
}

/// The table in `lang` as a Fluent resource: [`Code::message_id`] `= <text>` per row
/// (`{detail}` as `{ $detail }`), then the error ID's label ([`ID_LABEL_MESSAGE`] `= Error ID:
/// { $id }`) - what an app gives the engine (`AppConfig::fluent_locales`) under the language's
/// tag, `en` and `de`.
#[must_use]
pub fn fluent_source(lang: Lang) -> String {
    let mut out = String::from("# The errors users see (azcloud-kit user_errors).\n");
    for row in ROWS {
        let text = match lang {
            Lang::En => row.en,
            Lang::De => row.de,
        };
        let _ = writeln!(
            out,
            "{} = {}",
            row.code.message_id(),
            text.replace("{detail}", "{ $detail }")
        );
    }
    let _ = writeln!(out, "{ID_LABEL_MESSAGE} = {}: {{ $id }}", lang.error_id());
    out
}
