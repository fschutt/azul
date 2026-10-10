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
}

impl Code {
    /// Every code, in the table's order.
    pub const ALL: [Code; 16] = [
        Code::Unavailable,
        Code::Maintenance,
        Code::Network,
        Code::QuotaExceeded,
        Code::ReadOnlyUnpaid,
        Code::CredentialsRevoked,
        Code::SignIn,
        Code::ObjectTooLarge,
        Code::NameConflict,
        Code::LinkBlocked,
        Code::Integrity,
        Code::DeletePaused,
        Code::WrongBlock,
        Code::NewerFormat,
        Code::Other,
        Code::DriveBanned,
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
            Code::ObjectTooLarge => "object_too_large",
            Code::NameConflict => "name_conflict",
            Code::LinkBlocked => "link_blocked",
            Code::Integrity => "integrity",
            Code::DeletePaused => "delete_paused",
            Code::WrongBlock => "wrong_block",
            Code::NewerFormat => "newer_format",
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

    /// Its row of [`ROWS`] (the row of [`Code::Other`] for one without a row).
    #[must_use]
    pub fn row(self) -> &'static Row {
        ROWS.iter()
            .find(|row| row.code == self)
            .or_else(|| ROWS.iter().find(|row| row.code == Code::Other))
            .unwrap_or(&ROWS[ROWS.len() - 1])
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
        en: "Your last payment didn't go through. Your files are safe and readable.",
        de: "Deine letzte Zahlung ist nicht durchgegangen. Deine Dateien sind sicher und lesbar.",
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
        en: "Deleting is paused on this drive for a moment - we'll try again.",
        de: "Löschen ist auf diesem Laufwerk kurz angehalten - wir versuchen es gleich noch \
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
];

/// The languages with texts of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    De,
}

impl Lang {
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
        let code = match service.azlin_error.as_deref() {
            Some(code) => Code::parse(code).unwrap_or(Code::Other),
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
            TokenError::Refused { status, code, .. } => {
                let known = Code::parse(code).filter(|code| *code != Code::Other);
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

    /// The text in `lang`, the error ID (the request ID) at its end when there is one.
    #[must_use]
    pub fn message(&self, lang: Lang) -> String {
        let row = self.code.row();
        let text = match lang {
            Lang::En => row.en,
            Lang::De => row.de,
        };
        let mut message = text.replace("{detail}", self.detail.trim());
        if let Some(id) = self.request_id.as_deref().filter(|id| !id.trim().is_empty()) {
            let _ = write!(message, " {}: {}", lang.error_id(), id.trim());
        }
        message
    }
}

/// A service without Azlin codes: busy (503, 429) or something else.
fn busy_or_other(status: u16) -> Code {
    match status {
        503 | 429 => Code::Unavailable,
        _ => Code::Other,
    }
}

/// The table in `lang` as a Fluent resource: `azlin-error-<code> = <text>` per row (`{detail}`
/// as `{ $detail }`), then the error ID's label (`azlin-id-label = Error ID: { $id }`).
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
            "azlin-error-{} = {}",
            row.code.as_str().replace('_', "-"),
            text.replace("{detail}", "{ $detail }")
        );
    }
    let _ = writeln!(out, "azlin-id-label = {}: {{ $id }}", lang.error_id());
    out
}
