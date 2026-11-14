//! `verify_signatures`: checks every signature of an entry and makes the `Proposal` that `admit` takes
//! (planning/architecture.md section 5.9 "every entry is signed by a device whose role allows it, with
//! a quorum where policy asks"; "an entry that fails its hash or signature check is fetched again from
//! another voter"; build unit S17, lane W1-S17c2).
//!
//! What this function decides and what it does not. It decides WHETHER THE BYTES HOLD: the entry
//! belongs to this account, its copy is intact (its hash is the hash of its preimage), the size of
//! every part is within its limit, the signer's signature verifies under the signer's key over the
//! preimage, and which cosignatures verify. It does NOT decide whether anybody may do what the entry
//! says: that is `admit`'s, against the state. The one key that does not come from the state is the
//! payments key (`TrustedKeys`); the keys of a `genesis` and of a `member_join` are the new device's own
//! (the entry is signed with the key it registers).
//!
//! A cosignature that does not verify, or that is made by a device the state does not know, is not an
//! error: it is left out, and so it counts for nothing toward a quorum (`cosig_over_another_hash_not_
//! counted`). The signer's own signature failing is `BadSignature`.

use azlin_chain::ChainRefusal;
use azlin_seams::{mutant, ByteCount, DeviceId, SigAlg, SignatureVerifier};

use crate::admit::{Invalid, Refusal};
use crate::hash::entry_hash;
use crate::ops::{Op, SEALED_MAX};
use crate::proposal::{
    Proposal, RawEntry, Signer, TrustedKeys, VerifiedSigner, ASSERTIONS_MAX, COSIGS_MAX,
    PREIMAGE_MAX, SIG_MAX_LEN,
};
use crate::state::TeamState;

/// The refusal of a part that is too big.
fn too_big(what: &'static str) -> Refusal {
    Refusal::Invalid(Invalid::Limit(what))
}

/// The size of every part of the entry against its limit.
fn check_shape(e: &RawEntry) -> Result<(), Refusal> {
    if ByteCount::of_len(e.preimage.len()) > PREIMAGE_MAX {
        return Err(too_big("the preimage of an entry"));
    }
    if e.sig.len() > SIG_MAX_LEN {
        return Err(too_big("the signature of an entry"));
    }
    if e.cosigs.len() > COSIGS_MAX || e.cosigs.iter().any(|(_, sig)| sig.len() > SIG_MAX_LEN) {
        return Err(too_big("the cosignatures of an entry"));
    }
    if e.assertions.len() > ASSERTIONS_MAX {
        return Err(too_big("the passkey assertions of an entry"));
    }
    if let Some(sealed) = &e.sealed {
        if ByteCount::of_len(sealed.len()) > SEALED_MAX {
            return Err(too_big("the sealed field of an entry"));
        }
    }
    Ok(())
}

/// The public key that must verify the signature of a device that signs an entry: the key the entry
/// registers for a `genesis` and a `member_join` (the new device signs its own entry), and the key of
/// the device row for every other op.
fn device_key(state: &TeamState, op: &Op, device: DeviceId) -> Result<[u8; 32], Refusal> {
    match op {
        Op::Genesis {
            device: new,
            device_keys,
            ..
        }
        | Op::MemberJoin {
            device: new,
            device_keys,
            ..
        } => {
            if *new == device {
                Ok(device_keys.sign)
            } else {
                Err(Refusal::SignerUnknown)
            }
        }
        _ => state
            .device_key(device)
            .copied()
            .ok_or(Refusal::SignerUnknown),
    }
}

/// Checks the signatures of an entry against the state it will be admitted to and makes the
/// `Proposal` that `admit` takes. `v` is the signature seam (`azlin_seams::Ed25519Verifier` in
/// production, `FakeScheme` in tests); `trusted` holds the payments keys.
///
/// The order of the checks: the account, the size of every part, the integrity of the copy (a copy that
/// is not intact is `Chain(Corrupt)`: fetch it again), then the signer's signature (`BadSignature`),
/// then each cosignature (the ones that hold are kept, the others dropped).
///
/// Implements planning/architecture.md section 5.9 (the signature fields `sig` and `cosigs`; "the
/// metadata group rejects an entry with a reason for ... a bad signature") and section 12 E1 (every
/// signature is checked over the entry).
///
/// Caller: the registry glue (inside `MetaDb::apply`), `replay::replay`, the tests; lane S47a calls it
/// as the first step of the production check.
pub fn verify_signatures(
    state: &TeamState,
    e: &RawEntry,
    v: &dyn SignatureVerifier,
    trusted: &TrustedKeys,
) -> Result<Proposal, Refusal> {
    if e.account != state.account() {
        return Err(Refusal::Chain(ChainRefusal::WrongChain));
    }
    check_shape(e)?;
    if entry_hash(&e.preimage) != e.hash {
        return Err(Refusal::Chain(ChainRefusal::Corrupt));
    }
    let signer = match e.signer {
        Signer::Device(device) => {
            let key = device_key(state, &e.op, device)?;
            v.verify(SigAlg::Ed25519, &key, &e.preimage, &e.sig)
                .map_err(|_| Refusal::BadSignature)?;
            VerifiedSigner::Device { device, key }
        }
        Signer::DelegateKey(key) => {
            v.verify(SigAlg::Ed25519, key.as_bytes(), &e.preimage, &e.sig)
                .map_err(|_| Refusal::BadSignature)?;
            VerifiedSigner::Delegate(key)
        }
        Signer::PaymentsKey => {
            let holds = trusted
                .payments()
                .iter()
                .any(|key| v.verify(SigAlg::Ed25519, key, &e.preimage, &e.sig).is_ok());
            if !holds {
                return Err(Refusal::BadSignature);
            }
            VerifiedSigner::Payments
        }
    };
    let mut cosigners: Vec<DeviceId> = Vec::new();
    for (device, sig) in &e.cosigs {
        if cosigners.contains(device) {
            continue;
        }
        let Some(key) = state.device_key(*device) else {
            continue;
        };
        let holds = mutant!("teamlog_cosig_hash_unchecked")
            || v.verify(SigAlg::Ed25519, key, &e.preimage, sig).is_ok();
        if holds {
            cosigners.push(*device);
        }
    }
    Ok(Proposal::new(e.clone(), signer, cosigners))
}
