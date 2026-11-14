//! Helpers the tests share: a seeded generator (SplitMix64, no dependency), hex, the fixture keys and
//! anchors (`fixture_anchors`, `issue_fixture`), manual signing of hand-made structures (the issuer
//! refuses what the verifier refuses, so the tests that need those sign them themselves), and the
//! crafting of raw credentials around an arbitrary payload value.
#![allow(dead_code)]

use azlin_cbor::{encode, IntMapBuilder, Limits, Value};
use azlin_creds::{
    Canonical, Certificate, Credential, CredentialClaims, CredentialIssuer, GrantClaims, NodeTime,
    SeedSigner, Signer, VerifierNow, VerifyRules, CREDENTIAL_CERT_V1, CREDENTIAL_V2,
};
use azlin_keys::{Secret, Span};
use azlin_regbase::{
    AccountId, AnchorSet, CredentialNonce, DeviceId, FamilyId, KeyEpoch, KeyWindow, LeaderTime,
    MemberId, MetaGroupId, PolicyEpoch, Signature64,
};

/// SplitMix64: the seeded generator of every property test. A failing case prints its seed, and
/// `SplitMix64::new(seed)` replays it.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n` (`n > 0`); the modulo bias is irrelevant for tests.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize]
    }

    pub fn id16(&mut self) -> [u8; 16] {
        let mut out = [0u8; 16];
        for chunk in out.chunks_mut(8) {
            let word = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        out
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd hex length: {text}");
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("hex digit"))
        .collect()
}

pub fn at(secs: u64) -> LeaderTime {
    LeaderTime::from_secs(secs)
}

pub fn node(secs: u64) -> NodeTime {
    NodeTime::from_secs(secs)
}

/// A trusted node clock reading `secs`.
pub fn now(secs: u64) -> VerifierNow {
    VerifierNow::trusted(node(secs))
}

pub fn signer(n: u8) -> SeedSigner {
    SeedSigner::from_seed(&Secret::from_bytes([n; 32]))
}

pub const GROUP: MetaGroupId = MetaGroupId::new(1);
pub const EPOCH: KeyEpoch = KeyEpoch::new(7);

/// The moment the fixture's credentials are issued, and their lifetime.
pub const ISSUED: u64 = 100_000;
pub const LIFETIME: u64 = 3_600;
/// The window of the fixture's certificate: it covers the whole of a day around `ISSUED`.
pub const CERT_FROM: u64 = 50_000;
pub const CERT_SPAN: u64 = 30 * 86_400;

/// The keys, the anchors and the certificate of one metadata group.
pub struct Fixture {
    pub identity: SeedSigner,
    pub cred: SeedSigner,
    pub anchors: AnchorSet,
    pub cert: Certificate,
    pub rules: VerifyRules,
}

/// The anchors of one group: the identity key of seed `identity_seed`, with these windows (a window
/// `(from, until)`, `until = None` for one that never ends).
pub fn fixture_anchors(
    group: MetaGroupId,
    identity_seed: u8,
    windows: &[(u64, Option<u64>)],
) -> AnchorSet {
    let identity = signer(identity_seed);
    AnchorSet {
        payments: Vec::new(),
        operator: Vec::new(),
        identity: windows
            .iter()
            .map(|(from, until)| {
                (
                    group,
                    KeyWindow {
                        public: identity.public(),
                        not_before: at(*from),
                        not_after: until.map(at),
                        revoked_from: None,
                    },
                )
            })
            .collect(),
        map_epoch: 1,
    }
}

/// A group with identity seed 1 (open-ended window from 0) and credential seed 2, certified for
/// `[CERT_FROM, CERT_FROM + CERT_SPAN)` at epoch `EPOCH`.
pub fn fixture() -> Fixture {
    let identity = signer(1);
    let cred = signer(2);
    let anchors = fixture_anchors(GROUP, 1, &[(0, None)]);
    let cert = azlin_creds::certify(
        &identity,
        GROUP,
        EPOCH,
        cred.public(),
        at(CERT_FROM),
        at(CERT_FROM + CERT_SPAN),
    )
    .expect("the fixture's certificate");
    Fixture {
        identity,
        cred,
        anchors,
        cert,
        rules: VerifyRules::default(),
    }
}

/// The claims of a device credential: a device and its family, no scopes.
pub fn grant() -> GrantClaims {
    GrantClaims {
        account: AccountId::from_bytes([0x11; 16]),
        member: MemberId::from_bytes([0x22; 16]),
        device: Some(DeviceId::from_bytes([0x33; 16])),
        family: Some(FamilyId::from_bytes([0x44; 16])),
        policy_epoch: PolicyEpoch::new(3),
        scopes: None,
        issued_at: at(ISSUED),
        expires_at: at(ISSUED + LIFETIME),
        nonce: CredentialNonce::from_bytes([0x55; 16]),
        sso: None,
    }
}

/// A credential through the real issuer (the probe and `issue`).
pub fn issue_fixture() -> (Fixture, Credential) {
    let f = fixture();
    let credential = {
        let issuer = CredentialIssuer::new(&f.cred, f.cert, &f.anchors, at(ISSUED))
            .expect("the fixture's issuer");
        issuer.issue(grant()).expect("the fixture's credential")
    };
    (f, credential)
}

/// The claims of a grant with the fixture's key epoch.
pub fn claims_of(g: GrantClaims) -> CredentialClaims {
    CredentialClaims {
        account: g.account,
        member: g.member,
        device: g.device,
        family: g.family,
        policy_epoch: g.policy_epoch,
        scopes: g.scopes,
        issued_at: g.issued_at,
        expires_at: g.expires_at,
        key_epoch: EPOCH,
        nonce: g.nonce,
        sso: g.sso,
    }
}

/// A credential of arbitrary claims, signed by `cred` and carrying `cert`: what the issuer refuses to
/// make and the verifier must refuse.
pub fn signed_credential(
    cred: &dyn Signer,
    cert: Certificate,
    claims: CredentialClaims,
) -> Credential {
    let payload = claims.canonical_bytes().expect("canonical claims");
    let sig = cred.sign(&CREDENTIAL_V2, &payload).expect("a signature");
    Credential::from_parts(claims, cert, sig).expect("a credential")
}

/// A certificate of arbitrary fields signed by `identity` over its body (`certify` refuses a window
/// that is not sane; a verifier must too).
pub fn signed_cert(
    identity: &dyn Signer,
    group: MetaGroupId,
    epoch: KeyEpoch,
    cred_pub: azlin_regbase::PublicKey32,
    not_before: u64,
    not_after: u64,
) -> Certificate {
    let mut cert = Certificate {
        group,
        epoch,
        cred_pub,
        identity_pub: identity.public(),
        not_before: at(not_before),
        not_after: at(not_after),
        identity_sig: Signature64::from_bytes([0u8; 64]),
    };
    let body = cert.canonical_bytes().expect("the body");
    cert.identity_sig = identity
        .sign(&CREDENTIAL_CERT_V1, &body)
        .expect("a signature");
    cert
}

/// The limits big enough for any hand-made structure of the tests.
pub const BIG: Limits = Limits {
    max_input: 1 << 20,
    max_depth: 8,
    max_items: 65_536,
    max_total_items: 1 << 20,
    max_text: 1 << 20,
};

/// The bytes of a credential around an arbitrary payload VALUE (a map the test built by hand): the
/// payload is signed by `cred`, the certificate is `cert`.
pub fn craft_credential(cred: &dyn Signer, cert: &Certificate, payload: &Value) -> Vec<u8> {
    let payload_bytes = encode(payload, &BIG).expect("the payload");
    let sig = cred
        .sign(&CREDENTIAL_V2, &payload_bytes)
        .expect("a signature");
    let outer = IntMapBuilder::new()
        .put(1, Value::Bytes(payload_bytes))
        .put(2, cert.to_value().expect("the certificate"))
        .put(3, Value::Bytes(sig.as_bytes().to_vec()))
        .build()
        .expect("the wrapper");
    encode(&outer, &BIG).expect("the credential")
}

/// The payload map of a credential's claims, key by key as the crate writes it (the keys are part of
/// the wire form; the vectors pin them).
pub fn payload_map(c: &CredentialClaims) -> IntMapBuilder {
    let mut b = IntMapBuilder::new()
        .put(1, Value::Bytes(c.account.as_bytes().to_vec()))
        .put(2, Value::Bytes(c.member.as_bytes().to_vec()));
    if let Some(device) = &c.device {
        b = b.put(3, Value::Bytes(device.as_bytes().to_vec()));
    }
    if let Some(family) = &c.family {
        b = b.put(4, Value::Bytes(family.as_bytes().to_vec()));
    }
    b.put(5, Value::Uint(u64::from(c.policy_epoch.get())))
        .put(7, Value::Uint(c.issued_at.as_secs()))
        .put(8, Value::Uint(c.expires_at.as_secs()))
        .put(9, Value::Uint(u64::from(c.key_epoch.get())))
        .put(10, Value::Bytes(c.nonce.as_bytes().to_vec()))
}

/// A span of `secs` seconds.
pub fn span(secs: u64) -> Span {
    Span::secs(secs)
}
