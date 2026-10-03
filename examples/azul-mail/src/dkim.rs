//! Client-side DKIM: the key AzMail signs with, the DNS record that publishes it, and whether
//! that record is out there yet.
//!
//! Direct delivery (`send.rs`, straight to each receiver's mail exchanger) needs no server of
//! our own - but a receiver only trusts a mail from a home connection when it can check who
//! wrote it. DKIM (RFC 6376) is that check: AzMail signs every outgoing mail with a private
//! key that never leaves this computer (it lives in the OS keyring, [`crate::send::
//! dkim_keyring_key`]), and the domain's DNS publishes the public half at
//! `<selector>._domainkey.<domain>` ([`record_name`], [`record_value`], [`zone_line`]). A
//! receiver that verifies the signature knows the mail comes from someone who holds the key of
//! the From address's domain, and DMARC passes on DKIM alone ([`setup_notes`]).
//!
//! - [`generate_key`]: a new RSA 2048 key (micromail's generator), the private half as PEM (a
//!   [`Secret`]: no `Display`, never logged), the public half as base64 SubjectPublicKeyInfo -
//!   the form OpenDKIM (`d2i_PUBKEY`), Gmail and every provider publish and verify.
//! - [`check_published`] / [`dns_report`]: asks DNS (UDP, the public resolvers micromail's DNS
//!   uses) for the record, the domain's DMARC and SPF records; [`txt_records`] reads the answer.
//!
//! Signing itself is micromail's (`send.rs`, `sign`); nothing here sends mail.

use crate::account::Secret;

/// What a default selector starts with (`azmail202610`: the month the key was made, so a new
/// key gets a new selector and the old record can stay until no mail signed with it is in
/// flight any more).
pub const SELECTOR_PREFIX: &str = "azmail";
/// The bits of a generated key (RFC 8301: at least 1024, 2048 recommended; 4096 does not fit
/// every DNS provider's TXT field). micromail's generator makes this size.
pub const KEY_BITS: usize = 2048;
/// DNS type TXT.
const DNS_TYPE_TXT: u16 = 16;
/// The longest character-string of a TXT record (RFC 1035 3.3).
const TXT_STRING_MAX: usize = 255;
/// Mail providers whose domains nobody but the provider can sign for, besides the ones the
/// account wizard knows (`account::PROVIDERS`).
const PROVIDER_DOMAINS: &[&str] = &[
    "yahoo.com",
    "ymail.com",
    "rocketmail.com",
    "aol.com",
    "gmx.de",
    "gmx.net",
    "gmx.at",
    "gmx.ch",
    "gmx.com",
    "web.de",
    "t-online.de",
    "freenet.de",
    "proton.me",
    "protonmail.com",
    "pm.me",
    "zoho.com",
    "yandex.ru",
    "yandex.com",
    "mail.ru",
    "mailbox.org",
    "posteo.de",
];

/// A new key: the private half for the keyring, the public half for the DNS record.
#[derive(Debug, Clone)]
pub struct KeyPair {
    /// PEM (PKCS#1), what `micromail::DkimConfig::from_pem` takes.
    pub private_pem: Secret,
    /// Base64 of the DER SubjectPublicKeyInfo: the `p=` of the DNS record.
    pub public_key: String,
}

/// A new RSA key. Takes a moment (a fraction of a second in a release build, seconds in a debug
/// one): call it from an azul `Thread`, never from a callback.
pub fn generate_key() -> Result<KeyPair, String> {
    let private_pem = Secret::new(
        micromail::generate_rsa_key_pem().map_err(|e| format!("No key could be made: {e}"))?,
    );
    let public_key = public_key_of(private_pem.expose())?;
    Ok(KeyPair {
        private_pem,
        public_key,
    })
}

/// The public half of a private key (PEM, PKCS#1 or PKCS#8), as base64 SubjectPublicKeyInfo.
pub fn public_key_of(private_pem: &str) -> Result<String, String> {
    use base64::Engine;
    use rsa::{pkcs1::DecodeRsaPrivateKey, pkcs8::DecodePrivateKey, pkcs8::EncodePublicKey};

    let pem = private_pem.trim();
    let private = rsa::RsaPrivateKey::from_pkcs1_pem(pem)
        .or_else(|_| rsa::RsaPrivateKey::from_pkcs8_pem(pem))
        .map_err(|_| {
            String::from("This is not an RSA private key in PEM form (PKCS#1 or PKCS#8).")
        })?;
    let der = private
        .to_public_key()
        .to_public_key_der()
        .map_err(|e| format!("The public key could not be written: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(der.as_bytes()))
}

/// The selector for a key made at `now` (seconds since 1970): `azmail<yyyy><mm>`.
pub fn default_selector(now: i64) -> String {
    let (year, month) = crate::message::year_month(now);
    format!("{SELECTOR_PREFIX}{year:04}{month:02}")
}

/// Whether `selector` can be a DNS label: 1 to 63 letters, digits and `-`, not starting or
/// ending with `-` (dots would make it several labels, which DKIM allows but nobody needs).
pub fn is_selector(selector: &str) -> bool {
    (1..=63).contains(&selector.len())
        && selector
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !selector.starts_with('-')
        && !selector.ends_with('-')
}

/// A domain as DNS names it here: trimmed, lower case, no root dot.
fn domain_name(domain: &str) -> String {
    domain.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// The name of the TXT record: `<selector>._domainkey.<domain>`.
pub fn record_name(selector: &str, domain: &str) -> String {
    format!("{}._domainkey.{}", selector.trim(), domain_name(domain))
}

/// The TXT record's value as a DNS provider's web form wants it (one line; the provider splits
/// it): `v=DKIM1; k=rsa; p=<public key>`.
pub fn record_value(public_key: &str) -> String {
    format!("v=DKIM1; k=rsa; p={}", public_key.trim())
}

/// The record as a line of a zone file: `<name>. 3600 IN TXT ( "..." "..." )`, the value split
/// into strings of at most 255 characters (a 2048-bit key is longer than one).
pub fn zone_line(selector: &str, domain: &str, public_key: &str) -> String {
    let mut strings = vec![String::from("v=DKIM1; k=rsa; ")];
    let key = format!("p={}", public_key.trim());
    // Base64 is ASCII: every byte index is a character boundary.
    let mut rest = key.as_str();
    while !rest.is_empty() {
        let (head, tail) = rest.split_at(rest.len().min(TXT_STRING_MAX));
        strings.push(head.to_string());
        rest = tail;
    }
    let quoted: Vec<String> = strings.iter().map(|s| format!("\"{s}\"")).collect();
    format!(
        "{}. 3600 IN TXT ( {} )",
        record_name(selector, domain),
        quoted.join(" ")
    )
}

/// Whether AzMail can sign for `domain` from this computer: not for a mail provider's domain
/// (gmail.com, outlook.com, ...), whose DNS nobody but the provider can change.
pub fn can_sign_for(domain: &str) -> Result<(), String> {
    let domain = domain_name(domain);
    if domain.is_empty() || !domain.contains('.') {
        return Err(String::from(
            "DKIM needs the domain of your address (example.org), one whose DNS you can edit.",
        ));
    }
    let provider = crate::account::PROVIDERS
        .iter()
        .flat_map(|p| p.domains.iter())
        .chain(PROVIDER_DOMAINS.iter())
        .any(|d| *d == domain);
    if provider {
        return Err(format!(
            "{domain} belongs to a mail provider: only the provider can publish DKIM keys for \
             it. Send through the provider's own server instead, or use an address at a \
             domain of your own."
        ));
    }
    Ok(())
}

/// The DMARC record to publish for `domain` while starting out: its name
/// (`_dmarc.<domain>`) and value (`v=DMARC1; p=none; rua=mailto:<reports_to>`).
pub fn dmarc_record(domain: &str, reports_to: &str) -> (String, String) {
    (
        format!("_dmarc.{}", domain_name(domain)),
        format!("v=DMARC1; p=none; rua=mailto:{}", reports_to.trim()),
    )
}

/// What the Sending page says under the DKIM record: DMARC, SPF, reverse DNS and port 25, for
/// `domain` and the sender `address`. One paragraph per entry.
pub fn setup_notes(domain: &str, address: &str) -> Vec<String> {
    let domain = domain_name(domain);
    let (dmarc_name, dmarc_value) = dmarc_record(&domain, address);
    vec![
        format!(
            "DMARC: publish a TXT record {dmarc_name} with \"{dmarc_value}\". DMARC passes on \
             DKIM alone, because the signature names {domain}, the From address's own domain. \
             Receivers send their reports to {}; once they look clean, p=quarantine asks them \
             to file failing mail as spam.",
            address.trim()
        ),
        format!(
            "SPF lists the computers that may send for {domain}. A home connection's address \
             belongs to your Internet provider and changes, so SPF cannot list it: keep the \
             domain's SPF record ending in ~all, not -all (or publish \"v=spf1 ~all\" if it has \
             none). DKIM carries the mail through DMARC; a hard -all makes receivers that check \
             SPF alone refuse it."
        ),
        String::from(
            "Reverse DNS (PTR): receivers look up the name of the address a mail comes from. A \
             home connection has the provider's generic name, and some receivers refuse such \
             addresses (Gmail: 5.7.25 without a PTR; Outlook and others: home address lists \
             such as Spamhaus PBL, 5.7.1). AzMail remembers each domain that refuses and keeps \
             that mail in the Outbox for a relay.",
        ),
        String::from(
            "Direct delivery talks to each receiver's mail server on port 25. Many home Internet \
             providers block outgoing port 25; when no mail server can be reached at all, \
             AzMail checks the port and says so.",
        ),
    ]
}

// ==== Is it published? ====

/// What DNS says about the DKIM record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Published {
    /// A record with this key is there.
    Matches,
    /// A DKIM record is there, with another key (its `p=`, shortened; empty: revoked).
    Different(String),
    /// No record at that name (yet: a new record can take up to an hour to show).
    Missing,
    /// DNS could not be asked (offline, a truncated answer): why.
    Unknown(String),
}

/// The DNS records a sender's domain needs, as DNS has them now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsReport {
    pub dkim: Published,
    /// The domain's DMARC record (`_dmarc.<domain>`), if it has one.
    pub dmarc: Option<String>,
    /// The domain's SPF record (`v=spf1 ...`), if it has one.
    pub spf: Option<String>,
}

fn be16(packet: &[u8], at: usize) -> Result<u16, String> {
    match packet.get(at..at + 2) {
        Some(b) => Ok(u16::from_be_bytes([b[0], b[1]])),
        None => Err(String::from("the DNS answer is cut off")),
    }
}

/// The position after the (possibly compressed) name at `pos`.
fn skip_name(packet: &[u8], mut pos: usize) -> Result<usize, String> {
    loop {
        let len = *packet
            .get(pos)
            .ok_or_else(|| String::from("the DNS answer is cut off"))? as usize;
        if len == 0 {
            return Ok(pos + 1);
        }
        if len & 0xC0 == 0xC0 {
            // A pointer ends the name.
            return if pos + 1 < packet.len() {
                Ok(pos + 2)
            } else {
                Err(String::from("the DNS answer is cut off"))
            };
        }
        pos += 1 + len;
    }
}

/// The TXT records in a DNS answer (each record's strings joined, as RFC 6376 3.6.2.2 reads
/// them); none for a name that does not exist (NXDOMAIN); an error for a malformed or truncated
/// answer or a server failure.
pub fn txt_records(packet: &[u8]) -> Result<Vec<String>, String> {
    if packet.len() < 12 {
        return Err(String::from("the DNS answer is too short"));
    }
    let flags = be16(packet, 2)?;
    if flags & 0x0200 != 0 {
        return Err(String::from(
            "the DNS answer was truncated (too long for one UDP packet)",
        ));
    }
    match flags & 0x000F {
        0 => {}
        3 => return Ok(Vec::new()),
        code => return Err(format!("the DNS server answered with error {code}")),
    }
    let questions = be16(packet, 4)?;
    let answers = be16(packet, 6)?;
    let mut pos = 12;
    for _ in 0..questions {
        pos = skip_name(packet, pos)? + 4;
    }
    let mut records = Vec::new();
    for _ in 0..answers {
        pos = skip_name(packet, pos)?;
        let kind = be16(packet, pos)?;
        let length = be16(packet, pos + 8)? as usize;
        pos += 10;
        let end = pos + length;
        let rdata = packet
            .get(pos..end)
            .ok_or_else(|| String::from("the DNS answer is cut off"))?;
        if kind == DNS_TYPE_TXT {
            let mut text = Vec::new();
            let mut at = 0;
            while at < rdata.len() {
                let len = rdata[at] as usize;
                let string = rdata
                    .get(at + 1..at + 1 + len)
                    .ok_or_else(|| String::from("a TXT record of the DNS answer is malformed"))?;
                text.extend_from_slice(string);
                at += 1 + len;
            }
            records.push(String::from_utf8_lossy(&text).into_owned());
        }
        pos = end;
    }
    Ok(records)
}

/// A DKIM key record's tags (`v`, `k`, `p`, ...), names trimmed.
fn tags(record: &str) -> Vec<(String, String)> {
    record
        .split(';')
        .filter_map(|tag| tag.split_once('='))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect()
}

/// Which of `records` (the TXT records at the DKIM name) publishes `public_key`.
pub fn match_record(records: &[String], public_key: &str) -> Published {
    let ours: String = public_key.chars().filter(|c| !c.is_whitespace()).collect();
    let mut other = None;
    for record in records {
        let tags = tags(record);
        let version = tags.iter().find(|(n, _)| n == "v").map(|(_, v)| v.as_str());
        if version.is_some_and(|v| !v.eq_ignore_ascii_case("DKIM1")) {
            continue;
        }
        let Some(key) = tags.iter().find(|(n, _)| n == "p").map(|(_, v)| v) else {
            continue;
        };
        let key: String = key.chars().filter(|c| !c.is_whitespace()).collect();
        if !ours.is_empty() && key == ours {
            return Published::Matches;
        }
        if other.is_none() {
            let shown = if key.len() > 32 {
                format!("{}...", &key[..24])
            } else {
                key
            };
            other = Some(shown);
        }
    }
    match other {
        Some(key) => Published::Different(key),
        None => Published::Missing,
    }
}

/// The TXT records at `name`, asked of the public resolvers.
fn lookup_txt(name: &str) -> Result<Vec<String>, String> {
    let packet = microdns::lookup_dns_records(name, DNS_TYPE_TXT, None)
        .map_err(|e| format!("DNS could not be asked for {name}: {e}"))?;
    txt_records(&packet).map_err(|e| format!("{name}: {e}"))
}

/// Asks DNS for the DKIM record of `selector` / `domain` (blocking: call it from an azul
/// `Thread`).
pub fn check_published(selector: &str, domain: &str, public_key: &str) -> Published {
    match lookup_txt(&record_name(selector, domain)) {
        Ok(records) => match_record(&records, public_key),
        Err(why) => Published::Unknown(why),
    }
}

/// The report as the Sending page shows it: one line for DKIM, DMARC and SPF each.
pub fn report_lines(report: &DnsReport) -> Vec<String> {
    let dkim = match &report.dkim {
        Published::Matches => String::from("DKIM record: published, with this key."),
        Published::Different(key) if key.is_empty() => String::from(
            "DKIM record: the key at this name is revoked (p= is empty): publish the record above.",
        ),
        Published::Different(key) => format!(
            "DKIM record: another key is published at this name (p={key}): publish the record \
             above instead."
        ),
        Published::Missing => String::from(
            "DKIM record: not found yet (a new record can take up to an hour to show).",
        ),
        Published::Unknown(why) => format!("DKIM record: DNS could not be asked ({why})."),
    };
    let dmarc = match &report.dmarc {
        Some(record) => format!("DMARC: {record}"),
        None => String::from("DMARC: no record yet (see the note below)."),
    };
    let spf = match &report.spf {
        Some(record) if record.to_ascii_lowercase().contains("-all") => format!(
            "SPF: {record} - it ends in -all, so receivers that check SPF alone refuse mail \
             from this computer; ~all is safer."
        ),
        Some(record) => format!("SPF: {record}"),
        None => String::from("SPF: no record (see the note below)."),
    };
    vec![dkim, dmarc, spf]
}

/// The DKIM, DMARC and SPF records of `domain` (blocking: call it from an azul `Thread`).
pub fn dns_report(selector: &str, domain: &str, public_key: &str) -> DnsReport {
    let first = |name: &str, prefix: &str| {
        lookup_txt(name).ok().and_then(|records| {
            records.into_iter().find(|r| {
                r.trim_start()
                    .get(..prefix.len())
                    .is_some_and(|p| p.eq_ignore_ascii_case(prefix))
            })
        })
    };
    let domain = domain_name(domain);
    DnsReport {
        dkim: check_published(selector, &domain, public_key),
        dmarc: first(&format!("_dmarc.{domain}"), "v=DMARC1"),
        spf: first(&domain, "v=spf1"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-01T08:30:00Z
    const OCT_1: i64 = 1_790_843_400;

    /// A DNS answer for `name`: one question, the given (type, rdata) answers, `flags` as the
    /// server set them.
    fn answer(name: &str, flags: u16, records: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&0x1234_u16.to_be_bytes());
        packet.extend_from_slice(&flags.to_be_bytes());
        packet.extend_from_slice(&1_u16.to_be_bytes());
        packet.extend_from_slice(&(records.len() as u16).to_be_bytes());
        packet.extend_from_slice(&0_u16.to_be_bytes());
        packet.extend_from_slice(&0_u16.to_be_bytes());
        for label in name.split('.') {
            packet.push(label.len() as u8);
            packet.extend_from_slice(label.as_bytes());
        }
        packet.push(0);
        packet.extend_from_slice(&DNS_TYPE_TXT.to_be_bytes());
        packet.extend_from_slice(&1_u16.to_be_bytes());
        for (kind, rdata) in records {
            // The question's name, by a pointer to offset 12.
            packet.extend_from_slice(&[0xC0, 0x0C]);
            packet.extend_from_slice(&kind.to_be_bytes());
            packet.extend_from_slice(&1_u16.to_be_bytes());
            packet.extend_from_slice(&3600_u32.to_be_bytes());
            packet.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
            packet.extend_from_slice(rdata);
        }
        packet
    }

    /// TXT rdata: each string behind its length byte.
    fn txt(strings: &[&str]) -> Vec<u8> {
        let mut rdata = Vec::new();
        for s in strings {
            rdata.push(s.len() as u8);
            rdata.extend_from_slice(s.as_bytes());
        }
        rdata
    }

    #[test]
    fn a_generated_key_signs_and_publishes_its_public_half_as_subject_public_key_info() {
        let pair = generate_key().expect("a key");
        let pem = pair.private_pem.expose();
        assert!(pem.contains("PRIVATE KEY-----"), "a PEM");
        // micromail signs with it.
        let config = micromail::DkimConfig::from_pem(pem, "azmail202610", "example.org")
            .expect("micromail reads the key");
        let signed =
            micromail::sign_message(b"From: a@example.org\r\nSubject: x\r\n\r\nhi\r\n", &config)
                .expect("it signs");
        assert!(signed.starts_with(b"DKIM-Signature: v=1; a=rsa-sha256;"));
        // The public half: a 2048-bit RSA SubjectPublicKeyInfo (the rsaEncryption OID and the
        // modulus length are always the same bytes, so the same base64 prefix).
        assert!(
            pair.public_key
                .starts_with("MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA"),
            "{}",
            pair.public_key
        );
        assert_eq!(public_key_of(pem).as_deref(), Ok(pair.public_key.as_str()));
        // The secret never shows.
        assert!(!format!("{pair:?}").contains("PRIVATE KEY"));
    }

    #[test]
    fn the_public_half_of_a_pkcs8_key_is_found_too() {
        // The throwaway 1024-bit test key of send.rs's tests, PKCS#8.
        let pem = concat!(
            "-----BEGIN ",
            "PRIVATE KEY-----\n",
            "MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGBAK9svAXG/GucB2/R\n",
            "OA7Fq+awV6fCCVbobJ2uNlhQIqTmJ3v/T/2SwKh9lhKDirXj9PwcqxJdOnqZDErY\n",
            "MsiDu+ix3dmW5mxcG3FFg7U/FsMJkANFkSvRrQiA5LQq/xdYWjD89JsvIHmicXlz\n",
            "aVsx657iz9PeyYtaWdxL/i12rfqvAgMBAAECgYB7oJuZTrSRebJcAQwKjRAqUVhU\n",
            "55ABaWcycJXoAwGHSJPG9RUAVS3lECx0+7MDoJUEH4gINx+BSt642Ehhu0Tu+xut\n",
            "pgiefm1SwH2NNh3teXHtvhRskK78sj8NGkvwb9mtTaUlp0aUHZNzgHGO/QnHqeyY\n",
            "GBwnvsPj6SClDubpEQJBAOKtrbgw/yJEEg9aaF6Lb0RSsZAmsjU4SJYAIxjwuhWx\n",
            "lQQFj2PRs0EsWnm46yhzNFCTZU8NZmx9qlyBFCj6SlUCQQDGHdILe2P8X2LFZZXj\n",
            "h/LJM01indeZk4baY0SKAfpEXXydbj8KZQbxEy94qG0HjF3TDVy+NFXSmka5+bBh\n",
            "d7zzAkEAjBBDClAEJfknq6LyYJEJtI7gNrEiZm4bs8vr4+pDIUp0SGLjIgueFoRA\n",
            "d3wCmiDtT2h0Le+avSi9DqGXgmZ9bQJAKZJoWPBzcqmxWCqQ4UXNtFqHioIEk71Z\n",
            "NspNv4fatC3J0F8p60x3wG5+L5toBYV2yqqrI15oA+FLpgq28Dzn8QJATS90urxx\n",
            "KtSKrA6f2G0TOmX3GHlzNdIgtMTA45wuSrK95iln2T0aue37BVtCuUq22UPobkZb\n",
            "Uvy4lwqlqL/aoA==\n",
            "-----END ",
            "PRIVATE KEY-----\n",
        );
        let public = public_key_of(pem).expect("the public half");
        // A 1024-bit SubjectPublicKeyInfo.
        assert!(
            public.starts_with("MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQ"),
            "{public}"
        );
        assert!(public_key_of("not a key").is_err());
    }

    #[test]
    fn the_record_is_named_by_selector_and_domain_and_split_for_zone_files() {
        assert_eq!(default_selector(OCT_1), "azmail202610");
        assert!(is_selector("azmail202610"));
        assert!(is_selector("s1-mail"));
        assert!(!is_selector(""));
        assert!(!is_selector("-x"));
        assert!(!is_selector("a b"));
        assert!(!is_selector("a.b"));
        assert!(!is_selector(&"a".repeat(64)));
        assert_eq!(
            record_name("azmail202610", "Example.ORG."),
            "azmail202610._domainkey.example.org"
        );
        let key = format!("MIIB{}", "A".repeat(388));
        assert_eq!(record_value(&key), format!("v=DKIM1; k=rsa; p={key}"));
        let line = zone_line("azmail202610", "example.org", &key);
        assert!(
            line.starts_with("azmail202610._domainkey.example.org. 3600 IN TXT ( \""),
            "{line}"
        );
        assert!(line.ends_with("\" )"), "{line}");
        // Every quoted string at most 255 characters, together the whole value.
        let strings: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
        assert!(strings.len() >= 2, "{line}");
        assert!(strings.iter().all(|s| s.len() <= 255), "{line}");
        assert_eq!(strings.concat(), record_value(&key));
    }

    #[test]
    fn a_mail_providers_domain_cannot_be_signed_for_from_here() {
        assert!(can_sign_for("example.org").is_ok());
        assert!(can_sign_for("mail.schuett.dev").is_ok());
        let gmail = can_sign_for("GMail.com").unwrap_err();
        assert!(gmail.contains("gmail.com"), "{gmail}");
        assert!(can_sign_for("outlook.com").is_err());
        assert!(can_sign_for("icloud.com").is_err());
        assert!(can_sign_for("").is_err());
    }

    #[test]
    fn the_notes_say_what_dmarc_spf_reverse_dns_and_port_25_need() {
        let (name, value) = dmarc_record("example.org", "ada@example.org");
        assert_eq!(name, "_dmarc.example.org");
        assert_eq!(value, "v=DMARC1; p=none; rua=mailto:ada@example.org");
        let notes = setup_notes("example.org", "ada@example.org").join("\n");
        assert!(notes.contains("_dmarc.example.org"), "{notes}");
        assert!(
            notes.contains("v=DMARC1; p=none; rua=mailto:ada@example.org"),
            "{notes}"
        );
        assert!(notes.contains("SPF"), "{notes}");
        assert!(notes.contains("~all"), "{notes}");
        assert!(notes.contains("PTR"), "{notes}");
        assert!(notes.contains("port 25"), "{notes}");
    }

    #[test]
    fn a_txt_answer_is_read_with_its_strings_joined() {
        let name = "azmail202610._domainkey.example.org";
        let packet = answer(
            name,
            0x8180,
            &[
                (DNS_TYPE_TXT, txt(&["v=DKIM1; k=rsa; ", "p=MIIBkey"])),
                // A CNAME on the way is skipped.
                (5, vec![0xC0, 0x0C]),
                (DNS_TYPE_TXT, txt(&["other"])),
            ],
        );
        assert_eq!(
            txt_records(&packet),
            Ok(vec![
                String::from("v=DKIM1; k=rsa; p=MIIBkey"),
                String::from("other")
            ])
        );
        // NXDOMAIN: no record, not an error.
        assert_eq!(txt_records(&answer(name, 0x8183, &[])), Ok(Vec::new()));
        // Truncated (TC) or a server failure: an error that says so.
        let truncated = txt_records(&answer(name, 0x8380, &[])).unwrap_err();
        assert!(truncated.contains("truncated"), "{truncated}");
        assert!(txt_records(&answer(name, 0x8182, &[])).is_err());
        // Malformed: a length past the end.
        let mut broken = answer(name, 0x8180, &[(DNS_TYPE_TXT, txt(&["abc"]))]);
        let at = broken.len() - 4;
        broken[at] = 200;
        assert!(txt_records(&broken).is_err());
        assert!(txt_records(&[0, 1, 2]).is_err());
    }

    #[test]
    fn the_published_record_is_matched_by_its_key() {
        let key = "MIIBIjANkey+/=";
        let ours = format!("v=DKIM1; k=rsa; p={key}");
        assert_eq!(match_record(&[ours.clone()], key), Published::Matches);
        // White space inside the base64 (a zone file split) does not matter.
        assert_eq!(
            match_record(&[String::from("v=DKIM1;k=rsa;p=MIIBIj ANkey+/=")], key),
            Published::Matches
        );
        assert_eq!(
            match_record(&[String::from("v=DKIM1; k=rsa; p=MIIBotherkey")], key),
            Published::Different(String::from("MIIBotherkey"))
        );
        assert_eq!(
            match_record(&[String::from("v=spf1 ~all")], key),
            Published::Missing
        );
        assert_eq!(match_record(&[], key), Published::Missing);
        // A revoked key (empty p=).
        assert_eq!(
            match_record(&[String::from("v=DKIM1; p=")], key),
            Published::Different(String::new())
        );
    }

    #[test]
    fn the_dns_check_reads_as_one_line_per_record() {
        let lines = report_lines(&DnsReport {
            dkim: Published::Matches,
            dmarc: Some(String::from("v=DMARC1; p=none")),
            spf: Some(String::from("v=spf1 mx -all")),
        });
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines[0].contains("published"), "{lines:?}");
        assert_eq!(lines[1], "DMARC: v=DMARC1; p=none");
        assert!(lines[2].starts_with("SPF: v=spf1 mx -all"), "{lines:?}");
        assert!(
            lines[2].contains("~all"),
            "a hard -all is pointed out: {lines:?}"
        );
        let missing = report_lines(&DnsReport {
            dkim: Published::Missing,
            dmarc: None,
            spf: None,
        });
        assert!(missing[0].contains("not found"), "{missing:?}");
        assert!(missing[1].contains("no record"), "{missing:?}");
        assert!(missing[2].contains("no record"), "{missing:?}");
        let other = report_lines(&DnsReport {
            dkim: Published::Different(String::from("MIIBother")),
            dmarc: None,
            spf: Some(String::from("v=spf1 ~all")),
        });
        assert!(other[0].contains("MIIBother"), "{other:?}");
        assert!(!other[2].contains("-all"), "{other:?}");
        let offline = report_lines(&DnsReport {
            dkim: Published::Unknown(String::from("timed out")),
            dmarc: None,
            spf: None,
        });
        assert!(offline[0].contains("timed out"), "{offline:?}");
    }
}
