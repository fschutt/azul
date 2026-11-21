//! T16-T20: the reference witness and the request it is asked with. The consistency proofs come from
//! `azlin-merkle` directly, the checkpoints from `sign_checkpoint`; the `Log` type is not involved.

use azlin_merkle::{empty_root, MemStore, Tree};
use azlin_tlog::{
    sign_checkpoint, AddRequest, Checkpoint, Hash, KeyName, LogKey, Note, NumberError, Origin,
    Policy, RequestError, SigKind, Span, TreeSize, Witness, WitnessError, WitnessRefusal,
    MAX_ORIGINS, MAX_PROOF_LINES, MAX_REQUEST_BYTES,
};
use base64::{engine::general_purpose::STANDARD, Engine};

use crate::common::{key, log_key, t, witness_key};

fn log_vkey() -> LogKey {
    LogKey {
        origin: Origin::azlin(),
        verifier: log_key().verifier(SigKind::Ed25519),
    }
}

fn new_witness() -> Witness {
    Witness::new(Box::new(witness_key(0)), vec![log_vkey()]).unwrap()
}

/// A tree of `n` leaves; the leaves from index 5 on differ with `variant` (two histories that agree on the
/// first five leaves and are forked after them).
fn tree_with(n: usize, variant: u8) -> Tree<MemStore> {
    let mut tree = Tree::new(MemStore::new()).unwrap();
    for i in 0..n {
        let data = if i < 5 || variant == 0 {
            format!("leaf {i}")
        } else {
            format!("forked {variant} leaf {i}")
        };
        tree.append(data.as_bytes()).unwrap();
    }
    tree
}

fn cp_note(tree: &Tree<MemStore>, size: u64) -> Note {
    let root = tree.root_at(TreeSize(size)).unwrap();
    let cp = Checkpoint::new(Origin::azlin(), TreeSize(size), root, vec![]).unwrap();
    sign_checkpoint(&cp, &log_key()).unwrap()
}

fn proof(tree: &Tree<MemStore>, old: u64, new: u64) -> Vec<Hash> {
    tree.consistency_proof(TreeSize(old), TreeSize(new))
        .unwrap()
        .path
}

/// The request an honest log makes: the proof from `old` to `new` and the checkpoint of size `new`.
fn request(tree: &Tree<MemStore>, old: u64, new: u64) -> AddRequest {
    AddRequest::new(TreeSize(old), proof(tree, old, new), cp_note(tree, new)).unwrap()
}

fn head(witness: &Witness) -> (TreeSize, Hash) {
    witness.head(&Origin::azlin()).unwrap()
}

#[test]
fn t16_a_witness_cosigns_a_consistent_checkpoint_and_remembers_it() {
    let tree = tree_with(9, 0);
    let mut witness = new_witness();
    assert_eq!(
        head(&witness),
        (TreeSize(0), empty_root()),
        "it holds nothing yet"
    );
    // size 1: old = 0 and no proof
    let sig = witness
        .add_checkpoint(&request(&tree, 0, 1), t(100))
        .unwrap();
    assert_eq!(
        head(&witness),
        (TreeSize(1), tree.root_at(TreeSize(1)).unwrap())
    );
    // the cosignature is a real one: a policy of this one witness accepts the note and reads the time
    let policy = Policy::new(
        log_vkey(),
        vec![witness_key(0).verifier(SigKind::Cosignature)],
        1,
        Span::from_secs(60),
    )
    .unwrap();
    let note = cp_note(&tree, 1).with_sig(sig).unwrap();
    assert_eq!(policy.check(&note, t(100)).unwrap().logged_since(), t(100));
    // size 5 from 1, with the proof azlin-merkle makes
    witness
        .add_checkpoint(&request(&tree, 1, 5), t(200))
        .unwrap();
    assert_eq!(
        head(&witness),
        (TreeSize(5), tree.root_at(TreeSize(5)).unwrap())
    );
    // a request that still says old = 1 meets a witness that holds 5: a conflict that carries the size
    assert_eq!(
        witness.add_checkpoint(&request(&tree, 1, 9), t(300)),
        Err(WitnessRefusal::Conflict { have: TreeSize(5) })
    );
    assert_eq!(
        head(&witness),
        (TreeSize(5), tree.root_at(TreeSize(5)).unwrap()),
        "nothing moved"
    );
    // old = 0 is as stale as old = 1
    assert_eq!(
        witness.add_checkpoint(&request(&tree, 0, 9), t(300)),
        Err(WitnessRefusal::Conflict { have: TreeSize(5) })
    );
    // with the size it holds the same checkpoint goes through
    let third = witness
        .add_checkpoint(&request(&tree, 5, 9), t(400))
        .unwrap();
    assert_eq!(
        head(&witness),
        (TreeSize(9), tree.root_at(TreeSize(9)).unwrap())
    );
    let note = cp_note(&tree, 9).with_sig(third).unwrap();
    assert_eq!(
        policy.check(&note, t(400)).unwrap().logged_since(),
        t(400),
        "the time it was given"
    );
    // the same checkpoint again is cosigned again, at the later time
    let again = witness
        .add_checkpoint(&request(&tree, 9, 9), t(500))
        .unwrap();
    let note = cp_note(&tree, 9).with_sig(again).unwrap();
    assert_eq!(policy.check(&note, t(500)).unwrap().logged_since(), t(500));
    // a witness never runs backwards: old = 9 is the only old it takes now
    for old in [0u64, 1, 5, 8] {
        assert_eq!(
            witness.add_checkpoint(&request(&tree, old, 9), t(600)),
            Err(WitnessRefusal::Conflict { have: TreeSize(9) }),
            "old {old}"
        );
    }
    // a log it is not configured with, and a log signature that is not the log's
    let other = Origin::new("other.example/log").unwrap();
    let cp = Checkpoint::new(
        other,
        TreeSize(1),
        tree.root_at(TreeSize(1)).unwrap(),
        vec![],
    )
    .unwrap();
    let foreign = AddRequest::new(
        TreeSize(9),
        vec![],
        sign_checkpoint(&cp, &log_key()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        witness.add_checkpoint(&foreign, t(700)),
        Err(WitnessRefusal::UnknownLog)
    );
    let cp = Checkpoint::new(
        Origin::azlin(),
        TreeSize(9),
        tree.root_at(TreeSize(9)).unwrap(),
        vec![],
    )
    .unwrap();
    let forged = sign_checkpoint(&cp, &key("azlin.io/release-log", 8)).unwrap();
    assert_eq!(
        witness.add_checkpoint(
            &AddRequest::new(TreeSize(9), vec![], forged).unwrap(),
            t(700)
        ),
        Err(WitnessRefusal::BadLogSignature)
    );
    let genuine = cp_note(&tree, 9);
    let line = genuine.sigs()[0].clone();
    let mut damaged_payload = line.payload().to_vec();
    damaged_payload[10] ^= 1;
    let damaged = Note::new(
        genuine.text().to_string(),
        vec![
            azlin_tlog::NoteSig::new(line.name().clone(), line.key_hash(), damaged_payload)
                .unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        witness.add_checkpoint(
            &AddRequest::new(TreeSize(9), vec![], damaged).unwrap(),
            t(700)
        ),
        Err(WitnessRefusal::BadLogSignature),
        "the log's own line with a damaged signature"
    );
    let unsigned = Note::new(cp.text(), vec![witness_key(1).cosign_for_test(&cp.text())]).unwrap();
    assert_eq!(
        witness.add_checkpoint(
            &AddRequest::new(TreeSize(9), vec![], unsigned).unwrap(),
            t(700)
        ),
        Err(WitnessRefusal::BadLogSignature),
        "a cosignature is no log signature"
    );
    assert_eq!(
        head(&witness),
        (TreeSize(9), tree.root_at(TreeSize(9)).unwrap())
    );
}

/// A cosignature of any witness key over a text, for a note that carries no log signature.
trait CosignForTest {
    fn cosign_for_test(&self, text: &str) -> azlin_tlog::NoteSig;
}

impl CosignForTest for azlin_tlog::MemoryKey {
    fn cosign_for_test(&self, text: &str) -> azlin_tlog::NoteSig {
        azlin_tlog::Cosigner::cosign(self, text, t(1)).unwrap()
    }
}

#[test]
fn t17_a_witness_refuses_a_fork() {
    let a = tree_with(9, 0);
    let b = tree_with(9, 1);
    assert_eq!(
        a.root_at(TreeSize(5)).unwrap(),
        b.root_at(TreeSize(5)).unwrap()
    );
    assert_ne!(
        a.root().unwrap(),
        b.root().unwrap(),
        "two histories of size 9"
    );
    let mut w = new_witness();
    w.add_checkpoint(&request(&a, 0, 5), t(1)).unwrap();
    w.add_checkpoint(&request(&a, 5, 9), t(2)).unwrap();
    let held = head(&w);
    // the same log key signed a second checkpoint of size 9 with another root: asked with old = 9, it is a
    // fork (an empty proof, a made-up proof: the fork is seen before the proof)
    let fork_note = cp_note(&b, 9);
    for proof in [vec![], vec![Hash([1; 32])]] {
        let req = AddRequest::new(TreeSize(9), proof, fork_note.clone()).unwrap();
        assert_eq!(w.add_checkpoint(&req, t(3)), Err(WitnessRefusal::Fork));
    }
    // asked with the honest proof from 5 it is stale: the witness holds 9, and says so
    assert_eq!(
        w.add_checkpoint(&request(&b, 5, 9), t(3)),
        Err(WitnessRefusal::Conflict { have: TreeSize(9) })
    );
    assert_eq!(head(&w), held, "nothing moved");
    // it is symmetric: a witness that followed B refuses A
    let mut w2 = new_witness();
    w2.add_checkpoint(&request(&b, 0, 5), t(1)).unwrap();
    w2.add_checkpoint(&request(&b, 5, 9), t(2)).unwrap();
    assert_eq!(
        w2.add_checkpoint(
            &AddRequest::new(TreeSize(9), vec![], cp_note(&a, 9)).unwrap(),
            t(3)
        ),
        Err(WitnessRefusal::Fork)
    );
    // an older size than the one held is refused: with old = 9 the old size is above the checkpoint's
    assert_eq!(
        w.add_checkpoint(
            &AddRequest::new(TreeSize(9), vec![], cp_note(&a, 5)).unwrap(),
            t(4)
        ),
        Err(WitnessRefusal::Shrunk)
    );
    // and so is any old above the size of the checkpoint, whether the witness holds that size or not
    assert_eq!(
        w.add_checkpoint(
            &AddRequest::new(TreeSize(7), vec![], cp_note(&a, 5)).unwrap(),
            t(4)
        ),
        Err(WitnessRefusal::Shrunk)
    );
    // an older checkpoint asked with a stale old is a conflict: the log learns the size
    assert_eq!(
        w.add_checkpoint(&request(&a, 3, 5), t(4)),
        Err(WitnessRefusal::Conflict { have: TreeSize(9) })
    );
    assert_eq!(head(&w), held, "nothing moved");
    // the honest extension still works after all the refusals
    let mut honest = tree_with(9, 0);
    for i in 9..12 {
        honest.append(format!("leaf {i}").as_bytes()).unwrap();
    }
    w.add_checkpoint(&request(&honest, 9, 12), t(5)).unwrap();
    assert_eq!(head(&w).0, TreeSize(12));
}

#[test]
fn t18_a_witness_refuses_a_bad_proof() {
    let tree = tree_with(9, 0);
    let other = tree_with(9, 1);
    let good = proof(&tree, 5, 9);
    assert!(!good.is_empty());
    let note = cp_note(&tree, 9);
    let mut w = new_witness();
    w.add_checkpoint(&request(&tree, 0, 5), t(1)).unwrap();
    let held = head(&w);
    let ask = |w: &mut Witness, path: Vec<Hash>| {
        w.add_checkpoint(
            &AddRequest::new(TreeSize(5), path, note.clone()).unwrap(),
            t(2),
        )
    };
    // one flipped hash, at every position and in every byte class
    for i in 0..good.len() {
        for (byte, bit) in [(0usize, 0u8), (13, 5), (31, 7)] {
            let mut path = good.clone();
            path[i].0[byte] ^= 1 << bit;
            assert_eq!(
                ask(&mut w, path),
                Err(WitnessRefusal::BadProof),
                "hash {i} byte {byte}"
            );
        }
    }
    // a proof for another pair of sizes
    for (old, new) in [
        (3u64, 9u64),
        (1, 9),
        (5, 8),
        (4, 9),
        (5, 6),
        (7, 9),
        (9, 9),
        (0, 9),
    ] {
        let path = proof(&tree, old, new);
        if path == good {
            continue;
        }
        assert_eq!(
            ask(&mut w, path),
            Err(WitnessRefusal::BadProof),
            "the proof from {old} to {new}"
        );
    }
    // a missing hash, at every position, none at all, and an extra one
    for i in 0..good.len() {
        let mut path = good.clone();
        path.remove(i);
        assert_eq!(
            ask(&mut w, path),
            Err(WitnessRefusal::BadProof),
            "hash {i} missing"
        );
    }
    assert_eq!(ask(&mut w, vec![]), Err(WitnessRefusal::BadProof));
    let mut longer = good.clone();
    longer.push(Hash([7; 32]));
    assert_eq!(ask(&mut w, longer), Err(WitnessRefusal::BadProof));
    let mut longer = good.clone();
    longer.insert(0, good[0]);
    assert_eq!(ask(&mut w, longer), Err(WitnessRefusal::BadProof));
    // the hashes in another order, and a proof of the right shape for the other history
    let mut swapped = good.clone();
    swapped.reverse();
    if swapped != good {
        assert_eq!(ask(&mut w, swapped), Err(WitnessRefusal::BadProof));
    }
    assert_eq!(
        ask(&mut w, proof(&other, 5, 9)),
        Err(WitnessRefusal::BadProof)
    );
    assert_eq!(head(&w), held, "no refusal moved the head");
    // the good proof is accepted, and only then does the head move
    ask(&mut w, good).unwrap();
    assert_eq!(head(&w), (TreeSize(9), tree.root_at(TreeSize(9)).unwrap()));
}

#[test]
fn t19_the_first_checkpoint_needs_no_proof() {
    let tree = tree_with(9, 0);
    // old = 0 and an empty proof: any size at all
    let mut w = new_witness();
    w.add_checkpoint(
        &AddRequest::new(TreeSize(0), vec![], cp_note(&tree, 9)).unwrap(),
        t(1),
    )
    .unwrap();
    assert_eq!(head(&w), (TreeSize(9), tree.root_at(TreeSize(9)).unwrap()));
    // old = 0 with a proof, however plausible, is refused
    let mut fresh = new_witness();
    for path in [
        vec![Hash([1; 32])],
        proof(&tree, 5, 9),
        proof(&tree, 1, 9),
        vec![tree.root_at(TreeSize(9)).unwrap()],
    ] {
        assert_eq!(
            fresh.add_checkpoint(
                &AddRequest::new(TreeSize(0), path, cp_note(&tree, 9)).unwrap(),
                t(1)
            ),
            Err(WitnessRefusal::BadProof)
        );
    }
    assert_eq!(head(&fresh), (TreeSize(0), empty_root()));
    // the empty tree: a checkpoint of size 0 is cosigned with old = 0, and a first leaf follows with old = 0
    let empty = tree_with(0, 0);
    fresh
        .add_checkpoint(
            &AddRequest::new(TreeSize(0), vec![], cp_note(&empty, 0)).unwrap(),
            t(2),
        )
        .unwrap();
    assert_eq!(head(&fresh), (TreeSize(0), empty_root()));
    assert_eq!(
        fresh.add_checkpoint(
            &AddRequest::new(TreeSize(0), vec![Hash([2; 32])], cp_note(&empty, 0)).unwrap(),
            t(2)
        ),
        Err(WitnessRefusal::BadProof),
        "even for the empty tree"
    );
    fresh.add_checkpoint(&request(&tree, 0, 1), t(3)).unwrap();
    assert_eq!(head(&fresh).0, TreeSize(1));
}

#[test]
fn t20_the_add_request_wire_form_is_strict() {
    let tree = tree_with(9, 0);
    let req = request(&tree, 5, 9);
    let bytes = req.to_bytes();
    assert_eq!(AddRequest::parse(&bytes).unwrap(), req);
    assert_eq!(
        (req.old(), req.proof().len()),
        (TreeSize(5), proof(&tree, 5, 9).len())
    );
    // the wire form of c2sp.org/tlog-witness, built here
    let mut want = String::from("old 5\n");
    for hash in proof(&tree, 5, 9) {
        want.push_str(&STANDARD.encode(hash.0));
        want.push('\n');
    }
    want.push('\n');
    want.push_str(&String::from_utf8(cp_note(&tree, 9).to_bytes()).unwrap());
    assert_eq!(String::from_utf8(bytes.clone()).unwrap(), want);
    // old = 0 and an empty proof: "old 0", the empty line, the note
    let first = request(&tree, 0, 3);
    let text = String::from_utf8(first.to_bytes()).unwrap();
    assert!(
        text.starts_with("old 0\n\nazlin.io/release-log/v1\n3\n"),
        "{text}"
    );
    assert_eq!(
        AddRequest::parse(first.to_bytes().as_slice()).unwrap(),
        first
    );
    // malformed requests, each with its own error
    let note_text = String::from_utf8(cp_note(&tree, 9).to_bytes()).unwrap();
    let h = STANDARD.encode([5u8; 32]);
    let cases: Vec<(&str, String, RequestError)> = vec![
        (
            "no empty line after the proof",
            format!("old 5\n{h}\n"),
            RequestError::NoBlankLine,
        ),
        (
            "nothing after the old line",
            "old 5\n".to_string(),
            RequestError::NoBlankLine,
        ),
        (
            "the note straight after the old line",
            format!("old 5\n{note_text}"),
            RequestError::ProofLine,
        ),
        ("no old line", note_text.clone(), RequestError::NoOldLine),
        (
            "old without a space",
            format!("old5\n\n{note_text}"),
            RequestError::NoOldLine,
        ),
        (
            "the wrong word",
            format!("older 5\n\n{note_text}"),
            RequestError::NoOldLine,
        ),
        (
            "old not decimal",
            format!("old five\n\n{note_text}"),
            RequestError::Old(NumberError::NotDecimal),
        ),
        (
            "old with a sign",
            format!("old +5\n\n{note_text}"),
            RequestError::Old(NumberError::NotDecimal),
        ),
        (
            "old negative",
            format!("old -5\n\n{note_text}"),
            RequestError::Old(NumberError::NotDecimal),
        ),
        (
            "old with a trailing space",
            format!("old 5 \n\n{note_text}"),
            RequestError::Old(NumberError::NotDecimal),
        ),
        (
            "old empty",
            format!("old \n\n{note_text}"),
            RequestError::Old(NumberError::NotDecimal),
        ),
        (
            "old with a leading zero",
            format!("old 05\n\n{note_text}"),
            RequestError::Old(NumberError::LeadingZero),
        ),
        (
            "old over u64",
            format!("old 18446744073709551616\n\n{note_text}"),
            RequestError::Old(NumberError::Overflow),
        ),
        (
            "old with a carriage return",
            format!("old 5\r\n\n{note_text}"),
            RequestError::Old(NumberError::NotDecimal),
        ),
        (
            "a proof line of 31 bytes",
            format!("old 5\n{}\n\n{note_text}", STANDARD.encode([5u8; 31])),
            RequestError::ProofLine,
        ),
        (
            "a proof line of 33 bytes",
            format!("old 5\n{}\n\n{note_text}", STANDARD.encode([5u8; 33])),
            RequestError::ProofLine,
        ),
        (
            "a proof line that is no base64",
            format!("old 5\n!!!!\n\n{note_text}"),
            RequestError::ProofLine,
        ),
        (
            "a proof line without padding",
            format!("old 5\n{}\n\n{note_text}", h.trim_end_matches('=')),
            RequestError::ProofLine,
        ),
        (
            "a proof line with a space",
            format!("old 5\n{h} \n\n{note_text}"),
            RequestError::ProofLine,
        ),
        (
            "64 proof lines",
            format!(
                "old 5\n{}\n{note_text}",
                (0..MAX_PROOF_LINES + 1)
                    .map(|_| h.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            RequestError::TooMany,
        ),
        (
            "a checkpoint that is no note",
            "old 5\n\nnot a note".to_string(),
            RequestError::Note(azlin_tlog::NoteError::NoFinalNewline),
        ),
        (
            "an empty checkpoint",
            "old 5\n\n".to_string(),
            RequestError::Note(azlin_tlog::NoteError::NoFinalNewline),
        ),
    ];
    for (what, text, want) in cases {
        assert_eq!(AddRequest::parse(text.as_bytes()), Err(want), "{what}");
    }
    // 63 proof lines are the most
    let sixty_three = format!(
        "old 5\n{}\n\n{note_text}",
        (0..MAX_PROOF_LINES)
            .map(|_| h.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(
        AddRequest::parse(sixty_three.as_bytes())
            .unwrap()
            .proof()
            .len(),
        MAX_PROOF_LINES
    );
    // not UTF-8, and over the size limit
    let mut bad_utf8 = bytes.clone();
    bad_utf8[2] = 0xFF;
    assert_eq!(AddRequest::parse(&bad_utf8), Err(RequestError::NotUtf8));
    assert_eq!(
        AddRequest::parse(&vec![b'a'; MAX_REQUEST_BYTES + 1]),
        Err(RequestError::TooLarge)
    );
    // the constructor holds the proof limit too
    let note = cp_note(&tree, 9);
    assert!(AddRequest::new(
        TreeSize(5),
        vec![Hash([1; 32]); MAX_PROOF_LINES],
        note.clone()
    )
    .is_ok());
    assert_eq!(
        AddRequest::new(TreeSize(5), vec![Hash([1; 32]); MAX_PROOF_LINES + 1], note).err(),
        Some(RequestError::TooMany)
    );
}

#[test]
fn t20_a_witness_is_bounded_and_configured_once_per_log() {
    let vkey = |i: usize| LogKey {
        origin: Origin::new(&format!("log{i}.example/v1")).unwrap(),
        verifier: log_key().verifier(SigKind::Ed25519),
    };
    let cosigner = || -> Box<dyn azlin_tlog::Cosigner> { Box::new(witness_key(0)) };
    // 64 logs are the limit
    let sixty_four: Vec<LogKey> = (0..MAX_ORIGINS).map(vkey).collect();
    let w = Witness::new(cosigner(), sixty_four).unwrap();
    assert_eq!(w.name(), &KeyName::new("witness.example/w1").unwrap());
    assert!(w.head(&Origin::new("log63.example/v1").unwrap()).is_some());
    assert_eq!(w.head(&Origin::new("log64.example/v1").unwrap()), None);
    let sixty_five: Vec<LogKey> = (0..MAX_ORIGINS + 1).map(vkey).collect();
    assert_eq!(
        Witness::new(cosigner(), sixty_five).err(),
        Some(WitnessError::TooMany)
    );
    // an origin twice, a log key of the wrong type, a log key that is no point
    assert_eq!(
        Witness::new(cosigner(), vec![vkey(1), vkey(1)]).err(),
        Some(WitnessError::DuplicateOrigin)
    );
    let mut wrong = vkey(1);
    wrong.verifier.kind = SigKind::Cosignature;
    assert_eq!(
        Witness::new(cosigner(), vec![wrong]).err(),
        Some(WitnessError::WrongKind)
    );
    let mut weak = vkey(1);
    weak.verifier.public = {
        let mut identity = [0u8; 32];
        identity[0] = 1;
        identity
    };
    assert_eq!(
        Witness::new(cosigner(), vec![weak]).err(),
        Some(WitnessError::BadKey)
    );
    // a witness with no log refuses everything as an unknown log
    let mut none = Witness::new(cosigner(), vec![]).unwrap();
    let tree = tree_with(3, 0);
    assert_eq!(
        none.add_checkpoint(&request(&tree, 0, 3), t(1)),
        Err(WitnessRefusal::UnknownLog)
    );
    // the note must be a checkpoint at all
    let mut w = new_witness();
    let text = "hello\n".to_string();
    let sig = azlin_tlog::NoteSigner::sign_text(&log_key(), text.as_bytes()).unwrap();
    let line = azlin_tlog::NoteSig::new(
        KeyName::new("azlin.io/release-log").unwrap(),
        azlin_tlog::NoteSigner::key_hash(&log_key()),
        sig.to_vec(),
    )
    .unwrap();
    let not_a_checkpoint = Note::new(text, vec![line]).unwrap();
    assert!(matches!(
        w.add_checkpoint(
            &AddRequest::new(TreeSize(0), vec![], not_a_checkpoint).unwrap(),
            t(1)
        ),
        Err(WitnessRefusal::BadCheckpoint(_))
    ));
}
