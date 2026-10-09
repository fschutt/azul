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
//! Flags: `\Seen`, `\Flagged` and `\Answered` are the drive's state markers (every device sees
//! them); `\Deleted`, `\Draft` and keywords live in the bridge's memory until EXPUNGE or a
//! restart. A mailbox's UIDs come from [`crate::uids`].

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
/// Keywords one message keeps at most (they live in memory).
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
    /// The flags the drive's markers give.
    #[must_use]
    pub fn from_marks(marks: Marks) -> Flags {
        Flags {
            seen: marks.seen,
            answered: marks.answered,
            flagged: marks.flagged,
            ..Flags::default()
        }
    }

    /// The part the drive keeps.
    #[must_use]
    pub fn marks(&self) -> Marks {
        Marks {
            seen: self.seen,
            flagged: self.flagged,
            answered: self.answered,
        }
    }

    /// Only the part the bridge keeps in memory.
    #[must_use]
    pub fn volatile(&self) -> Flags {
        Flags {
            deleted: self.deleted,
            draft: self.draft,
            keywords: self.keywords.clone(),
            ..Flags::default()
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
    /// `\Deleted`, `\Draft` and keywords, by (mailbox, name).
    volatile: Mutex<HashMap<(String, String), Flags>>,
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
            volatile: Mutex::new(HashMap::new()),
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

    pub(crate) fn volatile_of(&self, mailbox: &str, name: &str) -> Flags {
        self.volatile
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(mailbox.to_string(), name.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn set_volatile(&self, mailbox: &str, name: &str, flags: &Flags) {
        let volatile = flags.volatile();
        let mut map = self
            .volatile
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = (mailbox.to_string(), name.to_string());
        if volatile == Flags::default() {
            map.remove(&key);
        } else {
            map.insert(key, volatile);
        }
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
