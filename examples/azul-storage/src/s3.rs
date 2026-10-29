//! An S3-compatible bucket as a drive: AWS S3, Cloudflare R2, MinIO.
//!
//! Six calls of the S3 API, each signed with SigV4: ListObjectsV2 (with
//! continuation tokens), GetObject (with `Range`), PutObject (also conditional:
//! `If-None-Match: *`, `If-Match`; a streamed body or a file above [`PART_SIZE`] as a
//! multipart upload, [`PARALLEL_PARTS`] parts at once, a file's resumable: see
//! [`crate::multipart`]), CopyObject, DeleteObject and HeadObject; any other
//! request (a conditional read) is signed the same way by [`S3Drive::send_raw`].
//! Error answers become
//! [`ServiceError`]s that say what the service said. The requests are built here and sent through a
//! [`Transport`], so the same code runs over azul's HTTP client in the apps and
//! over a recording fake in the tests.

use std::{
    fmt,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use serde::{Deserialize, Serialize};

use crate::{
    sigv4::{self, SigningParams, EMPTY_SHA256},
    time::{amz_date, now_unix, parse_http_date},
    ops, same_etag, xml, AzlinError, BatchDelete, BatchStream, ByteRange, CallLimits, Drive,
    DriveError, HttpCall,
    HttpReply, ListPage, ListRequest, Method, ObjectInfo, Precondition, ServiceError, Transport,
    DELETE_BATCH,
};

/// S3's longest key, in bytes.
const MAX_KEY_BYTES: usize = 1024;

/// Bytes of one part of a multipart upload, and the most [`Drive::put_from`] sends in one
/// PUT (S3 takes parts of 5 MiB to 5 GiB, all but the last at least 5 MiB).
pub const PART_SIZE: usize = 16 * 1024 * 1024;
/// Parts of one upload in flight at once.
pub const PARALLEL_PARTS: usize = 4;

/// Where the bucket is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Config {
    /// `https://s3.eu-central-1.amazonaws.com`, `https://<account>.r2.cloudflarestorage.com`,
    /// `http://127.0.0.1:9000`. A path after the host is kept in front of the bucket.
    pub endpoint: String,
    /// `us-east-1`; R2 takes `auto`.
    pub region: String,
    pub bucket: String,
    /// `true`: `<endpoint>/<bucket>/<key>` (MinIO, local servers); `false`:
    /// `<bucket>.<endpoint host>/<key>` (virtual-host style, AWS's default).
    pub path_style: bool,
}

/// An access key. `Debug` never shows any of it.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    /// For temporary credentials (STS); sent as `x-amz-security-token`.
    pub session_token: Option<String>,
}

/// The keyring entry's JSON.
#[derive(Serialize, Deserialize)]
struct StoredCredentials {
    access_key_id: String,
    secret_access_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session_token: Option<String>,
}

impl Credentials {
    #[must_use]
    pub fn new(access_key_id: &str, secret_access_key: &str) -> Self {
        Credentials {
            access_key_id: access_key_id.to_string(),
            secret_access_key: secret_access_key.to_string(),
            session_token: None,
        }
    }

    #[must_use]
    pub fn with_session_token(mut self, token: &str) -> Self {
        self.session_token = Some(token.to_string());
        self
    }

    /// The one string stored in the OS keyring for a drive (JSON).
    #[must_use]
    pub fn to_keyring_secret(&self) -> String {
        serde_json::to_string(&StoredCredentials {
            access_key_id: self.access_key_id.clone(),
            secret_access_key: self.secret_access_key.clone(),
            session_token: self.session_token.clone(),
        })
        .unwrap_or_default()
    }

    /// Reads [`Self::to_keyring_secret`] back.
    pub fn from_keyring_secret(secret: &str) -> Result<Self, DriveError> {
        // The parser's message is not passed on: it could quote the secret.
        let stored: StoredCredentials = serde_json::from_str(secret).map_err(|_| {
            DriveError::InvalidConfig(String::from(
                "the keyring entry does not hold a drive's credentials",
            ))
        })?;
        if stored.access_key_id.is_empty() || stored.secret_access_key.is_empty() {
            return Err(DriveError::InvalidConfig(String::from(
                "the keyring entry has an empty access key or secret key",
            )));
        }
        Ok(Credentials {
            access_key_id: stored.access_key_id,
            secret_access_key: stored.secret_access_key,
            session_token: stored.session_token,
        })
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("access_key_id", &"<hidden>")
            .field("secret_access_key", &"<hidden>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<hidden>"),
            )
            .finish()
    }
}

/// The parts of an endpoint URL.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Endpoint {
    scheme: String,
    /// Lowercase; an IPv6 address keeps its brackets.
    host: String,
    /// `None` for the scheme's default port.
    port: Option<u16>,
    /// `/storage` or empty; never ends in `/`.
    base_path: String,
}

impl Endpoint {
    fn parse(text: &str) -> Result<Self, DriveError> {
        let bad = |why: &str| {
            DriveError::InvalidConfig(format!("the endpoint \"{text}\" is not usable: {why}"))
        };
        let text = text.trim();
        let (scheme, rest) = text
            .split_once("://")
            .ok_or_else(|| bad("it needs http:// or https:// in front"))?;
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(bad("only http:// and https:// are supported"));
        }
        if rest.contains(['?', '#', '@']) {
            return Err(bad("it may not have a query, a fragment or a user name"));
        }
        if rest.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(bad("it contains a space"));
        }
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let (host, port) = if let Some(after_bracket) = authority.strip_prefix('[') {
            let end = after_bracket
                .find(']')
                .ok_or_else(|| bad("an IPv6 address needs its closing ]"))?;
            let host = &authority[..end + 2];
            let port = match &after_bracket[end + 1..] {
                "" => None,
                p => Some(p.strip_prefix(':').ok_or_else(|| bad("a bad port"))?),
            };
            (host, port)
        } else {
            match authority.rsplit_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            }
        };
        if host.is_empty() {
            return Err(bad("it has no host"));
        }
        let port = match port {
            Some(p) => Some(p.parse::<u16>().map_err(|_| bad("a bad port"))?),
            None => None,
        };
        let default_port = if scheme == "https" { 443 } else { 80 };
        Ok(Endpoint {
            host: host.to_ascii_lowercase(),
            port: port.filter(|p| *p != default_port),
            base_path: path.trim_end_matches('/').to_string(),
            scheme,
        })
    }

    /// `host[:port]`, as in the `Host` header.
    fn authority(&self) -> String {
        match self.port {
            Some(port) => format!("{}:{port}", self.host),
            None => self.host.clone(),
        }
    }
}

/// A bucket name that fits in a URL; in a host name (virtual-host style) it must
/// also be a DNS label sequence.
fn check_bucket(bucket: &str, path_style: bool) -> Result<(), DriveError> {
    let bad = |why: &str| {
        DriveError::InvalidConfig(format!("the bucket name \"{bucket}\" is not usable: {why}"))
    };
    if bucket.is_empty() {
        return Err(bad("it is empty"));
    }
    if bucket.contains('/') || bucket.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad("it contains a / or a space"));
    }
    if !path_style {
        let dns_like = (3..=63).contains(&bucket.len())
            && bucket
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
            && bucket.starts_with(|c: char| c.is_ascii_alphanumeric())
            && bucket.ends_with(|c: char| c.is_ascii_alphanumeric());
        if !dns_like {
            return Err(bad(
                "in the host name (virtual-host style) it must be 3-63 lowercase letters, digits, \
                 dots or dashes; use path-style URLs for other names",
            ));
        }
    }
    Ok(())
}

fn check_s3_key(key: &str) -> Result<(), DriveError> {
    if key.is_empty() {
        return Err(DriveError::InvalidKey {
            key: String::new(),
            reason: "it is empty",
        });
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(DriveError::InvalidKey {
            key: key.to_string(),
            reason: "it is longer than 1024 bytes",
        });
    }
    Ok(())
}

/// A `Content-Type` for an upload, from the key's extension.
fn content_type_for(key: &str) -> &'static str {
    let extension = key
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "txt" | "md" | "csv" | "log" => "text/plain; charset=utf-8",
        "eml" => "message/rfc822",
        "ics" => "text/calendar",
        "json" => "application/json",
        "html" | "htm" => "text/html; charset=utf-8",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// Text for an XML element: `&`, `<` and `>` escaped (an ETag's quotes may stay).
/// The `Content-MD5` of `body`: base64 of its MD5 digest. S3 asks for it on DeleteObjects (a
/// body that names what to delete must arrive whole).
pub(crate) fn content_md5(body: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use md5::{Digest, Md5};
    let digest = Md5::digest(body);
    STANDARD.encode(&digest[..])
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// An entity tag as `If-Match` sends it: in quotes (a weak one, `W/"..."`, as it is).
fn quoted_etag(etag: &str) -> String {
    let etag = etag.trim();
    if etag.starts_with('"') || etag.starts_with("W/") {
        etag.to_string()
    } else {
        format!("\"{etag}\"")
    }
}

/// How often a conditional PUT is sent when S3 answers 409 (another conditional write of
/// the object in flight).
const CONDITIONAL_TRIES: u32 = 4;

/// The header a conditional write sends: `If-None-Match: *` or `If-Match: "<etag>"`.
fn condition_header(condition: &Precondition) -> (String, String) {
    match condition {
        Precondition::Absent => (String::from("if-none-match"), String::from("*")),
        Precondition::Matches(etag) => (String::from("if-match"), quoted_etag(etag)),
    }
}

/// A short random wait before a conditional write that met another one in flight (409) is
/// sent again (`attempt` from 1: the ceiling doubles).
fn conflict_pause(attempt: u32) {
    let ceiling = 5u64 << attempt;
    let wait = 1 + crate::ids::random_seed() % ceiling;
    std::thread::sleep(std::time::Duration::from_millis(wait));
}

/// The error of a failed answer: a missing key and a bad range become their own
/// variants, everything else a readable [`ServiceError`].
fn failure(reply: &HttpReply, key: Option<&str>) -> DriveError {
    let mut error = xml::parse_error(reply.status, &String::from_utf8_lossy(&reply.body));
    // An Azlin node's code and pause, and the request ID a body without one leaves out (D33).
    let header = |name: &str| {
        reply
            .header(name)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    error.azlin_error = header("x-azlin-error").map(|code| AzlinError::from_wire(&code));
    error.retry_after = header("retry-after").and_then(|secs| secs.parse::<u64>().ok());
    if error.request_id.is_none() {
        error.request_id = header("x-amz-request-id");
    }
    // A refusal without a code (a HEAD's answer has no body) dated far from this clock is the
    // refusal of a request signed with a clock that is off: its Date is the node's clock.
    let bare = error.code == crate::xml::status_code_name(reply.status);
    if bare && matches!(reply.status, 400 | 403) && error.azlin_error.is_none() {
        let served = header("date").and_then(|date| crate::time::parse_http_date(&date));
        if let Some(served) = served {
            let off = served.abs_diff(crate::time::now_unix());
            if off > MAX_CLOCK_SKEW_SECS {
                error.code = String::from("RequestTimeTooSkewed");
                error.message = format!(
                    "this computer's clock is {} minutes off the storage node's",
                    off / 60
                );
            }
        }
    }
    match (error.code.as_str(), key) {
        ("NoSuchKey" | "NotFound", Some(key)) => DriveError::NotFound {
            key: key.to_string(),
        },
        ("InvalidRange", Some(key)) => DriveError::InvalidRange {
            key: key.to_string(),
        },
        _ => DriveError::Service(error),
    }
}

/// How far a request's signed time may be off the node's clock (S3's 15 minutes).
pub const MAX_CLOCK_SKEW_SECS: u64 = 15 * 60;

/// The body of a ListObjectsV2 answer as a page, as [`Drive::list`] reads it (for a listing
/// sent with [`S3Drive::send_raw`]).
pub fn parse_listing(xml: &str) -> Result<ListPage, DriveError> {
    xml::parse_list(xml)
}

/// Whether a request may be sent again after its first try got no answer, or an answer that
/// does not say whether it was done (a 5xx that is no refusal). Every request of an
/// [`S3Drive`] says it ([`Routed::idempotency`]), so a router never repeats a write blind.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Idempotency {
    /// Sent twice it does what it does once: a read, a PUT of a whole body, a part, a delete,
    /// a copy.
    #[default]
    Safe,
    /// A conditional write (`If-Match`, `If-None-Match: *`): sent again after it was done it
    /// is refused 412, which the caller takes as the conflict it resolves anyway.
    Conditional,
    /// Not to be repeated blind: what the first try did is found out as [`Verify`] says.
    Unsafe(Verify),
}

/// How the outcome of an [`Idempotency::Unsafe`] request is found out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verify {
    /// CompleteMultipartUpload: done when the key's ETag is the upload's ([`multipart_etag`]).
    /// `None` when the parts' ETags are no MD5s: nothing can be checked, it is sent again.
    Completion { etag: Option<String> },
    /// CreateMultipartUpload: sent again, an upload the first try created stays empty and the
    /// node expires it after a week; nothing is lost and nothing can be found out.
    LeavesEmptyUpload,
}

/// The ETag a node names an upload of `parts` (number, ETag) by once it is complete: the MD5
/// of the parts' MD5s, then `-<count>` (as S3 and the Azlin nodes answer). `None` when a part's
/// ETag is no MD5 (a service may name parts otherwise): then it says nothing of the whole.
#[must_use]
pub fn multipart_etag(parts: &[(usize, String)]) -> Option<String> {
    use md5::{Digest, Md5};
    if parts.is_empty() {
        return None;
    }
    let mut md5s: Vec<u8> = Vec::with_capacity(parts.len() * 16);
    for (_, etag) in parts {
        let hex = xml::strip_quotes(etag);
        if hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        for i in (0..32).step_by(2) {
            md5s.push(u8::from_str_radix(&hex[i..i + 2], 16).ok()?);
        }
    }
    let digest = Md5::digest(&md5s);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("{hex}-{}", parts.len()))
}

/// One request of an [`S3Drive`] as its [`Router`] sends it: what it is about, and how to sign
/// it for an endpoint (SigV4 signs the host, so every endpoint tried gets its own signature).
pub struct Routed<'a> {
    method: Method,
    key: Option<&'a str>,
    endpoint: &'a str,
    sign: &'a dyn Fn(&str) -> Result<HttpCall, DriveError>,
    /// Signs a HEAD of another key for an endpoint ([`Routed::probe_for`]).
    probe: Option<&'a dyn Fn(&str, &str) -> Result<HttpCall, DriveError>>,
    /// Whether it may be sent again ([`Idempotency::Safe`] unless the drive says otherwise).
    idempotency: Idempotency,
}

impl<'a> Routed<'a> {
    /// A request on the object `key` (`None`: the bucket) of a drive at `endpoint`; `sign` makes
    /// it for any endpoint (a URL like [`S3Config::endpoint`]).
    pub fn new(
        method: Method,
        key: Option<&'a str>,
        endpoint: &'a str,
        sign: &'a dyn Fn(&str) -> Result<HttpCall, DriveError>,
    ) -> Self {
        Routed {
            method,
            key,
            endpoint,
            sign,
            probe: None,
            idempotency: Idempotency::Safe,
        }
    }

    /// The same request, saying whether it may be sent twice.
    #[must_use]
    pub fn with_idempotency(mut self, idempotency: Idempotency) -> Self {
        self.idempotency = idempotency;
        self
    }

    /// Whether it may be sent again when its outcome is not known.
    #[must_use]
    pub fn idempotency(&self) -> &Idempotency {
        &self.idempotency
    }

    /// The same request, able to sign a probe: `probe(endpoint, key)` makes a HEAD of `key` for
    /// `endpoint` with the drive's credentials ([`Routed::probe_for`]).
    #[must_use]
    pub fn with_probe(
        mut self,
        probe: &'a dyn Fn(&str, &str) -> Result<HttpCall, DriveError>,
    ) -> Self {
        self.probe = Some(probe);
        self
    }

    #[must_use]
    pub fn method(&self) -> Method {
        self.method
    }

    /// The object the request is about; `None`: the bucket (a listing).
    #[must_use]
    pub fn key(&self) -> Option<&'a str> {
        self.key
    }

    /// The drive's own endpoint ([`S3Config::endpoint`]).
    #[must_use]
    pub fn endpoint(&self) -> &'a str {
        self.endpoint
    }

    /// The request signed for `endpoint` (scheme, host, port, an optional base path).
    ///
    /// # Errors
    ///
    /// An endpoint that is not an http(s) URL.
    pub fn signed_for(&self, endpoint: &str) -> Result<HttpCall, DriveError> {
        (self.sign)(endpoint)
    }

    /// A signed HEAD of `key` (not this request's object) for `endpoint`, with the drive's
    /// credentials and clock: a probe - one cheap request whose answer, of any status, says a
    /// way to the bucket works (an iroh lane asks it before a node's first request).
    ///
    /// # Errors
    ///
    /// An endpoint that is not an http(s) URL, a key S3 cannot take, or a request made without
    /// [`Routed::with_probe`].
    pub fn probe_for(&self, endpoint: &str, key: &str) -> Result<HttpCall, DriveError> {
        match self.probe {
            Some(probe) => probe(endpoint, key),
            None => Err(DriveError::InvalidConfig(String::from(
                "this request cannot sign a probe",
            ))),
        }
    }
}

/// Where an [`S3Drive`]'s requests go and how often they are tried: the seam of a drive whose
/// bucket answers at several endpoints (an Azlin drive's block endpoint and nodes,
/// azcloud-kit's failover). [`Router::send`] sends one request - [`Routed::signed_for`] each
/// endpoint it tries, through `transport` - and returns the answer the drive reads (any
/// status), or why no endpoint answered. Without a router every request goes once to the
/// drive's endpoint.
pub trait Router: Send + Sync {
    fn send(
        &self,
        request: &Routed<'_>,
        transport: &dyn Transport,
    ) -> Result<HttpReply, DriveError>;
}

/// An Azlin node's HeadBucket header: the bytes it stores for the bucket.
pub const USED_BYTES_HEADER: &str = "x-azlin-used-bytes";
/// An Azlin node's HeadBucket header: the quota of the drive's tier, of stored bytes.
pub const QUOTA_BYTES_HEADER: &str = "x-azlin-quota-bytes";

/// What a storage service counts of a bucket ([`S3Drive::space`]): `None` what it did not say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BucketSpace {
    /// The bytes stored for the bucket (an Azlin node's count: what the quota counts).
    pub used_bytes: Option<u64>,
    /// The quota of stored bytes.
    pub quota_bytes: Option<u64>,
}

/// A bucket, reached through a [`Transport`].
pub struct S3Drive {
    config: S3Config,
    endpoint: Endpoint,
    credentials: Credentials,
    transport: Box<dyn Transport>,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
    /// The part size of a streamed upload ([`PART_SIZE`]).
    part_size: usize,
    /// Parts in flight at once ([`PARALLEL_PARTS`]).
    parallel: AtomicUsize,
    /// Where the state files of resumable uploads go; `None`: the app's
    /// ([`crate::multipart::resume_folder`]).
    resume_dir: Option<PathBuf>,
    /// Where each request goes and how often it is tried; `None`: once, to the endpoint.
    router: Option<Arc<dyn Router>>,
    /// The uplink as this drive's uploads measured it: the part size and the parts in flight
    /// of the next upload follow it ([`crate::multipart::plan_parts`]).
    link: crate::multipart::LinkEstimate,
    /// Uploads in flight right now: they share the link a sample measured.
    uploading: AtomicUsize,
}

impl fmt::Debug for S3Drive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Drive")
            .field("config", &self.config)
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

impl S3Drive {
    /// Checks the endpoint and the bucket name; sends nothing.
    pub fn new(
        config: S3Config,
        credentials: Credentials,
        transport: Box<dyn Transport>,
    ) -> Result<Self, DriveError> {
        let endpoint = Endpoint::parse(&config.endpoint)?;
        check_bucket(&config.bucket, config.path_style)?;
        if config.region.trim().is_empty() {
            return Err(DriveError::InvalidConfig(String::from(
                "a region is needed (\"us-east-1\"; R2 takes \"auto\")",
            )));
        }
        Ok(S3Drive {
            config,
            endpoint,
            credentials,
            transport,
            clock: Box::new(now_unix),
            part_size: PART_SIZE,
            parallel: AtomicUsize::new(PARALLEL_PARTS),
            resume_dir: None,
            router: None,
            link: crate::multipart::LinkEstimate::default(),
            uploading: AtomicUsize::new(0),
        })
    }

    /// Signs with this clock (seconds since 1970) instead of the system's.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    /// Streams [`Drive::put_from`] bodies in parts of up to `part_size` bytes (at least one; a
    /// real service wants 5 MiB and more) instead of [`PART_SIZE`]. On a slow link the parts
    /// get smaller, down to 5 MiB (or `part_size`, when that is less), and go one at a time
    /// ([`crate::multipart::plan_parts`]).
    #[must_use]
    pub fn with_part_size(mut self, part_size: usize) -> Self {
        self.part_size = part_size.max(1);
        self
    }

    /// Sends up to `parallel` parts of an upload at once (at least one) instead of
    /// [`PARALLEL_PARTS`].
    #[must_use]
    pub fn with_parallel(self, parallel: usize) -> Self {
        self.set_parallel(parallel);
        self
    }

    /// Sends up to `parallel` parts of an upload at once (at least one) from now on.
    pub fn set_parallel(&self, parallel: usize) {
        self.parallel.store(parallel.max(1), Ordering::Relaxed);
    }

    /// Keeps the state files of resumable uploads ([`Drive::put_file`]) in `folder` instead of
    /// the app's ([`crate::multipart::set_resume_folder`]).
    #[must_use]
    pub fn with_resume_dir(mut self, folder: impl Into<PathBuf>) -> Self {
        self.resume_dir = Some(folder.into());
        self
    }

    /// Sends every request through `router` (an endpoint failover: several endpoints, retries)
    /// instead of once to the drive's endpoint.
    #[must_use]
    pub fn with_router(mut self, router: Arc<dyn Router>) -> Self {
        self.router = Some(router);
        self
    }

    /// The part size of an upload.
    #[must_use]
    pub fn part_size(&self) -> usize {
        self.part_size
    }

    /// Parts of an upload in flight at once.
    #[must_use]
    pub fn parallel(&self) -> usize {
        self.parallel.load(Ordering::Relaxed)
    }

    /// The part size and the parts in flight of the next upload: [`S3Drive::part_size`] and
    /// [`S3Drive::parallel`] until an upload measured the link, then what fits it.
    #[must_use]
    pub fn upload_plan(&self) -> (usize, usize) {
        crate::multipart::plan_parts(self.link.rate(), self.part_size, self.parallel())
    }

    /// The uplink as this drive's uploads measured it; `None` before the first big one.
    #[must_use]
    pub fn measured_link(&self) -> Option<crate::BytesPerSec> {
        self.link.rate()
    }

    /// Sends an upload of `len` bytes with `send`; one the service took teaches the link
    /// estimate how fast the link moved it (shared with the uploads in flight beside it). A
    /// refusal is not counted: a service may refuse before it read the body.
    fn timed_upload(
        &self,
        len: usize,
        send: impl FnOnce() -> Result<HttpReply, DriveError>,
    ) -> Result<HttpReply, DriveError> {
        let sharing = self.uploading.fetch_add(1, Ordering::SeqCst) + 1;
        let started = std::time::Instant::now();
        let result = send();
        self.uploading.fetch_sub(1, Ordering::SeqCst);
        if result.as_ref().is_ok_and(HttpReply::is_success) {
            self.link.sample(len, started.elapsed(), sharing);
        }
        result
    }

    /// [`Drive::put_from`] with the new version's ETag (without its quotes) when the service
    /// said it: one PUT for a body of one part or less, else a multipart upload.
    pub fn put_stream(
        &self,
        key: &str,
        body: &mut dyn Read,
    ) -> Result<(u64, Option<String>), DriveError> {
        check_s3_key(key)?;
        crate::multipart::upload_stream(self, key, body, None)
    }

    /// Where the state files of resumable uploads go, if anywhere.
    #[must_use]
    pub fn resume_dir(&self) -> Option<PathBuf> {
        self.resume_dir
            .clone()
            .or_else(crate::multipart::resume_folder)
    }

    /// Now, by the drive's clock (seconds since 1970).
    pub(crate) fn now(&self) -> u64 {
        (self.clock)()
    }

    /// Aborts this bucket's unfinished uploads in the resume folder that will not be resumed -
    /// stale, or their file is gone or changed - and forgets them; how many. Uploads that get no
    /// answer stay for the next sweep.
    pub fn abort_stale_uploads(&self) -> usize {
        crate::multipart::abort_stale(self)
    }

    #[must_use]
    pub fn config(&self) -> &S3Config {
        &self.config
    }

    /// What the storage service counts of the bucket: one HeadBucket, whose answer from an
    /// Azlin node carries [`USED_BYTES_HEADER`] and [`QUOTA_BYTES_HEADER`] (the bytes it stores
    /// for the bucket - compressed and encrypted on the devices first - and the quota of the
    /// drive's tier, both of stored bytes). Another S3 says neither: both `None`.
    ///
    /// # Errors
    ///
    /// The bucket's refusal or no answer, as every request's.
    pub fn space(&self) -> Result<BucketSpace, DriveError> {
        let reply = self.request(
            Method::Head,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "",
            CallLimits::json(),
            Idempotency::Safe,
        )?;
        if !reply.is_success() {
            return Err(failure(&reply, None));
        }
        let number = |name: &str| {
            reply
                .header(name)
                .and_then(|value| value.trim().parse::<u64>().ok())
        };
        Ok(BucketSpace {
            used_bytes: number(USED_BYTES_HEADER),
            quota_bytes: number(QUOTA_BYTES_HEADER),
        })
    }

    /// An Azlin node's signed count of the bytes the bucket holds (HeadBucket's
    /// [`azlin_contract::USAGE_PROOF_HEADER`]), as it came: the token server judges a move to a
    /// smaller tier by it. `None` from another S3 or an older node.
    ///
    /// # Errors
    ///
    /// The bucket's refusal or no answer, as every request's.
    pub fn usage_proof(&self) -> Result<Option<String>, DriveError> {
        let reply = self.request(
            Method::Head,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "",
            CallLimits::json(),
            Idempotency::Safe,
        )?;
        if !reply.is_success() {
            return Err(failure(&reply, None));
        }
        Ok(reply
            .header(azlin_contract::USAGE_PROOF_HEADER)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()))
    }

    /// The host the requests to `endpoint` go to.
    fn host_at(&self, endpoint: &Endpoint) -> String {
        if self.config.path_style {
            endpoint.authority()
        } else {
            format!("{}.{}", self.config.bucket, endpoint.authority())
        }
    }

    /// The host the requests go to.
    fn host(&self) -> String {
        self.host_at(&self.endpoint)
    }

    /// The path of the bucket itself (ListObjectsV2) at `endpoint`.
    fn bucket_path_at(&self, endpoint: &Endpoint) -> String {
        if self.config.path_style {
            format!(
                "{}/{}",
                endpoint.base_path,
                sigv4::uri_encode(&self.config.bucket, true)
            )
        } else {
            format!("{}/", endpoint.base_path)
        }
    }

    /// The path of an object at `endpoint`: the key encoded, its slashes kept.
    fn object_path_at(&self, endpoint: &Endpoint, key: &str) -> String {
        if self.config.path_style {
            format!(
                "{}/{}/{}",
                endpoint.base_path,
                sigv4::uri_encode(&self.config.bucket, true),
                sigv4::uri_encode(key, false)
            )
        } else {
            format!("{}/{}", endpoint.base_path, sigv4::uri_encode(key, false))
        }
    }

    /// The path of an object: the key encoded, its slashes kept.
    fn object_path(&self, key: &str) -> String {
        self.object_path_at(&self.endpoint, key)
    }

    /// One request on the object `key` (`None`: the bucket) signed for `endpoint`. `extra` are
    /// further headers to send and sign (`range`); `payload_hash` is the body's SHA-256.
    #[allow(clippy::too_many_arguments)]
    fn build_at(
        &self,
        endpoint: &Endpoint,
        method: Method,
        key: Option<&str>,
        query: &[(String, String)],
        extra: &[(String, String)],
        body: Vec<u8>,
        payload_hash: &str,
        content_type: &str,
        limits: CallLimits,
    ) -> HttpCall {
        let date = amz_date((self.clock)());
        let host = self.host_at(endpoint);
        let path = match key {
            Some(key) => self.object_path_at(endpoint, key),
            None => self.bucket_path_at(endpoint),
        };
        let mut headers = vec![
            (
                String::from("x-amz-content-sha256"),
                payload_hash.to_string(),
            ),
            (String::from("x-amz-date"), date.clone()),
        ];
        headers.extend(extra.iter().cloned());
        if let Some(token) = &self.credentials.session_token {
            headers.push((String::from("x-amz-security-token"), token.clone()));
        }
        let mut signed_headers = headers.clone();
        signed_headers.push((String::from("host"), host.clone()));
        let signed = sigv4::sign(
            &SigningParams {
                access_key_id: &self.credentials.access_key_id,
                secret_access_key: &self.credentials.secret_access_key,
                region: &self.config.region,
                service: "s3",
                amz_date: &date,
            },
            method.as_str(),
            &path,
            query,
            &signed_headers,
            payload_hash,
        );
        headers.push((String::from("authorization"), signed.authorization));
        let query_string = sigv4::canonical_query(query);
        let url = if query_string.is_empty() {
            format!("{}://{host}{path}", endpoint.scheme)
        } else {
            format!("{}://{host}{path}?{query_string}", endpoint.scheme)
        };
        HttpCall {
            method,
            url,
            headers,
            content_type: if body.is_empty() {
                String::new()
            } else {
                content_type.to_string()
            },
            body,
            limits,
        }
    }

    /// Sends one request on the object `key` (`None`: the bucket): to the drive's endpoint, or
    /// through its [`Router`], which signs it anew for every endpoint it tries.
    #[allow(clippy::too_many_arguments)]
    fn request(
        &self,
        method: Method,
        key: Option<&str>,
        query: Vec<(String, String)>,
        extra: Vec<(String, String)>,
        body: Vec<u8>,
        content_type: &str,
        limits: CallLimits,
        idempotency: Idempotency,
    ) -> Result<HttpReply, DriveError> {
        let payload_hash = if body.is_empty() {
            EMPTY_SHA256.to_string()
        } else {
            sigv4::sha256_hex(&body)
        };
        let Some(router) = &self.router else {
            let call = self.build_at(
                &self.endpoint,
                method,
                key,
                &query,
                &extra,
                body,
                &payload_hash,
                content_type,
                limits,
            );
            return self.send(&call);
        };
        let sign = |url: &str| -> Result<HttpCall, DriveError> {
            let same = url.trim().trim_end_matches('/')
                == self.config.endpoint.trim().trim_end_matches('/');
            let endpoint = if same {
                self.endpoint.clone()
            } else {
                Endpoint::parse(url)?
            };
            Ok(self.build_at(
                &endpoint,
                method,
                key,
                &query,
                &extra,
                body.clone(),
                &payload_hash,
                content_type,
                limits,
            ))
        };
        let probe = |url: &str, probe_key: &str| -> Result<HttpCall, DriveError> {
            check_s3_key(probe_key)?;
            let endpoint = Endpoint::parse(url)?;
            Ok(self.build_at(
                &endpoint,
                Method::Head,
                Some(probe_key),
                &[],
                &[],
                Vec::new(),
                EMPTY_SHA256,
                "",
                CallLimits::json(),
            ))
        };
        let routed = Routed::new(method, key, &self.config.endpoint, &sign)
            .with_probe(&probe)
            .with_idempotency(idempotency);
        router.send(&routed, self.transport.as_ref())
    }

    /// A link anyone holding it can download `key` with for `expires_secs` (S3 takes at most
    /// seven days): SigV4's query-string signature - the request's signature rides in the URL,
    /// the payload unsigned, the host the only signed header. Nothing is sent; the bucket answers
    /// the link's GET as if it came from these keys (AzDrive's Share > Copy link).
    pub fn presigned_get_url(&self, key: &str, expires_secs: u64) -> Result<String, DriveError> {
        /// What S3 takes as a presigned URL's lifetime at most.
        const MAX_EXPIRES_SECS: u64 = 7 * 24 * 3600;
        /// The payload hash of a presigned request: its body is not known when it is signed.
        const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";
        check_s3_key(key)?;
        let date = amz_date((self.clock)());
        let day = date.get(..8).unwrap_or(&date).to_string();
        let host = self.host();
        let path = self.object_path(key);
        let mut query = vec![
            (
                String::from("X-Amz-Algorithm"),
                String::from(sigv4::ALGORITHM),
            ),
            (
                String::from("X-Amz-Credential"),
                format!(
                    "{}/{day}/{}/s3/aws4_request",
                    self.credentials.access_key_id, self.config.region
                ),
            ),
            (String::from("X-Amz-Date"), date.clone()),
            (
                String::from("X-Amz-Expires"),
                expires_secs.clamp(1, MAX_EXPIRES_SECS).to_string(),
            ),
            (String::from("X-Amz-SignedHeaders"), String::from("host")),
        ];
        if let Some(token) = &self.credentials.session_token {
            query.push((String::from("X-Amz-Security-Token"), token.clone()));
        }
        let signed = sigv4::sign(
            &SigningParams {
                access_key_id: &self.credentials.access_key_id,
                secret_access_key: &self.credentials.secret_access_key,
                region: &self.config.region,
                service: "s3",
                amz_date: &date,
            },
            "GET",
            &path,
            &query,
            &[(String::from("host"), host.clone())],
            UNSIGNED_PAYLOAD,
        );
        query.push((String::from("X-Amz-Signature"), signed.signature));
        Ok(format!(
            "{}://{host}{path}?{}",
            self.endpoint.scheme,
            sigv4::canonical_query(&query)
        ))
    }

    /// One signed request of the S3 API that the [`Drive`] calls do not cover - a conditional
    /// write (`If-Match`, `If-None-Match: *`), a conditional read, a multipart upload: on the
    /// object `key`, or on the bucket itself without one. `query` and the `extra` headers are
    /// signed with the rest; `content_type` names the body and is not signed. The answer comes
    /// back whatever its status ([`S3Drive::failure_of`] reads a failed one); an `Err` is a key
    /// S3 cannot take or a request that got no answer at all.
    ///
    /// Its limits ([`CallLimits`]): an upload's with a body, an object's read whole for a GET,
    /// a document's otherwise; [`S3Drive::send_with`] names them. `idempotency` says whether
    /// the drive's router may send it again when its outcome is not known.
    #[allow(clippy::too_many_arguments)]
    pub fn send_raw(
        &self,
        idempotency: Idempotency,
        method: Method,
        key: Option<&str>,
        query: Vec<(String, String)>,
        extra: Vec<(String, String)>,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<HttpReply, DriveError> {
        let limits = CallLimits::for_request(method, body.len());
        self.send_with(limits, idempotency, method, key, query, extra, body, content_type)
    }

    /// [`S3Drive::send_raw`] with the limits the caller knows (a listing's, a range's).
    #[allow(clippy::too_many_arguments)]
    pub fn send_with(
        &self,
        limits: CallLimits,
        idempotency: Idempotency,
        method: Method,
        key: Option<&str>,
        query: Vec<(String, String)>,
        extra: Vec<(String, String)>,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<HttpReply, DriveError> {
        if let Some(key) = key {
            check_s3_key(key)?;
        }
        self.request(method, key, query, extra, body, content_type, limits, idempotency)
    }

    /// The error of a failed answer to a request on `key` (`None`: on the bucket itself): a
    /// missing key and a bad range are their own variants, everything else the service's
    /// [`crate::ServiceError`].
    #[must_use]
    pub fn failure_of(reply: &HttpReply, key: Option<&str>) -> DriveError {
        failure(reply, key)
    }

    /// DeleteObjects: `keys` (at most [`DELETE_BATCH`]) deleted in one request, in quiet mode
    /// (the answer names only the keys that were not deleted). One result per key, in order; a
    /// key that was not there counts as deleted. A service without the call (405, 501) gets one
    /// DELETE per key instead.
    pub fn delete_objects(&self, keys: &[&str]) -> Vec<Result<(), DriveError>> {
        if keys.is_empty() {
            return Vec::new();
        }
        let mut body = String::from("<Delete><Quiet>true</Quiet>");
        for key in keys {
            body.push_str("<Object><Key>");
            body.push_str(&xml_escape(key));
            body.push_str("</Key></Object>");
        }
        body.push_str("</Delete>");
        let md5 = content_md5(body.as_bytes());
        let reply = match self.send_raw(
            Idempotency::Safe,
            Method::Post,
            None,
            vec![(String::from("delete"), String::new())],
            vec![(String::from("content-md5"), md5)],
            body.into_bytes(),
            "application/xml",
        ) {
            Ok(reply) => reply,
            Err(e) => return keys.iter().map(|_| Err(e.clone())).collect(),
        };
        if matches!(reply.status, 405 | 501) {
            return keys.iter().map(|key| self.delete(key)).collect();
        }
        if !reply.is_success() {
            let e = failure(&reply, None);
            return keys.iter().map(|_| Err(e.clone())).collect();
        }
        let text = String::from_utf8_lossy(&reply.body);
        let errors = if text.trim().is_empty() {
            Vec::new()
        } else {
            match xml::parse_delete_errors(&text) {
                Ok(errors) => errors,
                Err(e) => return keys.iter().map(|_| Err(e.clone())).collect(),
            }
        };
        keys.iter()
            .map(|key| match errors.iter().find(|(named, _, _)| named == key) {
                Some((_, code, message)) if code != "NoSuchKey" => {
                    Err(DriveError::Service(ServiceError {
                        status: reply.status,
                        code: code.clone(),
                        message: message.clone(),
                        resource: Some((*key).to_string()),
                        ..ServiceError::default()
                    }))
                }
                _ => Ok(()),
            })
            .collect()
    }

    /// PutObject of `bytes`: the new version's ETag, when the service sent one.
    pub(crate) fn put_object(
        &self,
        key: &str,
        bytes: Vec<u8>,
    ) -> Result<Option<String>, DriveError> {
        let len = bytes.len();
        let reply = self.timed_upload(len, || {
            self.object_call(
                Method::Put,
                key,
                Vec::new(),
                bytes,
                CallLimits::upload(),
                Idempotency::Safe,
            )
        })?;
        if !reply.is_success() {
            return Err(failure(&reply, Some(key)));
        }
        Ok(reply
            .header("etag")
            .map(xml::strip_quotes)
            .filter(|e| !e.is_empty()))
    }

    /// CreateMultipartUpload: the upload's id.
    pub fn start_multipart(&self, key: &str) -> Result<String, DriveError> {
        let reply = self.send_raw(
            Idempotency::Unsafe(Verify::LeavesEmptyUpload),
            Method::Post,
            Some(key),
            vec![(String::from("uploads"), String::new())],
            Vec::new(),
            Vec::new(),
            "",
        )?;
        if !reply.is_success() {
            return Err(failure(&reply, Some(key)));
        }
        xml::first_text(&String::from_utf8_lossy(&reply.body), "UploadId")
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                DriveError::Protocol(format!(
                    "{key}: the service started no multipart upload (no UploadId)"
                ))
            })
    }

    /// UploadPart `number` (from 1) of the upload `upload`: the part's ETag as the service sent
    /// it (quotes included).
    pub fn upload_part(
        &self,
        key: &str,
        upload: &str,
        number: usize,
        bytes: Vec<u8>,
    ) -> Result<String, DriveError> {
        let query = vec![
            (String::from("partNumber"), number.to_string()),
            (String::from("uploadId"), upload.to_string()),
        ];
        let len = bytes.len();
        let reply = self.timed_upload(len, || {
            self.send_raw(
                Idempotency::Safe,
                Method::Put,
                Some(key),
                query,
                Vec::new(),
                bytes,
                "application/octet-stream",
            )
        })?;
        if !reply.is_success() {
            return Err(failure(&reply, Some(key)));
        }
        reply
            .header("etag")
            .map(str::trim)
            .filter(|etag| !etag.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                DriveError::Protocol(format!("{key}: part {number} came back without an ETag"))
            })
    }

    /// CompleteMultipartUpload of `parts` (number, ETag), only when `condition` holds when it is
    /// asked (S3 checks `If-None-Match: *` / `If-Match` on the completion): the new version's
    /// ETag when the service says it; [`DriveError::Conflict`] for a 412 (nothing was written;
    /// the parts are still there - abort them). A completion whose earlier answer was lost is
    /// answered `NoSuchUpload` or 412 when asked again: the key holding the upload's own ETag
    /// ([`multipart_etag`]) says it was done, so it is.
    pub fn complete_multipart(
        &self,
        key: &str,
        upload: &str,
        parts: &[(usize, String)],
        condition: Option<&Precondition>,
    ) -> Result<Option<String>, DriveError> {
        let mut manifest = String::from("<CompleteMultipartUpload>");
        for (number, etag) in parts {
            manifest.push_str(&format!(
                "<Part><PartNumber>{number}</PartNumber><ETag>{}</ETag></Part>",
                xml_escape(etag)
            ));
        }
        manifest.push_str("</CompleteMultipartUpload>");
        let extra: Vec<(String, String)> = condition.map(condition_header).into_iter().collect();
        let expected = multipart_etag(parts);
        let done_before = || {
            expected
                .as_deref()
                .is_some_and(|etag| self.holds_version(key, etag))
        };
        let mut attempt = 1;
        let reply = loop {
            let reply = self.send_raw(
                Idempotency::Unsafe(Verify::Completion {
                    etag: expected.clone(),
                }),
                Method::Post,
                Some(key),
                vec![(String::from("uploadId"), upload.to_string())],
                extra.clone(),
                manifest.clone().into_bytes(),
                "application/xml",
            )?;
            if reply.status == 412 {
                if done_before() {
                    return Ok(expected.clone());
                }
                return Err(DriveError::Conflict {
                    key: key.to_string(),
                });
            }
            if reply.status == 404 && done_before() {
                return Ok(expected.clone());
            }
            // Another conditional write in flight (409): nothing was written; ask again.
            if reply.status == 409 && condition.is_some() && attempt < CONDITIONAL_TRIES {
                conflict_pause(attempt);
                attempt += 1;
                continue;
            }
            break reply;
        };
        if !reply.is_success() {
            return Err(failure(&reply, Some(key)));
        }
        // A 200 can still carry an error: the service answers early and finishes later.
        let done = String::from_utf8_lossy(&reply.body);
        if done.contains("<Error>") {
            let error = xml::parse_error(500, &done);
            if error.code == "PreconditionFailed" {
                if done_before() {
                    return Ok(expected.clone());
                }
                return Err(DriveError::Conflict {
                    key: key.to_string(),
                });
            }
            return Err(DriveError::Service(error));
        }
        Ok(reply
            .header("etag")
            .map(str::to_string)
            .or_else(|| xml::first_text(&done, "ETag"))
            .map(|e| xml::strip_quotes(&e))
            .filter(|e| !e.is_empty()))
    }

    /// Whether `key` is now the version with the entity tag `etag` (one HEAD; no answer is no).
    fn holds_version(&self, key: &str, etag: &str) -> bool {
        matches!(Drive::head(self, key), Ok(info)
            if info.etag.as_deref().is_some_and(|found| found.eq_ignore_ascii_case(etag)))
    }

    /// AbortMultipartUpload: the parts sent so far are not left (and billed) behind.
    pub fn abort_multipart(&self, key: &str, upload: &str) -> Result<(), DriveError> {
        let reply = self.send_raw(
            Idempotency::Safe,
            Method::Delete,
            Some(key),
            vec![(String::from("uploadId"), upload.to_string())],
            Vec::new(),
            Vec::new(),
            "",
        )?;
        if reply.is_success() {
            Ok(())
        } else {
            Err(failure(&reply, Some(key)))
        }
    }

    /// ListParts: the parts the service holds of the upload `upload`, page by page.
    pub fn list_parts(
        &self,
        key: &str,
        upload: &str,
    ) -> Result<Vec<crate::multipart::UploadedPart>, DriveError> {
        let mut out = Vec::new();
        let mut marker: Option<String> = None;
        loop {
            let mut query = vec![(String::from("uploadId"), upload.to_string())];
            if let Some(marker) = &marker {
                query.push((String::from("part-number-marker"), marker.clone()));
            }
            let reply = self.send_with(
                CallLimits::listing(),
                Idempotency::Safe,
                Method::Get,
                Some(key),
                query,
                Vec::new(),
                Vec::new(),
                "",
            )?;
            if !reply.is_success() {
                return Err(failure(&reply, Some(key)));
            }
            let (parts, next) = xml::parse_parts(&String::from_utf8_lossy(&reply.body))?;
            out.extend(parts);
            match next {
                Some(next) if marker.as_deref() != Some(next.as_str()) => marker = Some(next),
                _ => return Ok(out),
            }
        }
    }

    fn send(&self, call: &HttpCall) -> Result<HttpReply, DriveError> {
        self.transport.send(call).map_err(DriveError::Transport)
    }

    /// A request on one object.
    fn object_call(
        &self,
        method: Method,
        key: &str,
        extra: Vec<(String, String)>,
        body: Vec<u8>,
        limits: CallLimits,
        idempotency: Idempotency,
    ) -> Result<HttpReply, DriveError> {
        check_s3_key(key)?;
        self.request(
            method,
            Some(key),
            Vec::new(),
            extra,
            body,
            content_type_for(key),
            limits,
            idempotency,
        )
    }
}

impl Drive for S3Drive {
    /// Its own resume folder, else the app's ([`crate::multipart::set_resume_folder`]).
    fn resume_folder(&self) -> Option<PathBuf> {
        self.resume_dir()
    }

    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let mut query = vec![
            (String::from("list-type"), String::from("2")),
            (String::from("prefix"), request.prefix.clone()),
            (String::from("max-keys"), request.page_size().to_string()),
        ];
        if let Some(delimiter) = &request.delimiter {
            query.push((String::from("delimiter"), delimiter.clone()));
        }
        if let Some(token) = &request.continuation {
            query.push((String::from("continuation-token"), token.clone()));
        }
        let reply = self.request(
            Method::Get,
            None,
            query,
            Vec::new(),
            Vec::new(),
            "",
            CallLimits::listing(),
            Idempotency::Safe,
        )?;
        if !reply.is_success() {
            return Err(failure(&reply, None));
        }
        let text = std::str::from_utf8(&reply.body)
            .map_err(|_| DriveError::Protocol(String::from("the listing is not UTF-8")))?;
        xml::parse_list(text)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let reply = self.object_call(
            Method::Get,
            key,
            Vec::new(),
            Vec::new(),
            CallLimits::whole_object(),
            Idempotency::Safe,
        )?;
        if reply.is_success() {
            Ok(reply.body)
        } else {
            Err(failure(&reply, Some(key)))
        }
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let reply = self.object_call(
            Method::Get,
            key,
            vec![(String::from("range"), range.header_value())],
            Vec::new(),
            CallLimits::range(range),
            Idempotency::Safe,
        )?;
        match reply.status {
            206 => Ok(reply.body),
            // A server that ignores `Range` sends the whole object: cut the range out.
            200 => {
                let len = reply.body.len() as u64;
                if range.start >= len {
                    return Err(DriveError::InvalidRange {
                        key: key.to_string(),
                    });
                }
                let end = range.end.map_or(len - 1, |end| end.min(len - 1));
                Ok(reply.body[range.start as usize..=end as usize].to_vec())
            }
            _ => Err(failure(&reply, Some(key))),
        }
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.put_object(key, bytes.to_vec()).map(|_| ())
    }

    /// One PutObject for a body of one part or less ([`PART_SIZE`]); a bigger one goes up as a
    /// multipart upload, [`PARALLEL_PARTS`] parts at once (that many in memory), read to its
    /// end before it is completed, aborted when anything fails ([`crate::multipart`]).
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        check_s3_key(key)?;
        crate::multipart::upload_stream(self, key, body, None).map(|(written, _)| written)
    }

    /// [`S3Drive::put_from`]'s upload, completed only when `condition` holds (a body of one
    /// part or less: [`Drive::put_if`]); a 412 is [`DriveError::Conflict`] and the parts are
    /// aborted.
    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        check_s3_key(key)?;
        crate::multipart::upload_stream(self, key, body, Some(condition)).map(|(_, etag)| etag)
    }

    /// One PutObject for a file of one part or less; a bigger one goes up as a multipart
    /// upload, [`PARALLEL_PARTS`] parts at once, read where each part lies - resumable with a
    /// resume folder: an upload a killed app left behind goes on with the parts that are
    /// missing ([`crate::multipart`]).
    fn put_file(
        &self,
        key: &str,
        path: &Path,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<u64, DriveError> {
        check_s3_key(key)?;
        crate::multipart::upload_file(self, key, path, None, progress).map(|(written, _)| written)
    }

    /// [`Drive::put_file`] that never completes with bytes other than `check` names: the file
    /// must be as scanned when the upload starts and when its parts are complete (hashed whole
    /// once more then); a cut-off upload resumes from its state file with the missing parts.
    fn put_file_checked(
        &self,
        key: &str,
        path: &Path,
        check: &crate::checked::FileCheck,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<crate::checked::CheckedUpload, DriveError> {
        check_s3_key(key)?;
        crate::multipart::upload_file_checked(self, key, path, check, progress)
    }

    /// One PutObject with `If-None-Match: *` or `If-Match: "<etag>"` (AWS S3, R2 and
    /// MinIO honour both); a 412 is [`DriveError::Conflict`].
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let header = condition_header(condition);
        // A 409 (ConditionalRequestConflict: another conditional write of the object was in
        // flight, nothing was written) is sent again after a short random wait - the condition
        // is checked anew, so a write that went through meanwhile is a 412 then. After
        // CONDITIONAL_TRIES the 409 is the answer.
        let mut attempt = 1;
        loop {
            let reply = self.object_call(
                Method::Put,
                key,
                vec![header.clone()],
                bytes.to_vec(),
                CallLimits::upload(),
                Idempotency::Conditional,
            )?;
            if reply.is_success() {
                return Ok(reply
                    .header("etag")
                    .map(xml::strip_quotes)
                    .filter(|e| !e.is_empty()));
            }
            if reply.status == 412 {
                return Err(DriveError::Conflict {
                    key: key.to_string(),
                });
            }
            if reply.status == 409 && attempt < CONDITIONAL_TRIES {
                conflict_pause(attempt);
                attempt += 1;
                continue;
            }
            return Err(failure(&reply, Some(key)));
        }
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        let reply = self.object_call(
            Method::Delete,
            key,
            Vec::new(),
            Vec::new(),
            CallLimits::json(),
            Idempotency::Safe,
        )?;
        if reply.is_success() {
            return Ok(());
        }
        match failure(&reply, Some(key)) {
            // Deleting what is not there is done (S3 itself answers 204).
            DriveError::NotFound { .. } => Ok(()),
            other => Err(other),
        }
    }

    /// [`crate::PARALLEL_FILES`] uploads at once, each its own (multipart when big) upload.
    fn put_from_many(
        &self,
        items: &mut [BatchStream<'_>],
    ) -> Vec<Result<Option<String>, DriveError>> {
        ops::put_from_parallel(self, items)
    }

    /// The versions an item names checked with a HEAD each (an edit made elsewhere beats a
    /// delete), then [`DELETE_BATCH`] keys a request ([`S3Drive::delete_objects`]).
    fn delete_many(&self, items: &[BatchDelete<'_>]) -> Vec<Result<(), DriveError>> {
        let mut results: Vec<Option<Result<(), DriveError>>> = vec![None; items.len()];
        let mut go: Vec<usize> = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if let Err(e) = check_s3_key(item.key) {
                results[i] = Some(Err(e));
                continue;
            }
            if let Some(version) = item.version {
                match self.head(item.key) {
                    Ok(info) if same_etag(info.etag.as_deref(), version) => {}
                    Ok(_) => {
                        results[i] = Some(Err(DriveError::Conflict {
                            key: item.key.to_string(),
                        }));
                        continue;
                    }
                    Err(DriveError::NotFound { .. }) => {
                        results[i] = Some(Ok(()));
                        continue;
                    }
                    Err(e) => {
                        results[i] = Some(Err(e));
                        continue;
                    }
                }
            }
            go.push(i);
        }
        for chunk in go.chunks(DELETE_BATCH) {
            let keys: Vec<&str> = chunk.iter().map(|&i| items[i].key).collect();
            for (&i, result) in chunk.iter().zip(self.delete_objects(&keys)) {
                results[i] = Some(result);
            }
        }
        results
            .into_iter()
            .map(|result| result.unwrap_or(Ok(())))
            .collect()
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let reply = self.object_call(
            Method::Head,
            key,
            Vec::new(),
            Vec::new(),
            CallLimits::json(),
            Idempotency::Safe,
        )?;
        if !reply.is_success() {
            return Err(failure(&reply, Some(key)));
        }
        Ok(ObjectInfo {
            key: key.to_string(),
            size: reply
                .header("content-length")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(0),
            modified: reply.header("last-modified").and_then(parse_http_date),
            etag: reply
                .header("etag")
                .map(xml::strip_quotes)
                .filter(|e| !e.is_empty()),
        })
    }

    /// One CopyObject: the service copies, nothing passes through here (an
    /// object up to 5 GB). A 200 answer can still carry an error body.
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        check_s3_key(from)?;
        if let Some(folder) = [from, to].into_iter().find(|k| k.ends_with('/')) {
            return Err(DriveError::InvalidKey {
                key: folder.to_string(),
                reason: "a folder is copied object by object",
            });
        }
        let source = format!("/{}/{}", self.config.bucket, sigv4::uri_encode(from, false));
        let reply = self.object_call(
            Method::Put,
            to,
            vec![(String::from("x-amz-copy-source"), source)],
            Vec::new(),
            CallLimits::json(),
            Idempotency::Safe,
        )?;
        if !reply.is_success() {
            return Err(failure(&reply, Some(from)));
        }
        let body = String::from_utf8_lossy(&reply.body);
        if body.contains("<Error>") {
            return Err(DriveError::Service(xml::parse_error(500, &body)));
        }
        if !body.contains("CopyObjectResult") {
            // A server that ignored `x-amz-copy-source` wrote an empty object.
            return Err(DriveError::Protocol(format!(
                "{from}: the service did not confirm the copy (no CopyObjectResult)"
            )));
        }
        Ok(())
    }

    /// One HEAD: the headers that describe the object (not the request), by
    /// readable names; `x-amz-meta-<name>` as `<name>`.
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        let reply = self.object_call(
            Method::Head,
            key,
            Vec::new(),
            Vec::new(),
            CallLimits::json(),
            Idempotency::Safe,
        )?;
        if !reply.is_success() {
            return Err(failure(&reply, Some(key)));
        }
        let mut pairs = Vec::new();
        for (name, value) in &reply.headers {
            let lower = name.to_ascii_lowercase();
            let shown = match lower.as_str() {
                "content-type" => "Content-Type".to_string(),
                "content-encoding" => "Content-Encoding".to_string(),
                "content-disposition" => "Content-Disposition".to_string(),
                "content-language" => "Content-Language".to_string(),
                "cache-control" => "Cache-Control".to_string(),
                "expires" => "Expires".to_string(),
                "etag" => "ETag".to_string(),
                "x-amz-storage-class" => "Storage class".to_string(),
                "x-amz-server-side-encryption" => "Encryption".to_string(),
                "x-amz-version-id" => "Version".to_string(),
                "x-amz-website-redirect-location" => "Redirect".to_string(),
                other => match other.strip_prefix("x-amz-meta-") {
                    Some(user) if !user.is_empty() => user.to_string(),
                    _ => continue,
                },
            };
            let value = if lower == "etag" {
                xml::strip_quotes(value)
            } else {
                value.trim().to_string()
            };
            pairs.push((shown, value));
        }
        Ok(pairs)
    }
}
