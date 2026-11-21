//! Check 5 of the brief (capability grants, M6): a capability is stored only as the hash of its secret,
//! the presented secret gets the role and a wrong secret nothing, the comparison is constant time in the
//! hash (`subtle`, the whole list, no early exit), a revoked capability stops at once, and the secret never
//! appears in an effect, an error or a debug output.
//!
//! Implements planning/architecture.md section 5.15 M6 ("a capability grant is a bearer secret ... The
//! server then sees a capability being used, not account B. It is revoked like a link") and section 5.4
//! (`reg_grant`: "for a capability, the hash of its secret").

mod common;

use std::fs;

use azlin_seams::{AccountKind, Hash32, Secret};
use azlin_spaces::{
    access, capability_hash, charge, Charge, ChargeRefusal, Grantee, HistoryMode, Payer, Principal,
    SpaceKind, SpaceRefusal, SpaceRole, CAPABILITY_HASH_DOMAIN,
};
use common::*;

fn team() -> World {
    World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full)
}

fn holder(n: u8) -> Principal {
    Principal::Capability(secret(n))
}

fn flags(caps: azlin_spaces::Caps) -> (bool, bool, bool) {
    (caps.can_read(), caps.can_write(), caps.can_manage())
}

#[test]
fn capability_is_stored_only_as_a_hash() {
    let s = secret(1);
    let hash = capability_hash(&s);
    // The hash is the BLAKE3 `derive_key` of the secret under the crate's own context, re-derived here
    // independently, and it is neither the secret nor the plain BLAKE3 of it.
    assert_eq!(
        *hash.as_bytes(),
        blake3::derive_key(CAPABILITY_HASH_DOMAIN, s.expose())
    );
    assert_ne!(hash.as_bytes(), s.expose());
    assert_ne!(hash.as_bytes(), blake3::hash(s.expose()).as_bytes());
    assert_ne!(capability_hash(&secret(2)), hash);

    // The chain and the state hold the hash and nothing else: the only grantee that is a capability is
    // the hash, and no other field of any row holds the secret's bytes.
    let mut w = team();
    w.do_by(1, grant_op(cap(1), SpaceRole::Viewer));
    let row = w
        .state
        .grant(&Grantee::Capability(hash))
        .expect("the capability grant is stored under its hash");
    assert!(row.is_active());
    for grant in w.state.grants() {
        if let Grantee::Capability(stored) = grant.grantee {
            assert_eq!(stored, hash);
        }
    }
}

#[test]
fn presented_secret_gets_the_role_and_a_wrong_secret_nothing() {
    let mut w = team();
    w.do_by(1, grant_op(cap(1), SpaceRole::Editor));
    w.do_by(1, grant_op(cap(2), SpaceRole::Viewer));
    w.do_by(1, grant_op(cap(3), SpaceRole::Uploader));
    assert_eq!(
        flags(access(&w.state, &holder(1), at(5_000))),
        (true, true, false)
    );
    assert_eq!(
        flags(access(&w.state, &holder(2), at(5_000))),
        (true, false, false)
    );
    assert_eq!(
        flags(access(&w.state, &holder(3), at(5_000))),
        (false, true, false)
    );

    // A secret that no grant names gets nothing.
    assert!(access(&w.state, &holder(4), at(5_000)).is_empty());
    // So does the right secret with one bit changed.
    let mut flipped = *secret(1).expose();
    flipped[17] ^= 0x01;
    let wrong = Principal::Capability(Secret::from_bytes(flipped));
    assert!(access(&w.state, &wrong, at(5_000)).is_empty());
    // And the hash of a secret is not a secret: presenting the stored hash opens nothing.
    let hash_as_secret =
        Principal::Capability(Secret::from_bytes(*capability_hash(&secret(1)).as_bytes()));
    assert!(access(&w.state, &hash_as_secret, at(5_000)).is_empty());
    // A space with no capability grants opens nothing to any secret.
    let plain = team();
    assert!(access(&plain.state, &holder(1), at(5_000)).is_empty());

    // The bearer writes into the space and the owner pays for it (a capability has no account of its own).
    assert_eq!(
        charge(&w.state, &holder(1), bytes(10), bytes(0)),
        Charge::Owner
    );
    assert_eq!(
        charge(&w.state, &holder(2), bytes(10), bytes(0)),
        Charge::Refused(ChargeRefusal::NoWriteGrant)
    );
    assert_eq!(
        charge(&w.state, &holder(4), bytes(10), bytes(0)),
        Charge::Refused(ChargeRefusal::NoWriteGrant)
    );
}

/// The text of the function `name` of a source file: from its `fn` line to the first line that is a lone
/// closing brace.
fn function_text(source: &str, name: &str) -> String {
    let start = source
        .find(&format!("fn {name}"))
        .unwrap_or_else(|| panic!("the source has no `fn {name}`"));
    let rest = &source[start..];
    let end = rest.find("\n}\n").expect("the function ends");
    rest[..end].to_string()
}

#[test]
fn capability_comparison_is_constant_time_in_the_hash() {
    // The comparison is `subtle`'s, over the whole list, with no early exit and no `==` on hashes.
    let source = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/access.rs"))
        .expect("src/access.rs");
    assert!(source.contains("use subtle::ConstantTimeEq;"));
    let scan = function_text(&source, "capability_grants");
    assert!(scan.contains(".ct_eq("), "the comparison is subtle's ct_eq");
    for forbidden in [
        "==",
        "!=",
        "break",
        "return",
        ".find(",
        ".any(",
        ".contains(",
    ] {
        assert!(
            !scan.contains(forbidden),
            "the capability scan must not use `{forbidden}`: it compares every stored hash, always"
        );
    }

    // And whatever the position of the matching hash among many, the right secrets open exactly their
    // own grants and the wrong ones open nothing.
    let mut w = team();
    for i in 0..200usize {
        // Hashes spread over the whole range of the first byte, so the real ones sort anywhere among them.
        let mut hash = [0u8; 32];
        hash[0] = (i * 13 % 256) as u8;
        hash[1..9].copy_from_slice(&(i as u64).to_be_bytes());
        hash[9] = 0x77;
        w.do_by(
            1,
            grant_op(
                Grantee::Capability(Hash32::from_bytes(hash)),
                SpaceRole::Viewer,
            ),
        );
    }
    for (n, role) in [
        (1u8, SpaceRole::Editor),
        (2, SpaceRole::Uploader),
        (3, SpaceRole::Viewer),
    ] {
        w.do_by(1, grant_op(cap(n), role));
    }
    assert_eq!(
        flags(access(&w.state, &holder(1), at(5_000))),
        (true, true, false)
    );
    assert_eq!(
        flags(access(&w.state, &holder(2), at(5_000))),
        (false, true, false)
    );
    assert_eq!(
        flags(access(&w.state, &holder(3), at(5_000))),
        (true, false, false)
    );
    for n in 10..60u8 {
        assert!(
            access(&w.state, &holder(n), at(5_000)).is_empty(),
            "secret {n}"
        );
    }
}

#[test]
fn revoking_a_capability_stops_it_at_once() {
    let mut w = team();
    w.do_by(1, grant_op(cap(1), SpaceRole::Editor));
    assert_eq!(
        flags(access(&w.state, &holder(1), at(5_000))),
        (true, true, false)
    );
    assert_eq!(
        charge(&w.state, &holder(1), bytes(1), bytes(0)),
        Charge::Owner
    );

    let effects = w.do_by(1, revoke_op(cap(1)));
    // At once: the very state the entry was applied to refuses the bearer.
    assert!(access(&w.state, &holder(1), at(2_000)).is_empty());
    assert_eq!(
        charge(&w.state, &holder(1), bytes(1), bytes(0)),
        Charge::Refused(ChargeRefusal::NoWriteGrant)
    );
    // The hash is still a row (the registry keeps `removed_at`), ended.
    let row = w.state.grant(&cap(1)).expect("the row stays");
    assert!(!row.is_active() && row.removed_at.is_some());
    // The wrap of the capability stops being served and a rekey is due: the bearer knows the key.
    assert_eq!(effects.stop_serving.len(), 1);
    assert_eq!(effects.stop_serving[0].to, cap(1));
    assert_eq!(
        w.state.rekey_due(),
        Some(azlin_spaces::RekeyReason::Removal)
    );
    // A capability that is revoked cannot be revoked again.
    assert_eq!(
        w.refused_by(1, revoke_op(cap(1))),
        SpaceRefusal::NoSuchGrant
    );
    // A new grant of the same hash is a new grant: the same secret opens again.
    w.do_by(1, grant_op(cap(1), SpaceRole::Viewer));
    assert_eq!(
        flags(access(&w.state, &holder(1), at(5_000))),
        (true, false, false)
    );
}

#[test]
fn a_capability_cannot_be_a_manager_and_is_paid_by_the_owner() {
    let mut w = team();
    assert_eq!(
        w.refused_by(1, grant_op(cap(1), SpaceRole::Manager)),
        SpaceRefusal::CapabilityCannotManage
    );
    for role in [SpaceRole::Editor, SpaceRole::Uploader] {
        assert_eq!(
            w.refused_by(1, grant_full(cap(1), role, Some(100), Payer::Grantee)),
            SpaceRefusal::CapabilityPaidByOwner,
            "{role:?}"
        );
    }
    // The owner pays, up to the cap the owner set for the bearer.
    w.do_by(
        1,
        grant_full(cap(1), SpaceRole::Editor, Some(100), Payer::Owner),
    );
    assert_eq!(
        charge(&w.state, &holder(1), bytes(100), bytes(0)),
        Charge::Owner
    );
    assert_eq!(
        charge(&w.state, &holder(1), bytes(101), bytes(0)),
        Charge::Refused(ChargeRefusal::CapExceeded)
    );
}

#[test]
fn capability_never_appears_in_effects_or_debug_output() {
    let s = secret(7);
    let hex: String = s
        .expose()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let list = format!("{:?}", s.expose().to_vec());
    let decimal = list.trim_matches(|c| c == '[' || c == ']').to_string();
    assert_eq!(hex.len(), 64);
    assert!(decimal.contains(", "));

    let mut w = team();
    let entry = w.build(device(1), grant_op(cap(7), SpaceRole::Editor), &[]);
    let admitted = w
        .admit_entry(&entry)
        .expect("a manager grants a capability");
    let mut outputs = vec![
        format!("{entry:?}"),
        format!("{admitted:?}"),
        format!("{:?}", w.state),
    ];
    let effects = w.do_by(1, grant_op(cap(7), SpaceRole::Editor));
    outputs.push(format!("{effects:?}"));
    outputs.push(format!("{:?}", w.state));
    outputs.push(format!("{:?}", w.state.grant(&cap(7))));
    outputs.push(format!("{:?}", w.state.chain()));
    // The refusals that mention a capability.
    let refusal = w.refused_by(1, grant_op(cap(7), SpaceRole::Manager));
    outputs.push(format!("{refusal:?} {refusal}"));
    // The principal that holds the secret, what it is allowed and what it is charged.
    let principal = Principal::Capability(secret(7));
    outputs.push(format!("{principal:?}"));
    outputs.push(format!("{:?}", access(&w.state, &principal, at(5_000))));
    outputs.push(format!(
        "{:?}",
        charge(&w.state, &principal, bytes(5), bytes(0))
    ));
    // The revocation and the effects of it.
    let revoked = w.do_by(1, revoke_op(cap(7)));
    outputs.push(format!("{revoked:?}"));
    outputs.push(format!("{:?}", w.state));

    for (i, output) in outputs.iter().enumerate() {
        assert!(!output.contains(&hex), "output {i} holds the secret as hex");
        assert!(
            !output.contains(&decimal),
            "output {i} holds the secret as a list of numbers"
        );
    }
    // The scan can see: the HASH is in the state and the effects, and the secret's own debug is redacted.
    let hash = capability_hash(&s).to_string();
    assert!(outputs[2].contains(&hash) || outputs[3].contains(&hash));
    assert!(outputs[4].contains(&hash));
    assert!(format!("{principal:?}").contains("Secret<32>(***)"));
}
