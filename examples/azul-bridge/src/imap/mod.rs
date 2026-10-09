//! The IMAP4rev1 server (RFC 3501) for Apple Mail, Outlook and Thunderbird, over a
//! [`MailStore`].
//!
//! Offered: `IMAP4rev1 SASL-IR LITERAL+ ID ENABLE IDLE NAMESPACE UNSELECT UIDPLUS MOVE
//! SPECIAL-USE CHILDREN AUTH=PLAIN AUTH=LOGIN` - LOGIN / AUTHENTICATE, LIST / LSUB, SELECT /
//! EXAMINE, STATUS, CREATE / DELETE / RENAME, (UN)SUBSCRIBE, APPEND (APPENDUID), FETCH (flags,
//! UID, size, internal date, ENVELOPE, BODY / BODYSTRUCTURE, BODY[...] / BODY.PEEK[...] with
//! partials, the RFC822 forms), STORE, SEARCH (every RFC 3501 key), COPY / MOVE (COPYUID),
//! EXPUNGE / UID EXPUNGE, CLOSE, UNSELECT, CHECK, NOOP, IDLE (the store is polled behind it),
//! LOGOUT. IMAP4rev2 (RFC 9051) is not announced: its cheap parts - MOVE, UIDPLUS,
//! SPECIAL-USE, NAMESPACE, UNSELECT, LITERAL-, SASL-IR, ID, IDLE - are here as the rev1
//! extensions they started as.
//!
//! Flags are the drive's state markers, so every device sees them (`AZLIN_MAIL.md` section 11):
//! `\Seen`, `\Flagged`, `\Answered`, `\Deleted` (the `deleted` marker AzMail hides a message
//! by) and keywords (label markers). `\Draft` is no marker: the Drafts folder's messages have
//! it. A mailbox's UIDs come from [`crate::uids`].

pub mod fetch;
pub mod parse;
pub mod search;
pub mod session;

use std::{
    collections::{BTreeSet, HashMap},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    auth::{Credentials, FailureGate},
    limits::Limits,
    net::{self, Conn},
    sent::SentRegistry,
    store::{MailStore, Marks, StoreError},
    uids::UidMaps,
};

/// What the server offers once signed in.
pub const CAPABILITIES: &str =
    "IMAP4rev1 SASL-IR LITERAL+ ID ENABLE IDLE NAMESPACE UNSELECT UIDPLUS MOVE SPECIAL-USE CHILDREN";
/// ...and before: the sign-in methods too.
pub const CAPABILITIES_BEFORE_LOGIN: &str = "IMAP4rev1 SASL-IR LITERAL+ ID ENABLE IDLE NAMESPACE \
     UNSELECT UIDPLUS MOVE SPECIAL-USE CHILDREN AUTH=PLAIN AUTH=LOGIN";
/// Keywords one message gets at most (each is a marker object).
pub const MAX_KEYWORDS: usize = 32;
/// How long a listing of every message's marks is reused (a client's STATUS of every
/// mailbox in a row).
const MARKS_CACHE: Duration = Duration::from_secs(2);

/// A message's flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flags {
    pub seen: bool,
    pub answered: bool,
    pub flagged: bool,
    pub deleted: bool,
    pub draft: bool,
    pub keywords: BTreeSet<String>,
}

impl Flags {
    /// The flags of a message with the drive's `marks`, in the Drafts folder or not.
    #[must_use]
    pub fn from_marks(marks: &Marks, in_drafts: bool) -> Flags {
        Flags {
            seen: marks.seen,
            answered: marks.answered,
            flagged: marks.flagged,
            deleted: marks.deleted,
            draft: in_drafts,
            keywords: marks.labels.clone(),
        }
    }

    /// What the drive keeps of them (`\Draft` follows from the folder).
    #[must_use]
    pub fn marks(&self) -> Marks {
        Marks {
            seen: self.seen,
            flagged: self.flagged,
            answered: self.answered,
            deleted: self.deleted,
            labels: self.keywords.clone(),
        }
    }

    /// Whether the flag or keyword `flag` (any case) is set.
    #[must_use]
    pub fn has(&self, flag: &str) -> bool {
        match flag.to_ascii_lowercase().as_str() {
            "\\seen" => self.seen,
            "\\answered" => self.answered,
            "\\flagged" => self.flagged,
            "\\deleted" => self.deleted,
            "\\draft" => self.draft,
            "\\recent" => false,
            _ => self.keywords.iter().any(|k| k.eq_ignore_ascii_case(flag)),
        }
    }

    /// Sets (`on`) or clears `flag`; `\Recent`, `\*` and an unknown system flag change
    /// nothing, a keyword past [`MAX_KEYWORDS`] is not kept.
    pub fn set(&mut self, flag: &str, on: bool) {
        match flag.to_ascii_lowercase().as_str() {
            "\\seen" => self.seen = on,
            "\\answered" => self.answered = on,
            "\\flagged" => self.flagged = on,
            "\\deleted" => self.deleted = on,
            "\\draft" => self.draft = on,
            other if other.starts_with('\\') => {}
            _ => {
                if on {
                    if !self.has(flag) && self.keywords.len() < MAX_KEYWORDS {
                        self.keywords.insert(flag.to_string());
                    }
                } else {
                    self.keywords.retain(|k| !k.eq_ignore_ascii_case(flag));
                }
            }
        }
    }

    /// `(\Seen \Flagged $Forwarded)`.
    #[must_use]
    pub fn render(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for (on, name) in [
            (self.seen, "\\Seen"),
            (self.answered, "\\Answered"),
            (self.flagged, "\\Flagged"),
            (self.deleted, "\\Deleted"),
            (self.draft, "\\Draft"),
        ] {
            if on {
                parts.push(name);
            }
        }
        parts.extend(self.keywords.iter().map(String::as_str));
        format!("({})", parts.join(" "))
    }
}

/// The IMAP server: what every session shares.
pub struct Imap {
    pub store: Arc<dyn MailStore>,
    pub uids: Arc<UidMaps>,
    pub credentials: Credentials,
    pub gate: Arc<FailureGate>,
    pub limits: Limits,
    pub sent: Arc<SentRegistry>,
    marks: Mutex<Option<(Instant, Arc<HashMap<String, Marks>>)>>,
}

impl std::fmt::Debug for Imap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Imap")
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

impl Imap {
    #[must_use]
    pub fn new(
        store: Arc<dyn MailStore>,
        uids: Arc<UidMaps>,
        credentials: Credentials,
        gate: Arc<FailureGate>,
        limits: Limits,
        sent: Arc<SentRegistry>,
    ) -> Imap {
        Imap {
            store,
            uids,
            credentials,
            gate,
            limits,
            sent,
            marks: Mutex::new(None),
        }
    }

    /// Serves one connection until it ends.
    pub fn handle<C: Conn>(&self, conn: C) {
        session::Session::new(self, conn).run();
    }

    /// Every message's marks (reused for [`MARKS_CACHE`]).
    ///
    /// # Errors
    ///
    /// The store's.
    pub(crate) fn marks(&self) -> Result<Arc<HashMap<String, Marks>>, StoreError> {
        let mut cached = self
            .marks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((at, marks)) = cached.as_ref() {
            if at.elapsed() < MARKS_CACHE {
                return Ok(marks.clone());
            }
        }
        let marks = Arc::new(self.store.marks()?);
        *cached = Some((Instant::now(), marks.clone()));
        Ok(marks)
    }

    /// Forgets the marks listing (after a mark was written).
    pub(crate) fn marks_changed(&self) {
        *self
            .marks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    /// Writes the marks of `before` -> `after` that differ for the message `id`: the yes / no
    /// markers and the labels (keywords). `\Draft` is not written (it follows the folder).
    ///
    /// # Errors
    ///
    /// The store's; the marks written before it stay.
    pub(crate) fn write_marks(&self, id: &str, before: &Flags, after: &Flags) -> Result<(), StoreError> {
        use crate::store::Mark;
        let result: Result<(), StoreError> = (|| {
            for (mark, was, is) in [
                (Mark::Seen, before.seen, after.seen),
                (Mark::Flagged, before.flagged, after.flagged),
                (Mark::Answered, before.answered, after.answered),
                (Mark::Deleted, before.deleted, after.deleted),
            ] {
                if was != is {
                    self.store.set_mark(id, mark, is)?;
                }
            }
            for label in after.keywords.difference(&before.keywords) {
                self.store.set_label(id, label, true)?;
            }
            for label in before.keywords.difference(&after.keywords) {
                self.store.set_label(id, label, false)?;
            }
            Ok(())
        })();
        self.marks_changed();
        result
    }
}

fn busy(stream: &mut std::net::TcpStream) {
    let _ = net::send(stream, b"* BYE Too many connections to the Azlin Bridge\r\n");
}

/// Serves IMAP on `listener` until it fails (each connection on a thread of its own).
pub fn serve(imap: Arc<Imap>, listener: TcpListener) {
    let max = imap.limits.max_connections;
    let handler: net::Handler = Arc::new(move |stream| imap.handle(stream));
    net::serve(listener, max, handler, busy);
}

#[cfg(test)]
mod tests;
