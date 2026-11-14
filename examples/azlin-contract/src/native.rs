//! The protocol `azlin/1` over QUIC, as tables (planning/architecture.md 2.5): the verbs and their numbers,
//! the status codes and their classes, the limits, the flag bits and the frame layouts.
//!
//! Build unit S5, "Contract 4 first": these are the plan's own tables as constants, so that the node (S6),
//! the client library, the apps and the tests all read one thing. azlin-proto's `contract4` tests parse
//! `planning/architecture.md` and fail when a row of its tables and a constant here differ; the golden
//! tables in `fixtures/c4/native_protocol.json` are written from these constants.
//!
//! The plan lists the verbs "per range" and the statuses by name; the numbers follow two rules that the drift
//! register records (X-DELTA-W1S5-1 and -2) and the doc tests hold to the plan's tables:
//!
//! - a **verb number** is `(range << 8) | position`, the position counted from 1 in the range's row of the
//!   verb table (0 is never a verb, so an all-zero frame is no verb);
//! - a **status number** is the status's position in the status table, counted from 0 (`ok`) in reading order.
//!
//! Both are append-only: a row added in the middle of a table renumbers the rows after it, and the doc tests
//! fail until the numbers are renumbered on purpose.

// ---- verbs --------------------------------------------------------------------------------------

/// The range of a verb: the high byte of its number (2.5 "Verbs").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum VerbRange {
    Session = 0x00,
    Data = 0x01,
    Space = 0x02,
    Sharing = 0x03,
    Team = 0x04,
    Durability = 0x05,
    Region = 0x07,
}

/// 0x06 is reserved: no verb has it, and a frame with it answers `unsupported_verb`.
pub const RESERVED_RANGE: u8 = 0x06;

impl VerbRange {
    pub const ALL: &'static [VerbRange] = &[
        VerbRange::Session,
        VerbRange::Data,
        VerbRange::Space,
        VerbRange::Sharing,
        VerbRange::Team,
        VerbRange::Durability,
        VerbRange::Region,
    ];

    #[must_use]
    pub const fn number(self) -> u8 {
        self as u8
    }

    /// The word the plan's table gives the range (`0x01 data`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            VerbRange::Session => "session",
            VerbRange::Data => "data",
            VerbRange::Space => "space",
            VerbRange::Sharing => "sharing",
            VerbRange::Team => "team",
            VerbRange::Durability => "durability",
            VerbRange::Region => "region",
        }
    }
}

macro_rules! verbs {
    ($($variant:ident = $num:literal, $name:literal, $range:ident;)+) => {
        /// A verb of `azlin/1`: the only verb table (2.5). A number is its range and its position in the
        /// range's row, so a number is part of the contract.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum Verb {
            $($variant = $num,)+
        }

        impl Verb {
            /// Every verb, in the plan's table order.
            pub const ALL: &'static [Verb] = &[$(Verb::$variant,)+];

            #[must_use]
            pub const fn number(self) -> u16 {
                self as u16
            }

            /// The name the plan writes in backticks.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Verb::$variant => $name,)+
                }
            }

            #[must_use]
            pub const fn range(self) -> VerbRange {
                match self {
                    $(Verb::$variant => VerbRange::$range,)+
                }
            }

            /// The verb with this number, if there is one: 0x06xx, position 0 and anything past a row's end
            /// are none (a node answers `unsupported_verb`).
            #[must_use]
            pub fn from_number(n: u16) -> Option<Verb> {
                match n {
                    $($num => Some(Verb::$variant),)+
                    _ => None,
                }
            }

            #[must_use]
            pub fn from_name(name: &str) -> Option<Verb> {
                match name {
                    $($name => Some(Verb::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

verbs! {
    Hello = 0x0001, "hello", Session;
    Ping = 0x0002, "ping", Session;
    Map = 0x0003, "map", Session;
    Prices = 0x0004, "prices", Session;
    Refresh = 0x0005, "refresh", Session;
    Watch = 0x0006, "watch", Session;
    Lockdown = 0x0007, "lockdown", Session;
    DeviceEnrollBegin = 0x0008, "device_enroll_begin", Session;
    DeviceEnrollFinish = 0x0009, "device_enroll_finish", Session;
    DeviceRemove = 0x000a, "device_remove", Session;
    MemberAssert = 0x000b, "member_assert", Session;
    PushRegister = 0x000c, "push_register", Session;
    PackPut = 0x0101, "pack_put", Data;
    ShardPut = 0x0102, "shard_put", Data;
    ShardForward = 0x0103, "shard_forward", Data;
    PackCommit = 0x0104, "pack_commit", Data;
    ObjectGet = 0x0105, "object_get", Data;
    Locate = 0x0106, "locate", Data;
    Delete = 0x0107, "delete", Data;
    Cancel = 0x0108, "cancel", Data;
    HeadGet = 0x0201, "head_get", Space;
    HeadCas = 0x0202, "head_cas", Space;
    HeadHistory = 0x0203, "head_history", Space;
    RestoreAsOf = 0x0204, "restore_as_of", Space;
    SpaceKeysGet = 0x0205, "space_keys_get", Space;
    SpaceCreate = 0x0206, "space_create", Space;
    SpaceRekey = 0x0207, "space_rekey", Space;
    LogAppend = 0x0208, "log_append", Space;
    LogRead = 0x0209, "log_read", Space;
    RetentionSet = 0x020a, "retention_set", Space;
    LinkCreate = 0x0301, "link_create", Sharing;
    LinkUpdate = 0x0302, "link_update", Sharing;
    LinkRevoke = 0x0303, "link_revoke", Sharing;
    LinkList = 0x0304, "link_list", Sharing;
    LinkResolve = 0x0305, "link_resolve", Sharing;
    LinkLogin = 0x0306, "link_login", Sharing;
    GrantAdd = 0x0307, "grant_add", Sharing;
    GrantRemove = 0x0308, "grant_remove", Sharing;
    RequestCreate = 0x0309, "request_create", Sharing;
    RequestUpload = 0x030a, "request_upload", Sharing;
    RequestRevoke = 0x030b, "request_revoke", Sharing;
    InboxList = 0x030c, "inbox_list", Sharing;
    TeamLogAppend = 0x0401, "team_log_append", Team;
    TeamLogGet = 0x0402, "team_log_get", Team;
    EntitlementPut = 0x0403, "entitlement_put", Team;
    AuditConfig = 0x0404, "audit_config", Team;
    Verify = 0x0501, "verify", Durability;
    LockSet = 0x0502, "lock_set", Durability;
    LockGet = 0x0503, "lock_get", Durability;
    LegalHold = 0x0504, "legal_hold", Durability;
    ReplicateStream = 0x0701, "replicate_stream", Region;
    ReplicaStatus = 0x0702, "replica_status", Region;
    SpacePromote = 0x0703, "space_promote", Region;
}

/// The verbs whose write the client must find out about before it repeats them: a lost connection or a
/// timeout is not `elsewhere` for them (2.5: "for the others (`head_cas`, `log_append`,
/// `team_log_append`) the client first reads whether the write landed").
pub const READ_BEFORE_REPEAT: &[Verb] = &[Verb::HeadCas, Verb::LogAppend, Verb::TeamLogAppend];

/// The verbs that may carry the 0-RTT flag: a shard is named by (pack id, position) and verified by its
/// BLAKE3 hash, so a repeated write is harmless (2.5 "Idempotent by construction").
pub const ZERO_RTT_VERBS: &[Verb] = &[Verb::ShardPut];

impl Verb {
    /// Whether a lost connection on this verb may simply be repeated elsewhere without first reading
    /// whether the write landed (2.5).
    #[must_use]
    pub fn repeats_without_reading(self) -> bool {
        !READ_BEFORE_REPEAT.contains(&self)
    }

    /// Whether the 0-RTT flag is allowed on this verb.
    #[must_use]
    pub fn allows_zero_rtt(self) -> bool {
        ZERO_RTT_VERBS.contains(&self)
    }
}

// ---- statuses -----------------------------------------------------------------------------------

text_enum! {
    /// What a client does with an answer (2.5 "Status codes", 1.4 rule 10).
    pub enum StatusClass {
        /// Show the problem, do not repeat.
        Fatal = "fatal",
        /// Repeat at once, after the fix the code names.
        Retry = "retry",
        /// Repeat on another node.
        Elsewhere = "elsewhere",
        /// Repeat after the delay the answer gives.
        After = "after",
    }
}

macro_rules! statuses {
    ($($variant:ident = $num:literal, $name:literal, $class:expr;)+) => {
        /// A status of a response: each has one class (2.5, 1.4 rule 10). The number is the status's
        /// position in the plan's table, from 0.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum Status {
            $($variant = $num,)+
        }

        impl Status {
            /// Every status, in the plan's table order.
            pub const ALL: &'static [Status] = &[$(Status::$variant,)+];

            #[must_use]
            pub const fn number(self) -> u16 {
                self as u16
            }

            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Status::$variant => $name,)+
                }
            }

            /// The class; none for `ok`.
            #[must_use]
            pub const fn class(self) -> Option<StatusClass> {
                match self {
                    $(Status::$variant => $class,)+
                }
            }

            #[must_use]
            pub fn from_number(n: u16) -> Option<Status> {
                match n {
                    $($num => Some(Status::$variant),)+
                    _ => None,
                }
            }

            #[must_use]
            pub fn from_name(name: &str) -> Option<Status> {
                match name {
                    $($name => Some(Status::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

statuses! {
    Ok = 0, "ok", None;
    Conflict = 1, "conflict", Some(StatusClass::Retry);
    MapStale = 2, "map_stale", Some(StatusClass::Retry);
    Overloaded = 3, "overloaded", Some(StatusClass::Elsewhere);
    Lagging = 4, "lagging", Some(StatusClass::Elsewhere);
    RetryAfter = 5, "retry_after", Some(StatusClass::After);
    NotReady = 6, "not_ready", Some(StatusClass::After);
    NotFound = 7, "not_found", Some(StatusClass::Fatal);
    Forbidden = 8, "forbidden", Some(StatusClass::Fatal);
    UnsupportedVerb = 9, "unsupported_verb", Some(StatusClass::Fatal);
    BadRequest = 10, "bad_request", Some(StatusClass::Fatal);
    NotSealed = 11, "not_sealed", Some(StatusClass::Fatal);
    Revoked = 12, "revoked", Some(StatusClass::Fatal);
    QuotaExceeded = 13, "quota_exceeded", Some(StatusClass::Fatal);
    Locked = 14, "locked", Some(StatusClass::Fatal);
    ClientTooOld = 15, "client_too_old", Some(StatusClass::Fatal);
}

/// "Retries are bounded (five attempts **est.**, with jittered backoff); then the error surfaces" (2.5).
pub const RETRY_ATTEMPTS: u32 = 5;

// ---- framing ------------------------------------------------------------------------------------

/// "request: `u16 verb | u16 flags | u32 header length | header (deterministic CBOR) | body`" (2.5), in
/// big-endian: X-DELTA-W1S5-3 (the plan names no byte order).
pub const REQUEST_LAYOUT: &str =
    "u16 verb | u16 flags | u32 header length | header (deterministic CBOR) | body";
/// "response: `u16 status | u32 header length | header | body`" (2.5).
pub const RESPONSE_LAYOUT: &str = "u16 status | u32 header length | header | body";
/// The fixed part of a request: `u16 verb | u16 flags | u32 header length`.
pub const REQUEST_PREFIX_BYTES: usize = 8;
/// The fixed part of a response: `u16 status | u32 header length`.
pub const RESPONSE_PREFIX_BYTES: usize = 6;
/// "flag bits: idempotent, 0-RTT allowed" (2.5): bit 0.
pub const FLAG_IDEMPOTENT: u16 = 0x0001;
/// Bit 1.
pub const FLAG_ZERO_RTT: u16 = 0x0002;
/// Every other bit is reserved and a frame that sets one is `bad_request`.
pub const FLAGS_KNOWN: u16 = FLAG_IDEMPOTENT | FLAG_ZERO_RTT;
/// "Key 0 (`crit`) lists keys the receiver must understand, or refuse" (2.5): the key of `crit` in a header.
pub const KEY_CRIT: u64 = 0;

/// The features `hello` names (2.5: "`delta`, `teams`, `links.v2`, `lock`, `sso`, `watch.stream`, `p2p`,
/// ..."): an open list, so a client ignores a name it does not know.
pub const FEATURES_KNOWN: &[&str] = &[
    "delta",
    "teams",
    "links.v2",
    "lock",
    "sso",
    "watch.stream",
    "p2p",
];

// ---- limits -------------------------------------------------------------------------------------

/// What a limit's default is (2.5 "Limits": the plan's second column). Sizes are bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitValue {
    Count(u64),
    Bytes(u64),
    /// "the path's bandwidth-delay product, at most 64 MiB": the cap; the value follows the path.
    PathBound {
        max_bytes: u64,
    },
    /// "one per (space, kind)".
    OnePerSpaceKind,
}

/// The rows of the plan's limits table, by the plan's label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LimitId {
    ConnectionsPerNode,
    ConnectionsPerDevice,
    OpenStreamsPerConnection,
    BytesInFlightPerConnection,
    OpenUploadSessionsPerDevice,
    RequestHeader,
    LogAppendRecord,
    UncompactedLogBytesPerAccount,
    WatchStreamsPerNode,
    SpacesPerWatch,
    QueuedEventsPerWatcher,
}

const KIB: u64 = 1 << 10;
const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

/// connections per node: new handshakes get Retry, then `overloaded` (3.5).
pub const CONNECTIONS_PER_NODE: u64 = 100_000;
/// connections per device: `overloaded` for the extra one.
pub const CONNECTIONS_PER_DEVICE: u64 = 8;
/// open streams per connection: no new stream credit (QUIC flow control).
pub const OPEN_STREAMS_PER_CONNECTION: u64 = 256;
/// bytes in flight per connection: the path's bandwidth-delay product, at most this many bytes.
pub const BYTES_IN_FLIGHT_MAX_BYTES: u64 = 64 * MIB;
/// open upload sessions per device: `retry_after`.
pub const OPEN_UPLOAD_SESSIONS_PER_DEVICE: u64 = 64;
/// request header, in bytes: `bad_request`.
pub const REQUEST_HEADER_MAX_BYTES: u64 = 16 * KIB;
/// `log_append` record, in bytes: `bad_request`.
pub const LOG_APPEND_RECORD_MAX_BYTES: u64 = 64 * KIB;
/// uncompacted append-log bytes per account (they count against its quota): `quota_exceeded`.
pub const UNCOMPACTED_LOG_MAX_BYTES: u64 = GIB;
/// `watch` streams per node: `overloaded`; the device goes to its backup home node (5.13).
pub const WATCH_STREAMS_PER_NODE: u64 = 50_000;
/// spaces in one `watch` subscription: `bad_request`.
pub const SPACES_PER_WATCH: u64 = 20_000;

impl LimitId {
    pub const ALL: &'static [LimitId] = &[
        LimitId::ConnectionsPerNode,
        LimitId::ConnectionsPerDevice,
        LimitId::OpenStreamsPerConnection,
        LimitId::BytesInFlightPerConnection,
        LimitId::OpenUploadSessionsPerDevice,
        LimitId::RequestHeader,
        LimitId::LogAppendRecord,
        LimitId::UncompactedLogBytesPerAccount,
        LimitId::WatchStreamsPerNode,
        LimitId::SpacesPerWatch,
        LimitId::QueuedEventsPerWatcher,
    ];

    /// The label of the plan's table row, with the backticks left out.
    #[must_use]
    pub const fn doc_label(self) -> &'static str {
        match self {
            LimitId::ConnectionsPerNode => "connections per node",
            LimitId::ConnectionsPerDevice => "connections per device",
            LimitId::OpenStreamsPerConnection => "open streams per connection",
            LimitId::BytesInFlightPerConnection => "bytes in flight per connection",
            LimitId::OpenUploadSessionsPerDevice => "open upload sessions per device",
            LimitId::RequestHeader => "request header",
            LimitId::LogAppendRecord => "log_append record",
            LimitId::UncompactedLogBytesPerAccount => {
                "uncompacted append-log bytes per account (they count against its quota)"
            }
            LimitId::WatchStreamsPerNode => "watch streams per node",
            LimitId::SpacesPerWatch => "spaces in one watch subscription",
            LimitId::QueuedEventsPerWatcher => "queued events per watcher",
        }
    }

    /// The name `hello` gives the limit.
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            LimitId::ConnectionsPerNode => "connections_per_node",
            LimitId::ConnectionsPerDevice => "connections_per_device",
            LimitId::OpenStreamsPerConnection => "open_streams_per_connection",
            LimitId::BytesInFlightPerConnection => "bytes_in_flight_per_connection",
            LimitId::OpenUploadSessionsPerDevice => "open_upload_sessions_per_device",
            LimitId::RequestHeader => "request_header",
            LimitId::LogAppendRecord => "log_append_record",
            LimitId::UncompactedLogBytesPerAccount => "uncompacted_log_bytes_per_account",
            LimitId::WatchStreamsPerNode => "watch_streams_per_node",
            LimitId::SpacesPerWatch => "spaces_per_watch",
            LimitId::QueuedEventsPerWatcher => "queued_events_per_space_kind",
        }
    }

    /// The default of the plan's second column (**est.**: the values come from the signed `node_config`).
    #[must_use]
    pub const fn default_value(self) -> LimitValue {
        match self {
            LimitId::ConnectionsPerNode => LimitValue::Count(CONNECTIONS_PER_NODE),
            LimitId::ConnectionsPerDevice => LimitValue::Count(CONNECTIONS_PER_DEVICE),
            LimitId::OpenStreamsPerConnection => LimitValue::Count(OPEN_STREAMS_PER_CONNECTION),
            LimitId::BytesInFlightPerConnection => LimitValue::PathBound {
                max_bytes: BYTES_IN_FLIGHT_MAX_BYTES,
            },
            LimitId::OpenUploadSessionsPerDevice => {
                LimitValue::Count(OPEN_UPLOAD_SESSIONS_PER_DEVICE)
            }
            LimitId::RequestHeader => LimitValue::Bytes(REQUEST_HEADER_MAX_BYTES),
            LimitId::LogAppendRecord => LimitValue::Bytes(LOG_APPEND_RECORD_MAX_BYTES),
            LimitId::UncompactedLogBytesPerAccount => LimitValue::Bytes(UNCOMPACTED_LOG_MAX_BYTES),
            LimitId::WatchStreamsPerNode => LimitValue::Count(WATCH_STREAMS_PER_NODE),
            LimitId::SpacesPerWatch => LimitValue::Count(SPACES_PER_WATCH),
            LimitId::QueuedEventsPerWatcher => LimitValue::OnePerSpaceKind,
        }
    }

    /// The status a request that goes over the limit gets, when the plan's third column names one.
    #[must_use]
    pub const fn at_limit(self) -> Option<Status> {
        match self {
            LimitId::ConnectionsPerNode
            | LimitId::ConnectionsPerDevice
            | LimitId::WatchStreamsPerNode => Some(Status::Overloaded),
            LimitId::OpenStreamsPerConnection
            | LimitId::BytesInFlightPerConnection
            | LimitId::QueuedEventsPerWatcher => None,
            LimitId::OpenUploadSessionsPerDevice => Some(Status::RetryAfter),
            LimitId::RequestHeader | LimitId::LogAppendRecord | LimitId::SpacesPerWatch => {
                Some(Status::BadRequest)
            }
            LimitId::UncompactedLogBytesPerAccount => Some(Status::QuotaExceeded),
        }
    }
}
