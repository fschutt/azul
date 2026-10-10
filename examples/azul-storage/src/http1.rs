//! One request and its answer as HTTP/1.1 bytes: what S3 over iroh sends on a bidirectional QUIC
//! stream to an Azlin node ([`S3_OVER_IROH_ALPN`], one request per stream) - the same signed
//! request that goes over TLS to the block endpoint. `Host` is the URL's authority (SigV4 signed
//! it), the body goes with its length, and `Connection: close` makes the node finish its side of
//! the stream after the answer, which is how the answer's end is known (the node's HTTP server
//! reads a stream this side finished early as a broken request, so this side stays open).
//!
//! Plain Rust without azul: the apps' transport over azul's iroh endpoint
//! (`crate::azul_iroh`, feature `azul`) sends these bytes.

use crate::{HttpCall, HttpReply, Method};

/// The ALPN of S3 over iroh: the protocol name an Azlin node's iroh endpoint accepts.
pub const S3_OVER_IROH_ALPN: &str = "azlin/s3/1";

/// Whether `text` can stand in a request head (no line break).
fn fits_a_line(text: &str) -> bool {
    !text.contains(['\r', '\n'])
}

/// The authority (`host[:port]`) and the request target (`/path?query`) of `url`.
fn split_url(url: &str) -> Result<(&str, String), String> {
    let (_, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("{url:?} is no http(s) URL"))?;
    let rest = rest.split('#').next().unwrap_or_default();
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, target) = rest.split_at(end);
    if authority.is_empty() {
        return Err(format!("{url:?} names no host"));
    }
    let target = if target.is_empty() {
        String::from("/")
    } else if target.starts_with('?') {
        format!("/{target}")
    } else {
        target.to_string()
    };
    Ok((authority, target))
}

/// `call` as the bytes of one HTTP/1.1 request.
///
/// # Errors
///
/// A URL that is none, or a header with a line break.
pub fn encode_request(call: &HttpCall) -> Result<Vec<u8>, String> {
    let (authority, target) = split_url(&call.url)?;
    if !fits_a_line(&target) {
        return Err(format!("{:?} cannot be sent", call.url));
    }
    let mut head = format!("{} {target} HTTP/1.1\r\n", call.method.as_str());
    let named = |name: &str| {
        call.headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
    };
    if !named("host") {
        head.push_str(&format!("host: {authority}\r\n"));
    }
    for (name, value) in &call.headers {
        if name.trim().is_empty() || !fits_a_line(name) || !fits_a_line(value) {
            return Err(format!("the header {name:?} cannot be sent"));
        }
        if name.eq_ignore_ascii_case("connection") || name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if !call.content_type.is_empty() && !named("content-type") {
        if !fits_a_line(&call.content_type) {
            return Err(String::from("the content type cannot be sent"));
        }
        head.push_str(&format!("content-type: {}\r\n", call.content_type));
    }
    let takes_body = matches!(call.method, Method::Put | Method::Post | Method::Patch);
    if takes_body || !call.body.is_empty() {
        head.push_str(&format!("content-length: {}\r\n", call.body.len()));
    }
    head.push_str("connection: close\r\n\r\n");
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(&call.body);
    Ok(bytes)
}

/// Where `needle` starts in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A chunked body put together (its trailers are dropped).
fn dechunk(mut body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let end = find(body, b"\r\n").ok_or("a chunk without its size line")?;
        let line = std::str::from_utf8(&body[..end]).map_err(|_| "a chunk size that is no text")?;
        let size_text = line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| format!("{size_text:?} is no chunk size"))?;
        body = &body[end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size + 2 {
            return Err(String::from("the answer broke off inside a chunk"));
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

/// The answer in `bytes` (everything the node wrote until it finished its side); `head_request`:
/// the answer of a HEAD, which has no body whatever its length says. Interim answers (`100
/// Continue`) are skipped.
///
/// # Errors
///
/// Bytes that are no HTTP/1.x answer, or an answer that broke off.
pub fn decode_reply(bytes: &[u8], head_request: bool) -> Result<HttpReply, String> {
    let mut rest = bytes;
    loop {
        let end = find(rest, b"\r\n\r\n").ok_or("the answer has no complete head")?;
        let head = std::str::from_utf8(&rest[..end]).map_err(|_| "the answer's head is no text")?;
        let body = &rest[end + 4..];
        let mut lines = head.split("\r\n");
        let status_line = lines.next().unwrap_or_default();
        let mut parts = status_line.splitn(3, ' ');
        if !parts.next().unwrap_or_default().starts_with("HTTP/1.") {
            return Err(format!("{status_line:?} is no HTTP/1.1 answer"));
        }
        let status: u16 = parts
            .next()
            .and_then(|code| code.trim().parse().ok())
            .ok_or_else(|| format!("{status_line:?} has no status"))?;
        let mut headers: Vec<(String, String)> = Vec::new();
        for line in lines.filter(|line| !line.is_empty()) {
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| format!("{line:?} is no header"))?;
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
        if (100..200).contains(&status) {
            rest = body;
            continue;
        }
        let header = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        };
        let chunked = header("transfer-encoding")
            .is_some_and(|coding| coding.to_ascii_lowercase().contains("chunked"));
        let body = if head_request || status == 204 || status == 304 {
            Vec::new()
        } else if chunked {
            dechunk(body)?
        } else if let Some(length) = header("content-length") {
            let length: usize = length
                .parse()
                .map_err(|_| format!("{length:?} is no length"))?;
            if body.len() < length {
                return Err(format!(
                    "the answer broke off after {} of {length} bytes",
                    body.len()
                ));
            }
            body[..length].to_vec()
        } else {
            body.to_vec()
        };
        return Ok(HttpReply {
            status,
            headers,
            body,
        });
    }
}
