//! S9E-17 `property_random_chains` (seeded, 2,000 cases): random identity windows, certificate windows,
//! issue and verify times, shapes, anchors, corrupted signatures and the verifier's placement of the
//! account (D-W1S9E3); `verify_credential` agrees with an independent oracle written here from the
//! rules of S9E-03 and S9E-04, one step per rule, with no code shared with the crate (plain `u64`
//! arithmetic).

use azlin_creds::{verify_credential, CredError, Scope, Scopes, Signer, VerifierNow, VerifyRules};
use azlin_regbase::{AnchorSet, KeyEpoch, MetaGroupId, Signature64};

use crate::support::*;

const DAY: u64 = 86_400;
const SKEW: u64 = 30;
const MAX_LIFETIME: u64 = 12 * 3_600;
const MAX_CERT: u64 = 45 * DAY;

#[derive(Clone, Debug)]
struct Case {
    trusted: bool,
    issued: u64,
    expires: u64,
    has_device: bool,
    has_family: bool,
    has_scopes: bool,
    epoch_matches: bool,
    cert_from: u64,
    cert_until: u64,
    /// 0 the identity key of the group, 1 an empty set, 2 the key under another group, 3 another key.
    anchors: u8,
    id_from: u64,
    id_until: Option<u64>,
    cert_sig_ok: bool,
    cred_sig_ok: bool,
    now: u64,
    /// The verifier's placement of the account (D-W1S9E3): 0 the group of the certificate, 1 the
    /// account is not known, 2 another group.
    placement: u8,
}

fn name(r: &Result<azlin_creds::VerifiedCredential, CredError>) -> &'static str {
    match r {
        Ok(_) => "ok",
        Err(CredError::ClockUntrusted) => "ClockUntrusted",
        Err(CredError::Malformed) => "Malformed",
        Err(CredError::LifetimeTooLong) => "LifetimeTooLong",
        Err(CredError::CertMalformed) => "CertMalformed",
        Err(CredError::CertMismatch) => "CertMismatch",
        Err(CredError::NotYetValid) => "NotYetValid",
        Err(CredError::Expired) => "Expired",
        Err(CredError::CertWindow) => "CertWindow",
        Err(CredError::UnknownIdentity) => "UnknownIdentity",
        Err(CredError::IdentityWindow) => "IdentityWindow",
        Err(CredError::BadCert) => "BadCert",
        Err(CredError::BadSignature) => "BadSignature",
        Err(CredError::UnknownAccount) => "UnknownAccount",
        Err(CredError::GroupMismatch) => "GroupMismatch",
        Err(CredError::UnknownScope) => "UnknownScope",
    }
}

/// The oracle: the rules in the order the documentation of `verify_credential` gives them, over plain
/// numbers. No call into the crate.
fn oracle(c: &Case) -> &'static str {
    if !c.trusted {
        return "ClockUntrusted";
    }
    // the shape of the claims
    if c.expires <= c.issued {
        return "Malformed";
    }
    if c.expires - c.issued > MAX_LIFETIME {
        return "LifetimeTooLong";
    }
    if !c.has_device && !c.has_scopes {
        return "Malformed";
    }
    if c.has_device && !c.has_family {
        return "Malformed";
    }
    // the certificate's window is sane
    if !(c.cert_until > c.cert_from && c.cert_until - c.cert_from <= MAX_CERT) {
        return "CertMalformed";
    }
    // the epoch is the certificate's
    if !c.epoch_matches {
        return "CertMismatch";
    }
    // time: not before issued - skew, before expires
    if c.issued >= SKEW && c.now < c.issued - SKEW {
        return "NotYetValid";
    }
    if c.now >= c.expires {
        return "Expired";
    }
    // the certificate covers the issue and the moment
    let inside_cert = |t: u64| c.cert_from <= t && t < c.cert_until;
    if !(inside_cert(c.issued) && inside_cert(c.now)) {
        return "CertWindow";
    }
    // the identity key
    if c.anchors != 0 {
        return "UnknownIdentity";
    }
    let inside_identity = |t: u64| c.id_from <= t && c.id_until.is_none_or(|end| t < end);
    if !(inside_identity(c.cert_from) && inside_identity(c.now)) {
        return "IdentityWindow";
    }
    if !c.cert_sig_ok {
        return "BadCert";
    }
    if !c.cred_sig_ok {
        return "BadSignature";
    }
    // the placement of the account, last: only an authentic chain can be an alarm
    match c.placement {
        0 => "ok",
        1 => "UnknownAccount",
        _ => "GroupMismatch",
    }
}

/// The placement a case asks for: mostly right, otherwise unknown or another group (drawn last, so the
/// draws before it are the ones the property test always made).
fn draw_placement(rng: &mut SplitMix64) -> u8 {
    match rng.below(10) {
        0 => 1,
        1 => 2,
        _ => 0,
    }
}

/// A case that is valid in every respect: the chain, the windows, the shape, the signatures.
fn draw_sane(rng: &mut SplitMix64) -> Case {
    let t0 = 2_000_000u64;
    let issued = t0 + rng.pick(&[0u64, 1, 29, 30, 31, 500, 10_000]);
    let lifetime = rng.pick(&[1u64, 2, 60, 3_600, MAX_LIFETIME - 1, MAX_LIFETIME]);
    let expires = issued + lifetime;
    let cert_from = issued - rng.pick(&[0u64, 1, 100, 1_000]);
    let cert_until = expires + rng.pick(&[0u64, 1, 100, 10_000]);
    let now = issued + rng.below(lifetime);
    let id_from = cert_from - rng.pick(&[0u64, 1, 100]);
    let id_until = if rng.chance(50) {
        None
    } else {
        Some(cert_until + rng.pick(&[0u64, 1, 100]))
    };
    Case {
        trusted: true,
        issued,
        expires,
        has_device: true,
        has_family: true,
        has_scopes: false,
        epoch_matches: true,
        cert_from,
        cert_until,
        anchors: 0,
        id_from,
        id_until,
        cert_sig_ok: true,
        cred_sig_ok: true,
        now,
        placement: 0,
    }
}

/// One fault of a rule, at the boundary of the rule where there is one.
fn fault(rng: &mut SplitMix64, c: &mut Case) {
    match rng.below(22) {
        0 => c.trusted = false,
        1 => c.expires = c.issued,
        2 => c.expires = c.issued - 1,
        3 => c.expires = c.issued + MAX_LIFETIME + 1,
        4 => c.expires = c.issued + MAX_LIFETIME,
        5 => c.has_family = false,
        6 => {
            c.has_device = false;
            c.has_scopes = false;
        }
        7 => {
            c.has_device = false;
            c.has_scopes = true;
        }
        8 => c.cert_until = c.cert_from,
        9 => c.cert_until = c.cert_from.saturating_sub(1),
        10 => c.cert_until = c.cert_from + MAX_CERT + 1,
        11 => c.cert_until = c.cert_from + MAX_CERT,
        12 => c.epoch_matches = false,
        13 => c.now = c.issued - rng.pick(&[31u64, 30, 29]),
        14 => c.now = c.expires - rng.pick(&[0u64, 1]) + rng.pick(&[0u64, 1]),
        15 => c.cert_from = c.issued + 1,
        16 => c.cert_until = c.issued,
        17 => c.cert_until = c.now,
        18 => c.anchors = rng.pick(&[1u8, 2, 3]),
        19 => match rng.below(5) {
            0 => c.id_from = c.cert_from + 1,
            1 => c.id_until = Some(c.cert_from),
            2 => c.id_until = Some(c.cert_from + 1),
            3 => c.id_until = Some(c.now),
            _ => c.id_until = Some(c.now + 1),
        },
        20 => c.cert_sig_ok = false,
        _ => c.cred_sig_ok = false,
    }
}

/// A case: a sane one with up to two faults, or a wild one.
fn draw(rng: &mut SplitMix64) -> Case {
    if rng.chance(30) {
        return draw_wild(rng);
    }
    let mut c = draw_sane(rng);
    for _ in 0..rng.pick(&[0u32, 1, 1, 1, 2]) {
        fault(rng, &mut c);
    }
    c.placement = draw_placement(rng);
    c
}

/// The numbers a case is made of, drawn so that equalities and off-by-one boundaries are frequent.
fn draw_wild(rng: &mut SplitMix64) -> Case {
    let t0 = 2_000_000u64;
    let issued = t0 + rng.pick(&[0, 1, 29, 30, 31, 500, 10_000]);
    let lifetime = rng.pick(&[
        0,
        1,
        2,
        60,
        3_600,
        MAX_LIFETIME - 1,
        MAX_LIFETIME,
        MAX_LIFETIME + 1,
        MAX_LIFETIME * 3,
    ]);
    let expires = if rng.chance(8) {
        issued.saturating_sub(rng.pick(&[0, 1, 5]))
    } else {
        issued + lifetime
    };
    let cert_from = rng.pick(&[
        issued.saturating_sub(10_000),
        issued.saturating_sub(100),
        issued.saturating_sub(1),
        issued,
        issued + 1,
        issued + 100,
    ]);
    let cert_until = rng.pick(&[
        cert_from,
        cert_from.saturating_sub(1),
        cert_from + 1,
        cert_from + 10_000,
        cert_from + MAX_CERT - 1,
        cert_from + MAX_CERT,
        cert_from + MAX_CERT + 1,
        issued,
        issued + 1,
        expires,
        expires + 1,
        issued + 100_000,
    ]);
    let mut edges = vec![
        issued,
        expires,
        cert_from,
        cert_until,
        issued.saturating_sub(SKEW),
        expires.saturating_sub(1),
    ];
    edges.push(rng.pick(&[issued + 5, expires + 5, cert_until + 5]));
    let nudge_up = rng.pick(&[0u64, 0, 0, 1]);
    let nudge_down = rng.pick(&[0u64, 0, 0, 1, 31]);
    let now = rng
        .pick(&edges)
        .saturating_add(nudge_up)
        .saturating_sub(nudge_down);
    let id_from = rng.pick(&[
        0,
        cert_from.saturating_sub(1),
        cert_from,
        cert_from + 1,
        now.saturating_sub(1),
        now,
        now + 1,
    ]);
    let id_until = match rng.below(8) {
        0..=2 => None,
        3 => Some(cert_from),
        4 => Some(cert_from + 1),
        5 => Some(now),
        6 => Some(now + 1),
        _ => Some(now + 1_000_000),
    };
    Case {
        trusted: rng.chance(92),
        issued,
        expires,
        has_device: rng.chance(70),
        has_family: rng.chance(85),
        has_scopes: rng.chance(60),
        epoch_matches: rng.chance(94),
        cert_from,
        cert_until,
        anchors: if rng.chance(86) {
            0
        } else {
            rng.pick(&[1, 2, 3])
        },
        id_from,
        id_until,
        cert_sig_ok: rng.chance(94),
        cred_sig_ok: rng.chance(94),
        now,
        placement: draw_placement(rng),
    }
}

fn run(c: &Case) -> Result<azlin_creds::VerifiedCredential, CredError> {
    let identity = signer(1);
    let cred = signer(2);
    // The certificate, signed by hand: the issuer would refuse some of these.
    let mut cert = signed_cert(
        &identity,
        GROUP,
        EPOCH,
        cred.public(),
        c.cert_from,
        c.cert_until,
    );
    if !c.cert_sig_ok {
        let mut bytes = *cert.identity_sig.as_bytes();
        bytes[5] ^= 0x10;
        cert.identity_sig = Signature64::from_bytes(bytes);
    }
    let mut claims = claims_of(grant());
    claims.issued_at = at(c.issued);
    claims.expires_at = at(c.expires);
    if !c.has_device {
        claims.device = None;
    }
    if !c.has_family {
        claims.family = None;
    }
    claims.scopes = if c.has_scopes {
        Some(
            Scopes::new(vec![Scope::LogAppend {
                log: azlin_creds::LogRef::from_bytes([3; 16]),
            }])
            .expect("one scope"),
        )
    } else {
        None
    };
    claims.key_epoch = if c.epoch_matches {
        EPOCH
    } else {
        KeyEpoch::new(EPOCH.get() + 1)
    };
    let signing_key = if c.cred_sig_ok { signer(2) } else { signer(8) };
    let credential = signed_credential(&signing_key, cert, claims);
    let windows = [(c.id_from, c.id_until)];
    let anchors: AnchorSet = match c.anchors {
        0 => fixture_anchors(GROUP, 1, &windows),
        1 => AnchorSet::empty(),
        2 => fixture_anchors(MetaGroupId::new(2), 1, &windows),
        _ => fixture_anchors(GROUP, 4, &windows),
    };
    let now = if c.trusted {
        VerifierNow::trusted(node(c.now))
    } else {
        VerifierNow::untrusted(node(c.now))
    };
    let account = grant().account;
    let groups = match c.placement {
        0 => FixedGroups::of(&[(account, GROUP)]),
        1 => FixedGroups::of(&[]),
        _ => FixedGroups::of(&[(account, MetaGroupId::new(3))]),
    };
    verify_credential(&credential, &anchors, &groups, now, &VerifyRules::default())
}

#[test]
fn s9e_17_the_verifier_agrees_with_the_oracle_on_2000_random_chains() {
    let mut seen = std::collections::BTreeMap::<&'static str, u32>::new();
    for index in 0..2_000u64 {
        let seed = 0x9E5_0017 ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut rng = SplitMix64::new(seed);
        let case = draw(&mut rng);
        let expected = oracle(&case);
        let actual = name(&run(&case));
        assert_eq!(actual, expected, "case {index} (seed {seed:#x}): {case:?}");
        *seen.entry(expected).or_insert(0) += 1;
    }
    // The generator reaches every refusal: a property test that never meets a rule proves nothing.
    for rule in [
        "ok",
        "ClockUntrusted",
        "Malformed",
        "LifetimeTooLong",
        "CertMalformed",
        "CertMismatch",
        "NotYetValid",
        "Expired",
        "CertWindow",
        "UnknownIdentity",
        "IdentityWindow",
        "BadCert",
        "BadSignature",
        "UnknownAccount",
        "GroupMismatch",
    ] {
        let count = seen.get(rule).copied().unwrap_or(0);
        assert!(
            count >= 8,
            "the generator met {rule} only {count} times: {seen:?}"
        );
    }
}
