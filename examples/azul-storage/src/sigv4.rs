//! AWS Signature Version 4, header-based, for S3-compatible services.
//!
//! <https://docs.aws.amazon.com/IAM/latest/UserGuide/create-signed-request.html>:
//! canonical request -> string to sign -> signature over a key derived from the
//! secret, the date, the region and the service. SHA-256 and HMAC come from
//! the RustCrypto crates already in azul's tree (`sha2`, `hmac`).
//!
//! S3 differs from the generic suite in one place: the canonical URI is the
//! path as sent, encoded once (no double encoding, no dot-segment removal).

/// The algorithm name in the string to sign and the `Authorization` header.
pub const ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// SHA-256 of an empty body, the payload hash of every request without one.
pub const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

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

/// Lowercase hex of the SHA-256 of `data`.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    let _ = data;
    todo!("RED")
}

/// URI-encodes as SigV4 wants: `A-Z a-z 0-9 - _ . ~` stay, `/` stays unless
/// `encode_slash`, every other byte of the UTF-8 becomes `%XX` (uppercase).
#[must_use]
pub fn uri_encode(input: &str, encode_slash: bool) -> String {
    let _ = (input, encode_slash);
    todo!("RED")
}

/// The canonical query string: every name and value encoded, sorted by name
/// then value, joined with `&`. The same string is the query of the URL, so what
/// is sent is what was signed.
#[must_use]
pub fn canonical_query(params: &[(String, String)]) -> String {
    let _ = params;
    todo!("RED")
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
    let _ = (params, method, canonical_uri, query, headers, payload_hash);
    todo!("RED")
}
