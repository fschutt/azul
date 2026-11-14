//! T26-T29: proof bundles and `verify_bundle`.

use azlin_merkle::{leaf_hash, verify_inclusion};
use azlin_tlog::{
    sign_checkpoint, verify_bundle, Arch, BundleError, BundleLine, Checkpoint, Digest, Expect,
    ExpectField, Hash, LeafIndex, LogError, LogKey, Name, Note, NoteSig, NumberError, Origin,
    Policy, PolicyError, ProofBundle, ProofError, ReleaseRecord, SigKind, Span, SubjectKind,
    TreeSize, Verifier, Version, MAX_BUNDLE_BYTES, MAX_NOTE_BYTES, MAX_RECORD_BYTES,
};
use base64::{engine::general_purpose::STANDARD, Engine};

use crate::common::{hex32, hex_string, key, kname, log_key, record, t, World, LOG_KEY_NAME};
use crate::gold;

fn expect_of(i: usize) -> Expect {
    let r = record(i);
    Expect {
        kind: r.kind,
        name: r.name,
        version: r.version,
        arch: r.arch,
        digest: Some(r.digest),
    }
}

/// A log of 12 releases cosigned by three witnesses at 300, 100 and 200 (relative to 1,700,000,000), a policy
/// of 2 of 3 and the bundle of release 7.
struct Fixture {
    world: World,
    note: Note,
    policy: Policy,
    bundle: ProofBundle,
}

const BASE: u64 = 1_700_000_000;
const NOW: u64 = BASE + 1_000;

fn fixture() -> Fixture {
    let mut world = World::new(3);
    world.grow_to(12);
    let note = world.note();
    let note = world.cosigned(&note, &[(0, BASE + 300), (1, BASE + 100), (2, BASE + 200)]);
    let policy = world.policy(2);
    let bundle = world.log.bundle(LeafIndex(7), &note).unwrap();
    Fixture {
        world,
        note,
        policy,
        bundle,
    }
}

fn check(f: &Fixture, bundle: &ProofBundle) -> Result<azlin_tlog::Logged, BundleError> {
    verify_bundle(bundle, &f.policy, t(NOW), &expect_of(7))
}

/// `note` with every signature line passed through `change` (`None` drops the line).
fn rebuilt(note: &Note, change: impl Fn(usize, &NoteSig) -> Option<NoteSig>) -> Note {
    let sigs: Vec<NoteSig> = note
        .sigs()
        .iter()
        .enumerate()
        .filter_map(|(i, sig)| change(i, sig))
        .collect();
    Note::new(note.text().to_string(), sigs).unwrap()
}

fn with_note(bundle: &ProofBundle, note: &Note) -> ProofBundle {
    let mut out = bundle.clone();
    out.checkpoint = note.to_bytes();
    out
}

fn flipped(sig: &NoteSig, byte: usize) -> NoteSig {
    let mut payload = sig.payload().to_vec();
    payload[byte] ^= 0x01;
    NoteSig::new(sig.name().clone(), sig.key_hash(), payload).unwrap()
}

#[test]
fn t26_log_witnesses_bundle_verify_end_to_end() {
    let f = fixture();
    let logged = check(&f, &f.bundle).unwrap();
    // the second smallest of the cosignature times 100, 200, 300
    assert_eq!(logged.logged_since(), t(BASE + 200));
    assert_eq!(logged.index(), LeafIndex(7));
    assert_eq!(logged.record(), &record(7));
    assert_eq!(logged.checkpoint().size(), TreeSize(12));
    assert_eq!(
        logged.cosigners(),
        [
            kname("witness.example/w1"),
            kname("witness.example/w2"),
            kname("witness.example/w3")
        ]
    );
    // every release of the log has a bundle that verifies, with its own record and index
    let world = f.world;
    let note = f.note;
    for i in 0..12 {
        let bundle = world.log.bundle(LeafIndex(i as u64), &note).unwrap();
        let logged = verify_bundle(&bundle, &f.policy, t(NOW), &expect_of(i)).unwrap();
        assert_eq!(logged.record(), &record(i));
        assert_eq!(logged.index(), LeafIndex(i as u64));
        assert_eq!(
            logged.record().security_fix,
            i.is_multiple_of(7),
            "the mark the waiting period reads"
        );
    }
    // through the text form, and with the digest left open
    let text = f.bundle.to_text().unwrap();
    let parsed = ProofBundle::parse(text.as_bytes()).unwrap();
    assert_eq!(parsed, f.bundle);
    let mut open = expect_of(7);
    open.digest = None;
    assert!(verify_bundle(&parsed, &f.policy, t(NOW), &open).is_ok());
    // a policy of 3 of 3 takes the same bundle, and the time is then the largest
    let strict = world.policy(3);
    assert_eq!(
        verify_bundle(&parsed, &strict, t(NOW), &expect_of(7))
            .unwrap()
            .logged_since(),
        t(BASE + 300)
    );
    // an empty log has no bundle, and a leaf past the checkpoint has none either
    assert!(world.log.bundle(LeafIndex(12), &note).is_err());
}

#[test]
fn t26_the_log_makes_bundles_and_requests_only_for_its_own_checkpoints() {
    let mut world = World::new(1);
    world.grow_to(6);
    let own = world.note();
    assert!(world.log.bundle(LeafIndex(1), &own).is_ok());
    assert!(world.log.add_request(TreeSize(0), &own).is_ok());
    // a checkpoint of another history of the same size: another root
    let mut other = World::new(1);
    other.grow_to(5);
    other.log.append(&record(500)).unwrap();
    let foreign = other.note();
    assert_eq!(
        world.log.bundle(LeafIndex(1), &foreign).err(),
        Some(LogError::NotOurs)
    );
    assert_eq!(
        world.log.add_request(TreeSize(0), &foreign).err(),
        Some(LogError::NotOurs)
    );
    // a checkpoint of a larger tree than the log has reached, and one of another origin
    let mut bigger = World::new(1);
    bigger.grow_to(9);
    assert_eq!(
        world.log.bundle(LeafIndex(1), &bigger.note()).err(),
        Some(LogError::NotOurs)
    );
    let elsewhere = Origin::new("other.example/log").unwrap();
    let root = world.log.root().unwrap();
    let cp = Checkpoint::new(elsewhere, TreeSize(6), root, vec![]).unwrap();
    let note = sign_checkpoint(&cp, &log_key()).unwrap();
    assert_eq!(
        world.log.bundle(LeafIndex(1), &note).err(),
        Some(LogError::NotOurs)
    );
    // an old size above the checkpoint's, a leaf past it, a note that is no checkpoint
    assert_eq!(
        world.log.add_request(TreeSize(7), &own).err(),
        Some(LogError::OldAfterNew)
    );
    assert_eq!(
        world.log.bundle(LeafIndex(6), &own).err(),
        Some(LogError::IndexOutOfRange)
    );
    let junk = Note::new("hello\n".to_string(), own.sigs().to_vec()).unwrap();
    assert!(matches!(
        world.log.bundle(LeafIndex(0), &junk),
        Err(LogError::Checkpoint(_))
    ));
    // the checkpoint of an older size of this log is its own
    let mut grown = World::new(1);
    grown.grow_to(3);
    assert!(world.log.bundle(LeafIndex(2), &grown.note()).is_ok());
}

#[test]
fn t27_the_bundle_text_is_strict_and_round_trips() {
    // the bundle of a log of three releases, made by this crate, is the text made outside it
    let mut world = World::new(2);
    for text in gold::RECORDS {
        world
            .log
            .append(&ReleaseRecord::parse(text.as_bytes()).unwrap())
            .unwrap();
    }
    assert_eq!(hex_string(&world.log.root().unwrap().0), gold::ROOT_HEX);
    for (text, want) in gold::RECORDS.iter().zip(gold::LEAVES_HEX) {
        let leaf = ReleaseRecord::parse(text.as_bytes()).unwrap().leaf_hash();
        assert_eq!(hex_string(&leaf.0), want, "the leaf hashes hashlib gave");
    }
    let note = world.note();
    let cosigned = world.cosigned(&note, &[(0, gold::TIME_W1), (1, gold::TIME_W2)]);
    assert_eq!(String::from_utf8(cosigned.to_bytes()).unwrap(), gold::NOTE);
    let bundle = world.log.bundle(LeafIndex(1), &cosigned).unwrap();
    assert_eq!(bundle.to_text().unwrap(), gold::BUNDLE);
    // parse(to_text(b)) == b, for the golden text and for every bundle of a log of 20
    let parsed = ProofBundle::parse(gold::BUNDLE.as_bytes()).unwrap();
    assert_eq!(parsed, bundle);
    assert_eq!(
        parsed
            .proof
            .path
            .iter()
            .map(|h| hex_string(&h.0))
            .collect::<Vec<_>>(),
        gold::PATH_HEX
    );
    let mut big = World::new(1);
    big.grow_to(20);
    let note = big.note();
    let note = big.cosigned(&note, &[(0, 5)]);
    for i in 0..20 {
        let b = big.log.bundle(LeafIndex(i), &note).unwrap();
        assert_eq!(
            ProofBundle::parse(b.to_text().unwrap().as_bytes()).unwrap(),
            b
        );
    }
    // the golden text verifies with the public keys the goldens were made with, not with MemoryKey's
    let verifier = |name: &str, kind: SigKind, public: &str| Verifier {
        name: kname(name),
        kind,
        public: hex32(public),
    };
    let policy = |quorum: u8| {
        Policy::new(
            LogKey {
                origin: Origin::azlin(),
                verifier: verifier(LOG_KEY_NAME, SigKind::Ed25519, gold::LOG_PUBLIC),
            },
            vec![
                verifier("witness.example/w1", SigKind::Cosignature, gold::W1_PUBLIC),
                verifier("witness.example/w2", SigKind::Cosignature, gold::W2_PUBLIC),
            ],
            quorum,
            Span::from_secs(600),
        )
        .unwrap()
    };
    let expect = Expect {
        kind: SubjectKind::Worker,
        name: Name::new("azmail-worker").unwrap(),
        version: Version::new("2.0.1").unwrap(),
        arch: Arch::Any,
        digest: Some(Digest::of(b"worker 2.0.1 statement")),
    };
    let now = t(gold::TIME_W2 + 1_000);
    let logged = verify_bundle(&parsed, &policy(2), now, &expect).unwrap();
    assert_eq!(logged.logged_since(), t(gold::TIME_W2));
    assert_eq!(logged.index(), LeafIndex(1));
    assert!(logged.record().security_fix);
    assert_eq!(
        verify_bundle(&parsed, &policy(1), now, &expect)
            .unwrap()
            .logged_since(),
        t(gold::TIME_W1)
    );
    // strictness: each malformed text has its own error
    let lines: Vec<&str> = gold::BUNDLE.lines().collect();
    assert_eq!(lines.len(), 7);
    let (header, leaf, size, p1, p2, rec, cp) = (
        lines[0], lines[1], lines[2], lines[3], lines[4], lines[5], lines[6],
    );
    let join = |ls: &[&str]| -> String { format!("{}\n", ls.join("\n")) };
    let owned_join = |ls: &[String]| -> String { format!("{}\n", ls.join("\n")) };
    let many_paths: Vec<String> = [header, leaf, size, p1, p2]
        .iter()
        .map(|s| s.to_string())
        .chain((0..63).map(|_| p1.to_string()))
        .chain([rec.to_string(), cp.to_string()])
        .collect();
    let line = |l: BundleLine| BundleError::Line(l);
    let cases: Vec<(&str, String, BundleError)> = vec![
        (
            "another header",
            join(&["azlin-proof-v2", leaf, size, p1, p2, rec, cp]),
            line(BundleLine::Header),
        ),
        (
            "no header",
            join(&[leaf, size, p1, p2, rec, cp]),
            line(BundleLine::Header),
        ),
        (
            "no leaf line",
            join(&[header, size, p1, p2, rec, cp]),
            line(BundleLine::Leaf),
        ),
        (
            "leaf and size swapped",
            join(&[header, size, leaf, p1, p2, rec, cp]),
            line(BundleLine::Leaf),
        ),
        (
            "an upper-case key",
            join(&[header, "LEAF=1", size, p1, p2, rec, cp]),
            line(BundleLine::Leaf),
        ),
        (
            "spaces around the equals sign",
            join(&[header, "leaf = 1", size, p1, p2, rec, cp]),
            line(BundleLine::Leaf),
        ),
        (
            "a leading zero in the leaf",
            join(&[header, "leaf=01", size, p1, p2, rec, cp]),
            BundleError::Number {
                line: BundleLine::Leaf,
                error: NumberError::LeadingZero,
            },
        ),
        (
            "a word for the leaf",
            join(&[header, "leaf=one", size, p1, p2, rec, cp]),
            BundleError::Number {
                line: BundleLine::Leaf,
                error: NumberError::NotDecimal,
            },
        ),
        (
            "a negative leaf",
            join(&[header, "leaf=-1", size, p1, p2, rec, cp]),
            BundleError::Number {
                line: BundleLine::Leaf,
                error: NumberError::NotDecimal,
            },
        ),
        (
            "an empty size",
            join(&[header, leaf, "size=", p1, p2, rec, cp]),
            BundleError::Number {
                line: BundleLine::Size,
                error: NumberError::NotDecimal,
            },
        ),
        (
            "a size over u64",
            join(&[header, leaf, "size=18446744073709551616", p1, p2, rec, cp]),
            BundleError::Number {
                line: BundleLine::Size,
                error: NumberError::Overflow,
            },
        ),
        (
            "a path that is no base64",
            join(&[header, leaf, size, "path=!!!!", p2, rec, cp]),
            BundleError::BadHash,
        ),
        (
            "a path of 31 bytes",
            join(&[
                header,
                leaf,
                size,
                &format!("path={}", STANDARD.encode([1u8; 31])),
                p2,
                rec,
                cp,
            ]),
            BundleError::BadHash,
        ),
        (
            "a path without padding",
            join(&[header, leaf, size, p1.trim_end_matches('='), p2, rec, cp]),
            BundleError::BadHash,
        ),
        (
            "an empty path line",
            join(&[header, leaf, size, "path=", p2, rec, cp]),
            BundleError::BadHash,
        ),
        (
            "65 path lines",
            owned_join(&many_paths),
            BundleError::TooManyPath,
        ),
        (
            "a record without padding",
            join(&[header, leaf, size, p1, p2, rec.trim_end_matches('='), cp]),
            BundleError::Base64(BundleLine::Record),
        ),
        (
            "a checkpoint with a space",
            join(&[header, leaf, size, p1, p2, rec, &format!("{cp} ")]),
            BundleError::Base64(BundleLine::Checkpoint),
        ),
        (
            "no record line",
            join(&[header, leaf, size, p1, p2, cp]),
            line(BundleLine::Record),
        ),
        (
            "no checkpoint line",
            join(&[header, leaf, size, p1, p2, rec]),
            line(BundleLine::Checkpoint),
        ),
        (
            "a path after the record",
            join(&[header, leaf, size, p1, rec, p2, cp]),
            line(BundleLine::Checkpoint),
        ),
        (
            "the record twice",
            join(&[header, leaf, size, p1, p2, rec, rec, cp]),
            line(BundleLine::Checkpoint),
        ),
        (
            "an empty line in the middle",
            join(&[header, leaf, size, "", p1, p2, rec, cp]),
            line(BundleLine::Record),
        ),
        (
            "a trailing line",
            join(&[header, leaf, size, p1, p2, rec, cp, "extra=1"]),
            BundleError::Trailing,
        ),
        (
            "a trailing empty line",
            format!("{}\n", gold::BUNDLE),
            BundleError::Trailing,
        ),
        (
            "no final newline",
            gold::BUNDLE.trim_end().to_string(),
            BundleError::NoFinalNewline,
        ),
        (
            "CRLF line ends",
            gold::BUNDLE.replace('\n', "\r\n"),
            line(BundleLine::Header),
        ),
        ("nothing", String::new(), BundleError::NoFinalNewline),
    ];
    assert!(cases.len() >= 25);
    for (what, text, want) in cases {
        assert_eq!(ProofBundle::parse(text.as_bytes()), Err(want), "{what}");
    }
    // not UTF-8, and the size limits: 64 KiB is the most, a record of 4097 bytes is too long
    let mut bad = gold::BUNDLE.as_bytes().to_vec();
    bad[3] = 0xFF;
    assert_eq!(ProofBundle::parse(&bad), Err(BundleError::NotUtf8));
    assert_eq!(
        ProofBundle::parse(&vec![b'a'; MAX_BUNDLE_BYTES + 1]),
        Err(BundleError::TooLarge)
    );
    assert_eq!(
        ProofBundle::parse(&vec![b'a'; MAX_BUNDLE_BYTES]),
        Err(BundleError::NoFinalNewline)
    );
    let long_record = join(&[
        header,
        leaf,
        size,
        p1,
        p2,
        &format!(
            "record={}",
            STANDARD.encode(vec![b'r'; MAX_RECORD_BYTES + 1])
        ),
        cp,
    ]);
    assert_eq!(
        ProofBundle::parse(long_record.as_bytes()),
        Err(BundleError::TooLarge)
    );
    let at_limit = join(&[
        header,
        leaf,
        size,
        p1,
        p2,
        &format!("record={}", STANDARD.encode(vec![b'r'; MAX_RECORD_BYTES])),
        cp,
    ]);
    assert!(
        ProofBundle::parse(at_limit.as_bytes()).is_ok(),
        "the limit itself is fine"
    );
    // to_text holds the same limits and 64 hashes of path are the most
    let mut over = bundle.clone();
    over.record = vec![b'r'; MAX_RECORD_BYTES + 1];
    assert_eq!(over.to_text(), Err(BundleError::TooLarge));
    let mut over = bundle.clone();
    over.checkpoint = vec![b'c'; MAX_NOTE_BYTES + 1];
    assert_eq!(over.to_text(), Err(BundleError::TooLarge));
    let mut over = bundle.clone();
    over.proof.path = vec![Hash([1; 32]); 65];
    assert_eq!(over.to_text(), Err(BundleError::TooManyPath));
    let mut sixty_four = bundle;
    sixty_four.proof.path = vec![Hash([1; 32]); 64];
    assert_eq!(
        ProofBundle::parse(sixty_four.to_text().unwrap().as_bytes()).unwrap(),
        sixty_four
    );
}

#[test]
fn t28_verify_bundle_checks_every_link() {
    let f = fixture();
    check(&f, &f.bundle).unwrap();
    let proof_failed =
        |r: Result<azlin_tlog::Logged, BundleError>, want: ProofError, what: &str| {
            assert_eq!(r.err(), Some(BundleError::ProofFailed(want)), "{what}");
        };

    // 1. the record: every byte, damaged two ways: the record does not parse, or it is another record and
    // the leaf is not the one the proof is for; never accepted
    let record_bytes = f.bundle.record.clone();
    assert!(record_bytes.len() > 150);
    for i in 0..record_bytes.len() {
        for mask in [0x01u8, 0x20] {
            let mut bundle = f.bundle.clone();
            bundle.record[i] ^= mask;
            match check(&f, &bundle) {
                Err(BundleError::Record(_) | BundleError::ProofFailed(_)) => {}
                other => panic!("record byte {i} ^ {mask:#x}: {other:?}"),
            }
        }
    }
    // the record changed into another valid record: the proof is for another leaf
    let mut marked = record(7);
    marked.security_fix = !marked.security_fix;
    let mut bundle = f.bundle.clone();
    bundle.record = marked.to_bytes();
    proof_failed(
        check(&f, &bundle),
        ProofError::RootMismatch,
        "the security fix mark flipped",
    );
    let mut bundle = f.bundle.clone();
    bundle.record = record(8).to_bytes();
    proof_failed(
        check(&f, &bundle),
        ProofError::RootMismatch,
        "the record of another release",
    );
    // bytes added to or taken from the record
    let edits: [fn(&mut Vec<u8>); 3] = [
        |r| r.push(b'\n'),
        |r| {
            r.pop();
        },
        |r| r.clear(),
    ];
    for edit in edits {
        let mut bundle = f.bundle.clone();
        edit(&mut bundle.record);
        assert!(matches!(check(&f, &bundle), Err(BundleError::Record(_))));
    }

    // 2. the path: every hash with a bit flipped, a hash dropped, a hash added
    let path = f.bundle.proof.path.clone();
    assert!(path.len() >= 3);
    for i in 0..path.len() {
        for byte in [0usize, 17, 31] {
            let mut bundle = f.bundle.clone();
            bundle.proof.path[i].0[byte] ^= 0x04;
            proof_failed(
                check(&f, &bundle),
                ProofError::RootMismatch,
                &format!("hash {i} byte {byte}"),
            );
        }
        let mut bundle = f.bundle.clone();
        bundle.proof.path.remove(i);
        proof_failed(
            check(&f, &bundle),
            ProofError::PathLength,
            &format!("hash {i} dropped"),
        );
    }
    let mut bundle = f.bundle.clone();
    bundle.proof.path.push(Hash([9; 32]));
    proof_failed(check(&f, &bundle), ProofError::PathLength, "a hash added");
    let mut bundle = f.bundle.clone();
    bundle.proof.path.clear();
    proof_failed(check(&f, &bundle), ProofError::PathLength, "no path");

    // 3. the index
    for index in [0u64, 6, 8, 3, 11] {
        let mut bundle = f.bundle.clone();
        bundle.proof.index = LeafIndex(index);
        assert!(
            matches!(check(&f, &bundle), Err(BundleError::ProofFailed(_))),
            "index {index}"
        );
    }
    for index in [12u64, 13, u64::MAX] {
        let mut bundle = f.bundle.clone();
        bundle.proof.index = LeafIndex(index);
        proof_failed(
            check(&f, &bundle),
            ProofError::IndexOutOfRange,
            "an index past the tree",
        );
    }

    // 4. the size of the tree: the checkpoint's, nothing else
    for size in [0u64, 1, 11, 13, 14, 24, u64::MAX] {
        let mut bundle = f.bundle.clone();
        bundle.proof.size = TreeSize(size);
        assert_eq!(
            check(&f, &bundle).err(),
            Some(BundleError::SizeMismatch {
                proof: TreeSize(size),
                checkpoint: TreeSize(12)
            }),
            "size {size}"
        );
    }

    // 5. the checkpoint: its root, its size, its text, its origin
    let text = f.note.text().to_string();
    let other_root = text.replacen(text.lines().nth(2).unwrap(), &STANDARD.encode([3u8; 32]), 1);
    let forged = Note::new(other_root, f.note.sigs().to_vec()).unwrap();
    proof_failed(
        check(&f, &with_note(&f.bundle, &forged)),
        ProofError::RootMismatch,
        "another root",
    );
    let other_size =
        Note::new(text.replacen("\n12\n", "\n13\n", 1), f.note.sigs().to_vec()).unwrap();
    assert!(matches!(
        check(&f, &with_note(&f.bundle, &other_size)),
        Err(BundleError::SizeMismatch { .. })
    ));
    let junk = Note::new("hello\n".to_string(), f.note.sigs().to_vec()).unwrap();
    assert!(matches!(
        check(&f, &with_note(&f.bundle, &junk)),
        Err(BundleError::Checkpoint(_))
    ));
    for bytes in [
        vec![],
        b"not a note".to_vec(),
        f.note.to_bytes()[..40].to_vec(),
    ] {
        let mut bundle = f.bundle.clone();
        bundle.checkpoint = bytes;
        assert!(matches!(check(&f, &bundle), Err(BundleError::Note(_))));
    }
    // the checkpoint of another origin, even with a proof that fits
    let other = Origin::new("other.example/log").unwrap();
    let swapped = Note::new(
        text.replacen("azlin.io/release-log/v1", other.as_str(), 1),
        f.note.sigs().to_vec(),
    )
    .unwrap();
    assert_eq!(
        check(&f, &with_note(&f.bundle, &swapped)).err(),
        Some(BundleError::NotWitnessed(PolicyError::OriginMismatch))
    );

    // 6. the signatures: the log's, and each cosignature in turn, in the payload and in the time
    let log_flipped = rebuilt(&f.note, |i, sig| {
        Some(if i == 0 { flipped(sig, 9) } else { sig.clone() })
    });
    assert_eq!(
        check(&f, &with_note(&f.bundle, &log_flipped)).err(),
        Some(BundleError::NotWitnessed(PolicyError::BadSignature {
            name: kname(LOG_KEY_NAME)
        }))
    );
    for line in 1..=3 {
        for byte in [3usize, 8, 40] {
            let broken = rebuilt(&f.note, |i, sig| {
                Some(if i == line {
                    flipped(sig, byte)
                } else {
                    sig.clone()
                })
            });
            assert_eq!(
                check(&f, &with_note(&f.bundle, &broken)).err(),
                Some(BundleError::NotWitnessed(PolicyError::BadSignature {
                    name: f.note.sigs()[line].name().clone()
                })),
                "cosignature {line} byte {byte}"
            );
        }
    }
    // a quorum that is not there: one cosignature of two needed, none
    for keep in [1usize, 0] {
        let thin = rebuilt(&f.note, |i, sig| {
            (i == 0 || (1..=keep).contains(&i)).then(|| sig.clone())
        });
        assert_eq!(
            check(&f, &with_note(&f.bundle, &thin)).err(),
            Some(BundleError::NotWitnessed(PolicyError::QuorumNotMet {
                have: keep,
                need: 2
            })),
            "{keep} cosignatures"
        );
    }
    // no log signature at all
    let no_log = rebuilt(&f.note, |i, sig| (i != 0).then(|| sig.clone()));
    assert_eq!(
        check(&f, &with_note(&f.bundle, &no_log)).err(),
        Some(BundleError::NotWitnessed(PolicyError::LogNotSigned))
    );
    // a log whose key was stolen, with no witness that agreed: the history is there, the witnesses are not
    let stolen = {
        let mut world = World::new(0);
        world.grow_to(12);
        world.note()
    };
    let history = f.world.log.bundle(LeafIndex(7), &f.note).unwrap();
    assert_eq!(
        check(&f, &with_note(&history, &stolen)).err(),
        Some(BundleError::NotWitnessed(PolicyError::QuorumNotMet {
            have: 0,
            need: 2
        }))
    );
    // a log with another key: its own history, signed with a key the policy does not know
    let attacker = {
        let mut world = World::new(0);
        world.log_key = key(LOG_KEY_NAME, 8);
        world.grow_to(12);
        world.note()
    };
    assert_eq!(
        check(&f, &with_note(&history, &attacker)).err(),
        Some(BundleError::NotWitnessed(PolicyError::LogNotSigned))
    );
    // the cosignatures lie in the future of the verifier's clock
    assert!(matches!(
        verify_bundle(&f.bundle, &f.policy, t(BASE - 1_000), &expect_of(7)),
        Err(BundleError::NotWitnessed(
            PolicyError::FutureCosignature { .. }
        ))
    ));

    // 7. what the caller expects: each field in turn
    let mut e = expect_of(7);
    e.kind = SubjectKind::PhoneApp;
    assert_eq!(
        verify_bundle(&f.bundle, &f.policy, t(NOW), &e).err(),
        Some(BundleError::ExpectMismatch(ExpectField::Kind))
    );
    let mut e = expect_of(7);
    e.name = Name::new("app-77").unwrap();
    assert_eq!(
        verify_bundle(&f.bundle, &f.policy, t(NOW), &e).err(),
        Some(BundleError::ExpectMismatch(ExpectField::Name))
    );
    let mut e = expect_of(7);
    e.version = Version::new("1.7.1").unwrap();
    assert_eq!(
        verify_bundle(&f.bundle, &f.policy, t(NOW), &e).err(),
        Some(BundleError::ExpectMismatch(ExpectField::Version))
    );
    let mut e = expect_of(7);
    e.arch = if e.arch == Arch::Any {
        Arch::Arm64
    } else {
        Arch::Any
    };
    assert_eq!(
        verify_bundle(&f.bundle, &f.policy, t(NOW), &e).err(),
        Some(BundleError::ExpectMismatch(ExpectField::Arch))
    );
    let mut e = expect_of(7);
    e.digest = Some(Digest::of(b"another statement"));
    assert_eq!(
        verify_bundle(&f.bundle, &f.policy, t(NOW), &e).err(),
        Some(BundleError::ExpectMismatch(ExpectField::Digest))
    );
    // the bundle of release 7 is not release 8's
    assert!(verify_bundle(&f.bundle, &f.policy, t(NOW), &expect_of(8)).is_err());
    // and the policy of another log refuses a bundle of this one
    let other_policy = Policy::new(
        LogKey {
            origin: Origin::azlin(),
            verifier: key(LOG_KEY_NAME, 8).verifier(SigKind::Ed25519),
        },
        f.policy.witnesses().to_vec(),
        2,
        Span::from_secs(600),
    )
    .unwrap();
    assert_eq!(
        verify_bundle(&f.bundle, &other_policy, t(NOW), &expect_of(7)).err(),
        Some(BundleError::NotWitnessed(PolicyError::LogNotSigned))
    );
}

#[test]
fn t28_the_proof_size_is_the_checkpoints_size() {
    // The Merkle verifier reads the size only through the shape of the proof: the proof of leaf 0 in a tree
    // of 33 leaves leads to the same root when it claims 34 leaves (the sibling sides are the same).
    let mut world = World::new(2);
    world.grow_to(33);
    let note = world.note();
    let cosigned = world.cosigned(&note, &[(0, 10), (1, 20)]);
    let policy = world.policy(2);
    let good = world.log.bundle(LeafIndex(0), &cosigned).unwrap();
    let e = expect_of(0);
    assert!(verify_bundle(&good, &policy, t(100), &e).is_ok());
    let root33 = world.log.root().unwrap();
    let mut lie = good.clone();
    lie.proof.size = TreeSize(34);
    assert_eq!(
        verify_inclusion(&leaf_hash(&lie.record), &lie.proof, &root33),
        Ok(()),
        "the premise: the Merkle verifier alone does not see the lie"
    );
    assert_eq!(
        verify_bundle(&lie, &policy, t(100), &e).err(),
        Some(BundleError::SizeMismatch {
            proof: TreeSize(34),
            checkpoint: TreeSize(33)
        })
    );
    let mut lie = good.clone();
    lie.proof.size = TreeSize(40);
    assert!(matches!(
        verify_bundle(&lie, &policy, t(100), &e),
        Err(BundleError::SizeMismatch { .. })
    ));
}

#[test]
fn t29_an_old_bundle_verifies_against_its_own_checkpoint() {
    let mut world = World::new(3);
    world.grow_to(5);
    let note5 = world.note();
    let cosigned5 = world.cosigned(&note5, &[(0, 100), (1, 200)]);
    let policy = world.policy(2);
    let old = world.log.bundle(LeafIndex(2), &cosigned5).unwrap();
    let e = expect_of(2);
    let logged = verify_bundle(&old, &policy, t(1_000), &e).unwrap();
    assert_eq!(
        (logged.checkpoint().size(), logged.logged_since()),
        (TreeSize(5), t(200))
    );
    // the log grows to 50 and is cosigned again: the witnesses are asked with proofs from size 5
    world.grow_to(50);
    let note50 = world.note();
    let cosigned50 = world.cosigned(&note50, &[(0, 300), (1, 400)]);
    let new = world.log.bundle(LeafIndex(2), &cosigned50).unwrap();
    // the bundle made at size 5 still verifies, against its own checkpoint, with its own time
    let logged = verify_bundle(&old, &policy, t(2_000), &e).unwrap();
    assert_eq!(
        (logged.checkpoint().size(), logged.logged_since()),
        (TreeSize(5), t(200))
    );
    let logged = verify_bundle(&new, &policy, t(2_000), &e).unwrap();
    assert_eq!(
        (logged.checkpoint().size(), logged.logged_since()),
        (TreeSize(50), t(400))
    );
    assert!(new.proof.path.len() > old.proof.path.len());
    // through the text form after the growth
    let again = ProofBundle::parse(old.to_text().unwrap().as_bytes()).unwrap();
    assert!(verify_bundle(&again, &policy, t(2_000), &e).is_ok());
    // a bundle whose checkpoint the quorum did not cosign never verifies: the third witness alone
    let alone = world.cosigned(&note50, &[(2, 500)]);
    let weak = world.log.bundle(LeafIndex(2), &alone).unwrap();
    assert_eq!(
        verify_bundle(&weak, &policy, t(2_000), &e).err(),
        Some(BundleError::NotWitnessed(PolicyError::QuorumNotMet {
            have: 1,
            need: 2
        }))
    );
    // nor does the proof of one checkpoint with the note of another
    let mut mixed = old.clone();
    mixed.checkpoint = cosigned50.to_bytes();
    assert!(matches!(
        verify_bundle(&mixed, &policy, t(2_000), &e),
        Err(BundleError::SizeMismatch { .. })
    ));
    // a bundle of the log is for leaves inside the checkpoint: leaf 7 is not in the tree of 5
    assert!(world.log.bundle(LeafIndex(7), &cosigned5).is_err());
    // the bundle of every leaf of the old checkpoint still verifies after the growth
    for i in 0..5 {
        let b = world.log.bundle(LeafIndex(i as u64), &cosigned5).unwrap();
        assert!(verify_bundle(&b, &policy, t(3_000), &expect_of(i)).is_ok());
    }
}
