//! T11-T15: the witness rule.

use azlin_tlog::{
    sign_checkpoint, Checkpoint, CheckpointError, Cosigner, Hash, KeyName, LogKey, MemoryKey, Note,
    NoteError, NoteSig, Origin, Policy, PolicyError, SigKind, Span, TreeSize, UnixTime, Verifier,
    MAX_WITNESSES,
};

use crate::common::{key, kname, log_key, sha, t, witness_key, LOG_KEY_NAME};

fn root() -> Hash {
    Hash(sha(b"policy test root"))
}

fn checkpoint_of(origin: &Origin, size: u64) -> Checkpoint {
    Checkpoint::new(origin.clone(), TreeSize(size), root(), vec![]).unwrap()
}

/// A checkpoint of Azlin's origin signed by the log key.
fn base_note() -> Note {
    sign_checkpoint(&checkpoint_of(&Origin::azlin(), 12), &log_key()).unwrap()
}

/// `note` with the cosignatures of witnesses `at` (index, time), made directly by the witness keys.
fn cosigned(note: &Note, at: &[(usize, u64)]) -> Note {
    let mut out = note.clone();
    for &(w, time) in at {
        out = out
            .with_sig(witness_key(w).cosign(note.text(), t(time)).unwrap())
            .unwrap();
    }
    out
}

fn log_vkey() -> LogKey {
    LogKey {
        origin: Origin::azlin(),
        verifier: log_key().verifier(SigKind::Ed25519),
    }
}

fn verifiers(n: usize) -> Vec<Verifier> {
    (0..n)
        .map(|i| witness_key(i).verifier(SigKind::Cosignature))
        .collect()
}

fn policy(n: usize, quorum: u8, max_future: u64) -> Policy {
    Policy::new(
        log_vkey(),
        verifiers(n),
        quorum,
        Span::from_secs(max_future),
    )
    .unwrap()
}

fn w(i: usize) -> KeyName {
    witness_key(i).name().clone()
}

#[test]
fn t11_quorum_is_k_of_n() {
    let policy = policy(3, 2, 600);
    let base = base_note();
    let now = t(1_000);
    assert_eq!(policy.quorum(), 2);
    // no cosignature, one cosignature: not enough
    assert_eq!(
        policy.check(&base, now),
        Err(PolicyError::QuorumNotMet { have: 0, need: 2 })
    );
    for i in 0..3 {
        let one = cosigned(&base, &[(i, 100)]);
        assert_eq!(
            policy.check(&one, now),
            Err(PolicyError::QuorumNotMet { have: 1, need: 2 }),
            "witness {i}"
        );
    }
    // two are enough, whichever two; the result names them in the order of the policy
    for (a, b) in [(0usize, 1usize), (0, 2), (1, 2), (2, 0)] {
        let two = cosigned(&base, &[(a, 100), (b, 200)]);
        let witnessed = policy.check(&two, now).unwrap();
        let mut want = vec![w(a), w(b)];
        want.sort_by_key(|name| (0..3).position(|i| &w(i) == name));
        assert_eq!(witnessed.cosigners(), want.as_slice());
        assert_eq!(witnessed.checkpoint().size(), TreeSize(12));
    }
    let three = cosigned(&base, &[(0, 100), (1, 200), (2, 300)]);
    assert_eq!(policy.check(&three, now).unwrap().cosigners().len(), 3);
    // the same witness twice counts once: a note cannot hold two lines of one (name, key hash)
    let first = witness_key(0).cosign(base.text(), t(100)).unwrap();
    let again = witness_key(0).cosign(base.text(), t(101)).unwrap();
    assert_eq!(
        Note::new(
            base.text().to_string(),
            vec![base.sigs()[0].clone(), first.clone(), again]
        ),
        Err(NoteError::DuplicateSignature)
    );
    // an unknown witness counts for nothing, any number of them
    let mut with_unknown = cosigned(&base, &[(0, 100)]);
    for i in 5..9 {
        with_unknown = with_unknown
            .with_sig(witness_key(i).cosign(base.text(), t(100)).unwrap())
            .unwrap();
    }
    assert_eq!(
        policy.check(&with_unknown, now),
        Err(PolicyError::QuorumNotMet { have: 1, need: 2 })
    );
    // ... and garbage from an unknown key does not hurt a quorum that is there
    let garbage = NoteSig::new(
        kname("junk.example/key"),
        azlin_tlog::KeyHash::from_bytes([1, 1, 1, 1]),
        vec![3; 72],
    )
    .unwrap();
    let two_and_junk = cosigned(&base, &[(0, 100), (1, 200)])
        .with_sig(garbage)
        .unwrap();
    assert!(policy.check(&two_and_junk, now).is_ok());
    // a bad signature of a known witness is an error, not a missing vote, even with a quorum beside it
    let valid = witness_key(2).cosign(base.text(), t(300)).unwrap();
    let mut payload = valid.payload().to_vec();
    payload[20] ^= 0x10;
    let bad = NoteSig::new(valid.name().clone(), valid.key_hash(), payload).unwrap();
    let mut note = cosigned(&base, &[(0, 100), (1, 200)]);
    note = note.with_sig(bad).unwrap();
    assert_eq!(
        policy.check(&note, now),
        Err(PolicyError::BadSignature { name: w(2) })
    );
    // quorum 1 and quorum 3 on the same note
    assert!(self::policy(3, 1, 600)
        .check(&cosigned(&base, &[(1, 5)]), now)
        .is_ok());
    assert_eq!(
        self::policy(3, 3, 600).check(&cosigned(&base, &[(1, 5), (2, 6)]), now),
        Err(PolicyError::QuorumNotMet { have: 2, need: 3 })
    );
    assert!(self::policy(3, 3, 600).check(&three, now).is_ok());
}

#[test]
fn t12_logged_since_is_the_quorum_th_smallest_time() {
    let base = base_note();
    let now = t(10_000);
    let since = |n: usize, quorum: u8, at: &[(usize, u64)]| -> UnixTime {
        policy(n, quorum, 600)
            .check(&cosigned(&base, at), now)
            .unwrap()
            .logged_since()
    };
    // witnesses at 100, 150, 400 with quorum 2 -> 150, in every assignment of the times
    for at in [
        [(0, 100), (1, 150), (2, 400)],
        [(0, 400), (1, 100), (2, 150)],
        [(0, 150), (1, 400), (2, 100)],
        [(2, 100), (1, 150), (0, 400)],
    ] {
        assert_eq!(since(3, 2, &at), t(150), "{at:?}");
    }
    // one malicious witness backdates to 0: the second smallest is an honest member's time, never 0
    assert_eq!(since(3, 2, &[(0, 0), (1, 100), (2, 150)]), t(100));
    assert_eq!(since(3, 2, &[(1, 0), (0, 100), (2, 400)]), t(100));
    // quorum 1 of one witness is its time, even a time of 0
    assert_eq!(since(1, 1, &[(0, 777)]), t(777));
    assert_eq!(since(1, 1, &[(0, 0)]), t(0));
    // quorum 3 is the largest; quorum 1 of three is the smallest (the weakest policy)
    assert_eq!(since(3, 3, &[(0, 100), (1, 150), (2, 400)]), t(400));
    assert_eq!(since(3, 1, &[(0, 100), (1, 150), (2, 400)]), t(100));
    // equal times
    assert_eq!(since(3, 2, &[(0, 100), (1, 100), (2, 100)]), t(100));
    // five witnesses, quorum 3: the third smallest of 90, 30, 70, 50, 10 is 50
    assert_eq!(
        since(5, 3, &[(0, 90), (1, 30), (2, 70), (3, 50), (4, 10)]),
        t(50)
    );
    // with more cosignatures than the quorum the answer is still the quorum-th smallest of all of them
    assert_eq!(since(3, 2, &[(0, 300), (1, 200), (2, 100)]), t(200));
    // fewer than the quorum malicious witnesses cannot move it before the honest time at that rank: two
    // backdated witnesses against quorum 3 do not get below the honest one
    assert_eq!(
        since(5, 3, &[(0, 0), (1, 0), (2, 500), (3, 600), (4, 700)]),
        t(500)
    );
}

#[test]
fn t13_future_times_are_refused() {
    let base = base_note();
    let now = 1_000u64;
    let p = policy(1, 1, 60);
    assert_eq!(p.max_future(), Span::from_secs(60));
    // at now + max_future: accepted; one second later: refused, naming the witness
    assert!(p.check(&cosigned(&base, &[(0, now + 60)]), t(now)).is_ok());
    assert_eq!(
        p.check(&cosigned(&base, &[(0, now + 61)]), t(now)),
        Err(PolicyError::FutureCosignature { name: w(0) })
    );
    // the past is always fine
    assert!(p.check(&cosigned(&base, &[(0, now)]), t(now)).is_ok());
    assert!(p.check(&cosigned(&base, &[(0, 0)]), t(now)).is_ok());
    assert!(p.check(&cosigned(&base, &[(0, now - 1)]), t(now)).is_ok());
    // no slack at all
    let strict = policy(1, 1, 0);
    assert!(strict.check(&cosigned(&base, &[(0, now)]), t(now)).is_ok());
    assert_eq!(
        strict.check(&cosigned(&base, &[(0, now + 1)]), t(now)),
        Err(PolicyError::FutureCosignature { name: w(0) })
    );
    // a future cosignature next to a quorum of good ones refuses the note: the operator drops it
    let three = policy(3, 2, 60);
    assert_eq!(
        three.check(&cosigned(&base, &[(0, 10), (1, 20), (2, now + 61)]), t(now)),
        Err(PolicyError::FutureCosignature { name: w(2) })
    );
    assert!(three
        .check(&cosigned(&base, &[(0, 10), (1, 20), (2, now + 60)]), t(now))
        .is_ok());
    // the largest times do not overflow
    let edge = policy(1, 1, 0);
    assert!(edge
        .check(&cosigned(&base, &[(0, u64::MAX)]), t(u64::MAX))
        .is_ok());
    assert!(matches!(
        edge.check(&cosigned(&base, &[(0, u64::MAX)]), t(u64::MAX - 1)),
        Err(PolicyError::FutureCosignature { .. })
    ));
    // the time helpers
    assert_eq!(t(5).plus(Span::from_secs(7)), Some(t(12)));
    assert_eq!(t(u64::MAX).plus(Span::from_secs(1)), None);
    assert_eq!(t(12).since(t(5)), Some(Span::from_secs(7)));
    assert_eq!(t(5).since(t(12)), None);
    assert_eq!(t(5).get(), 5);
    assert_eq!(Span::from_secs(9).get(), 9);
}

#[test]
fn t14_wrong_origin_or_log_key_is_refused() {
    let p = policy(1, 1, 600);
    let now = t(1_000);
    let ok = cosigned(&base_note(), &[(0, 100)]);
    assert!(p.check(&ok, now).is_ok());
    // another origin, signed by the right key and cosigned
    let other = Origin::new("other.example/release-log").unwrap();
    let note = sign_checkpoint(&checkpoint_of(&other, 12), &log_key()).unwrap();
    assert_eq!(
        p.check(&cosigned(&note, &[(0, 100)]), now),
        Err(PolicyError::OriginMismatch)
    );
    // another version of the origin
    let v2 = Origin::new("azlin.io/release-log/v2").unwrap();
    let note = sign_checkpoint(&checkpoint_of(&v2, 12), &log_key()).unwrap();
    assert_eq!(
        p.check(&cosigned(&note, &[(0, 100)]), now),
        Err(PolicyError::OriginMismatch)
    );
    // the right origin, signed by another key under the same name: the key hash differs, the line is unknown
    let forged =
        sign_checkpoint(&checkpoint_of(&Origin::azlin(), 12), &key(LOG_KEY_NAME, 8)).unwrap();
    assert_eq!(
        p.check(&cosigned(&forged, &[(0, 100)]), now),
        Err(PolicyError::LogNotSigned)
    );
    // the right key under another name: the key hash holds the name
    let renamed = sign_checkpoint(
        &checkpoint_of(&Origin::azlin(), 12),
        &key("evil.example/log", 7),
    )
    .unwrap();
    assert_eq!(
        p.check(&cosigned(&renamed, &[(0, 100)]), now),
        Err(PolicyError::LogNotSigned)
    );
    // a log signature whose bytes are wrong
    let good = base_note();
    let mut payload = good.sigs()[0].payload().to_vec();
    payload[5] ^= 1;
    let bad = NoteSig::new(
        good.sigs()[0].name().clone(),
        good.sigs()[0].key_hash(),
        payload,
    )
    .unwrap();
    let broken = Note::new(good.text().to_string(), vec![bad]).unwrap();
    assert_eq!(
        p.check(&cosigned(&broken, &[(0, 100)]), now),
        Err(PolicyError::BadSignature {
            name: kname(LOG_KEY_NAME)
        })
    );
    // only witnesses signed it: the log did not
    let text = good.text().to_string();
    let no_log = Note::new(
        text.clone(),
        vec![witness_key(0).cosign(&text, t(100)).unwrap()],
    )
    .unwrap();
    assert_eq!(p.check(&no_log, now), Err(PolicyError::LogNotSigned));
    // the log's key signing as a cosigner (type 0x04) is no log signature
    let as_cosigner =
        Note::new(text.clone(), vec![log_key().cosign(&text, t(100)).unwrap()]).unwrap();
    assert_eq!(p.check(&as_cosigner, now), Err(PolicyError::LogNotSigned));
    // a note whose text is no checkpoint, however well it is signed
    let not_cp = Note::new(
        "hello\n".to_string(),
        vec![MemoryKey::from_seed(kname(LOG_KEY_NAME), [7; 32]).sign_text_for_test("hello\n")],
    )
    .unwrap();
    assert_eq!(
        p.check(&not_cp, now),
        Err(PolicyError::Checkpoint(CheckpointError::TooFewLines))
    );
    // the policy names the log it trusts
    assert_eq!(p.log().origin, Origin::azlin());
    assert_eq!(p.witnesses().len(), 1);
}

/// A log signature of type 0x01 over any text, to build a validly signed note that is no checkpoint.
trait SignAnyText {
    fn sign_text_for_test(&self, text: &str) -> NoteSig;
}

impl SignAnyText for MemoryKey {
    fn sign_text_for_test(&self, text: &str) -> NoteSig {
        let signature = azlin_tlog::NoteSigner::sign_text(self, text.as_bytes()).unwrap();
        NoteSig::new(
            self.name().clone(),
            azlin_tlog::NoteSigner::key_hash(self),
            signature.to_vec(),
        )
        .unwrap()
    }
}

#[test]
fn t15_policy_is_bounded() {
    let new = |witnesses: Vec<Verifier>, quorum: u8| {
        Policy::new(log_vkey(), witnesses, quorum, Span::from_secs(60))
    };
    // 16 witnesses are the limit
    assert!(new(verifiers(MAX_WITNESSES), 1).is_ok());
    assert!(new(verifiers(MAX_WITNESSES), MAX_WITNESSES as u8).is_ok());
    assert_eq!(
        new(verifiers(MAX_WITNESSES + 1), 1).err(),
        Some(PolicyError::TooMany)
    );
    // the quorum: zero fails closed, above n is refused, and so is a policy with no witness
    assert_eq!(new(verifiers(3), 0).err(), Some(PolicyError::QuorumZero));
    assert_eq!(new(verifiers(3), 4).err(), Some(PolicyError::QuorumTooHigh));
    assert_eq!(
        new(verifiers(3), 255).err(),
        Some(PolicyError::QuorumTooHigh)
    );
    assert_eq!(new(vec![], 1).err(), Some(PolicyError::QuorumTooHigh));
    assert_eq!(new(vec![], 0).err(), Some(PolicyError::QuorumZero));
    assert!(new(verifiers(3), 3).is_ok());
    // two witnesses with one name, or with one key under two names, would be one vote counted twice
    let mut same_name = verifiers(2);
    same_name[1].name = same_name[0].name.clone();
    assert_eq!(new(same_name, 1).err(), Some(PolicyError::DuplicateWitness));
    let mut same_key = verifiers(2);
    same_key[1].public = same_key[0].public;
    assert_eq!(new(same_key, 1).err(), Some(PolicyError::DuplicateWitness));
    // the log as a witness: its operator would cosign alone
    let mut with_log = verifiers(2);
    with_log.push(log_key().verifier(SigKind::Cosignature));
    assert_eq!(new(with_log, 1).err(), Some(PolicyError::WitnessIsLog));
    // the types: a log key is type 0x01, a witness type 0x04
    let mut wrong = verifiers(2);
    wrong[0].kind = SigKind::Ed25519;
    assert_eq!(new(wrong, 1).err(), Some(PolicyError::WrongKind));
    let mut log = log_vkey();
    log.verifier.kind = SigKind::Cosignature;
    assert_eq!(
        Policy::new(log, verifiers(2), 1, Span::from_secs(60)).err(),
        Some(PolicyError::WrongKind)
    );
    // a key that is not a usable point: the identity (small order) and a y that is on no curve point
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let mut weak = verifiers(2);
    weak[1].public = identity;
    assert_eq!(new(weak, 1).err(), Some(PolicyError::BadKey));
    let mut found_off_curve = false;
    for y in 2u8..=64 {
        let mut candidate = verifiers(2);
        candidate[1].public = {
            let mut bytes = [0u8; 32];
            bytes[0] = y;
            bytes
        };
        if new(candidate, 1).err() == Some(PolicyError::BadKey) {
            found_off_curve = true;
            break;
        }
    }
    assert!(
        found_off_curve,
        "some y below 65 is on no point of the curve"
    );
    let mut bad_log = log_vkey();
    bad_log.verifier.public = identity;
    assert_eq!(
        Policy::new(bad_log, verifiers(2), 1, Span::from_secs(60)).err(),
        Some(PolicyError::BadKey)
    );
    // the accessors give back what was built
    let p = new(verifiers(3), 2).unwrap();
    assert_eq!(p.witnesses(), verifiers(3).as_slice());
    assert_eq!(p.quorum(), 2);
    assert_eq!(p.log(), &log_vkey());
}
