//! Small helpers without further dependencies: webhook signatures, ids, form bodies, dates, and
//! the checks of the test data.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

/// Lowercase hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    out
}

/// The hex HMAC-SHA256 of `data` under `key`.
/// Standard base64 with padding (KOMOJU's Basic authentication), without a crate for it.
pub fn b64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The test keys of fixtures/rsa (test only, nothing secret): the EBANX stand-in signs its notifications with the notify
/// key, and checks requests signed with the JWS key (EBANX registers a merchant's public key).
pub const EBANX_NOTIFY_KEY: &[u8] = include_bytes!("../fixtures/rsa/notify-test-key.der");
pub const EBANX_NOTIFY_PUB: &[u8] = include_bytes!("../fixtures/rsa/notify-test-pub.der");
pub const EBANX_JWS_KEY: &[u8] = include_bytes!("../fixtures/rsa/jws-test-key.der");
pub const EBANX_JWS_PUB: &[u8] = include_bytes!("../fixtures/rsa/jws-test-pub.der");
/// The notify key's fingerprint as the stand-in's headers name it (the SHA-1 of its SPKI).
pub const EBANX_NOTIFY_FINGERPRINT: &str = "878B319E3647D8F706A4B992D71D3A054A411B58";
/// The id the stand-in gave the registered JWS key.
pub const EBANX_JWS_KID: &str = "sandbox-kid";

/// The RSASSA-PKCS1-v1_5 SHA-1 signature of `msg` under the PKCS#8 key `der`, base64.
pub fn rsa_sha1_sign(der: &[u8], msg: &[u8]) -> Option<String> {
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::signature::{SignatureEncoding, Signer};
    let key = rsa::RsaPrivateKey::from_pkcs8_der(der).ok()?;
    let sig = rsa::pkcs1v15::SigningKey::<sha1::Sha1>::new(key).try_sign(msg).ok()?;
    Some(b64(&sig.to_vec()))
}

/// Whether `sig` is the RSASSA-PKCS1-v1_5 SHA-256 signature of `msg` under the SPKI key `der`.
pub fn rsa_sha256_ok(der: &[u8], msg: &[u8], sig: &[u8]) -> bool {
    use rsa::pkcs8::DecodePublicKey;
    use rsa::signature::Verifier;
    let (Ok(key), Ok(sig)) = (rsa::RsaPublicKey::from_public_key_der(der), rsa::pkcs1v15::Signature::try_from(sig)) else {
        return false;
    };
    rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(key).verify(msg, &sig).is_ok()
}

/// Base64url without padding, decoded.
pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut bits: u32 = 0;
    let (mut n, mut out) = (0u32, Vec::with_capacity(s.len() * 3 / 4));
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(v);
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    Some(out)
}

/// The hex HMAC-SHA256 of `data` under `key` (KOMOJU's webhooks, the idempotency fingerprints).
pub fn komoju_signature(secret: &str, body: &[u8]) -> String {
    hmac_hex(secret.as_bytes(), body)
}

pub fn hmac_hex(key: &[u8], data: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    // HMAC takes a key of any length, so this never fails; an empty digest would never verify
    let Ok(mut mac) = <Hmac<sha2::Sha256> as Mac>::new_from_slice(key) else {
        return String::new();
    };
    mac.update(data);
    hex(mac.finalize().into_bytes().as_slice())
}

/// The `Stripe-Signature` header Stripe sends with `body` at `t` (unix seconds) under the
/// endpoint's `secret`: `t=<t>,v1=<hex HMAC-SHA256 of "<t>.<body>">`.
pub fn stripe_signature(secret: &str, t: i64, body: &[u8]) -> String {
    let mut payload = format!("{t}.").into_bytes();
    payload.extend_from_slice(body);
    format!("t={t},v1={}", hmac_hex(secret.as_bytes(), &payload))
}

/// The `Webhook-Signature` header GoCardless sends with `body`: the hex HMAC-SHA256 of the body.
pub fn gocardless_signature(secret: &str, body: &[u8]) -> String {
    hmac_hex(secret.as_bytes(), body)
}

/// The signature dLocal puts after `V2-HMAC-SHA256, Signature: ` on its requests and its
/// notifications: the hex HMAC-SHA256 under the secret key of the login, the `X-Date` and the body.
pub fn dlocal_signature(secret: &str, login: &str, date: &str, body: &[u8]) -> String {
    let mut msg = Vec::with_capacity(login.len() + date.len() + body.len());
    msg.extend_from_slice(login.as_bytes());
    msg.extend_from_slice(date.as_bytes());
    msg.extend_from_slice(body);
    hmac_hex(secret.as_bytes(), &msg)
}

/// 64 unpredictable bits: std's per-process random hash keys over a counter and the time. Good
/// enough for the ids of a local test tool, which grant nothing outside it.
fn random_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    h.write_u128(nanos);
    h.finish()
}

/// `prefix` and `len` random characters of `alphabet`.
fn random_id(prefix: &str, len: usize, alphabet: &[u8]) -> String {
    let mut out = String::from(prefix);
    let mut bits = random_u64();
    let mut left = 10;
    for _ in 0..len {
        if left == 0 {
            bits = random_u64();
            left = 10;
        }
        let i = usize::try_from(bits % alphabet.len() as u64).unwrap_or(0);
        out.push(char::from(*alphabet.get(i).unwrap_or(&b'0')));
        bits /= alphabet.len() as u64;
        left -= 1;
    }
    out
}

/// A Stripe-looking id: `prefix` and 24 lowercase letters and digits.
pub fn id(prefix: &str) -> String {
    random_id(prefix, 24, b"abcdefghijklmnopqrstuvwxyz0123456789")
}

/// A GoCardless-looking id: `prefix` and 12 capitals and digits.
pub fn upper_id(prefix: &str) -> String {
    random_id(prefix, 12, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")
}

/// `application/x-www-form-urlencoded`, brackets percent-encoded (as Stripe's libraries send it).
pub fn form_encode<K: AsRef<str>, V: AsRef<str>>(pairs: &[(K, V)]) -> String {
    fn enc(s: &str, out: &mut String) {
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(char::from(b)),
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
    }
    let mut out = String::new();
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        enc(k.as_ref(), &mut out);
        out.push('=');
        enc(v.as_ref(), &mut out);
    }
    out
}

/// The pairs of a form body or query (`+` is a space, `%XX` a byte, bad UTF-8 replaced).
pub fn form_decode(body: &str) -> Vec<(String, String)> {
    fn dec(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while let Some(&b) = bytes.get(i) {
            let hexdigit = |at: usize| bytes.get(at).and_then(|c| char::from(*c).to_digit(16));
            match b {
                b'+' => out.push(b' '),
                b'%' => match (hexdigit(i + 1), hexdigit(i + 2)) {
                    (Some(h), Some(l)) => {
                        out.push(u8::try_from(h * 16 + l).unwrap_or(b'?'));
                        i += 2;
                    }
                    _ => out.push(b'%'),
                },
                _ => out.push(b),
            }
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }
    body.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (dec(k), dec(v)),
            None => (dec(p), String::new()),
        })
        .collect()
}

/// `(year, month, day)` of a day number since 1970-01-01 (Howard Hinnant's civil_from_days).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `(year, month, day)` of a unix time.
pub fn civil_now(unix: i64) -> (i64, i64, i64) {
    civil(unix.div_euclid(86_400))
}

/// `2026-10-17T12:00:00.000Z`, as GoCardless writes times.
pub fn rfc3339(unix: i64) -> String {
    let (y, m, d) = civil(unix.div_euclid(86_400));
    let s = unix.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z", s / 3600, s % 3600 / 60, s % 60)
}

/// `2026-10-17`.
pub fn ymd(unix: i64) -> String {
    let (y, m, d) = civil(unix.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// The metadata the backend set on an object: its reference (`provider_ref`) and nothing else.
pub fn ref_meta(r: &Option<String>) -> serde_json::Value {
    match r {
        Some(r) => serde_json::json!({ "provider_ref": r }),
        None => serde_json::json!({}),
    }
}

/// The digits of `s`.
pub fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

/// An IBAN without spaces, in capitals.
pub fn compact_iban(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Whether a compact IBAN's check digits are right (ISO 13616, mod 97).
pub fn iban_checksum_ok(iban: &str) -> bool {
    if iban.len() < 15 || iban.len() > 34 || !iban.is_ascii() {
        return false;
    }
    let (head, tail) = iban.split_at(4);
    let mut rem: u32 = 0;
    for c in tail.chars().chain(head.chars()) {
        let Some(v) = c.to_digit(36) else {
            return false;
        };
        rem = if v >= 10 { (rem * 100 + v) % 97 } else { (rem * 10 + v) % 97 };
    }
    rem == 1
}

/// `**** 3201`: what a page may show of an IBAN.
pub fn masked(iban: &str) -> String {
    let tail: String = iban.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{} **** {tail}", iban.get(..2).unwrap_or(""))
}

/// The time now, unix seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
