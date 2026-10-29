//! What one answer of an Azlin service means for the request that got it, read in one place:
//! the token server's `{"error": ...}` body with its `"spent"` and `Retry-After`, a storage
//! node's `x-azlin-error` with its S3 code and `Retry-After`, and no answer at all. Every caller
//! asks here - the token server's calls ([`crate::token`]), the failover of a drive's requests
//! ([`crate::failover`]), the sync's retries ([`crate::sync::retry`]), the open grants
//! ([`crate::outbox`]) and the words the user sees ([`crate::user_errors`]) - so a code means
//! the same everywhere, and a new code of the services is one row here.
//!
//! An answer is [`Outcome::Done`] (the caller reads it), [`Outcome::Transient`] (it passes by
//! itself: the same request again, after the pause the service named) or
//! [`Outcome::Refused`] (asking again does not change it: what it is, as the code the user is
//! told). A transient answer says whether the request may have done something ([`Spent`]):
//! only one that spent nothing may go again whatever the request is.

use std::time::Duration;

use azlin_contract::{
    AzlinError, CheckoutRefusal, ALREADY_ANSWERED, IDEMPOTENCY_REQUIRED, RATE_LIMITED,
};
use azul_storage::{DriveError, HttpReply, ServiceError};
use serde_json::Value;

use crate::{token::TokenError, user_errors::Code};

/// Whether a request whose answer is transient may have done something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spent {
    /// The service says it did nothing (its rate limit, `"spent": false`, a node that refused
    /// before the work): the same request may go again, whatever it does.
    Nothing,
    /// It may have (no answer, a failure that does not say): only a request that answers the
    /// same when it comes twice - a read, a grant named by its idempotency key - goes again at
    /// once; a drive token's refresh keeps the token for its next try.
    Maybe,
}

/// How an error code passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// By itself: asked again unchanged, it may work.
    Transient(Spent),
    /// Not by asking again.
    Final,
}

/// Which service answered: a node names its code in `x-azlin-error`, the token server in its
/// body, and one word can mean another thing at each (`unavailable`: a node refused before the
/// work, the token server may have spent the token).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    TokenServer,
    Node,
}

/// What one answer means for its request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// An answer for the caller to read: a success, or one that says what is (a node's 304,
    /// 404 or 412).
    Done,
    /// It passes by itself: the same request again - the same drive token, the same
    /// idempotency key, the same body - not before `retry_after` when the service named one,
    /// and at another node of the drive first when `elsewhere`.
    Transient {
        /// What the user is told while it lasts.
        code: Code,
        retry_after: Option<Duration>,
        spent: Spent,
        elsewhere: bool,
    },
    /// Asking again does not change it: what it is.
    Refused(Code),
}

impl Outcome {
    /// Whether it passes by itself.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Outcome::Transient { .. })
    }

    /// What the user is told; `None` for an answer that is no trouble.
    #[must_use]
    pub fn code(&self) -> Option<Code> {
        match self {
            Outcome::Done => None,
            Outcome::Transient { code, .. } | Outcome::Refused(code) => Some(*code),
        }
    }

    /// The pause the service asked for.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Outcome::Transient { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

/// One code of the table: its word on the wire, what the user is told, how it passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub wire: &'static str,
    pub code: Code,
    pub pass: Pass,
}

/// The token server's codes that are no variant of the contract's [`AzlinError`] or
/// [`CheckoutRefusal`] (those carry their own status and transience, read in [`meaning`]).
pub const TOKEN_ROWS: &[Row] = &[
    // azlin_contract::RATE_LIMITED: refused before its route ran.
    Row {
        wire: RATE_LIMITED,
        code: Code::RateLimited,
        pass: Pass::Transient(Spent::Nothing),
    },
    // RefreshRefusal::NotVerified, ArchiveRefusal::NotVerified: nothing was spent (503).
    Row {
        wire: "not_verified",
        code: Code::Unavailable,
        pass: Pass::Transient(Spent::Nothing),
    },
    // RefreshRefusal::TryAgain, IssueRefusal::TryAgain, RedeemRefusal::TryAgain: the same
    // request met another at the same moment; nothing was spent (503).
    Row {
        wire: "try_again",
        code: Code::Unavailable,
        pass: Pass::Transient(Spent::Nothing),
    },
    // RefreshRefusal::Unavailable: the token server could not finish (503); spent unless its
    // body says `"spent": false`.
    Row {
        wire: "unavailable",
        code: Code::Unavailable,
        pass: Pass::Transient(Spent::Maybe),
    },
    // azlin_contract::ALREADY_ANSWERED: a grant asked again whose first answer was used (409).
    Row {
        wire: ALREADY_ANSWERED,
        code: Code::AlreadyAnswered,
        pass: Pass::Final,
    },
    // azlin_contract::IDEMPOTENCY_REQUIRED: a grant sent without its key (400) - this app's
    // mistake, said as what went wrong.
    Row {
        wire: IDEMPOTENCY_REQUIRED,
        code: Code::Other,
        pass: Pass::Final,
    },
];

/// The S3 codes of a node's answer without `x-azlin-error` that mean more than their status.
pub const S3_ROWS: &[Row] = &[
    // A request signed with a clock too far from the node's (S3's 15 minutes).
    Row {
        wire: "RequestTimeTooSkewed",
        code: Code::ClockSkew,
        pass: Pass::Final,
    },
];

/// The shortest pause after a 429 that names none: a client asked to slow down never asks again
/// at once.
pub const MIN_RATE_PAUSE: Duration = Duration::from_secs(1);

/// What a node's code means: the contract's transient codes refused before the work (another
/// node at once, or after the pause they name); a request that failed inside the node (a panic
/// answers 500 `InternalError`) may have done something and goes to another node first.
fn node_code(code: AzlinError) -> (Code, Pass) {
    let pass = if code.is_transient() {
        Pass::Transient(Spent::Nothing)
    } else if code == AzlinError::Internal {
        Pass::Transient(Spent::Maybe)
    } else {
        Pass::Final
    };
    (Code::from(code), pass)
}

/// What a refused order means: the contract's transient refusals that the token server answers
/// with a 5xx did nothing (the provider or the price ladder was missing for a moment) and are
/// asked again at once; a 4xx one (a tier sold out for now) waits for the buyer.
fn checkout_code(refusal: &CheckoutRefusal) -> (Code, Pass) {
    let pass = if refusal.is_transient() && refusal.status() >= 500 {
        Pass::Transient(Spent::Nothing)
    } else {
        Pass::Final
    };
    (Code::from(refusal), pass)
}

/// What the code `wire` means when `from` sent it; `None` for one this build does not know.
#[must_use]
pub fn meaning(from: Service, wire: &str) -> Option<(Code, Pass)> {
    let wire = wire.trim();
    if wire.is_empty() {
        return None;
    }
    let rows = match from {
        Service::TokenServer => TOKEN_ROWS,
        Service::Node => S3_ROWS,
    };
    if let Some(row) = rows.iter().find(|row| row.wire == wire) {
        return Some((row.code, row.pass));
    }
    if let Some(code) = AzlinError::parse(wire) {
        return Some(node_code(code));
    }
    match from {
        Service::TokenServer => {
            CheckoutRefusal::parse(wire).map(|refusal| checkout_code(&refusal))
        }
        Service::Node => None,
    }
}

/// What an answer with no code this build knows means, by its status: a client too fast (429:
/// refused before the work), a timeout or a failure on the way (408, 5xx: maybe done), an app too
/// old (426); anything else does not pass by asking again.
#[must_use]
pub fn by_status(status: u16) -> (Code, Pass) {
    match status {
        429 => (Code::Unavailable, Pass::Transient(Spent::Nothing)),
        408 | 502..=504 => (Code::Unavailable, Pass::Transient(Spent::Maybe)),
        426 => (Code::UpdateApp, Pass::Final),
        // Not implemented, not this HTTP version, no room: the same request meets the same.
        501 | 505 | 507 => (Code::Other, Pass::Final),
        500..=599 => (Code::Other, Pass::Transient(Spent::Maybe)),
        _ => (Code::Other, Pass::Final),
    }
}

/// The code and the pass of a refusal `from` sent with `status` and the code `wire` (`None`: it
/// named none): its row, else its status.
fn meaning_or_status(from: Service, status: u16, wire: Option<&str>) -> (Code, Pass) {
    wire.and_then(|wire| meaning(from, wire))
        .unwrap_or_else(|| by_status(status))
}

/// A transient outcome of `from`'s answer with `status`: the pause it named (a 429, and a rate
/// limit, never less than [`MIN_RATE_PAUSE`]), and whether another node may answer at once - a
/// node's trouble is its own (busy, draining, failed inside), while a drive whose deletes are
/// paused, and a client asked to slow down, meet the same at every node.
fn transient(
    from: Service,
    status: u16,
    code: Code,
    spent: Spent,
    retry_after: Option<u64>,
) -> Outcome {
    let mut retry_after = retry_after.map(Duration::from_secs);
    if status == 429 || code == Code::RateLimited {
        retry_after = Some(retry_after.map_or(MIN_RATE_PAUSE, |pause| pause.max(MIN_RATE_PAUSE)));
    }
    Outcome::Transient {
        code,
        retry_after,
        spent,
        elsewhere: from == Service::Node && status != 429 && code != Code::DeletePaused,
    }
}

/// The outcome of a refusal `from` sent: its status, its code (`None`: it named none), the
/// seconds of its `Retry-After`, what its body says it spent (believed over the code's rule).
#[must_use]
pub fn classify(
    from: Service,
    status: u16,
    code: Option<&str>,
    retry_after: Option<u64>,
    said_spent: Option<bool>,
) -> Outcome {
    match meaning_or_status(from, status, code) {
        (code, Pass::Final) => Outcome::Refused(code),
        (code, Pass::Transient(spent)) => {
            let spent = match said_spent {
                Some(false) => Spent::Nothing,
                Some(true) => Spent::Maybe,
                None => spent,
            };
            transient(from, status, code, spent, retry_after)
        }
    }
}

/// The seconds of an answer's `Retry-After`, when it names a number.
fn retry_after_of(reply: &HttpReply) -> Option<u64> {
    reply
        .header("retry-after")
        .and_then(|secs| secs.trim().parse::<u64>().ok())
}

/// What the token server's answer `reply` means.
#[must_use]
pub fn of_token_reply(reply: &HttpReply) -> Outcome {
    if reply.is_success() {
        return Outcome::Done;
    }
    let body: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
    classify(
        Service::TokenServer,
        reply.status,
        body["error"].as_str(),
        retry_after_of(reply),
        body["spent"].as_bool(),
    )
}

/// What a storage node's answer `reply` means for the failover: a success, and a refusal below
/// 500 without an Azlin code (a 304, 404, 412, a plain S3 redirect, a HEAD's bodyless 403), are
/// the caller's to read.
#[must_use]
pub fn of_node_reply(reply: &HttpReply) -> Outcome {
    if reply.is_success() {
        return Outcome::Done;
    }
    let code = reply
        .header("x-azlin-error")
        .map(str::trim)
        .filter(|code| !code.is_empty());
    if code.is_none() && reply.status < 500 && !matches!(reply.status, 408 | 429) {
        return Outcome::Done;
    }
    classify(Service::Node, reply.status, code, retry_after_of(reply), None)
}

/// What a refusal a drive read into a [`ServiceError`] means: its Azlin code, else its code (an
/// S3 code, or the token server's code a refresh's refusal carries), else its status.
#[must_use]
pub fn of_service(service: &ServiceError) -> Outcome {
    let known = service
        .azlin_error
        .filter(|code| *code != AzlinError::Unknown)
        .map(node_code)
        .or_else(|| meaning(Service::Node, &service.code))
        .or_else(|| meaning(Service::TokenServer, &service.code));
    match known.unwrap_or_else(|| by_status(service.status)) {
        (code, Pass::Final) => Outcome::Refused(code),
        (code, Pass::Transient(spent)) => {
            transient(Service::Node, service.status, code, spent, service.retry_after)
        }
    }
}

/// No answer at all (DNS, a connection refused or cut, a timeout): it may have arrived, and
/// another node may answer.
#[must_use]
pub fn no_answer() -> Outcome {
    Outcome::Transient {
        code: Code::Network,
        retry_after: None,
        spent: Spent::Maybe,
        elsewhere: true,
    }
}

/// What a drive's error means; [`Outcome::Done`] for one the user caused or chose (no such file,
/// a name that cannot be one, a local disk), which is no answer of a service.
#[must_use]
pub fn of_drive_error(error: &DriveError) -> Outcome {
    match error {
        DriveError::Service(service) => of_service(service),
        DriveError::Transport(_) => no_answer(),
        DriveError::Denied { .. } => Outcome::Refused(Code::SignIn),
        DriveError::Locked { .. } => Outcome::Refused(Code::Locked),
        DriveError::KeyRefused { .. } => Outcome::Refused(Code::KeyRefused),
        DriveError::Conflict { .. } => Outcome::Refused(Code::NameConflict),
        DriveError::Corrupt { .. } => Outcome::Refused(Code::Integrity),
        DriveError::Protocol(_) => Outcome::Refused(Code::Other),
        DriveError::NotFound { .. }
        | DriveError::InvalidKey { .. }
        | DriveError::InvalidRange { .. }
        | DriveError::Io(_)
        | DriveError::Unsupported(_)
        | DriveError::InvalidConfig(_) => Outcome::Done,
    }
}

/// What a token server's error means. An answer cut off or unreadable may have been a grant's
/// first answer: it passes like no answer, and its key asks again.
#[must_use]
pub fn of_token_error(error: &TokenError) -> Outcome {
    match error {
        TokenError::Connect(_) => no_answer(),
        TokenError::Protocol(_) => Outcome::Transient {
            code: Code::Other,
            retry_after: None,
            spent: Spent::Maybe,
            elsewhere: false,
        },
        TokenError::SignIn(_) => Outcome::Refused(Code::CredentialsRevoked),
        TokenError::Config(_) => Outcome::Refused(Code::Other),
        TokenError::Refused { status, code, .. } => {
            classify(Service::TokenServer, *status, Some(code.as_str()), None, None)
        }
        TokenError::Busy {
            status,
            code,
            retry_after,
            spent,
            ..
        } => {
            let (code, _) = meaning_or_status(Service::TokenServer, *status, Some(code.as_str()));
            transient(Service::TokenServer, *status, code, *spent, *retry_after)
        }
    }
}
