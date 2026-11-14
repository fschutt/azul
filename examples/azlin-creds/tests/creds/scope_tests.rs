//! S9E-06 `scopes_only_narrow` (planning/architecture.md section 5.11) and S9E-20
//! `scopes_for_app_principals_come_from_the_row` (section 5.3, 6.8).

use azlin_cbor::Value;
use azlin_creds::{
    verify_credential, verify_credential_bytes, AppPerms, AppScope, CredError, Credential,
    CredentialIssuer, IssueError, LinkRef, LogRef, RequestRef, Scope, ScopedOp, Scopes,
    ScopesError, WireError, MAX_SCOPES,
};
use azlin_regbase::SpaceId;

use crate::support::*;

const SPACES: [[u8; 16]; 3] = [[0xA1; 16], [0xA2; 16], [0xA3; 16]];

fn space(i: usize) -> SpaceId {
    SpaceId::from_bytes(SPACES[i % SPACES.len()])
}

/// A random scope from a small universe, so that two random lists overlap often.
fn random_scope(rng: &mut SplitMix64) -> Scope {
    match rng.below(4) {
        0 => Scope::LinkRead {
            link: LinkRef::from_bytes([rng.below(3) as u8; 16]),
        },
        1 => Scope::RequestUpload {
            request: RequestRef::from_bytes([rng.below(3) as u8; 16]),
        },
        2 => Scope::LogAppend {
            log: LogRef::from_bytes([rng.below(3) as u8; 16]),
        },
        _ => Scope::App(AppScope {
            space: space(rng.below(3) as usize),
            perms: AppPerms::new(1 + rng.below(7) as u8).expect("1 to 7"),
        }),
    }
}

fn random_scopes(rng: &mut SplitMix64) -> Scopes {
    let n = rng.below(7) as usize;
    Scopes::new((0..n).map(|_| random_scope(rng)).collect()).expect("a few scopes")
}

/// Every operation of the universe: what a scope list can be asked.
fn all_ops() -> Vec<ScopedOp> {
    let mut ops = vec![ScopedOp::Unscoped];
    for i in 0..3u8 {
        ops.push(ScopedOp::LinkRead {
            link: LinkRef::from_bytes([i; 16]),
        });
        ops.push(ScopedOp::RequestUpload {
            request: RequestRef::from_bytes([i; 16]),
        });
        ops.push(ScopedOp::LogAppend {
            log: LogRef::from_bytes([i; 16]),
        });
    }
    for s in 0..3 {
        for bits in 1..=7u8 {
            ops.push(ScopedOp::App {
                space: space(s),
                need: AppPerms::new(bits).expect("1 to 7"),
            });
        }
    }
    ops
}

#[test]
fn s9e_06_narrow_is_an_intersection_property_of_1000_random_pairs() {
    let ops = all_ops();
    let mut rng = SplitMix64::new(0x5C0_9E5);
    let mut non_empty = 0;
    for case in 0..1_000 {
        let a = random_scopes(&mut rng);
        let b = random_scopes(&mut rng);
        let ab = a.narrow(&b);
        let ba = b.narrow(&a);
        assert!(ab.is_subset_of(&a), "case {case}: a.narrow(b) within a");
        assert!(ab.is_subset_of(&b), "case {case}: a.narrow(b) within b");
        assert_eq!(ab, ba, "case {case}: commutative");
        assert_eq!(a.narrow(&a), a, "case {case}: idempotent");
        assert!(ab.len() <= MAX_SCOPES);
        // The meaning: an operation is allowed by the narrowing exactly when BOTH allow it.
        for op in &ops {
            assert_eq!(
                ab.allows(op),
                a.allows(op) && b.allows(op),
                "case {case}: {op:?}"
            );
        }
        if !ab.is_empty() {
            non_empty += 1;
        }
    }
    assert!(
        non_empty > 100,
        "the generator makes overlapping lists ({non_empty})"
    );
}

#[test]
fn s9e_06_a_scope_list_is_canonical_by_construction() {
    let a = Scope::LinkRead {
        link: LinkRef::from_bytes([2; 16]),
    };
    let b = Scope::LinkRead {
        link: LinkRef::from_bytes([1; 16]),
    };
    let scopes = Scopes::new(vec![a, b, a, b]).expect("four entries");
    assert_eq!(scopes.as_slice(), &[b, a], "sorted and without duplicates");
    // Two app scopes of one space are one scope with the union of the permissions.
    let merged = Scopes::new(vec![
        Scope::App(AppScope {
            space: space(0),
            perms: AppPerms::READ,
        }),
        Scope::App(AppScope {
            space: space(0),
            perms: AppPerms::WRITE,
        }),
    ])
    .expect("two entries");
    assert_eq!(
        merged.as_slice(),
        &[Scope::App(AppScope {
            space: space(0),
            perms: AppPerms::READ.union(AppPerms::WRITE),
        })]
    );
    // Nothing is allowed that no scope names, and an operation of another target is refused.
    assert!(merged.allows(&ScopedOp::App {
        space: space(0),
        need: AppPerms::READ
    }));
    assert!(!merged.allows(&ScopedOp::App {
        space: space(0),
        need: AppPerms::MANAGE
    }));
    assert!(!merged.allows(&ScopedOp::App {
        space: space(1),
        need: AppPerms::READ
    }));
    assert!(!merged.allows(&ScopedOp::Unscoped), "scopes only narrow");
    // No permission, and a bit that is not defined, make no permission set.
    assert_eq!(AppPerms::new(0), None);
    assert_eq!(AppPerms::new(8), None);
    assert!(AppPerms::new(7).is_some());
}

#[test]
fn s9e_06_more_than_max_scopes_are_refused_never_truncated() {
    let many: Vec<Scope> = (0..=MAX_SCOPES as u8)
        .map(|i| Scope::LinkRead {
            link: LinkRef::from_bytes([i; 16]),
        })
        .collect();
    assert_eq!(many.len(), MAX_SCOPES + 1);
    assert_eq!(
        Scopes::new(many.clone()),
        Err(ScopesError::TooMany {
            count: MAX_SCOPES + 1
        })
    );
    assert!(Scopes::new(many[..MAX_SCOPES].to_vec()).is_ok());

    // On the wire: a credential whose payload lists one scope too many is `Malformed` (the strict
    // decoder says `TooMany`).
    let f = fixture();
    let mut claims = claims_of(grant());
    claims.device = None;
    claims.family = None;
    let scope_values: Vec<Value> = many
        .iter()
        .map(|s| match s {
            Scope::LinkRead { link } => {
                Value::Array(vec![Value::Uint(1), Value::Bytes(link.as_bytes().to_vec())])
            }
            _ => unreachable!("only link scopes here"),
        })
        .collect();
    let payload = payload_map(&claims)
        .put(6, Value::Array(scope_values))
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    assert_eq!(
        Credential::decode(&bytes).err(),
        Some(WireError::TooMany),
        "the strict decoder"
    );
    assert_eq!(
        verify_credential_bytes(&bytes, &f.anchors, now(ISSUED), &f.rules).err(),
        Some(CredError::Malformed)
    );
}

#[test]
fn s9e_06_an_unknown_scope_is_refused_never_read_as_allow_all() {
    let f = fixture();
    let mut claims = claims_of(grant());
    claims.device = None;
    claims.family = None;
    let id = Value::Bytes(vec![1; 16]);
    let known = Value::Array(vec![Value::Uint(1), id.clone()]);
    let unknown_kind = Value::Array(vec![Value::Uint(99), id.clone()]);
    let unknown_bit = Value::Array(vec![Value::Uint(4), id.clone(), Value::Uint(8)]);
    let zero_bits = Value::Array(vec![Value::Uint(4), id.clone(), Value::Uint(0)]);
    let wide_bits = Value::Array(vec![Value::Uint(4), id.clone(), Value::Uint(256)]);
    for (name, scope, expected) in [
        (
            "a kind this build does not know",
            unknown_kind,
            WireError::UnknownScope,
        ),
        (
            "a permission bit that is not defined",
            unknown_bit,
            WireError::UnknownScope,
        ),
        ("no permission at all", zero_bits, WireError::UnknownScope),
        (
            "permission bits that do not fit a byte",
            wide_bits,
            WireError::UnknownScope,
        ),
    ] {
        let payload = payload_map(&claims)
            .put(6, Value::Array(vec![known.clone(), scope]))
            .build()
            .expect("a payload");
        let bytes = craft_credential(&f.cred, &f.cert, &payload);
        assert_eq!(Credential::decode(&bytes).err(), Some(expected), "{name}");
        assert_eq!(
            verify_credential_bytes(&bytes, &f.anchors, now(ISSUED), &f.rules).err(),
            Some(CredError::UnknownScope),
            "{name}"
        );
    }
    // A scope of a known kind with the wrong arity is malformed, not unknown.
    let wrong_arity = Value::Array(vec![Value::Uint(1)]);
    let payload = payload_map(&claims)
        .put(6, Value::Array(vec![wrong_arity]))
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    assert_eq!(Credential::decode(&bytes).err(), Some(WireError::Malformed));
    // The control: the same payload with only the known scope verifies.
    let payload = payload_map(&claims)
        .put(6, Value::Array(vec![known]))
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    assert!(verify_credential_bytes(&bytes, &f.anchors, now(ISSUED), &f.rules).is_ok());
}

#[test]
fn s9e_06_a_scope_list_that_is_not_canonical_is_refused_on_the_wire() {
    let f = fixture();
    let mut claims = claims_of(grant());
    claims.device = None;
    claims.family = None;
    let link = |n: u8| Value::Array(vec![Value::Uint(1), Value::Bytes(vec![n; 16])]);
    let app = |n: u8, bits: u64| {
        Value::Array(vec![
            Value::Uint(4),
            Value::Bytes(vec![n; 16]),
            Value::Uint(bits),
        ])
    };
    for (name, list) in [
        ("unsorted", vec![link(2), link(1)]),
        ("duplicate", vec![link(1), link(1)]),
        ("one space twice", vec![app(1, 1), app(1, 2)]),
    ] {
        let payload = payload_map(&claims)
            .put(6, Value::Array(list))
            .build()
            .expect("a payload");
        let bytes = craft_credential(&f.cred, &f.cert, &payload);
        assert_eq!(
            Credential::decode(&bytes).err(),
            Some(WireError::NotCanonical),
            "{name}"
        );
    }
    let payload = payload_map(&claims)
        .put(6, Value::Array(vec![link(1), link(2), app(1, 3)]))
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    assert!(
        Credential::decode(&bytes).is_ok(),
        "the canonical list decodes"
    );
}

#[test]
fn s9e_06_a_device_credential_has_no_scopes_and_the_verifier_leaves_it_so() {
    let (f, credential) = issue_fixture();
    let verified =
        verify_credential(&credential, &f.anchors, now(ISSUED), &f.rules).expect("verifies");
    assert!(
        verified.scopes().is_none(),
        "None stays None: the registry and the grants decide"
    );
}

#[test]
fn s9e_20_an_app_principals_credential_always_narrows() {
    let f = fixture();
    let scopes = Scopes::new(vec![Scope::App(AppScope {
        space: space(0),
        perms: AppPerms::READ,
    })])
    .expect("a scope");
    // device None, family None, scopes Some: fine.
    let mut app = claims_of(grant());
    app.device = None;
    app.family = None;
    app.scopes = Some(scopes.clone());
    let credential = signed_credential(&f.cred, f.cert, app.clone());
    let verified = verify_credential(&credential, &f.anchors, now(ISSUED), &f.rules)
        .expect("an app principal's credential verifies");
    assert_eq!(verified.scopes(), Some(&scopes));
    // device None and no scopes: malformed (it would be unrestricted by anything).
    let mut open = app.clone();
    open.scopes = None;
    let credential = signed_credential(&f.cred, f.cert, open.clone());
    assert_eq!(
        verify_credential(&credential, &f.anchors, now(ISSUED), &f.rules).err(),
        Some(CredError::Malformed)
    );
    // A device credential without its family: malformed.
    let mut device = claims_of(grant());
    device.family = None;
    let credential = signed_credential(&f.cred, f.cert, device);
    assert_eq!(
        verify_credential(&credential, &f.anchors, now(ISSUED), &f.rules).err(),
        Some(CredError::Malformed)
    );
    // Present and empty scopes allow nothing and are no credential either.
    let mut empty = app.clone();
    empty.scopes = Some(Scopes::empty());
    let credential = signed_credential(&f.cred, f.cert, empty);
    assert_eq!(
        verify_credential(&credential, &f.anchors, now(ISSUED), &f.rules).err(),
        Some(CredError::Malformed)
    );
    // The issuer refuses all three before it signs.
    let issuer = CredentialIssuer::new(&f.cred, f.cert, &f.anchors, at(ISSUED)).expect("issuer");
    let to_grant = |c: azlin_creds::CredentialClaims| azlin_creds::GrantClaims {
        account: c.account,
        member: c.member,
        device: c.device,
        family: c.family,
        policy_epoch: c.policy_epoch,
        scopes: c.scopes,
        issued_at: c.issued_at,
        expires_at: c.expires_at,
        nonce: c.nonce,
        sso: c.sso,
    };
    assert_eq!(
        issuer.issue(to_grant(open)).err(),
        Some(IssueError::Claims(CredError::Malformed))
    );
    assert!(issuer.issue(to_grant(app)).is_ok());
}
