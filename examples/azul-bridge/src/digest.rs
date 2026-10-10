//! HTTP Digest authentication (RFC 7616 with MD5 and `qop=auth`, RFC 2617's form) for WebDAV.
//!
//! Windows' WebClient - what Explorer's "Map network drive" and `net use` talk through - refuses
//! HTTP Basic over plain `http://` unless the registry says otherwise (`BasicAuthLevel`), and the
//! bridge's loopback connection has no TLS. It does speak Digest, so the bridge offers Digest
//! beside Basic: the password never crosses the socket, only a hash of it with a nonce the bridge
//! issued (valid for [`NONCE_SECS`], unique, at most [`MAX_NONCES`] at once). A request that
//! comes back with an expired nonce is told `stale=true` (the client retries without asking the
//! user); a wrong response counts as a wrong password. MD5 is RustCrypto's `md-5`, already in the
//! tree (lopdf, OpenDAL) and in dependency-justifications.toml.

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

use md5::{Digest, Md5};

use crate::auth::Credentials;

/// How long a nonce is good for.
pub const NONCE_SECS: u64 = 600;
/// Nonces kept at once; the oldest go first.
pub const MAX_NONCES: usize = 1_024;

/// MD5 of `text`, lower-case hex.
#[must_use]
pub fn md5_hex(text: &str) -> String {
    Md5::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The `response` of RFC 2617 / 7616 for MD5: with `qop=auth` (the `nc` and `cnonce` given),
/// else RFC 2069's.
#[must_use]
pub fn response_for(
    user: &str,
    realm: &str,
    password: &str,
    method: &str,
    uri: &str,
    nonce: &str,
    qop: Option<(&str, &str)>,
) -> String {
    let ha1 = md5_hex(&format!("{user}:{realm}:{password}"));
    let ha2 = md5_hex(&format!("{method}:{uri}"));
    match qop {
        Some((nc, cnonce)) => md5_hex(&format!("{ha1}:{nonce}:{nc}:{cnonce}:auth:{ha2}")),
        None => md5_hex(&format!("{ha1}:{nonce}:{ha2}")),
    }
}

/// The parameters of a `Digest` credential (`username="a", qop=auth, ...`), names in lower case,
/// quoted values unquoted.
#[must_use]
pub fn parse_params(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && (chars[i] == ',' || chars[i].is_whitespace()) {
            i += 1;
        }
        let start = i;
        while i < chars.len() && chars[i] != '=' && chars[i] != ',' {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect::<String>().trim().to_ascii_lowercase();
        if i >= chars.len() || chars[i] != '=' {
            continue;
        }
        i += 1;
        let mut value = String::new();
        if i < chars.len() && chars[i] == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                }
                value.push(chars[i]);
                i += 1;
            }
            i += 1;
        } else {
            while i < chars.len() && chars[i] != ',' {
                value.push(chars[i]);
                i += 1;
            }
            value = value.trim().to_string();
        }
        if !name.is_empty() {
            out.insert(name, value);
        }
    }
    out
}

/// What a `Digest` credential came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The user and password are right.
    Signed,
    /// Right but for a nonce that ran out: challenge again with `stale=true`.
    Stale,
    /// Wrong, or not a credential the bridge reads.
    Refused,
}

/// The nonces the bridge issued, and its realm.
#[derive(Debug)]
pub struct DigestAuth {
    realm: String,
    opaque: String,
    nonces: Mutex<VecDeque<(String, Instant)>>,
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    if getrandom::getrandom(&mut buf).is_err() {
        // Without the OS's randomness: the time, which is unique enough for a nonce here.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        return format!("{nanos:032x}");
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

impl DigestAuth {
    #[must_use]
    pub fn new(realm: &str) -> DigestAuth {
        DigestAuth {
            realm: realm.to_string(),
            opaque: random_hex(16),
            nonces: Mutex::new(VecDeque::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<(String, Instant)>> {
        self.nonces
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A `WWW-Authenticate` value with a new nonce (`stale`: the last one ran out).
    #[must_use]
    pub fn challenge(&self, stale: bool) -> String {
        let nonce = random_hex(16);
        {
            let mut nonces = self.lock();
            while nonces.len() >= MAX_NONCES {
                nonces.pop_front();
            }
            nonces.push_back((nonce.clone(), Instant::now()));
        }
        format!(
            "Digest realm=\"{}\", qop=\"auth\", algorithm=MD5, nonce=\"{nonce}\", opaque=\"{}\"{}",
            self.realm,
            self.opaque,
            if stale { ", stale=TRUE" } else { "" }
        )
    }

    /// Whether the nonce is one the bridge issued: `Some(true)` still good, `Some(false)` run
    /// out, `None` never issued (or dropped).
    fn nonce_state(&self, nonce: &str) -> Option<bool> {
        let nonces = self.lock();
        nonces
            .iter()
            .find(|(n, _)| n == nonce)
            .map(|(_, at)| at.elapsed() < Duration::from_secs(NONCE_SECS))
    }

    /// The `Authorization: Digest ...` value `params` (after `Digest `) of a request of
    /// `method` on `target`, checked against `credentials`.
    #[must_use]
    pub fn verify(&self, params: &str, method: &str, target: &str, credentials: &Credentials) -> Verdict {
        let p = parse_params(params);
        let get = |name: &str| p.get(name).map(String::as_str);
        let (Some(user), Some(realm), Some(nonce), Some(uri), Some(given)) = (
            get("username"),
            get("realm"),
            get("nonce"),
            get("uri"),
            get("response"),
        ) else {
            return Verdict::Refused;
        };
        if realm != self.realm
            || get("algorithm").is_some_and(|a| !a.eq_ignore_ascii_case("MD5"))
            || get("opaque").is_some_and(|o| o != self.opaque)
            || uri != target
        {
            return Verdict::Refused;
        }
        let qop = match get("qop") {
            Some(qop) if qop.eq_ignore_ascii_case("auth") => match (get("nc"), get("cnonce")) {
                (Some(nc), Some(cnonce)) => Some((nc, cnonce)),
                _ => return Verdict::Refused,
            },
            Some(_) => return Verdict::Refused,
            None => None,
        };
        let given = given.to_ascii_lowercase();
        let right = credentials.check_derived(user, &given, |password| {
            response_for(user, realm, password, method, uri, nonce, qop)
        });
        match (right, self.nonce_state(nonce)) {
            (true, Some(true)) => Verdict::Signed,
            (true, Some(false)) => Verdict::Stale,
            _ => Verdict::Refused,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_and_the_rfc_2617_response_come_out_as_the_rfc_says() {
        assert_eq!(md5_hex(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex("abc"), "900150983cd24fb0d6963f7d28e17f72");
        // RFC 2617 section 3.5.
        assert_eq!(
            response_for(
                "Mufasa",
                "testrealm@host.com",
                "Circle Of Life",
                "GET",
                "/dir/index.html",
                "dcd98b7102dd2f0e8b11d0f600bfb0c093",
                Some(("00000001", "0a4f113b")),
            ),
            "6629fae49393a05397450978507c4ef1"
        );
    }

    #[test]
    fn digest_parameters_are_read_quoted_or_not() {
        let p = parse_params(
            "username=\"ada@example.org\", realm=\"Azlin Bridge\", qop=auth, nc=00000001, \
             uri=\"/a,b/\", response=\"ABC\"",
        );
        assert_eq!(p["username"], "ada@example.org");
        assert_eq!(p["qop"], "auth");
        assert_eq!(p["uri"], "/a,b/");
        assert_eq!(p["response"], "ABC");
    }

    fn signed_header(auth: &DigestAuth, password: &str, uri: &str) -> String {
        let challenge = auth.challenge(false);
        let p = parse_params(challenge.trim_start_matches("Digest "));
        let response = response_for(
            "ada@example.org",
            &p["realm"],
            password,
            "PROPFIND",
            uri,
            &p["nonce"],
            Some(("00000001", "c1")),
        );
        format!(
            "username=\"ada@example.org\", realm=\"{}\", nonce=\"{}\", uri=\"{uri}\", qop=auth, \
             nc=00000001, cnonce=\"c1\", response=\"{response}\", opaque=\"{}\", algorithm=MD5",
            p["realm"], p["nonce"], p["opaque"]
        )
    }

    #[test]
    fn a_response_made_with_the_password_signs_in_and_anything_else_does_not() {
        let auth = DigestAuth::new("Azlin Bridge");
        let creds = Credentials::new("ada@example.org", "k7m2p-9qxat");
        let good = signed_header(&auth, "k7m2p-9qxat", "/docs/");
        assert_eq!(auth.verify(&good, "PROPFIND", "/docs/", &creds), Verdict::Signed);
        let typed_without_dashes = signed_header(&auth, "k7m2p9qxat", "/docs/");
        assert_eq!(auth.verify(&typed_without_dashes, "PROPFIND", "/docs/", &creds), Verdict::Signed);
        let wrong = signed_header(&auth, "guess", "/docs/");
        assert_eq!(auth.verify(&wrong, "PROPFIND", "/docs/", &creds), Verdict::Refused);
        // Another resource than the one the response was made for.
        assert_eq!(auth.verify(&good, "PROPFIND", "/other/", &creds), Verdict::Refused);
        // A nonce the bridge never issued.
        let foreign = good.replace("nonce=\"", "nonce=\"00");
        assert_eq!(auth.verify(&foreign, "PROPFIND", "/docs/", &creds), Verdict::Refused);
        assert_eq!(auth.verify("username=\"x\"", "GET", "/", &creds), Verdict::Refused);
        assert!(auth.challenge(true).ends_with(", stale=TRUE"));
    }
}
