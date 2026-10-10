//! One-time codes: HOTP (RFC 4226) and TOTP (RFC 6238) with SHA-1, SHA-256 or SHA-512, 6 to 8
//! digits, any period; the secret from an `otpauth://totp/...` URL (the QR code's text) or a bare
//! base32 secret (RFC 4648, any case, blanks, dashes and padding allowed).
//!
//! A [`Totp`] holds the decoded secret; it wipes it when dropped and never prints it.

use std::fmt;

use hmac::{Hmac, Mac};
use zeroize::Zeroize;

/// The HMAC's hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Algorithm {
    #[default]
    Sha1,
    Sha256,
    Sha512,
}

impl Algorithm {
    /// The name an otpauth URL uses.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Sha1 => "SHA1",
            Algorithm::Sha256 => "SHA256",
            Algorithm::Sha512 => "SHA512",
        }
    }

    /// The algorithm an otpauth URL names (any case, with or without the dash).
    #[must_use]
    pub fn parse(name: &str) -> Option<Algorithm> {
        match name.trim().to_ascii_uppercase().replace('-', "").as_str() {
            "SHA1" => Some(Algorithm::Sha1),
            "SHA256" => Some(Algorithm::Sha256),
            "SHA512" => Some(Algorithm::Sha512),
            _ => None,
        }
    }
}

/// The HMAC of `msg` under `key` with `algorithm`'s hash.
fn hmac(algorithm: Algorithm, key: &[u8], msg: &[u8]) -> Vec<u8> {
    const ANY_LENGTH: &str = "HMAC takes a key of any length";
    match algorithm {
        Algorithm::Sha1 => {
            let mut mac = <Hmac<sha1::Sha1> as Mac>::new_from_slice(key).expect(ANY_LENGTH);
            mac.update(msg);
            mac.finalize().into_bytes().to_vec()
        }
        Algorithm::Sha256 => {
            let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(key).expect(ANY_LENGTH);
            mac.update(msg);
            mac.finalize().into_bytes().to_vec()
        }
        Algorithm::Sha512 => {
            let mut mac = <Hmac<sha2::Sha512> as Mac>::new_from_slice(key).expect(ANY_LENGTH);
            mac.update(msg);
            mac.finalize().into_bytes().to_vec()
        }
    }
}

/// The RFC 4226 HOTP value of `counter`: HMAC, dynamic truncation, the low `digits` decimal
/// digits (`digits` is held to 1..=9).
#[must_use]
pub fn hotp(secret: &[u8], counter: u64, digits: u32, algorithm: Algorithm) -> u32 {
    let mut mac = hmac(algorithm, secret, &counter.to_be_bytes());
    let offset = usize::from(mac[mac.len() - 1] & 0x0f);
    let binary = (u32::from(mac[offset] & 0x7f) << 24)
        | (u32::from(mac[offset + 1]) << 16)
        | (u32::from(mac[offset + 2]) << 8)
        | u32::from(mac[offset + 3]);
    mac.zeroize();
    binary % 10u32.pow(digits.clamp(1, 9))
}

/// RFC 4648 base32, any case on the way in (azul-storage's, which the Azlin cash codes use
/// too).
pub use azul_storage::base32::{decode as base32_decode, encode as base32_encode};

/// A code for reading aloud: `482913` -> `482 913`, `94287082` -> `9428 7082`.
#[must_use]
pub fn group_code(code: &str) -> String {
    let code = code.trim();
    let n = code.chars().count();
    if n < 6 {
        return code.to_string();
    }
    let first: String = code.chars().take(n / 2).collect();
    let rest: String = code.chars().skip(n / 2).collect();
    format!("{first} {rest}")
}

/// `%XX` escapes of a URL part decoded (bad escapes kept as they are).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A URL part with everything but the unreserved characters (RFC 3986) escaped.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The shortest bare secret taken (80 bits, what most services issue): shorter text in the field
/// is more likely a word than a key.
const MIN_BARE_SECRET: usize = 10;

/// A TOTP generator.
#[derive(Clone, PartialEq, Eq)]
pub struct Totp {
    secret: Vec<u8>,
    pub algorithm: Algorithm,
    pub digits: u32,
    /// Seconds per code.
    pub period: u64,
    pub issuer: String,
    pub account: String,
}

impl fmt::Debug for Totp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Totp")
            .field("secret", &"***")
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("period", &self.period)
            .field("issuer", &self.issuer)
            .field("account", &self.account)
            .finish()
    }
}

impl Drop for Totp {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

impl Totp {
    /// SHA-1, 6 digits, 30 seconds over `secret` (the defaults of every authenticator app).
    #[must_use]
    pub fn new(secret: Vec<u8>) -> Totp {
        Totp {
            secret,
            algorithm: Algorithm::Sha1,
            digits: 6,
            period: 30,
            issuer: String::new(),
            account: String::new(),
        }
    }

    /// The generator of an item's one-time field: an `otpauth://totp/...` URL or a base32
    /// secret. `Err` says why not (never quoting the secret).
    pub fn parse(text: &str) -> Result<Totp, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("The one-time code field is empty.".to_string());
        }
        let scheme = "otpauth://";
        let is_url = text
            .get(..scheme.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(scheme));
        if !is_url {
            let secret = base32_decode(text)
                .filter(|b| b.len() >= MIN_BARE_SECRET)
                .ok_or(
                "This is neither an otpauth:// URL nor a base32 secret of 16 characters or more.",
            )?;
            return Ok(Totp::new(secret));
        }
        let rest = &text[scheme.len()..];
        let (kind, rest) = rest.split_once('/').unwrap_or((rest, ""));
        if kind.eq_ignore_ascii_case("hotp") {
            return Err(
                "Counter-based (HOTP) codes are not supported; only time-based (TOTP) codes."
                    .to_string(),
            );
        }
        if !kind.eq_ignore_ascii_case("totp") {
            return Err("The otpauth URL is not for a time-based code (totp).".to_string());
        }
        let (label, query) = rest.split_once('?').unwrap_or((rest, ""));
        let label = percent_decode(label);
        let (mut issuer, account) = match label.split_once(':') {
            Some((issuer, account)) => (issuer.trim().to_string(), account.trim().to_string()),
            None => (String::new(), label.trim().to_string()),
        };
        let mut secret: Option<Vec<u8>> = None;
        let mut totp_algorithm = Algorithm::Sha1;
        let (mut digits, mut period) = (6u32, 30u64);
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let mut value = percent_decode(value);
            match key.to_ascii_lowercase().as_str() {
                "secret" => {
                    secret = Some(
                        base32_decode(&value)
                            .filter(|b| !b.is_empty())
                            .ok_or("The secret in the otpauth URL is not base32.")?,
                    );
                }
                "issuer" => issuer = value.trim().to_string(),
                "algorithm" => {
                    totp_algorithm = Algorithm::parse(&value)
                        .ok_or("The otpauth URL names a hash other than SHA1, SHA256 or SHA512.")?;
                }
                "digits" => {
                    digits = value
                        .trim()
                        .parse::<u32>()
                        .ok()
                        .filter(|d| (6..=8).contains(d))
                        .ok_or("The otpauth URL asks for codes of other than 6 to 8 digits.")?;
                }
                "period" => {
                    period = value
                        .trim()
                        .parse::<u64>()
                        .ok()
                        .filter(|p| (1..=3600).contains(p))
                        .ok_or("The otpauth URL's period is not 1 to 3600 seconds.")?;
                }
                _ => {}
            }
            value.zeroize();
        }
        let secret = secret.ok_or("The otpauth URL has no secret.")?;
        Ok(Totp {
            secret,
            algorithm: totp_algorithm,
            digits,
            period,
            issuer,
            account,
        })
    }

    /// The code at `unix` seconds since 1970, zero-padded to `digits`.
    #[must_use]
    pub fn code_at(&self, unix: u64) -> String {
        let code = hotp(
            &self.secret,
            unix / self.period.max(1),
            self.digits,
            self.algorithm,
        );
        format!("{code:0width$}", width = self.digits.clamp(1, 9) as usize)
    }

    /// Seconds until the code at `unix` changes (1..=period).
    #[must_use]
    pub fn remaining(&self, unix: u64) -> u64 {
        let period = self.period.max(1);
        period - unix % period
    }

    /// The generator as an `otpauth://totp/` URL (for export and the edit field).
    #[must_use]
    pub fn to_url(&self) -> String {
        let label = if self.issuer.is_empty() {
            percent_encode(&self.account)
        } else {
            format!(
                "{}:{}",
                percent_encode(&self.issuer),
                percent_encode(&self.account)
            )
        };
        let mut url = format!(
            "otpauth://totp/{label}?secret={}",
            base32_encode(&self.secret)
        );
        if !self.issuer.is_empty() {
            url.push_str("&issuer=");
            url.push_str(&percent_encode(&self.issuer));
        }
        url.push_str(&format!(
            "&algorithm={}&digits={}&period={}",
            self.algorithm.name(),
            self.digits,
            self.period
        ));
        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED_SHA1: &[u8] = b"12345678901234567890";
    const SEED_SHA256: &[u8] = b"12345678901234567890123456789012";
    const SEED_SHA512: &[u8] = b"1234567890123456789012345678901234567890123456789012345678901234";

    #[test]
    fn hotp_matches_the_rfc_4226_appendix_d_values() {
        let expected = [
            755_224, 287_082, 359_152, 969_429, 338_314, 254_676, 287_922, 162_583, 399_871,
            520_489,
        ];
        for (counter, want) in expected.iter().enumerate() {
            assert_eq!(
                hotp(SEED_SHA1, counter as u64, 6, Algorithm::Sha1),
                *want,
                "count {counter}"
            );
        }
    }

    #[test]
    fn totp_matches_the_rfc_6238_appendix_b_values_for_sha1_sha256_and_sha512() {
        // (time, SHA1, SHA256, SHA512), 8 digits, 30 seconds.
        let table: [(u64, &str, &str, &str); 6] = [
            (59, "94287082", "46119246", "90693936"),
            (1_111_111_109, "07081804", "68084774", "25091201"),
            (1_111_111_111, "14050471", "67062674", "99943326"),
            (1_234_567_890, "89005924", "91819424", "93441116"),
            (2_000_000_000, "69279037", "90698825", "38618901"),
            (20_000_000_000, "65353130", "77737706", "47863826"),
        ];
        for (time, sha1, sha256, sha512) in table {
            for (seed, algorithm, want) in [
                (SEED_SHA1, Algorithm::Sha1, sha1),
                (SEED_SHA256, Algorithm::Sha256, sha256),
                (SEED_SHA512, Algorithm::Sha512, sha512),
            ] {
                let mut totp = Totp::new(seed.to_vec());
                totp.algorithm = algorithm;
                totp.digits = 8;
                assert_eq!(totp.code_at(time), want, "{} at {time}", algorithm.name());
            }
        }
    }

    #[test]
    fn base32_matches_the_rfc_4648_vectors() {
        for (plain, coded) in [
            ("", ""),
            ("f", "MY"),
            ("fo", "MZXQ"),
            ("foo", "MZXW6"),
            ("foob", "MZXW6YQ"),
            ("fooba", "MZXW6YTB"),
            ("foobar", "MZXW6YTBOI"),
        ] {
            assert_eq!(base32_encode(plain.as_bytes()), coded);
            assert_eq!(base32_decode(coded).as_deref(), Some(plain.as_bytes()));
        }
        assert_eq!(
            base32_decode("MZXW6YQ=").as_deref(),
            Some(&b"foob"[..]),
            "padding"
        );
        assert_eq!(
            base32_decode("mzxw 6ytb-oi").as_deref(),
            Some(&b"foobar"[..]),
            "case, blanks, dashes"
        );
        assert_eq!(base32_decode("MZXW1"), None, "1 is no base32 digit");
        assert_eq!(
            base32_decode("M"),
            None,
            "one character is no length base32 has"
        );
    }

    #[test]
    fn an_otpauth_url_gives_the_secret_issuer_account_and_parameters() {
        let totp = Totp::parse(
            "otpauth://totp/CodeHost%20(example):dev%40example.org?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ\
             &issuer=CodeHost%20(example)&algorithm=SHA256&digits=8&period=60",
        )
        .expect("a TOTP URL");
        assert_eq!(totp.issuer, "CodeHost (example)");
        assert_eq!(totp.account, "dev@example.org");
        assert_eq!(totp.algorithm, Algorithm::Sha256);
        assert_eq!((totp.digits, totp.period), (8, 60));
        // The secret is "12345678901234567890": the SHA1 vector's seed.
        let mut sha1 = totp.clone();
        sha1.algorithm = Algorithm::Sha1;
        sha1.period = 30;
        assert_eq!(sha1.code_at(59), "94287082");
        // The URL it writes reads back the same.
        assert_eq!(Totp::parse(&totp.to_url()).expect("its own URL"), totp);
    }

    #[test]
    fn a_bare_secret_takes_the_defaults_and_ignores_case_and_blanks() {
        let a = Totp::parse("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").expect("base32");
        assert_eq!((a.algorithm, a.digits, a.period), (Algorithm::Sha1, 6, 30));
        assert_eq!(a.code_at(59), "287082");
        let url =
            Totp::parse("otpauth://totp/Mail?secret=JBSWY3DPEHPK3PXP").expect("URL without issuer");
        assert_eq!(url.account, "Mail");
        assert_eq!(url.issuer, "");
    }

    #[test]
    fn bad_one_time_fields_are_refused_with_a_reason_and_no_secret() {
        for bad in [
            "",
            "   ",
            "otpauth://hotp/X?secret=JBSWY3DPEHPK3PXP&counter=1",
            "otpauth://totp/X?issuer=Y",
            "otpauth://totp/X?secret=JBSWY3DPEHPK3PXP&digits=12",
            "otpauth://totp/X?secret=JBSWY3DPEHPK3PXP&algorithm=MD5",
            "otpauth://totp/X?secret=JBSWY3DPEHPK3PXP&period=0",
            "not base32 at all",
        ] {
            let err = Totp::parse(bad).expect_err(bad);
            assert!(!err.is_empty());
            assert!(!err.contains("JBSWY3DPEHPK3PXP"), "{err}");
        }
    }

    #[test]
    fn the_code_counts_down_to_the_next_period() {
        let totp = Totp::new(SEED_SHA1.to_vec());
        assert_eq!(totp.remaining(59), 1);
        assert_eq!(totp.remaining(60), 30);
        assert_eq!(totp.remaining(61), 29);
    }

    #[test]
    fn codes_are_grouped_for_reading_and_secrets_never_print() {
        assert_eq!(group_code("482913"), "482 913");
        assert_eq!(group_code("94287082"), "9428 7082");
        assert_eq!(group_code("1234567"), "123 4567");
        let totp = Totp::parse("JBSWY3DPEHPK3PXP").expect("base32");
        assert!(!format!("{totp:?}").contains("JBSWY3DPEHPK3PXP"));
    }
}
