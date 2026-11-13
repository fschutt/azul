//! The bridge's limits: how much any one client may send or hold open. Every server reads
//! them from one [`Limits`]; the defaults suit one person's mail programs on one computer and
//! stop a misbehaving (or hostile) local process long before memory runs out.

use std::time::Duration;

/// Every limit of the three servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Connections one server holds at once; one more is answered "busy" and closed.
    pub max_connections: usize,
    /// An IMAP or SMTP command line, without its literals (RFC 7162 asks for at least 8192).
    pub line_bytes: usize,
    /// An IMAP literal in any command but APPEND (a mailbox name, a search string, a password).
    pub literal_bytes: usize,
    /// One IMAP command with all its literals (APPEND's message aside).
    pub command_bytes: usize,
    /// A message: IMAP APPEND's literal, SMTP's DATA.
    pub message_bytes: usize,
    /// Recipients of one SMTP mail.
    pub recipients: usize,
    /// An XML body of WebDAV (PROPFIND, PROPPATCH, LOCK).
    pub xml_bytes: usize,
    /// A file a WebDAV PUT writes (held in memory until the drive takes it).
    pub put_bytes: u64,
    /// An HTTP request's header block.
    pub header_bytes: usize,
    /// Header lines of one HTTP request.
    pub header_count: usize,
    /// Commands (IMAP, SMTP) or requests (HTTP) per second, on average, per connection...
    pub commands_per_second: u32,
    /// ...with this many at once before the rate applies.
    pub command_burst: u32,
    /// Wrong passwords on one connection before it is closed.
    pub auth_failures_per_connection: u32,
    /// Wrong passwords across the bridge within [`Limits::auth_window`] before every further
    /// sign-in waits [`Limits::auth_delay`] first.
    pub auth_failures_free: u32,
    pub auth_window: Duration,
    pub auth_delay: Duration,
    /// An IMAP connection that says nothing for this long is closed (RFC 3501: at least 30
    /// minutes; IDLE renews it).
    pub imap_idle: Duration,
    /// The same for SMTP (RFC 5321's server timeout: 5 minutes).
    pub smtp_idle: Duration,
    /// The same for a kept-alive HTTP connection between requests.
    pub http_idle: Duration,
    /// How often an IMAP IDLE looks at the drive for news.
    pub idle_poll: Duration,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_connections: 32,
            line_bytes: 64 * 1024,
            literal_bytes: 64 * 1024,
            command_bytes: 256 * 1024,
            message_bytes: 64 * 1024 * 1024,
            recipients: 200,
            xml_bytes: 1024 * 1024,
            put_bytes: 1024 * 1024 * 1024,
            header_bytes: 64 * 1024,
            header_count: 100,
            commands_per_second: 200,
            command_burst: 2_000,
            auth_failures_per_connection: 3,
            auth_failures_free: 10,
            auth_window: Duration::from_secs(60),
            auth_delay: Duration::from_secs(2),
            imap_idle: Duration::from_secs(31 * 60),
            smtp_idle: Duration::from_secs(5 * 60),
            http_idle: Duration::from_secs(120),
            idle_poll: Duration::from_secs(30),
        }
    }
}
