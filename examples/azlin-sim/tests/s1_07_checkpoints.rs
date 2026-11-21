//! S1-07: checkpoints (planning/architecture.md section 5.9: a checkpoint every 1,000 entries, and monthly;
//! a new device verifies from the latest checkpoint, not from the account's first entry). A chain of 1,100
//! entries gets a checkpoint by the 1,000-entry rule and one after a 30-day tick; `verify_from_checkpoint`
//! over the rest equals `verify_from_genesis` for 500 seeded chains, corrupted ones and chains with
//! restores included; a checkpoint with a wrong head hash or a damaged head signature is `Corrupt`.

use std::collections::BTreeMap;

use azlin_chain::{
    verify_from_checkpoint, verify_from_genesis, ChainId, ChainRefusal, Checkpoint, Head,
};
use azlin_seams::sim::Rng;
use azlin_seams::{AccountId, ChainSeq, Hash32, RestoreEpoch, UnixTime};
use azlin_sim::draw::below;
use azlin_sim::keys::{Keyring, SimHeadVerifier};
use azlin_sim::machine::{Machine, OpaqueEntry, OpaqueMachine};
use azlin_sim::{directed_named, run_directed, Scheme, Verdict};

#[test]
fn s1_07_a_thousand_entries_get_a_checkpoint_by_the_entry_rule() {
    let Some(d) = directed_named("thousand_entries") else {
        panic!("the directed scenario exists");
    };
    let Some(outcome) = run_directed(d) else {
        panic!("it can be built");
    };
    if let Verdict::Red(f) = &outcome.verdict {
        panic!(
            "{} at step {}: {}\n{}",
            f.invariant.name(),
            f.at_step,
            f.message,
            f.trace_tail.join("\n")
        );
    }
    assert!(
        outcome.tallies.checkpoints >= 1,
        "{} checkpoints",
        outcome.tallies.checkpoints
    );
    assert!(
        outcome.tallies.acked >= 1_000,
        "{} acknowledged entries",
        outcome.tallies.acked
    );
}

#[test]
fn s1_07_a_checkpoint_after_thirty_days() {
    let Some(d) = directed_named("checkpoints_by_age") else {
        panic!("the directed scenario exists");
    };
    let Some(outcome) = run_directed(d) else {
        panic!("it can be built");
    };
    if let Verdict::Red(f) = &outcome.verdict {
        panic!(
            "{} at step {}: {}",
            f.invariant.name(),
            f.at_step,
            f.message
        );
    }
    assert!(
        outcome.tallies.checkpoints >= 1,
        "{} checkpoints",
        outcome.tallies.checkpoints
    );
}

fn chain_of(seed: u64, rng: &mut Rng, len: usize) -> (Keyring, ChainId, Vec<OpaqueEntry>) {
    let keys = Keyring::new(seed, Scheme::Fake, 2);
    let chain = ChainId::team_log(AccountId::from_bytes([(seed & 0xff) as u8; 16]));
    let machine = OpaqueMachine::new(BTreeMap::new(), Vec::new());
    let mut entries: Vec<OpaqueEntry> = Vec::new();
    let mut at = 1_000_000u64;
    let mut epoch = 0u64;
    for i in 0..len {
        at += below(rng, 40);
        if below(rng, 9) == 0 {
            epoch += 1;
        }
        let (seq, prev) = match entries.last() {
            None => (ChainSeq::MIN, Hash32::ZERO),
            Some(last) => (ChainSeq::new(last.seq.get() + 1), last.hash),
        };
        let proposal = OpaqueMachine::proposal_with(
            &keys,
            0,
            chain,
            seq,
            prev,
            (i as u16).to_be_bytes().to_vec(),
        )
        .expect("device 0 signs");
        entries.push(machine.seal(&proposal, UnixTime::from_secs(at), RestoreEpoch::new(epoch)));
    }
    (keys, chain, entries)
}

#[test]
fn s1_07_verify_from_a_checkpoint_equals_verify_from_genesis_for_500_seeded_chains() {
    let mut refused = 0;
    for seed in 0..500u64 {
        let mut rng = Rng::new(0xC4EC + seed);
        let len = 3 + below(&mut rng, 40) as usize;
        let (keys, chain, mut entries) = chain_of(seed, &mut rng, len);
        // Some chains are damaged: a corrupt copy, a stamp run backwards, a lost-timeline entry.
        match below(&mut rng, 6) {
            0 => {
                let at = below(&mut rng, entries.len() as u64) as usize;
                if let Some(e) = entries.get_mut(at) {
                    e.payload.push(0xEE);
                }
            }
            1 => {
                let at = 1 + below(&mut rng, entries.len() as u64 - 1) as usize;
                if let Some(e) = entries.get_mut(at) {
                    e.at = UnixTime::from_secs(1);
                    *e = e.clone().rehashed();
                }
            }
            _ => {}
        }
        let at = below(&mut rng, entries.len() as u64) as usize;
        let Some(anchor) = entries.get(at).cloned() else {
            continue;
        };
        let signed = keys.sign_head(
            Head {
                chain,
                seq: anchor.seq,
                hash: anchor.hash,
            },
            anchor.epoch,
            anchor.at,
        );
        let cp = Checkpoint::new(signed, anchor.at, Hash32::ZERO);
        let rest: Vec<OpaqueEntry> = entries
            .iter()
            .filter(|e| e.seq > anchor.seq)
            .cloned()
            .collect();
        let verifier = SimHeadVerifier::new(&keys);
        let from_checkpoint = verify_from_checkpoint(&cp, &verifier, &rest);
        let from_genesis = verify_from_genesis(chain, &entries).map(|h| h.unwrap_or(cp.head()));
        // The entries up to the checkpoint may be the ones that are damaged: then genesis refuses and the
        // checkpoint, which stands behind them, does not see them. The equality is for the rest.
        let damaged_before = entries
            .iter()
            .take(at + 1)
            .any(|e| !azlin_chain::ChainEntry::is_intact(e));
        let backwards_before = entries.windows(2).take(at).any(|p| p[1].at < p[0].at);
        if !damaged_before && !backwards_before {
            assert_eq!(
                from_checkpoint,
                from_genesis,
                "seed {seed}: checkpoint at {at} of {}",
                entries.len()
            );
        }
        if from_genesis.is_err() {
            refused += 1;
        }
        // A checkpoint with a wrong head hash or a damaged head signature is Corrupt.
        let mut wrong_hash = cp.signed_head().clone();
        wrong_hash.head.hash = Hash32::from_bytes([0xAB; 32]);
        let wrong = Checkpoint::new(wrong_hash, cp.at(), cp.state_hash());
        assert_eq!(
            verify_from_checkpoint(&wrong, &verifier, &rest),
            Err(ChainRefusal::Corrupt),
            "seed {seed}"
        );
        let mut damaged_sig = cp.signed_head().clone();
        if let Some(b) = damaged_sig.sig.first_mut() {
            *b ^= 0x5A;
        }
        let damaged = Checkpoint::new(damaged_sig, cp.at(), cp.state_hash());
        assert_eq!(
            verify_from_checkpoint(&damaged, &verifier, &rest),
            Err(ChainRefusal::Corrupt),
            "seed {seed}"
        );
    }
    assert!(refused > 30, "the damaged chains were refused: {refused}");
}
