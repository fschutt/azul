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
/// every DNS provider's TXT field).
pub const KEY_BITS: usize = 2048;
/// DNS type TXT.
const DNS_TYPE_TXT: u16 = 16;
/// The longest character-string of a TXT record (RFC 1035 3.3).
const TXT_STRING_MAX: usize = 255;

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
    Err(String::from("not yet"))
}

/// The public half of a private key (PEM, PKCS#1 or PKCS#8), as base64 SubjectPublicKeyInfo.
pub fn public_key_of(private_pem: &str) -> Result<String, String> {
    let _ = private_pem;
    Err(String::from("not yet"))
}

/// The selector for a key made at `now` (seconds since 1970): `azmail<yyyy><mm>`.
pub fn default_selector(now: i64) -> String {
    let _ = now;
    String::new()
}

/// Whether `selector` can be a DNS label: 1 to 63 letters, digits and `-`, not starting or
/// ending with `-` (dots would make it several labels, which DKIM allows but nobody needs).
pub fn is_selector(selector: &str) -> bool {
    let _ = selector;
    false
}

/// The name of the TXT record: `<selector>._domainkey.<domain>`.
pub fn record_name(selector: &str, domain: &str) -> String {
    let _ = (selector, domain);
    String::new()
}

/// The TXT record's value as a DNS provider's web form wants it (one line; the provider splits
/// it): `v=DKIM1; k=rsa; p=<public key>`.
pub fn record_value(public_key: &str) -> String {
    let _ = public_key;
    String::new()
}

/// The record as a line of a zone file: `<name>. 3600 IN TXT ( "..." "..." )`, the value split
/// into strings of at most 255 characters (a 2048-bit key is longer than one).
pub fn zone_line(selector: &str, domain: &str, public_key: &str) -> String {
    let _ = (selector, domain, public_key);
    String::new()
}

/// Whether AzMail can sign for `domain` from this computer: not for a mail provider's domain
/// (gmail.com, outlook.com, ...), whose DNS nobody but the provider can change.
pub fn can_sign_for(domain: &str) -> Result<(), String> {
    let _ = domain;
    Ok(())
}

/// The DMARC record to publish for `domain` while starting out: its name
/// (`_dmarc.<domain>`) and value (`v=DMARC1; p=none; rua=mailto:<reports_to>`).
pub fn dmarc_record(domain: &str, reports_to: &str) -> (String, String) {
    let _ = (domain, reports_to);
    (String::new(), String::new())
}

/// What the Sending page says under the DKIM record: DMARC, SPF, reverse DNS and port 25, for
/// `domain` and the sender `address`. One paragraph per entry.
pub fn setup_notes(domain: &str, address: &str) -> Vec<String> {
    let _ = (domain, address);
    Vec::new()
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

/// The TXT records in a DNS answer (each record's strings joined, as RFC 6376 3.6.2.2 reads
/// them); none for a name that does not exist (NXDOMAIN); an error for a malformed or truncated
/// answer or a server failure.
pub fn txt_records(packet: &[u8]) -> Result<Vec<String>, String> {
    let _ = packet;
    Err(String::from("not yet"))
}

/// Which of `records` (the TXT records at the DKIM name) publishes `public_key`.
pub fn match_record(records: &[String], public_key: &str) -> Published {
    let _ = (records, public_key);
    Published::Missing
}

/// Asks DNS for the DKIM record of `selector` / `domain` (blocking: call it from an azul
/// `Thread`).
pub fn check_published(selector: &str, domain: &str, public_key: &str) -> Published {
    let _ = (selector, domain, public_key);
    Published::Unknown(String::from("not yet"))
}

/// The DKIM, DMARC and SPF records of `domain` (blocking: call it from an azul `Thread`).
pub fn dns_report(selector: &str, domain: &str, public_key: &str) -> DnsReport {
    DnsReport {
        dkim: check_published(selector, domain, public_key),
        dmarc: None,
        spf: None,
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
        let signed = micromail::sign_message(b"From: a@example.org\r\nSubject: x\r\n\r\nhi\r\n", &config)
            .expect("it signs");
        assert!(signed.starts_with(b"DKIM-Signature: v=1; a=rsa-sha256;"));
        // The public half: a 2048-bit RSA SubjectPublicKeyInfo (the rsaEncryption OID and the
        // modulus length are always the same bytes, so the same base64 prefix).
        assert!(
            pair.public_key.starts_with("MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA"),
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
        assert!(public.starts_with("MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQ"), "{public}");
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
        let strings: Vec<&str> = line
            .split('"')
            .skip(1)
            .step_by(2)
            .collect();
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
        assert!(notes.contains("v=DMARC1; p=none; rua=mailto:ada@example.org"), "{notes}");
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
            Ok(vec![String::from("v=DKIM1; k=rsa; p=MIIBkey"), String::from("other")])
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
        assert_eq!(match_record(&[String::from("v=spf1 ~all")], key), Published::Missing);
        assert_eq!(match_record(&[], key), Published::Missing);
        // A revoked key (empty p=).
        assert_eq!(
            match_record(&[String::from("v=DKIM1; p=")], key),
            Published::Different(String::new())
        );
    }
}
