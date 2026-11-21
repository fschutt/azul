//! The two attacks of the restore-epoch defect X-DELTA-W1S17A-3, told as LIB tests (the killers of
//! the replayable patches `mutants/X-DELTA-W1S17A-3.patch` and `mutants/X-DELTA-W1S17A-3-late.patch`;
//! the replay tool `scripts/mutants.py` runs lib tests only, and the same stories live as integration
//! tests in `tests/restore.rs`, `tests/pin.rs` and `tests/link.rs`).
//!
//! The stories use the crate's own public types and a small fake verifier (the one of `tests/common`,
//! cut down to the two verifiers these tests need): the heads and the restore records are signed over
//! `FakeScheme`, the restore epoch is part of what a head signature covers.
//!
//! Implements planning/architecture.md section 5.13 (a head never goes backwards; the only way back is
//! a restore), section 6.10 V3 (a restore epoch the metadata group signs) and section 2.17 (what a
//! restore re-anchors).

use azlin_seams::{
    hash32, signing_input, AccountId, ChainSeq, Domain, FakeScheme, KeyId, RestoreEpoch, SigAlg,
    SignatureVerifier, SpaceId, Stamp, UnixTime, VerifyError,
};

use crate::{
    reanchor, Alarm, ChainId, Head, HeadPin, HeadRefusal, HeadVerifier, Kept, PinVerdict,
    RefusalClass, RestoreRecord, RestoreSig, RestoreSigVerifier, RestoreSigner, SignedHead,
    StrictTrust,
};

const HEAD_DOMAIN: Domain = Domain::new("azlin-chain-test-head-v1");
const RESTORE_DOMAIN: Domain = Domain::new("azlin-chain-test-restore-v1");

fn at(secs: u64) -> Stamp {
    UnixTime::from_secs(secs)
}

fn seq(n: u64) -> ChainSeq {
    ChainSeq::new(n)
}

fn epoch(n: u64) -> RestoreEpoch {
    RestoreEpoch::new(n)
}

fn team_log() -> ChainId {
    ChainId::team_log(AccountId::from_bytes([1; 16]))
}

fn space(fill: u8) -> ChainId {
    ChainId::space(SpaceId::from_bytes([fill; 16]))
}

/// The group key of the tests.
fn group_key() -> KeyId {
    KeyId::from_bytes([0x61; 32])
}

/// The map key of the tests.
fn map_key() -> KeyId {
    KeyId::from_bytes([0x4D; 32])
}

/// A head of `chain` at `n` whose hash is derived from `(n, variant)`: two variants of one position
/// are two different entries there.
fn head_at(chain: ChainId, n: u64, variant: u8) -> Head {
    Head {
        chain,
        seq: seq(n),
        hash: hash32(
            "azlin-chain-test-head-hash",
            &[&n.to_be_bytes(), &[variant], &chain.id],
        ),
    }
}

/// The bytes a test head signature covers: kind, id, position, hash, restore epoch, signing stamp.
fn head_bytes(head: &Head, epoch: RestoreEpoch, signed_at: Stamp) -> Vec<u8> {
    let mut out = vec![head.chain.kind as u8];
    out.extend_from_slice(&head.chain.id);
    out.extend_from_slice(&head.seq.get().to_be_bytes());
    out.extend_from_slice(head.hash.as_bytes());
    out.extend_from_slice(&epoch.get().to_be_bytes());
    out.extend_from_slice(&signed_at.as_secs().to_be_bytes());
    out
}

fn head_preimage(head: &Head, epoch: RestoreEpoch, signed_at: Stamp) -> Vec<u8> {
    signing_input(&HEAD_DOMAIN, &head_bytes(head, epoch, signed_at))
}

/// A head of `chain` at `n` (variant `variant`) signed by the group key in the restore epoch
/// `in_epoch` at `signed_at` seconds.
fn group_head(chain: ChainId, n: u64, variant: u8, in_epoch: u64, signed_at: u64) -> SignedHead {
    let head = head_at(chain, n, variant);
    let restore_epoch = epoch(in_epoch);
    let signed_at = at(signed_at);
    let signer = group_key();
    let sig = FakeScheme::sign(&signer, &head_preimage(&head, restore_epoch, signed_at));
    SignedHead {
        head,
        epoch: restore_epoch,
        signed_at,
        signer,
        sig,
    }
}

/// A head verifier over `FakeScheme` that trusts the group key.
struct GroupHeads;

impl HeadVerifier for GroupHeads {
    fn verify_head(&self, h: &SignedHead) -> Result<(), VerifyError> {
        if h.signer != group_key() {
            return Err(VerifyError::BadKey);
        }
        FakeScheme.verify(
            SigAlg::Ed25519,
            &FakeScheme::public_key(&h.signer),
            &head_preimage(&h.head, h.epoch, h.signed_at),
            &h.sig,
        )
    }
}

/// The bytes a test restore signature covers: epoch, stamp, count and the positions.
fn restore_preimage(rec: &RestoreRecord) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&rec.epoch.get().to_be_bytes());
    out.extend_from_slice(&rec.at.as_secs().to_be_bytes());
    out.extend_from_slice(&(rec.positions.len() as u32).to_be_bytes());
    for head in &rec.positions {
        out.extend_from_slice(&head_bytes(head, RestoreEpoch::ZERO, at(0)));
    }
    signing_input(&RESTORE_DOMAIN, &out)
}

/// A restore record of `positions` at the restore epoch `number`, stamped `at_secs`, signed by the
/// map key.
fn map_signed(number: u64, at_secs: u64, positions: Vec<Head>) -> RestoreRecord {
    let mut rec = RestoreRecord {
        epoch: epoch(number),
        at: at(at_secs),
        positions,
        signatures: Vec::new(),
    };
    let sig = FakeScheme::sign(&map_key(), &restore_preimage(&rec));
    rec.signatures = vec![RestoreSig {
        by: RestoreSigner::MapKey,
        key: map_key(),
        alg: SigAlg::Ed25519,
        sig,
    }];
    rec
}

/// The signature verifier of restore records over `FakeScheme`: only the map key signs a record.
struct MapKeyRestores;

impl RestoreSigVerifier for MapKeyRestores {
    fn verify_signature(&self, rec: &RestoreRecord, sig: &RestoreSig) -> Result<(), VerifyError> {
        if sig.by != RestoreSigner::MapKey || sig.key != map_key() {
            return Err(VerifyError::BadKey);
        }
        FakeScheme.verify(
            sig.alg,
            &FakeScheme::public_key(&sig.key),
            &restore_preimage(rec),
            &sig.sig,
        )
    }
}

/// The attack on the way forward. The device pinned the team log at 10 (a head of the epoch 0); the
/// restore of the epoch 1 moved it back to the survivor at 6 and raised the floor. The server then
/// replays the heads of the lost timeline: validly signed by the group BEFORE the restore (epoch 0),
/// some of them above the survivor (the old timeline's entries 7 to 10 chain correctly from it).
/// Without the floor in `HeadPin::offer` the one at 10 advances the pin again; with it, every one of
/// them is refused as an alarm, whatever its position, and the pin does not move. The restored
/// timeline (epoch 1) goes on from the survivor.
#[test]
fn a_head_signed_before_the_restore_cannot_advance_the_pin_lib() {
    let heads = GroupHeads;
    let restores = MapKeyRestores;
    let strict = StrictTrust::new(&restores);
    let chain = team_log();
    let mut pin = HeadPin::new();

    // Before the restore: the head the server kept, epoch 0, signed at 100.
    let lost = group_head(chain, 10, 0, 0, 100);
    assert_eq!(lost.epoch, epoch(0));
    assert_eq!(pin.offer(lost.clone(), &heads), PinVerdict::Advanced);
    assert_eq!(pin.seq(&chain), Some(seq(10)));

    // The restore of the epoch 1, announced by the map key: the pin goes back to the survivor at 6.
    let record = map_signed(1, 5_000, vec![head_at(chain, 6, 0)]);
    reanchor(&mut pin, &record, &strict).expect("the map key signed the record");
    assert_eq!(pin.seq(&chain), Some(seq(6)));
    assert_eq!(pin.restore_epoch(), epoch(1));

    // The replay: position 10 is above the survivor, the signature is the group's and good.
    let refusal = HeadRefusal::BelowRestoreEpoch {
        head: epoch(0),
        floor: epoch(1),
    };
    let verdict = pin.offer(lost, &heads);
    assert_eq!(verdict, PinVerdict::Refused(refusal));
    assert_eq!(refusal.class(), RefusalClass::Alarm);
    assert_eq!(
        verdict.alarm(&chain),
        Some(Alarm::BelowRestoreEpoch {
            chain,
            got: epoch(0),
            floor: epoch(1),
        })
    );
    assert_eq!(pin.seq(&chain), Some(seq(6)), "the pin did not move");
    assert_eq!(pin.head(&chain), Some(head_at(chain, 6, 0)));

    // No head of the lost timeline, at any position and in any variant, advances or even pins.
    for position in [0u64, 3, 6, 7, 10, 11, 500] {
        for variant in [0u8, 1] {
            assert_eq!(
                pin.offer(
                    group_head(chain, position, variant, 0, position * 10),
                    &heads
                ),
                PinVerdict::Refused(refusal),
                "position {position}, variant {variant}"
            );
        }
    }
    assert_eq!(pin.seq(&chain), Some(seq(6)));

    // The restored timeline goes on from the survivor: a head of the epoch 1 advances the pin.
    assert_eq!(
        pin.offer(group_head(chain, 7, 0, 1, 7_000), &heads),
        PinVerdict::Advanced
    );
    assert_eq!(pin.seq(&chain), Some(seq(7)));
}

/// The attack on the way back. The device pinned the space at 20 (signed at 200) and, from a peer, a
/// head of the RESTORED timeline of the team log (position 12, epoch 1, signed at 600); it missed the
/// record of the epoch 1 (stamped 500). The record arrives late. Without the check in
/// `VerifiedRestore::apply` it would set the team log back to the survivor at 6, behind the progress
/// the device already made after the restore; with it the team log is KEPT (`Reanchored::kept`), the
/// space (signed before the record) is re-anchored, and the floor is raised in any case.
#[test]
fn a_late_record_cannot_reanchor_behind_a_newer_head_lib() {
    let heads = GroupHeads;
    let restores = MapKeyRestores;
    let strict = StrictTrust::new(&restores);
    let chain = team_log();
    let other = space(2);
    let mut pin = HeadPin::new();
    assert_eq!(
        pin.offer(group_head(other, 20, 0, 0, 200), &heads),
        PinVerdict::Advanced
    );
    assert_eq!(
        pin.offer(group_head(chain, 12, 0, 1, 600), &heads),
        PinVerdict::Advanced,
        "a head of a restore the device has not been announced yet is an ordinary head"
    );
    assert_eq!(
        pin.restore_epoch(),
        epoch(0),
        "the floor moves only with a record"
    );

    // The late record, stamped 500, lists the survivors of both chains.
    let record = map_signed(1, 500, vec![head_at(chain, 6, 0), head_at(other, 18, 0)]);
    let (verified, done) =
        reanchor(&mut pin, &record, &strict).expect("a record signed by the map key");

    // The team log is KEPT where it is: its pinned head was signed at 600, after the record at 500.
    assert_eq!(
        done.kept,
        vec![Kept {
            chain,
            pinned: head_at(chain, 12, 0),
            listed: head_at(chain, 6, 0),
        }]
    );
    assert_eq!(pin.head(&chain), Some(head_at(chain, 12, 0)));
    assert!(
        pin.signed_head(&chain).is_some(),
        "its signed head is still held"
    );

    // The space was signed at 200, before the record: re-anchored to its survivor.
    assert_eq!(done.moved.len(), 1);
    assert_eq!(pin.head(&other), Some(head_at(other, 18, 0)));

    // The floor is the record's epoch in any case, and the witness still lists both chains.
    assert_eq!(pin.restore_epoch(), epoch(1));
    assert_eq!(done.epoch, epoch(1));
    assert!(verified.covers(&chain) && verified.covers(&other));
}
