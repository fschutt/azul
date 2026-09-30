//! AWS Signature Version 4, header-based, for S3-compatible services.
//!
//! <https://docs.aws.amazon.com/IAM/latest/UserGuide/create-signed-request.html>:
//! canonical request -> string to sign -> signature over a key derived from the
//! secret, the date, the region and the service. SHA-256 and HMAC come from
//! the RustCrypto crates already in azul's tree (`sha2`, `hmac`).
//!
//! S3 differs from the generic suite in one place: the canonical URI is the
//! path as sent, encoded once (no double encoding, no dot-segment removal).

use std::collections::BTreeMap;

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// The algorithm name in the string to sign and the `Authorization` header.
pub const ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// SHA-256 of an empty body, the payload hash of every request without one.
pub const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";
const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

/// Who signs, for which region and service, at which time.
#[derive(Clone, Copy)]
pub struct SigningParams<'a> {
    pub access_key_id: &'a str,
    pub secret_access_key: &'a str,
    pub region: &'a str,
    pub service: &'a str,
    /// `20150830T123600Z`, the same value as the request's `x-amz-date` header.
    pub amz_date: &'a str,
}

/// The steps of one signature, for tests and for debugging a signature mismatch
/// (none of them holds the secret).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub canonical_request: String,
    pub string_to_sign: String,
    pub signed_headers: String,
    pub signature: String,
    /// The `Authorization` header value.
    pub authorization: String,
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX_LOWER[usize::from(b >> 4)] as char);
        out.push(HEX_LOWER[usize::from(b & 0x0f)] as char);
    }
    out
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(data);
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

/// Lowercase hex of the SHA-256 of `data`.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// URI-encodes as SigV4 wants: `A-Z a-z 0-9 - _ . ~` stay, `/` stays unless
/// `encode_slash`, every other byte of the UTF-8 becomes `%XX` (uppercase).
#[must_use]
pub fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        let keep = b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'_' | b'.' | b'~')
            || (b == b'/' && !encode_slash);
        if keep {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX_UPPER[usize::from(b >> 4)] as char);
            out.push(HEX_UPPER[usize::from(b & 0x0f)] as char);
        }
    }
    out
}

/// The canonical query string: every name and value encoded, sorted by name
/// then value, joined with `&`. The same string is the query of the URL, so what
/// is sent is what was signed.
#[must_use]
pub fn canonical_query(params: &[(String, String)]) -> String {
    let mut encoded: Vec<(String, String)> = params
        .iter()
        .map(|(name, value)| (uri_encode(name, true), uri_encode(value, true)))
        .collect();
    encoded.sort();
    encoded
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// The canonical headers block (`name:value\n` per header, lowercase names in
/// order, values trimmed with inner runs of spaces collapsed, repeated names
/// joined with `,`) and the signed-headers list (`host;x-amz-date`).
fn canonical_headers(headers: &[(String, String)]) -> (String, String) {
    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in headers {
        by_name
            .entry(name.trim().to_ascii_lowercase())
            .or_default()
            .push(value.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    let mut block = String::new();
    for (name, values) in &by_name {
        block.push_str(name);
        block.push(':');
        block.push_str(&values.join(","));
        block.push('\n');
    }
    let signed = by_name.keys().cloned().collect::<Vec<_>>().join(";");
    (block, signed)
}

/// Signs one request. `canonical_uri` is the path as sent (already encoded);
/// `query` holds the raw (unencoded) parameters; `headers` every header to sign,
/// `Host` included, names in any case.
#[must_use]
pub fn sign(
    params: &SigningParams<'_>,
    method: &str,
    canonical_uri: &str,
    query: &[(String, String)],
    headers: &[(String, String)],
    payload_hash: &str,
) -> Signed {
    let (header_block, signed_headers) = canonical_headers(headers);
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{}\n{header_block}\n{signed_headers}\n{payload_hash}",
        canonical_query(query)
    );
    let date = params.amz_date.get(..8).unwrap_or(params.amz_date);
    let scope = format!("{date}/{}/{}/aws4_request", params.region, params.service);
    let string_to_sign = format!(
        "{ALGORITHM}\n{}\n{scope}\n{}",
        params.amz_date,
        sha256_hex(canonical_request.as_bytes())
    );
    let date_key = hmac_sha256(
        format!("AWS4{}", params.secret_access_key).as_bytes(),
        date.as_bytes(),
    );
    let region_key = hmac_sha256(&date_key, params.region.as_bytes());
    let service_key = hmac_sha256(&region_key, params.service.as_bytes());
    let signing_key = hmac_sha256(&service_key, b"aws4_request");
    let signature = hex(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        params.access_key_id
    );
    Signed {
        canonical_request,
        string_to_sign,
        signed_headers,
        signature,
        authorization,
    }
}
