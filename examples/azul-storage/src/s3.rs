//! An S3-compatible bucket as a drive: AWS S3, Cloudflare R2, MinIO.
//!
//! Five calls of the S3 API, each signed with SigV4: ListObjectsV2 (with
//! continuation tokens), GetObject (with `Range`), PutObject, DeleteObject and
//! HeadObject. Error answers become [`ServiceError`]s that say what the
//! service said. The requests are built here and sent through a
//! [`Transport`], so the same code runs over azul's HTTP client in the apps and
//! over a recording fake in the tests.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    sigv4::{self, SigningParams, EMPTY_SHA256},
    time::{amz_date, now_unix, parse_http_date},
    xml, ByteRange, Drive, DriveError, HttpCall, HttpReply, ListPage, ListRequest, Method,
    ObjectInfo, Transport,
};

/// S3's longest key, in bytes.
const MAX_KEY_BYTES: usize = 1024;

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

/// The error of a failed answer: a missing key and a bad range become their own
/// variants, everything else a readable [`ServiceError`].
fn failure(reply: &HttpReply, key: Option<&str>) -> DriveError {
    let error = xml::parse_error(reply.status, &String::from_utf8_lossy(&reply.body));
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

/// A bucket, reached through a [`Transport`].
pub struct S3Drive {
    config: S3Config,
    endpoint: Endpoint,
    credentials: Credentials,
    transport: Box<dyn Transport>,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
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
        })
    }

    /// Signs with this clock (seconds since 1970) instead of the system's.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    #[must_use]
    pub fn config(&self) -> &S3Config {
        &self.config
    }

    /// The host the requests go to.
    fn host(&self) -> String {
        if self.config.path_style {
            self.endpoint.authority()
        } else {
            format!("{}.{}", self.config.bucket, self.endpoint.authority())
        }
    }

    /// The path of the bucket itself (ListObjectsV2).
    fn bucket_path(&self) -> String {
        if self.config.path_style {
            format!(
                "{}/{}",
                self.endpoint.base_path,
                sigv4::uri_encode(&self.config.bucket, true)
            )
        } else {
            format!("{}/", self.endpoint.base_path)
        }
    }

    /// The path of an object: the key encoded, its slashes kept.
    fn object_path(&self, key: &str) -> String {
        if self.config.path_style {
            format!(
                "{}/{}/{}",
                self.endpoint.base_path,
                sigv4::uri_encode(&self.config.bucket, true),
                sigv4::uri_encode(key, false)
            )
        } else {
            format!(
                "{}/{}",
                self.endpoint.base_path,
                sigv4::uri_encode(key, false)
            )
        }
    }

    /// One signed request. `extra` are further headers to send and sign (`range`).
    fn build(
        &self,
        method: Method,
        path: String,
        query: Vec<(String, String)>,
        extra: Vec<(String, String)>,
        body: Vec<u8>,
        content_type: &str,
    ) -> HttpCall {
        let date = amz_date((self.clock)());
        let payload_hash = if body.is_empty() {
            EMPTY_SHA256.to_string()
        } else {
            sigv4::sha256_hex(&body)
        };
        let host = self.host();
        let mut headers = vec![
            (String::from("x-amz-content-sha256"), payload_hash.clone()),
            (String::from("x-amz-date"), date.clone()),
        ];
        headers.extend(extra);
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
            &query,
            &signed_headers,
            &payload_hash,
        );
        headers.push((String::from("authorization"), signed.authorization));
        let query_string = sigv4::canonical_query(&query);
        let url = if query_string.is_empty() {
            format!("{}://{host}{path}", self.endpoint.scheme)
        } else {
            format!("{}://{host}{path}?{query_string}", self.endpoint.scheme)
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
    ) -> Result<HttpReply, DriveError> {
        check_s3_key(key)?;
        let call = self.build(
            method,
            self.object_path(key),
            Vec::new(),
            extra,
            body,
            content_type_for(key),
        );
        self.send(&call)
    }
}

impl Drive for S3Drive {
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
        let call = self.build(
            Method::Get,
            self.bucket_path(),
            query,
            Vec::new(),
            Vec::new(),
            "",
        );
        let reply = self.send(&call)?;
        if !reply.is_success() {
            return Err(failure(&reply, None));
        }
        let text = std::str::from_utf8(&reply.body)
            .map_err(|_| DriveError::Protocol(String::from("the listing is not UTF-8")))?;
        xml::parse_list(text)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let reply = self.object_call(Method::Get, key, Vec::new(), Vec::new())?;
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
        let reply = self.object_call(Method::Put, key, Vec::new(), bytes.to_vec())?;
        if reply.is_success() {
            Ok(())
        } else {
            Err(failure(&reply, Some(key)))
        }
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        let reply = self.object_call(Method::Delete, key, Vec::new(), Vec::new())?;
        if reply.is_success() {
            return Ok(());
        }
        match failure(&reply, Some(key)) {
            // Deleting what is not there is done (S3 itself answers 204).
            DriveError::NotFound { .. } => Ok(()),
            other => Err(other),
        }
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let reply = self.object_call(Method::Head, key, Vec::new(), Vec::new())?;
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

    /// One HEAD: the headers that describe the object (not the request), by
    /// readable names; `x-amz-meta-<name>` as `<name>`.
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        let reply = self.object_call(Method::Head, key, Vec::new(), Vec::new())?;
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
