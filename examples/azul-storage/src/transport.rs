//! How a request leaves the process: the one seam between the S3 client (which
//! builds and signs its requests, and reads the answers) and the network.
//!
//! The apps send through `AzulTransport` (azul's `HttpRequestConfig`, feature
//! `azul`); the tests through a fake that records the calls.
//!
//! Every call says what it may take and what its answer may weigh ([`CallLimits`]): the
//! caller knows its body and what it reads back, a shared transport does not. An upload's time
//! grows with its body, so a big part over a slow uplink gets the time it needs; a call that
//! reads a document (a listing, a token server's JSON) refuses an answer of any size beyond
//! its cap, so a hostile server cannot fill the memory with one.

use std::{fmt, time::Duration};

use crate::ByteRange;

/// What a document answer (JSON, an S3 error or a small XML answer) may weigh: 1 MiB.
pub const JSON_MAX: u64 = 1024 * 1024;
/// What a listing (ListObjectsV2, ListParts: up to 1000 entries) may weigh: 16 MiB.
pub const LISTING_MAX: u64 = 16 * 1024 * 1024;
/// What any answer may weigh at least: an error document comes back for a ranged read of a
/// few bytes too.
pub const ANSWER_FLOOR: u64 = JSON_MAX;
/// The time a call without a body to move may take (today's one minute).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
/// The time any transfer gets besides its bytes: connecting, TLS, the server's answer.
pub const TRANSFER_BASE: Duration = Duration::from_secs(30);
/// The slowest link a transfer is given time for: 32 KiB/s (about 256 kbit/s).
pub const TRANSFER_MIN_RATE: BytesPerSec = BytesPerSec(32 * 1024);

/// A rate in bytes per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BytesPerSec(pub u64);

impl BytesPerSec {
    /// How long `bytes` take at this rate (a zero rate is taken as one byte a second).
    #[must_use]
    pub fn time_for(self, bytes: u64) -> Duration {
        let rate = self.0.max(1);
        let nanos = u128::from(bytes % rate) * 1_000_000_000 / u128::from(rate);
        Duration::from_secs(bytes / rate) + Duration::from_nanos(nanos as u64)
    }
}

/// How long a call may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestTimeout {
    /// The whole call, whatever its size: for calls that move few bytes.
    Overall(Duration),
    /// `base` plus the time the call's bytes take at `min_rate`: the request body to send and
    /// the answer it expects ([`ExpectedBody`]), each on its own.
    Throughput {
        base: Duration,
        min_rate: BytesPerSec,
    },
}

/// What the answer of a call may weigh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedBody {
    /// A document (JSON, XML) of at most `max` bytes.
    Json { max: u64 },
    /// The bytes of a range (a server that ignores `Range` and sends a small object whole still
    /// fits: every answer may weigh [`ANSWER_FLOOR`]).
    Range(ByteRange),
    /// A stream of at most `max` bytes (`u64::MAX`: an object read whole, whatever its size).
    Stream { max: u64 },
}

impl ExpectedBody {
    /// The most bytes the answer may have.
    #[must_use]
    pub fn max_bytes(&self) -> u64 {
        match self {
            ExpectedBody::Json { max } | ExpectedBody::Stream { max } => *max,
            ExpectedBody::Range(range) => range_len(range).unwrap_or(u64::MAX).max(ANSWER_FLOOR),
        }
    }

    /// The bytes the answer's time is reckoned for: a document's cap, a range's length (a
    /// stream's size is not known, so it gets no time of its own).
    fn timed_bytes(&self) -> u64 {
        match self {
            ExpectedBody::Json { max } => *max,
            ExpectedBody::Range(range) => range_len(range).unwrap_or(0),
            ExpectedBody::Stream { .. } => 0,
        }
    }
}

/// The bytes of `range`; `None` for one that runs to the end.
fn range_len(range: &ByteRange) -> Option<u64> {
    range
        .end
        .map(|end| end.saturating_sub(range.start).saturating_add(1))
}

/// What a call may take and what its answer may weigh, set by the code that builds the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallLimits {
    pub timeout: RequestTimeout,
    pub answer: ExpectedBody,
}

/// The limits of one call as a transport applies them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferBudget {
    /// The whole call.
    pub overall: Duration,
    /// Sending the request body; `None`: only the overall time applies.
    pub send_body: Option<Duration>,
    /// Receiving the answer's body; `None`: only the overall time applies.
    pub recv_body: Option<Duration>,
    /// The most bytes the answer may have; a bigger one is refused, never cut short.
    pub max_answer: u64,
}

impl CallLimits {
    /// A call that reads a document: a minute, an answer of at most [`JSON_MAX`] (a token
    /// server, an OAuth token endpoint, an S3 HEAD / DELETE / copy).
    #[must_use]
    pub const fn json() -> Self {
        CallLimits {
            timeout: RequestTimeout::Overall(DEFAULT_TIMEOUT),
            answer: ExpectedBody::Json { max: JSON_MAX },
        }
    }

    /// A listing: a minute, an answer of at most [`LISTING_MAX`].
    #[must_use]
    pub const fn listing() -> Self {
        CallLimits {
            timeout: RequestTimeout::Overall(DEFAULT_TIMEOUT),
            answer: ExpectedBody::Json { max: LISTING_MAX },
        }
    }

    /// A call with a body to move (a PUT, an upload's part): its time grows with the body, its
    /// answer is a document.
    #[must_use]
    pub const fn upload() -> Self {
        CallLimits {
            timeout: RequestTimeout::Throughput {
                base: TRANSFER_BASE,
                min_rate: TRANSFER_MIN_RATE,
            },
            answer: ExpectedBody::Json { max: JSON_MAX },
        }
    }

    /// A ranged read: its time and its answer follow the range. A range to the end has no
    /// known length and reads like a whole object.
    #[must_use]
    pub const fn range(range: ByteRange) -> Self {
        match range.end {
            Some(_) => CallLimits {
                timeout: RequestTimeout::Throughput {
                    base: TRANSFER_BASE,
                    min_rate: TRANSFER_MIN_RATE,
                },
                answer: ExpectedBody::Range(range),
            },
            None => Self::whole_object(),
        }
    }

    /// An object read whole: a minute, as big as it is. Big objects are read in ranges
    /// ([`crate::transfer`]); this is for the small ones.
    #[must_use]
    pub const fn whole_object() -> Self {
        CallLimits {
            timeout: RequestTimeout::Overall(DEFAULT_TIMEOUT),
            answer: ExpectedBody::Stream { max: u64::MAX },
        }
    }

    /// The limits of a call the caller knows nothing more about than its verb and body: an
    /// upload's with a body, an object's read whole for a GET, a document's otherwise.
    #[must_use]
    pub fn for_request(method: Method, body_len: usize) -> Self {
        if body_len > 0 {
            Self::upload()
        } else if method == Method::Get {
            Self::whole_object()
        } else {
            Self::json()
        }
    }

    /// The limits of a call whose request body has `body_len` bytes.
    #[must_use]
    pub fn budget(&self, body_len: u64) -> TransferBudget {
        let max_answer = self.answer.max_bytes();
        match self.timeout {
            RequestTimeout::Overall(overall) => TransferBudget {
                overall,
                send_body: None,
                recv_body: None,
                max_answer,
            },
            RequestTimeout::Throughput { base, min_rate } => {
                let send = base + min_rate.time_for(body_len);
                let recv = base + min_rate.time_for(self.answer.timed_bytes());
                TransferBudget {
                    overall: send + recv,
                    send_body: Some(send),
                    recv_body: Some(recv),
                    max_answer,
                }
            }
        }
    }
}

/// The HTTP verbs the drives use: the S3 client's five, PATCH (the REST APIs of the consumer
/// clouds OpenDAL reaches) and WebDAV's (OpenDAL's WebDAV service).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Head,
    Put,
    Post,
    Delete,
    Patch,
    Options,
    /// WebDAV: a folder's listing, an item's properties.
    Propfind,
    /// WebDAV: set an item's properties.
    Proppatch,
    /// WebDAV: a new folder.
    Mkcol,
    /// WebDAV: a server-side copy.
    Copy,
    /// WebDAV: a server-side move.
    Move,
}

impl Method {
    /// Every verb, for [`Method::parse`].
    pub const ALL: [Method; 12] = [
        Method::Get,
        Method::Head,
        Method::Put,
        Method::Post,
        Method::Delete,
        Method::Patch,
        Method::Options,
        Method::Propfind,
        Method::Proppatch,
        Method::Mkcol,
        Method::Copy,
        Method::Move,
    ];

    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Put => "PUT",
            Method::Post => "POST",
            Method::Delete => "DELETE",
            Method::Patch => "PATCH",
            Method::Options => "OPTIONS",
            Method::Propfind => "PROPFIND",
            Method::Proppatch => "PROPPATCH",
            Method::Mkcol => "MKCOL",
            Method::Copy => "COPY",
            Method::Move => "MOVE",
        }
    }

    /// The verb of a wire name (any case); `None` for one the drives never send.
    #[must_use]
    pub fn parse(name: &str) -> Option<Method> {
        Method::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(name.trim()))
    }
}

/// One request, ready to send: the URL is final (encoded, query sorted) and the
/// headers carry the signature. `Host` is not among them: the HTTP client sets it
/// from the URL, and it was signed as such.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpCall {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The `Content-Type` of the body (not signed); empty without a body.
    pub content_type: String,
    /// What the call may take and what its answer may weigh; the transport applies them.
    pub limits: CallLimits,
}

impl HttpCall {
    /// The limits of this call, for its body.
    #[must_use]
    pub fn budget(&self) -> TransferBudget {
        self.limits.budget(self.body.len() as u64)
    }
}

/// Header values that must not show up in logs or `Debug` output.
const HIDDEN_HEADERS: &[&str] = &["authorization", "x-amz-security-token"];

impl fmt::Debug for HttpCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(name, value)| {
                let hidden = HIDDEN_HEADERS.iter().any(|h| name.eq_ignore_ascii_case(h));
                (
                    name.as_str(),
                    if hidden { "<hidden>" } else { value.as_str() },
                )
            })
            .collect();
        f.debug_struct("HttpCall")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("body_len", &self.body.len())
            .field("content_type", &self.content_type)
            .field("limits", &self.limits)
            .finish()
    }
}

/// The answer: any status (4xx / 5xx included, with their bodies).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpReply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpReply {
    /// The first header of this name, compared without case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 2xx.
    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// What a [`Transport`] puts in front of the reason of a request whose host name did not
/// resolve (the lookup failed or gave up): the failover reads it ([`is_dns_failure`]) and
/// reaches the host at its known addresses. azul's HTTP client says so by type
/// (`HttpError::DnsFailed`); the transports turn that into this sentence.
pub const DNS_FAILED: &str = "the name did not resolve";

/// Whether a transport's reason says the host name did not resolve: [`DNS_FAILED`], or what the
/// system resolvers and HTTP clients say without it.
#[must_use]
pub fn is_dns_failure(why: &str) -> bool {
    let why = why.to_ascii_lowercase();
    [
        DNS_FAILED,
        "dns",
        "resolve",
        "host not found",
        "lookup",
        "name or service not known",
        "nodename nor servname",
        "no such host",
    ]
    .iter()
    .any(|sign| why.contains(sign))
}

/// Sends one request and blocks until its answer. An `Err` is a request that got
/// no HTTP answer at all (DNS, connection, TLS, timeout), as a readable sentence; one whose
/// name did not resolve starts with [`DNS_FAILED`]. A network transport applies the call's
/// [`CallLimits`] ([`HttpCall::budget`]): its timeouts, and an answer beyond its cap is an
/// `Err`, never cut short.
pub trait Transport: Send + Sync {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String>;

    /// Connects to `host` at `addresses` (IP addresses, `ip` or `ip:port`) whenever its name
    /// does not resolve - the request still goes to `host`, and its TLS certificate is
    /// verified for that name: the last layer of a drive's failover, when DNS is down but the
    /// node's addresses are known. `false` when this transport cannot (the default) or no
    /// address was usable.
    fn fallback_addresses(&self, host: &str, addresses: &[String]) -> bool {
        let _ = (host, addresses);
        false
    }
}
