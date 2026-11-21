//! `verify_signatures`: checks every signature of an entry, binds the typed fields to the signed bytes and
//! makes the `Proposal` that `admit` takes (planning/architecture.md section 5.9 "every entry is signed by
//! a device whose role allows it, with a quorum where policy asks"; "an entry that fails its hash or
//! signature check is fetched again from another voter"; build unit S17, lanes W1-S17c2 and W1-S17c3).
//!
//! What this function decides and what it does not. It decides WHETHER THE BYTES HOLD: the entry
//! belongs to this account, its copy is intact (its hash is the hash of its preimage), the size of
//! every part is within its limit, THE TYPED FIELDS ARE THE FIELDS THE SIGNATURES COVER (the preimage is
//! decoded by the `EntryDecoder` seam and every field `admit` reads is compared with it: X-DELTA-W1S47A-1),
//! the signer's signature verifies under the signer's key over the preimage, and which cosignatures
//! verify. It does NOT decide whether anybody may do what the entry says: that is `admit`'s, against the
//! state. The one key that does not come from the state is the payments key (`TrustedKeys`); the keys of a
//! `genesis` and of a `member_join` are the new device's own (the entry is signed with the key it
//! registers).
//!
//! A cosignature that does not verify, or that is made by a device the state does not know, is not an
//! error: it is left out, and so it counts for nothing toward a quorum (`cosig_over_another_hash_not_
//! counted`). The signer's own signature failing is `BadSignature`, and so is an entry that is not what
//! was signed.

use azlin_chain::ChainRefusal;
use azlin_seams::{mutant, ByteCount, DeviceId, SigAlg, SignatureVerifier};

use crate::admit::{Invalid, Refusal};
use crate::decode::{canonical_of, EntryDecoder};
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

/// The typed fields of the entry are the fields of the bytes its signatures cover: the preimage must begin
/// with the team-log signature domain, the decoder must decode what follows it, and every field that
/// `admit` reads (the account, the position, the claimed time, the restore epoch, the signer, the op) must
/// equal what the bytes say. An entry that fails any of it is not what was signed: `BadSignature`, the
/// class of a copy whose signatures do not hold for it (fetch it again from another voter).
///
/// Implements planning/architecture.md section 5.9 ("every entry is signed by a device whose role
/// allows it"; "an entry that fails its hash or signature check is fetched again"), section 2.5 (the
/// domain of the signed structure) and section 12 E1 (X-DELTA-W1S47A-1: an old signed entry must not
/// make a new one by changing its typed position fields).
fn bind(e: &RawEntry, decoder: &dyn EntryDecoder) -> Result<(), Refusal> {
    let canonical = canonical_of(&e.preimage).ok_or(Refusal::BadSignature)?;
    let Ok(signed) = decoder.decode(canonical) else {
        return Err(Refusal::BadSignature);
    };
    let bound = signed.account == e.account
        && signed.seq == e.seq
        && signed.prev == e.prev
        && signed.at == e.at
        && signed.epoch == e.epoch
        && signed.signer == e.signer
        && signed.op == e.op;
    if bound {
        Ok(())
    } else {
        Err(Refusal::BadSignature)
    }
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
/// production, `FakeScheme` in tests); `trusted` holds the payments keys; `decoder` decodes the signed
/// bytes (the glue's strict decoder in production, `FakeDecoder` in tests).
///
/// The order of the checks: the account, the size of every part, the integrity of the copy (a copy that
/// is not intact is `Chain(Corrupt)`: fetch it again), the binding of the typed fields to the signed
/// bytes (`BadSignature`), then the signer's signature (`BadSignature`), then each cosignature (the ones
/// that hold are kept, the others dropped).
///
/// SIGNATURE CHANGE of lane W1-S17c3 (X-DELTA-W1S47A-1): the `decoder` parameter is new; a `Proposal`
/// now means that the signatures hold AND that the typed fields are the signed fields.
///
/// Implements planning/architecture.md section 5.9 (the signature fields `sig` and `cosigs`; "the
/// metadata group rejects an entry with a reason for ... a bad signature") and section 12 E1 (every
/// signature is checked over the entry, and the entry is what was signed).
///
/// Caller: the registry glue (inside `MetaDb::apply`, with the strict decoder of S5's contract types),
/// `replay::replay`, the tests; lane S47a calls it as the first step of the production check.
pub fn verify_signatures(
    state: &TeamState,
    e: &RawEntry,
    v: &dyn SignatureVerifier,
    trusted: &TrustedKeys,
    decoder: &dyn EntryDecoder,
) -> Result<Proposal, Refusal> {
    if e.account != state.account() {
        return Err(Refusal::Chain(ChainRefusal::WrongChain));
    }
    check_shape(e)?;
    if entry_hash(&e.preimage) != e.hash {
        return Err(Refusal::Chain(ChainRefusal::Corrupt));
    }
    bind(e, decoder)?;
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
