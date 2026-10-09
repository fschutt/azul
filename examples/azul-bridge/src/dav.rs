//! WebDAV (RFC 4918) class 1, and the minimal class 2 (LOCK / UNLOCK) that macOS Finder and
//! Windows Explorer need to write, over an azul-storage [`Drive`]: the bucket today, the
//! encrypted drive later - the server does not change.
//!
//! - OPTIONS, PROPFIND (Depth 0 and 1; `infinity` is refused as RFC 4918 allows) with the
//!   common properties (resourcetype, displayname, getcontentlength, getlastmodified,
//!   creationdate, getetag, getcontenttype, supportedlock, lockdiscovery; any other is listed
//!   as not found), GET / HEAD with one byte range, PUT (Content-Length or chunked), DELETE,
//!   MKCOL, COPY and MOVE (Destination, Overwrite, Depth), PROPPATCH (answered as done: dead
//!   properties are not kept, Windows only sets its timestamps), LOCK (exclusive and shared
//!   write locks, refresh, lock-null files) and UNLOCK, a lock enforced through the `If` header.
//! - Security: the request must name this computer (`Host` 127.0.0.1 / localhost / [::1] and
//!   the bridge's port: a DNS-rebinding page names its own host), must not come from a browser
//!   page (`Origin` present), signs in with HTTP Basic (the one user and password, compared in
//!   constant time); a path is percent-decoded and then refused when it has `..`, `.`, an empty
//!   level, a backslash, NUL or a control character; PROPFIND / PROPPATCH / LOCK bodies are
//!   limited in size and refused when they carry a DTD (no entity of any kind is expanded);
//!   the sync's bookkeeping (`.azlin/`) is neither listed nor reachable.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use azul_storage::{ops, ByteRange, Drive, DriveError, ObjectInfo};

use crate::{
    auth::{self, Credentials, FailureGate},
    dates,
    http::{self, Framing, Head, HttpError, Response, Status},
    limits::Limits,
    net::{self, Conn, Input, RateLimiter},
};

/// The folder the Azlin sync keeps its bookkeeping in: hidden.
pub const HIDDEN: &str = ".azlin";
/// A lock lasts at most this long without a refresh.
pub const MAX_LOCK_SECS: u64 = 3600;
/// ...and this long when the client asks for none.
pub const DEFAULT_LOCK_SECS: u64 = 600;

const DAV: &str = "DAV:";

/// A write lock.
#[derive(Debug, Clone)]
struct Lock {
    /// The locked key (a folder's ends in `/`).
    key: String,
    token: String,
    owner: String,
    exclusive: bool,
    infinite: bool,
    seconds: u64,
    expires: Instant,
}

impl Lock {
    /// Whether the lock covers `key`: itself, under a folder locked with depth infinity, or a
    /// folder above it (deleting or moving that folder touches the locked thing).
    fn covers(&self, key: &str) -> bool {
        if self.key == key || self.key.trim_end_matches('/') == key.trim_end_matches('/') {
            return true;
        }
        let folder = if key.is_empty() || key.ends_with('/') {
            key.to_string()
        } else {
            format!("{key}/")
        };
        (self.infinite && self.key.ends_with('/') && key.starts_with(&self.key))
            || self.key.starts_with(&folder)
    }
}

/// What a request's path names.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Resource {
    File(ObjectInfo),
    Folder,
    Missing,
}

/// The WebDAV server: what every connection shares.
pub struct Dav {
    drive: Arc<dyn Drive>,
    credentials: Credentials,
    gate: Arc<FailureGate>,
    limits: Limits,
    /// The port the bridge listens on (a request must name it); 0: any.
    port: u16,
    locks: Mutex<Vec<Lock>>,
}

impl std::fmt::Debug for Dav {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dav")
            .field("credentials", &self.credentials)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

/// `&`, `<`, `>`, `"` and `'` as XML writes them.
#[must_use]
pub fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// One path level percent-encoded (everything but RFC 3986's unreserved characters).
#[must_use]
pub fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for b in segment.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The href of `key` (`/a%20b/c.txt`, a folder's with its final `/`).
#[must_use]
pub fn href_of(key: &str, folder: bool) -> String {
    let mut out = String::from("/");
    let path: Vec<String> = key
        .split('/')
        .filter(|s| !s.is_empty())
        .map(encode_segment)
        .collect();
    out.push_str(&path.join("/"));
    if folder && !path.is_empty() {
        out.push('/');
    }
    out
}

fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(out)
}

/// The drive key a request target names, and whether it was written as a folder (a final `/`;
/// the root always is).
///
/// # Errors
///
/// 400 for a path that is not one or tries to leave the drive (`..`, `.`, an empty level, a
/// backslash, NUL, a control character, bytes that are not UTF-8); 404 for the sync's hidden
/// folder.
pub fn key_of(target: &str) -> Result<(String, bool), Status> {
    let path = match target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
    {
        Some(rest) => rest.find('/').map_or("/", |at| &rest[at..]),
        None => target,
    };
    let path = path.split(['?', '#']).next().unwrap_or_default();
    if !path.starts_with('/') {
        return Err(Status::BAD_REQUEST);
    }
    let decoded = percent_decode(path).ok_or(Status::BAD_REQUEST)?;
    let decoded = String::from_utf8(decoded).map_err(|_| Status::BAD_REQUEST)?;
    if decoded
        .chars()
        .any(|c| c == '\\' || c == '\0' || c.is_control())
    {
        return Err(Status::BAD_REQUEST);
    }
    let folder = decoded.ends_with('/');
    let inner = decoded.trim_start_matches('/');
    let inner = inner.strip_suffix('/').unwrap_or(inner);
    if inner.is_empty() {
        return Ok((String::new(), true));
    }
    let levels: Vec<&str> = inner.split('/').collect();
    for level in &levels {
        if level.is_empty() || *level == "." || *level == ".." {
            return Err(Status::BAD_REQUEST);
        }
    }
    if levels[0] == HIDDEN {
        return Err(Status::NOT_FOUND);
    }
    Ok((levels.join("/"), folder))
}

/// A file's media type by its extension.
#[must_use]
pub fn content_type_of(key: &str) -> &'static str {
    let extension = key
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "txt" | "log" => "text/plain; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "eml" => "message/rfc822",
        "ics" => "text/calendar",
        "vcf" => "text/vcard",
        _ => "application/octet-stream",
    }
}

/// Whether an XML body declares a DTD or an entity: refused before any parser sees it.
fn has_dtd(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("<!doctype") || lower.contains("<!entity")
}

/// A WebDAV request body as XML.
fn parse_xml(body: &[u8]) -> Result<Option<String>, Status> {
    let text = std::str::from_utf8(body).map_err(|_| Status::BAD_REQUEST)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    if has_dtd(text) {
        return Err(Status::BAD_REQUEST);
    }
    // Parsed once here to know it is well-formed; the callers parse the same text again.
    roxmltree::Document::parse(text).map_err(|_| Status::BAD_REQUEST)?;
    Ok(Some(text.to_string()))
}

fn is_dav(node: &roxmltree::Node<'_, '_>, name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && node.tag_name().namespace() == Some(DAV)
}

/// What PROPFIND asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PropRequest {
    All,
    Names,
    /// `(namespace, name)` of each property.
    Some(Vec<(String, String)>),
}

fn prop_request(body: &[u8]) -> Result<PropRequest, Status> {
    let Some(text) = parse_xml(body)? else {
        return Ok(PropRequest::All);
    };
    let doc = roxmltree::Document::parse(&text).map_err(|_| Status::BAD_REQUEST)?;
    let root = doc.root_element();
    if !is_dav(&root, "propfind") {
        return Err(Status::BAD_REQUEST);
    }
    for child in root.children().filter(roxmltree::Node::is_element) {
        if is_dav(&child, "allprop") {
            return Ok(PropRequest::All);
        }
        if is_dav(&child, "propname") {
            return Ok(PropRequest::Names);
        }
        if is_dav(&child, "prop") {
            let names = child
                .children()
                .filter(roxmltree::Node::is_element)
                .map(|p| {
                    (
                        p.tag_name().namespace().unwrap_or_default().to_string(),
                        p.tag_name().name().to_string(),
                    )
                })
                .collect();
            return Ok(PropRequest::Some(names));
        }
    }
    Err(Status::BAD_REQUEST)
}

/// The properties every resource can have, in DAV:.
const LIVE: [&str; 9] = [
    "resourcetype",
    "displayname",
    "getcontentlength",
    "getlastmodified",
    "creationdate",
    "getetag",
    "getcontenttype",
    "supportedlock",
    "lockdiscovery",
];

const SUPPORTED_LOCK: &str = "<D:supportedlock>\
    <D:lockentry><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockentry>\
    <D:lockentry><D:lockscope><D:shared/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockentry>\
    </D:supportedlock>";

/// An empty element of `(namespace, name)` for a response (its namespace declared on it).
fn empty_element(namespace: &str, name: &str) -> String {
    if namespace == DAV {
        format!("<D:{}/>", xml_escape(name))
    } else if namespace.is_empty() {
        format!("<{} xmlns=\"\"/>", xml_escape(name))
    } else {
        format!("<x:{} xmlns:x=\"{}\"/>", xml_escape(name), xml_escape(namespace))
    }
}

fn random_token() -> String {
    let mut bytes = [0u8; 16];
    // Without randomness the token is still unique per process (the time and a counter).
    if getrandom::getrandom(&mut bytes).is_err() {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        bytes[..8].copy_from_slice(&n.to_le_bytes());
        bytes[8..].copy_from_slice(&(t as u64).to_le_bytes());
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "opaquelocktoken:{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// `Second-600` / `Infinite` (the first one given) in seconds, within the bridge's maximum.
fn timeout_of(head: &Head) -> u64 {
    let Some(value) = head.header("Timeout") else {
        return DEFAULT_LOCK_SECS;
    };
    let first = value.split(',').next().unwrap_or_default().trim();
    if first.eq_ignore_ascii_case("Infinite") {
        return MAX_LOCK_SECS;
    }
    first
        .strip_prefix("Second-")
        .and_then(|s| s.parse::<u64>().ok())
        .map_or(DEFAULT_LOCK_SECS, |s| s.clamp(1, MAX_LOCK_SECS))
}

impl Dav {
    #[must_use]
    pub fn new(
        drive: Arc<dyn Drive>,
        credentials: Credentials,
        gate: Arc<FailureGate>,
        limits: Limits,
        port: u16,
    ) -> Dav {
        Dav {
            drive,
            credentials,
            gate,
            limits,
            port,
            locks: Mutex::new(Vec::new()),
        }
    }

    fn locks(&self) -> std::sync::MutexGuard<'_, Vec<Lock>> {
        let mut locks = self
            .locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        locks.retain(|lock| lock.expires > now);
        locks
    }

    /// Whether a lock someone holds keeps this request from changing `key` (the request did
    /// not name the lock's token in its `If` header).
    fn locked_out(&self, key: &str, head: &Head) -> bool {
        let presented = head.header("If").unwrap_or_default();
        self.locks()
            .iter()
            .any(|lock| lock.covers(key) && !presented.contains(&lock.token))
    }

    /// Drops the locks of `key` and of everything under it (it was deleted or moved away); a
    /// lock of a folder above it stays.
    fn forget_locks_under(&self, key: &str) {
        let folder = format!("{}/", key.trim_end_matches('/'));
        let mut locks = self.locks();
        locks.retain(|lock| {
            let gone = lock.key == key || lock.key == folder || lock.key.starts_with(&folder);
            !gone
        });
    }

    // ---- the connection ----

    /// Whether the request names this computer: `Host` 127.0.0.1, localhost or [::1], with
    /// the bridge's port.
    fn host_ok(&self, head: &Head) -> bool {
        let Some(host) = head.header("Host") else {
            return false;
        };
        let (name, port) = if let Some(rest) = host.strip_prefix('[') {
            match rest.split_once(']') {
                Some((name, after)) => (name, after.strip_prefix(':')),
                None => return false,
            }
        } else {
            match host.rsplit_once(':') {
                Some((name, port)) => (name, Some(port)),
                None => (host, None),
            }
        };
        let local = matches!(name.to_ascii_lowercase().as_str(), "127.0.0.1" | "localhost" | "::1");
        let port_ok = match port {
            Some(port) => self.port == 0 || port.parse::<u16>().ok() == Some(self.port),
            None => self.port == 0 || self.port == 80,
        };
        local && port_ok
    }

    /// The checks before a request's body is read: the host, no browser, the sign-in. `Err`
    /// is the answer, and whether the connection must close after it.
    fn guard(&self, head: &Head, failures: &mut u32) -> Result<(), (Response, bool)> {
        if !self.host_ok(head) {
            return Err((
                Response::text(Status::FORBIDDEN, "The bridge answers this computer's own address only."),
                true,
            ));
        }
        if head.header("Origin").is_some() {
            return Err((
                Response::text(Status::FORBIDDEN, "The bridge does not answer web pages."),
                true,
            ));
        }
        if head.method == "OPTIONS" {
            return Ok(());
        }
        let challenge = || {
            Response::text(Status::UNAUTHORIZED, "Sign in with the bridge's user and password.")
                .with_header("WWW-Authenticate", "Basic realm=\"Azlin Bridge\", charset=\"UTF-8\"")
        };
        let Some(value) = head.header("Authorization") else {
            return Err((challenge(), false));
        };
        let decoded = value
            .strip_prefix("Basic ")
            .or_else(|| value.strip_prefix("basic "))
            .and_then(auth::decode_base64)
            .unwrap_or_default();
        let (user, password) = match decoded.iter().position(|b| *b == b':') {
            Some(at) => (&decoded[..at], &decoded[at + 1..]),
            None => (&decoded[..], &[][..]),
        };
        self.gate.before_attempt();
        if self.credentials.check(user, password) {
            return Ok(());
        }
        self.gate.failed();
        *failures += 1;
        let close = *failures >= self.limits.auth_failures_per_connection;
        Err((challenge(), close))
    }

    /// The most a request's body may be.
    fn body_limit(&self, method: &str) -> u64 {
        if method == "PUT" {
            self.limits.put_bytes
        } else {
            self.limits.xml_bytes as u64
        }
    }

    /// Serves one connection: requests until it closes, goes quiet or a request is refused.
    pub fn handle<C: Conn>(&self, mut conn: C) {
        let _ = conn.set_read_timeout(Some(self.limits.http_idle));
        let mut input = Input::new();
        let mut rate = RateLimiter::new(self.limits.commands_per_second, self.limits.command_burst);
        let mut failures = 0u32;
        loop {
            let head = match http::read_head(
                &mut input,
                &mut conn,
                self.limits.header_bytes,
                self.limits.header_count,
            ) {
                Ok(head) => head,
                Err(HttpError::Gone) => return,
                Err(HttpError::Refuse(status)) => {
                    refuse_and_close(&mut conn, &Response::text(status, status.1));
                    return;
                }
            };
            rate.take();
            let keep_alive = head.keep_alive();
            let framing = match head.framing() {
                Ok(framing) => framing,
                Err(status) => {
                    refuse_and_close(&mut conn, &Response::text(status, status.1));
                    return;
                }
            };
            if let Err((response, close)) = self.guard(&head, &mut failures) {
                // A small body can be read past to keep the connection; a big one closes it.
                let small = matches!(framing, Framing::Length(n) if n <= self.limits.xml_bytes as u64);
                let expects = head
                    .header("Expect")
                    .is_some_and(|e| e.eq_ignore_ascii_case("100-continue"));
                let close = close || !small || expects;
                if close {
                    refuse_and_close(&mut conn, &response);
                    return;
                }
                if http::read_body(&mut input, &mut conn, framing, self.limits.xml_bytes as u64).is_err() {
                    return;
                }
                let _ = http::write_response(&mut conn, &response, false, keep_alive);
                if !keep_alive {
                    return;
                }
                continue;
            }
            if head
                .header("Expect")
                .is_some_and(|e| e.eq_ignore_ascii_case("100-continue"))
                && net::send(&mut conn, b"HTTP/1.1 100 Continue\r\n\r\n").is_err()
            {
                return;
            }
            let body = match http::read_body(&mut input, &mut conn, framing, self.body_limit(&head.method)) {
                Ok(body) => body,
                Err(HttpError::Gone) => return,
                Err(HttpError::Refuse(status)) => {
                    refuse_and_close(&mut conn, &Response::text(status, status.1));
                    return;
                }
            };
            let response = self.respond(&head, &body);
            let head_only = head.method == "HEAD";
            if http::write_response(&mut conn, &response, head_only, keep_alive).is_err() || !keep_alive {
                return;
            }
        }
    }

    // ---- the methods ----

    /// The answer to one request whose sign-in and body are done.
    #[must_use]
    pub fn respond(&self, head: &Head, body: &[u8]) -> Response {
        let (key, folder_syntax) = match key_of(&head.target) {
            Ok(found) => found,
            Err(status) => return Response::text(status, "Not a path of the drive."),
        };
        let result = match head.method.as_str() {
            "OPTIONS" => Ok(Response::new(Status::OK)
                .with_header("DAV", "1, 2")
                .with_header("MS-Author-Via", "DAV")
                .with_header(
                    "Allow",
                    "OPTIONS, GET, HEAD, PUT, DELETE, MKCOL, COPY, MOVE, PROPFIND, PROPPATCH, LOCK, UNLOCK",
                )),
            "PROPFIND" => self.propfind(head, &key, folder_syntax, body),
            "PROPPATCH" => self.proppatch(head, &key, folder_syntax, body),
            "GET" | "HEAD" => self.get(head, &key, folder_syntax),
            "PUT" => self.put(head, &key, folder_syntax, body),
            "DELETE" => self.delete(head, &key, folder_syntax),
            "MKCOL" => self.mkcol(head, &key, body),
            "COPY" | "MOVE" => self.copy_or_move(head, &key, folder_syntax, head.method == "MOVE"),
            "LOCK" => self.lock(head, &key, folder_syntax, body),
            "UNLOCK" => self.unlock(head, &key),
            _ => Ok(Response::text(Status::METHOD_NOT_ALLOWED, "Not a WebDAV method the bridge offers.")
                .with_header("Allow", "OPTIONS, GET, HEAD, PUT, DELETE, MKCOL, COPY, MOVE, PROPFIND, PROPPATCH, LOCK, UNLOCK")),
        };
        result.unwrap_or_else(|e| match e {
            DriveError::NotFound { .. } => Response::text(Status::NOT_FOUND, "Not there."),
            DriveError::InvalidKey { reason, .. } => Response::text(Status::CONFLICT, reason),
            DriveError::Denied { .. } => Response::text(Status::FORBIDDEN, "The drive refused."),
            other => Response::text(Status::BAD_GATEWAY, &other.to_string()),
        })
    }

    fn resource(&self, key: &str, folder_syntax: bool) -> Result<Resource, DriveError> {
        if key.is_empty() {
            return Ok(Resource::Folder);
        }
        if !folder_syntax {
            match self.drive.head(key) {
                Ok(info) => return Ok(Resource::File(info)),
                Err(DriveError::NotFound { .. }) => {}
                Err(e) => return Err(e),
            }
        }
        if ops::folder_exists(&*self.drive, &format!("{key}/"))? {
            Ok(Resource::Folder)
        } else {
            Ok(Resource::Missing)
        }
    }

    /// Whether the folder that holds `key` is there (the root always is).
    fn parent_exists(&self, key: &str) -> Result<bool, DriveError> {
        match key.rsplit_once('/') {
            None => Ok(true),
            Some((parent, _)) => ops::folder_exists(&*self.drive, &format!("{parent}/")),
        }
    }

    fn lockdiscovery(&self, key: &str) -> String {
        let locks = self.locks();
        let active: Vec<&Lock> = locks.iter().filter(|l| l.covers(key)).collect();
        if active.is_empty() {
            return String::from("<D:lockdiscovery/>");
        }
        let mut out = String::from("<D:lockdiscovery>");
        for lock in active {
            out.push_str(&active_lock(lock));
        }
        out.push_str("</D:lockdiscovery>");
        out
    }

    /// One `<D:response>` of a PROPFIND.
    fn prop_response(&self, key: &str, folder: bool, info: Option<&ObjectInfo>, request: &PropRequest) -> String {
        let name = key.rsplit('/').next().unwrap_or_default();
        let mut found: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        let names_only = matches!(request, PropRequest::Names);
        let live = |prop: &str| -> Option<String> {
            Some(match prop {
                "resourcetype" if folder => String::from("<D:resourcetype><D:collection/></D:resourcetype>"),
                "resourcetype" => String::from("<D:resourcetype/>"),
                "displayname" => format!("<D:displayname>{}</D:displayname>", xml_escape(name)),
                "getcontentlength" => format!("<D:getcontentlength>{}</D:getcontentlength>", info?.size),
                "getlastmodified" => format!(
                    "<D:getlastmodified>{}</D:getlastmodified>",
                    dates::http_date(i64::try_from(info?.modified?).unwrap_or(0))
                ),
                "creationdate" => format!(
                    "<D:creationdate>{}</D:creationdate>",
                    azul_storage::time::iso8601(info?.modified?)
                ),
                "getetag" => format!("<D:getetag>\"{}\"</D:getetag>", xml_escape(info?.etag.as_deref()?)),
                "getcontenttype" if !folder => {
                    format!("<D:getcontenttype>{}</D:getcontenttype>", content_type_of(key))
                }
                "supportedlock" => String::from(SUPPORTED_LOCK),
                "lockdiscovery" => self.lockdiscovery(key),
                _ => return None,
            })
        };
        match request {
            PropRequest::All | PropRequest::Names => {
                for prop in LIVE {
                    if let Some(value) = live(prop) {
                        found.push(if names_only { format!("<D:{prop}/>") } else { value });
                    }
                }
            }
            PropRequest::Some(names) => {
                for (namespace, prop) in names {
                    let value = (namespace == DAV).then(|| live(prop)).flatten();
                    match value {
                        Some(value) => found.push(value),
                        None => missing.push(empty_element(namespace, prop)),
                    }
                }
            }
        }
        let mut out = format!("<D:response><D:href>{}</D:href>", xml_escape(&href_of(key, folder)));
        if !found.is_empty() {
            out.push_str("<D:propstat><D:prop>");
            out.push_str(&found.concat());
            out.push_str("</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>");
        }
        if !missing.is_empty() {
            out.push_str("<D:propstat><D:prop>");
            out.push_str(&missing.concat());
            out.push_str("</D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat>");
        }
        out.push_str("</D:response>");
        out
    }

    fn propfind(&self, head: &Head, key: &str, folder_syntax: bool, body: &[u8]) -> Result<Response, DriveError> {
        let depth = head.header("Depth").unwrap_or("infinity").to_ascii_lowercase();
        if depth != "0" && depth != "1" {
            return Ok(Response::new(Status::FORBIDDEN).with_body(
                "application/xml; charset=utf-8",
                b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\
                  <D:error xmlns:D=\"DAV:\"><D:propfind-finite-depth/></D:error>"
                    .to_vec(),
            ));
        }
        let request = match prop_request(body) {
            Ok(request) => request,
            Err(status) => return Ok(Response::text(status, "Not a PROPFIND body the bridge reads.")),
        };
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?><D:multistatus xmlns:D=\"DAV:\">");
        match self.resource(key, folder_syntax)? {
            Resource::Missing => return Ok(Response::text(Status::NOT_FOUND, "Not there.")),
            Resource::File(info) => out.push_str(&self.prop_response(key, false, Some(&info), &request)),
            Resource::Folder => {
                out.push_str(&self.prop_response(key, true, None, &request));
                if depth == "1" {
                    let prefix = if key.is_empty() { String::new() } else { format!("{key}/") };
                    let level = ops::list_folder_all(&*self.drive, &prefix)?;
                    for folder in &level.folders {
                        let child = folder.trim_end_matches('/');
                        if child == HIDDEN || child.is_empty() {
                            continue;
                        }
                        out.push_str(&self.prop_response(child, true, None, &request));
                    }
                    for object in &level.objects {
                        // A folder's own marker is the folder, not a child.
                        if object.key == prefix || object.key.ends_with('/') {
                            continue;
                        }
                        out.push_str(&self.prop_response(&object.key, false, Some(object), &request));
                    }
                }
            }
        }
        out.push_str("</D:multistatus>");
        Ok(Response::new(Status::MULTI_STATUS).with_body("application/xml; charset=utf-8", out.into_bytes()))
    }

    fn proppatch(&self, head: &Head, key: &str, folder_syntax: bool, body: &[u8]) -> Result<Response, DriveError> {
        let folder = match self.resource(key, folder_syntax)? {
            Resource::Missing => return Ok(Response::text(Status::NOT_FOUND, "Not there.")),
            Resource::File(_) => false,
            Resource::Folder => true,
        };
        if self.locked_out(key, head) {
            return Ok(Response::text(Status::LOCKED, "Locked."));
        }
        let text = match parse_xml(body) {
            Ok(Some(text)) => text,
            Ok(None) | Err(_) => return Ok(Response::text(Status::BAD_REQUEST, "Not a PROPPATCH body.")),
        };
        let Ok(doc) = roxmltree::Document::parse(&text) else {
            return Ok(Response::text(Status::BAD_REQUEST, "Not a PROPPATCH body."));
        };
        let root = doc.root_element();
        if !is_dav(&root, "propertyupdate") {
            return Ok(Response::text(Status::BAD_REQUEST, "Not a PROPPATCH body."));
        }
        // Every property named in set / remove: answered as done (dead properties are not
        // kept; Windows sets its file times this way and stops copying on a refusal).
        let mut props = String::new();
        for action in root.children().filter(|n| is_dav(n, "set") || is_dav(n, "remove")) {
            for prop in action.children().filter(|n| is_dav(n, "prop")) {
                for p in prop.children().filter(roxmltree::Node::is_element) {
                    props.push_str(&empty_element(p.tag_name().namespace().unwrap_or_default(), p.tag_name().name()));
                }
            }
        }
        let out = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><D:multistatus xmlns:D=\"DAV:\"><D:response>\
             <D:href>{}</D:href><D:propstat><D:prop>{props}</D:prop>\
             <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>",
            xml_escape(&href_of(key, folder))
        );
        Ok(Response::new(Status::MULTI_STATUS).with_body("application/xml; charset=utf-8", out.into_bytes()))
    }

    fn get(&self, head: &Head, key: &str, folder_syntax: bool) -> Result<Response, DriveError> {
        match self.resource(key, folder_syntax)? {
            Resource::Missing => Ok(Response::text(Status::NOT_FOUND, "Not there.")),
            Resource::Folder => {
                let prefix = if key.is_empty() { String::new() } else { format!("{key}/") };
                let level = ops::list_folder_all(&*self.drive, &prefix)?;
                let mut html = format!(
                    "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{0}</title></head><body><h1>{0}</h1><ul>",
                    xml_escape(&href_of(key, true))
                );
                for folder in &level.folders {
                    let child = folder.trim_end_matches('/');
                    if child == HIDDEN {
                        continue;
                    }
                    let name = child.rsplit('/').next().unwrap_or_default();
                    html.push_str(&format!(
                        "<li><a href=\"{}\">{}/</a></li>",
                        xml_escape(&href_of(child, true)),
                        xml_escape(name)
                    ));
                }
                for object in level.objects.iter().filter(|o| !o.key.ends_with('/')) {
                    html.push_str(&format!(
                        "<li><a href=\"{}\">{}</a> ({} bytes)</li>",
                        xml_escape(&href_of(&object.key, false)),
                        xml_escape(object.name()),
                        object.size
                    ));
                }
                html.push_str("</ul></body></html>");
                Ok(Response::new(Status::OK).with_body("text/html; charset=utf-8", html.into_bytes()))
            }
            Resource::File(info) => {
                let mut response_headers: Vec<(String, String)> = vec![
                    (String::from("Accept-Ranges"), String::from("bytes")),
                    (String::from("Content-Type"), content_type_of(key).to_string()),
                ];
                if let Some(etag) = &info.etag {
                    response_headers.push((String::from("ETag"), format!("\"{etag}\"")));
                }
                if let Some(modified) = info.modified {
                    response_headers.push((
                        String::from("Last-Modified"),
                        dates::http_date(i64::try_from(modified).unwrap_or(0)),
                    ));
                }
                let range = head.header("Range").and_then(|r| parse_range(r, info.size));
                let head_only = head.method == "HEAD";
                let (status, body, length) = match range {
                    Some(Err(())) => {
                        let mut response = Response::text(Status::RANGE_NOT_SATISFIABLE, "Outside the file.");
                        response.headers.push((String::from("Content-Range"), format!("bytes */{}", info.size)));
                        return Ok(response);
                    }
                    Some(Ok((start, end))) => {
                        response_headers.push((
                            String::from("Content-Range"),
                            format!("bytes {start}-{end}/{}", info.size),
                        ));
                        let body = if head_only {
                            Vec::new()
                        } else {
                            self.drive.get_range(key, ByteRange::new(start, Some(end)))?
                        };
                        (Status::PARTIAL, body, end - start + 1)
                    }
                    None => {
                        let body = if head_only { Vec::new() } else { self.drive.get(key)? };
                        (Status::OK, body, info.size)
                    }
                };
                if head_only {
                    response_headers.push((String::from("Content-Length"), length.to_string()));
                }
                Ok(Response {
                    status,
                    headers: response_headers,
                    body,
                })
            }
        }
    }

    fn put(&self, head: &Head, key: &str, folder_syntax: bool, body: &[u8]) -> Result<Response, DriveError> {
        if key.is_empty() || folder_syntax {
            return Ok(Response::text(Status::METHOD_NOT_ALLOWED, "A folder is made with MKCOL."));
        }
        let existed = match self.resource(key, false)? {
            Resource::Folder => {
                return Ok(Response::text(Status::METHOD_NOT_ALLOWED, "A folder has this name."));
            }
            Resource::File(_) => true,
            Resource::Missing => false,
        };
        if !self.parent_exists(key)? {
            return Ok(Response::text(Status::CONFLICT, "The folder it would go into is not there."));
        }
        if self.locked_out(key, head) {
            return Ok(Response::text(Status::LOCKED, "Locked."));
        }
        self.drive.put(key, body)?;
        let mut response = Response::new(if existed { Status::NO_CONTENT } else { Status::CREATED });
        if let Ok(info) = self.drive.head(key) {
            if let Some(etag) = info.etag {
                response = response.with_header("ETag", format!("\"{etag}\""));
            }
        }
        Ok(response)
    }

    fn delete(&self, head: &Head, key: &str, folder_syntax: bool) -> Result<Response, DriveError> {
        if key.is_empty() {
            return Ok(Response::text(Status::FORBIDDEN, "The drive itself is not deleted."));
        }
        let resource = self.resource(key, folder_syntax)?;
        if resource == Resource::Missing {
            return Ok(Response::text(Status::NOT_FOUND, "Not there."));
        }
        if self.locked_out(key, head) {
            return Ok(Response::text(Status::LOCKED, "Locked."));
        }
        match resource {
            Resource::File(_) => self.drive.delete(key)?,
            _ => self.drive.delete_folder(&format!("{key}/"))?,
        }
        self.forget_locks_under(key);
        Ok(Response::new(Status::NO_CONTENT))
    }

    fn mkcol(&self, head: &Head, key: &str, body: &[u8]) -> Result<Response, DriveError> {
        if !body.is_empty() {
            return Ok(Response::text(Status::UNSUPPORTED_MEDIA, "MKCOL takes no body."));
        }
        if key.is_empty() || self.resource(key, false)? != Resource::Missing {
            return Ok(Response::text(Status::METHOD_NOT_ALLOWED, "Something has this name."));
        }
        if !self.parent_exists(key)? {
            return Ok(Response::text(Status::CONFLICT, "The folder it would go into is not there."));
        }
        if self.locked_out(key, head) {
            return Ok(Response::text(Status::LOCKED, "Locked."));
        }
        self.drive.create_folder(&format!("{key}/"))?;
        Ok(Response::new(Status::CREATED))
    }

    fn copy_or_move(&self, head: &Head, key: &str, folder_syntax: bool, moving: bool) -> Result<Response, DriveError> {
        let Some(destination) = head.header("Destination") else {
            return Ok(Response::text(Status::BAD_REQUEST, "No Destination."));
        };
        let (target, _) = match key_of(destination) {
            Ok(found) => found,
            Err(status) => return Ok(Response::text(status, "Not a destination in the drive.")),
        };
        if key.is_empty() || target.is_empty() {
            return Ok(Response::text(Status::FORBIDDEN, "The drive itself is not copied or moved."));
        }
        let source = self.resource(key, folder_syntax)?;
        let folder = match source {
            Resource::Missing => return Ok(Response::text(Status::NOT_FOUND, "Not there.")),
            Resource::File(_) => false,
            Resource::Folder => true,
        };
        if target == key || (folder && target.starts_with(&format!("{key}/"))) {
            return Ok(Response::text(Status::FORBIDDEN, "Not into itself."));
        }
        if !self.parent_exists(&target)? {
            return Ok(Response::text(Status::CONFLICT, "The destination's folder is not there."));
        }
        if self.locked_out(&target, head) || (moving && self.locked_out(key, head)) {
            return Ok(Response::text(Status::LOCKED, "Locked."));
        }
        let overwrite = !head
            .header("Overwrite")
            .is_some_and(|o| o.eq_ignore_ascii_case("F"));
        let existing = self.resource(&target, false)?;
        let replaced = existing != Resource::Missing;
        if replaced {
            if !overwrite {
                return Ok(Response::text(Status::PRECONDITION_FAILED, "The destination is there."));
            }
            match existing {
                Resource::File(_) => self.drive.delete(&target)?,
                _ => self.drive.delete_folder(&format!("{target}/"))?,
            }
            self.forget_locks_under(&target);
        }
        if !folder {
            if moving {
                self.drive.rename(key, &target)?;
            } else {
                self.drive.copy(key, &target)?;
            }
        } else if moving {
            self.drive.rename(&format!("{key}/"), &format!("{target}/"))?;
        } else {
            let depth_zero = head.header("Depth") == Some("0");
            let from = format!("{key}/");
            let to = format!("{target}/");
            self.drive.create_folder(&to)?;
            if !depth_zero {
                for object in ops::list_all(&*self.drive, &from)? {
                    let rest = &object.key[from.len()..];
                    if rest.is_empty() {
                        continue;
                    }
                    let dest = format!("{to}{rest}");
                    if object.key.ends_with('/') {
                        self.drive.put(&dest, &[])?;
                    } else {
                        self.drive.copy(&object.key, &dest)?;
                    }
                }
            }
        }
        if moving {
            self.forget_locks_under(key);
        }
        Ok(Response::new(if replaced { Status::NO_CONTENT } else { Status::CREATED }))
    }

    fn lock(&self, head: &Head, key: &str, folder_syntax: bool, body: &[u8]) -> Result<Response, DriveError> {
        let text = match parse_xml(body) {
            Ok(text) => text,
            Err(status) => return Ok(Response::text(status, "Not a LOCK body.")),
        };
        let seconds = timeout_of(head);
        let Some(text) = text else {
            // A refresh: the token comes in the If header.
            let presented = head.header("If").unwrap_or_default().to_string();
            let mut locks = self.locks();
            let Some(lock) = locks
                .iter_mut()
                .find(|l| l.covers(key) && presented.contains(&l.token))
            else {
                return Ok(Response::text(Status::PRECONDITION_FAILED, "No lock of yours to refresh."));
            };
            lock.seconds = seconds;
            lock.expires = Instant::now() + Duration::from_secs(seconds);
            let xml = active_lock(lock);
            drop(locks);
            return Ok(lock_answer(Status::OK, &xml, None));
        };
        let Ok(doc) = roxmltree::Document::parse(&text) else {
            return Ok(Response::text(Status::BAD_REQUEST, "Not a LOCK body."));
        };
        let root = doc.root_element();
        if !is_dav(&root, "lockinfo") {
            return Ok(Response::text(Status::BAD_REQUEST, "Not a LOCK body."));
        }
        let exclusive = !root
            .descendants()
            .any(|n| is_dav(&n, "shared"));
        let owner = root
            .children()
            .find(|n| is_dav(n, "owner"))
            .map(|n| {
                n.descendants()
                    .filter_map(|d| d.text())
                    .collect::<String>()
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        let infinite = head
            .header("Depth")
            .is_none_or(|d| d.eq_ignore_ascii_case("infinity"));
        let resource = self.resource(key, folder_syntax)?;
        let lock_key = match &resource {
            Resource::Folder => format!("{key}/"),
            _ => key.to_string(),
        };
        {
            let locks = self.locks();
            let conflict = locks.iter().any(|l| {
                (l.covers(&lock_key) || (infinite && l.key.starts_with(&lock_key) && lock_key.ends_with('/')))
                    && (l.exclusive || exclusive)
            });
            if conflict {
                return Ok(Response::text(Status::LOCKED, "Locked by someone else."));
            }
        }
        // A lock on a name that is not there makes an empty file (RFC 4918 9.10.4).
        let created = if resource == Resource::Missing {
            if key.is_empty() || folder_syntax || !self.parent_exists(key)? {
                return Ok(Response::text(Status::CONFLICT, "The folder it would go into is not there."));
            }
            self.drive.put(key, &[])?;
            true
        } else {
            false
        };
        let lock = Lock {
            key: lock_key,
            token: random_token(),
            owner,
            exclusive,
            infinite,
            seconds,
            expires: Instant::now() + Duration::from_secs(seconds),
        };
        let xml = active_lock(&lock);
        let token = lock.token.clone();
        self.locks().push(lock);
        Ok(lock_answer(
            if created { Status::CREATED } else { Status::OK },
            &xml,
            Some(&token),
        ))
    }

    fn unlock(&self, head: &Head, key: &str) -> Result<Response, DriveError> {
        let token = head
            .header("Lock-Token")
            .unwrap_or_default()
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string();
        let mut locks = self.locks();
        let before = locks.len();
        locks.retain(|l| !(l.token == token && l.covers(key)));
        if locks.len() == before {
            return Ok(Response::text(Status::CONFLICT, "No such lock here."));
        }
        Ok(Response::new(Status::NO_CONTENT))
    }
}

/// Answers with `response` and closes - after reading for a moment what the client still sends
/// (a body the bridge refused), so it sees the answer instead of a reset (a lingering close).
fn refuse_and_close<C: Conn>(conn: &mut C, response: &Response) {
    use std::io::Read;

    if http::write_response(conn, response, false, false).is_err() {
        return;
    }
    let _ = conn.set_read_timeout(Some(Duration::from_millis(500)));
    let mut scratch = [0u8; 16 * 1024];
    let mut total = 0usize;
    while total < 16 * 1024 * 1024 {
        match conn.read(&mut scratch) {
            Ok(0) | Err(_) => break,
            Ok(n) => total += n,
        }
    }
}

fn active_lock(lock: &Lock) -> String {
    format!(
        "<D:activelock><D:locktype><D:write/></D:locktype><D:lockscope>{}</D:lockscope>\
         <D:depth>{}</D:depth><D:owner>{}</D:owner><D:timeout>Second-{}</D:timeout>\
         <D:locktoken><D:href>{}</D:href></D:locktoken>\
         <D:lockroot><D:href>{}</D:href></D:lockroot></D:activelock>",
        if lock.exclusive { "<D:exclusive/>" } else { "<D:shared/>" },
        if lock.infinite { "infinity" } else { "0" },
        xml_escape(&lock.owner),
        lock.seconds,
        lock.token,
        xml_escape(&href_of(&lock.key, lock.key.ends_with('/')))
    )
}

fn lock_answer(status: Status, active: &str, token: Option<&str>) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><D:prop xmlns:D=\"DAV:\"><D:lockdiscovery>{active}</D:lockdiscovery></D:prop>"
    );
    let mut response = Response::new(status).with_body("application/xml; charset=utf-8", body.into_bytes());
    if let Some(token) = token {
        response = response.with_header("Lock-Token", format!("<{token}>"));
    }
    response
}

/// One byte range of a `Range` header for a file of `size` bytes: `Some(Ok((first, last)))`,
/// `Some(Err(()))` when it is outside the file, `None` for no range or one the bridge ignores
/// (several ranges, another unit: the whole file then).
fn parse_range(value: &str, size: u64) -> Option<Result<(u64, u64), ()>> {
    let spec = value.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (first, last) = spec.split_once('-')?;
    let (first, last) = (first.trim(), last.trim());
    if first.is_empty() {
        // The last n bytes.
        let n: u64 = last.parse().ok()?;
        if n == 0 || size == 0 {
            return Some(Err(()));
        }
        return Some(Ok((size.saturating_sub(n), size - 1)));
    }
    let start: u64 = first.parse().ok()?;
    let end: u64 = if last.is_empty() {
        size.saturating_sub(1)
    } else {
        last.parse::<u64>().ok()?.min(size.saturating_sub(1))
    };
    if start >= size || end < start {
        return Some(Err(()));
    }
    Some(Ok((start, end)))
}

fn busy(stream: &mut std::net::TcpStream) {
    let _ = http::write_response(
        stream,
        &Response::text(Status(503, "Service Unavailable"), "Too many connections."),
        false,
        false,
    );
}

/// Serves WebDAV on `listener` until it fails.
pub fn serve(dav: Arc<Dav>, listener: std::net::TcpListener) {
    let max = dav.limits.max_connections;
    let handler: net::Handler = Arc::new(move |stream| dav.handle(stream));
    net::serve(listener, max, handler, busy);
}

#[cfg(test)]
mod tests;
