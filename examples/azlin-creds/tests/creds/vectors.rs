//! The golden vectors of the credential family (the brief's S9E-14, the credential and certificate
//! half): `tests/vectors/credential_cert_v1.json` and `tests/vectors/credential_v2.json`. They pin the
//! CANONICAL BYTES (the integer keys, the order, the shortest forms), the signing keys' seeds and the
//! signatures, so that a change of the wire form is a red test and not a silent fork. INT-WIRE copies
//! them to azlin-contract's `fixtures/c4_*.json`.
//!
//! The files are written only under `UPDATE_VECTORS=1` and never when `CI` is set; otherwise the test
//! compares the rows it computes with the file and fails with the first difference. A second test reads
//! the FILE alone, decodes every row through the crate's decoders and verifies every signature with
//! azlin-keys, so a vector is checked by something that did not compute it.

use std::path::PathBuf;

use azlin_creds::Canonical;
use azlin_creds::{
    verify_credential_bytes, AppPerms, AppScope, Certificate, Credential, CredentialClaims,
    LinkRef, Scope, Scopes, SsoClaim, CREDENTIAL_CERT_V1, CREDENTIAL_V2,
};
use azlin_keys::sign::verify;
use azlin_keys::{Secret, SignKeypair, Signature};
use azlin_regbase::{SmallText, SpaceId};

use crate::support::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub kind: String,
    pub context: String,
    pub hex: String,
    pub extra: String,
    pub expect: String,
}

impl Row {
    pub fn new(name: &str, kind: &str, context: &str, hex: &str, extra: &str, expect: &str) -> Row {
        Row {
            name: name.to_string(),
            kind: kind.to_string(),
            context: context.to_string(),
            hex: hex.to_string(),
            extra: extra.to_string(),
            expect: expect.to_string(),
        }
    }
}

pub fn vector_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vectors")
        .join(file)
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            other => panic!("vector files hold printable ASCII only, not {other:?}"),
        }
    }
    out.push('"');
    out
}

/// One object per line, so a diff shows one vector.
fn render_json(rows: &[Row]) -> String {
    let mut out = String::from("[\n");
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "  {{\"name\": {}, \"valid\": true, \"kind\": {}, \"context\": {}, \"hex\": {}, \"extra\": {}, \"expect\": {}}}",
            json_str(&r.name),
            json_str(&r.kind),
            json_str(&r.context),
            json_str(&r.hex),
            json_str(&r.extra),
            json_str(&r.expect)
        ));
        out.push_str(if i + 1 < rows.len() { ",\n" } else { "\n" });
    }
    out.push_str("]\n");
    out
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(|b| b.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) {
        self.ws();
        assert_eq!(
            self.s.get(self.i),
            Some(&c),
            "vector file: expected {:?} at byte {}",
            c as char,
            self.i
        );
        self.i += 1;
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn string(&mut self) -> String {
        self.eat(b'"');
        let mut out = String::new();
        loop {
            let b = *self.s.get(self.i).expect("unterminated string");
            self.i += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let e = *self.s.get(self.i).expect("unterminated escape");
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        other => panic!("vector file: bad escape {:?}", other as char),
                    }
                }
                _ => out.push(b as char),
            }
        }
        out
    }

    fn boolean(&mut self) -> bool {
        self.ws();
        for (word, value) in [("true", true), ("false", false)] {
            if self.s[self.i..].starts_with(word.as_bytes()) {
                self.i += word.len();
                return value;
            }
        }
        panic!("vector file: expected a boolean at byte {}", self.i)
    }

    fn object(&mut self) -> Row {
        self.eat(b'{');
        let mut row = Row::new("", "", "", "", "", "");
        loop {
            let key = self.string();
            self.eat(b':');
            match key.as_str() {
                "name" => row.name = self.string(),
                "valid" => assert!(self.boolean(), "every row of these files is a valid vector"),
                "kind" => row.kind = self.string(),
                "context" => row.context = self.string(),
                "hex" => row.hex = self.string(),
                "extra" => row.extra = self.string(),
                "expect" => row.expect = self.string(),
                other => panic!("vector file: unknown key {other:?}"),
            }
            if self.peek() == Some(b',') {
                self.i += 1;
            } else {
                break;
            }
        }
        self.eat(b'}');
        row
    }

    fn rows(&mut self) -> Vec<Row> {
        self.eat(b'[');
        let mut rows = Vec::new();
        loop {
            rows.push(self.object());
            if self.peek() == Some(b',') {
                self.i += 1;
            } else {
                break;
            }
        }
        self.eat(b']');
        rows
    }
}

pub fn parse_json(text: &str) -> Vec<Row> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let rows = p.rows();
    p.ws();
    assert_eq!(p.i, text.len(), "vector file: trailing bytes");
    rows
}

/// Compares `computed` with the vector file, or writes it under `UPDATE_VECTORS=1` (never in CI).
pub fn check_or_update(file: &str, computed: &[Row]) {
    let path = vector_path(file);
    if std::env::var_os("UPDATE_VECTORS").is_some_and(|v| v == "1") {
        assert!(
            std::env::var_os("CI").is_none(),
            "UPDATE_VECTORS=1 refuses to write {file} when CI is set"
        );
        std::fs::create_dir_all(path.parent().expect("a parent folder")).expect("vectors folder");
        std::fs::write(&path, render_json(computed)).expect("write the vector file");
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{file} is missing or unreadable ({e}): UPDATE_VECTORS=1 writes it, never in CI")
    });
    let on_disk = parse_json(&text);
    assert_eq!(on_disk.len(), computed.len(), "{file}: the number of rows");
    for (disk, mine) in on_disk.iter().zip(computed) {
        assert_eq!(
            disk, mine,
            "{file}: row {} differs from what the crate computes",
            mine.name
        );
    }
}

pub fn seed_text(n: u8) -> String {
    format!("seed={}", to_hex(&[n; 32]))
}

fn cert_rows() -> Vec<Row> {
    let f = fixture();
    let body = f.cert.canonical_bytes().expect("body");
    vec![
        Row::new(
            "cert_fixture_body",
            "cert_body",
            "azlin-credential-cert-v1",
            &to_hex(&body),
            &seed_text(1),
            &to_hex(f.cert.identity_sig.as_bytes()),
        ),
        Row::new(
            "cert_fixture_wire",
            "cert_wire",
            "",
            &to_hex(&f.cert.encode().expect("wire")),
            &seed_text(1),
            "ok",
        ),
    ]
}

/// The claims of an app principal's credential: scopes and an sso fact, no device, no family.
fn app_claims() -> CredentialClaims {
    let mut c = claims_of(grant());
    c.device = None;
    c.family = None;
    c.scopes = Some(
        Scopes::new(vec![
            Scope::App(AppScope {
                space: SpaceId::from_bytes([9; 16]),
                perms: AppPerms::READ.union(AppPerms::WRITE),
            }),
            Scope::LinkRead {
                link: LinkRef::from_bytes([1; 16]),
            },
        ])
        .expect("two scopes"),
    );
    c.sso = Some(SsoClaim {
        provider: SmallText::new("entra-contoso").expect("a provider"),
        auth_time: at(ISSUED - 60),
    });
    c
}

fn credential_rows() -> Vec<Row> {
    let f = fixture();
    let mut rows = Vec::new();
    for (name, claims) in [
        ("credential_device", claims_of(grant())),
        ("credential_app", app_claims()),
    ] {
        let credential = signed_credential(&f.cred, f.cert, claims);
        rows.push(Row::new(
            &format!("{name}_payload"),
            "credential_payload",
            "azlin-credential-v2",
            &to_hex(credential.payload()),
            &seed_text(2),
            &to_hex(credential.signature().as_bytes()),
        ));
        rows.push(Row::new(
            &format!("{name}_wire"),
            "credential_wire",
            "",
            &to_hex(&credential.encode().expect("wire")),
            &format!("{} group=1 now={ISSUED}", seed_text(1)),
            "ok",
        ));
    }
    rows
}

#[test]
fn s9e_14_the_certificate_vectors_are_what_the_crate_computes() {
    check_or_update("credential_cert_v1.json", &cert_rows());
}

#[test]
fn s9e_14_the_credential_vectors_are_what_the_crate_computes() {
    check_or_update("credential_v2.json", &credential_rows());
}

pub fn seed_of(extra: &str) -> [u8; 32] {
    let hex = extra
        .split_whitespace()
        .find_map(|part| part.strip_prefix("seed="))
        .expect("a seed in the row");
    <[u8; 32]>::try_from(from_hex(hex).as_slice()).expect("32 bytes")
}

#[test]
fn s9e_14_the_vector_files_verify_without_the_code_that_wrote_them() {
    for file in ["credential_cert_v1.json", "credential_v2.json"] {
        let text = std::fs::read_to_string(vector_path(file)).unwrap_or_else(|e| {
            panic!("{file} is missing ({e}): UPDATE_VECTORS=1 writes it, never in CI")
        });
        let rows = parse_json(&text);
        assert!(!rows.is_empty());
        for row in rows {
            let bytes = from_hex(&row.hex);
            match row.kind.as_str() {
                "cert_body" | "credential_payload" => {
                    let domain = match row.context.as_str() {
                        "azlin-credential-cert-v1" => CREDENTIAL_CERT_V1,
                        "azlin-credential-v2" => CREDENTIAL_V2,
                        other => panic!("{}: an unknown domain {other}", row.name),
                    };
                    let key = SignKeypair::from_seed(&Secret::from_bytes(seed_of(&row.extra)))
                        .verify_key();
                    let sig = Signature::from_bytes(
                        <[u8; 64]>::try_from(from_hex(&row.expect).as_slice()).expect("64 bytes"),
                    );
                    assert!(
                        verify(&domain, &bytes, &key, &sig).is_ok(),
                        "{}: the signature over domain || 0x00 || bytes",
                        row.name
                    );
                    if row.kind == "credential_payload" {
                        let claims = CredentialClaims::decode_payload(&bytes).expect("claims");
                        assert_eq!(
                            claims.canonical_bytes().expect("again"),
                            bytes,
                            "{}",
                            row.name
                        );
                    }
                }
                "cert_wire" => {
                    let cert = Certificate::decode(&bytes).expect("a certificate");
                    assert_eq!(cert.encode().expect("again"), bytes, "{}", row.name);
                    let anchors = fixture_anchors(GROUP, 1, &[(0, None)]);
                    azlin_creds::verify_cert(&cert, &anchors).expect("verifies");
                }
                "credential_wire" => {
                    let credential = Credential::decode(&bytes).expect("a credential");
                    assert_eq!(credential.encode().expect("again"), bytes, "{}", row.name);
                    let anchors = fixture_anchors(GROUP, 1, &[(0, None)]);
                    verify_credential_bytes(
                        &bytes,
                        &anchors,
                        now(ISSUED),
                        &azlin_creds::VerifyRules::default(),
                    )
                    .unwrap_or_else(|e| panic!("{}: {e}", row.name));
                }
                other => panic!("{}: an unknown kind {other}", row.name),
            }
        }
    }
}
