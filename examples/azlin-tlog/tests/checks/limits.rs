//! T34 and T35: no secret in the debug output, and every limit refused at limit + 1 and accepted at the limit.

use azlin_merkle::MAX_LEAVES;
use azlin_tlog::{
    sign_checkpoint, AddRequest, Alarm, BundleError, Checkpoint, CheckpointError, Cosigner, Hash,
    KeyName, LeafIndex, LogKey, MemoryKey, Monitor, NameError, Note, NoteError, NoteSig, Origin,
    Policy, PolicyError, ProofBundle, RecordError, ReleaseRecord, RequestError, SigKind, Span,
    TreeSize, Verifier, Witness, WitnessError, MAX_BUNDLE_BYTES, MAX_EXTENSIONS,
    MAX_EXTENSION_BYTES, MAX_INDEX_RECORDS, MAX_NAME_BYTES, MAX_NOTE_BYTES, MAX_ORIGINS,
    MAX_PROOF_LINES, MAX_RECORD_BYTES, MAX_REQUEST_BYTES, MAX_SIGNATURES, MAX_SIG_PAYLOAD,
    MAX_TEXT_BYTES, MAX_WITNESSES,
};
use base64::{engine::general_purpose::STANDARD, Engine};

use crate::common::{hex_string, kname, log_key, record, t, witness_key, World};

#[test]
fn t34_no_secret_in_debug() {
    let seed = [0xA5u8; 32];
    let k = MemoryKey::from_seed(kname("secret.example/key"), seed);
    let public = k.public();
    let needles: Vec<String> = vec![
        "a5a5a5a5".to_string(),
        "A5A5A5A5".to_string(),
        "165, 165, 165".to_string(),
        "165,165".to_string(),
        "\\xa5".to_string(),
        "paWlpaWl".to_string(),
        hex_string(&public),
        hex_string(&public[..8]),
        STANDARD.encode(public)[..12].to_string(),
        format!("{:?}", &public[..4])
            .trim_matches(|c| c == '[' || c == ']')
            .to_string(),
    ];
    let clean = |what: &str, shown: &str| {
        for needle in &needles {
            assert!(
                !shown.contains(needle.as_str()),
                "{what} shows key material ({needle}): {shown}"
            );
        }
    };
    // MemoryKey: its name and nothing else, in both forms, and not Clone (it has no derive to look at; this
    // is the text it prints)
    for shown in [format!("{k:?}"), format!("{k:#?}")] {
        clean("MemoryKey Debug", &shown);
        assert!(shown.contains("secret.example/key"), "{shown}");
        assert!(shown.starts_with("MemoryKey"), "{shown}");
    }
    // every error type, produced by a flow that uses the key, shown and debug-printed
    let mut shown: Vec<(String, String)> = Vec::new();
    let mut push = |what: &str, text: String| shown.push((what.to_string(), text));
    let origin = Origin::azlin();
    let cp = Checkpoint::new(origin.clone(), TreeSize(3), Hash([7; 32]), vec![]).unwrap();
    let note = sign_checkpoint(&cp, &k).unwrap();
    let verifier = k.verifier(SigKind::Ed25519);
    let mut payload = note.sigs()[0].payload().to_vec();
    payload[0] ^= 1;
    let bad = NoteSig::new(
        note.sigs()[0].name().clone(),
        note.sigs()[0].key_hash(),
        payload,
    )
    .unwrap();
    let broken = Note::new(note.text().to_string(), vec![bad]).unwrap();
    let e = broken.verify(std::slice::from_ref(&verifier)).unwrap_err();
    push("NoteError::BadSignature", format!("{e} | {e:?}"));
    let e = note.verify(&[]).unwrap_err();
    push("NoteError::NoKnownSignature", format!("{e} | {e:?}"));
    let mut weak = verifier.clone();
    weak.public = {
        let mut identity = [0u8; 32];
        identity[0] = 1;
        identity
    };
    let e = note.verify(&[weak]).unwrap_err();
    push("NoteError::BadKey", format!("{e} | {e:?}"));
    let e = Verifier::from_vkey(&format!(
        "secret.example/key+00000000+{}",
        STANDARD.encode([1u8; 33])
    ))
    .unwrap_err();
    push("NoteError::BadVkey", format!("{e} | {e:?}"));
    let e = k.cosign("not a checkpoint\n", t(1)).unwrap_err();
    push("SignError", format!("{e} | {e:?}"));
    let e = KeyName::new("secret key").unwrap_err();
    push("NameError", format!("{e} | {e:?}"));
    let witness_v = witness_key(0).verifier(SigKind::Cosignature);
    let log = LogKey {
        origin: origin.clone(),
        verifier: verifier.clone(),
    };
    let e = Policy::new(
        log.clone(),
        vec![k.verifier(SigKind::Cosignature)],
        1,
        Span::from_secs(1),
    )
    .unwrap_err();
    push("PolicyError::WitnessIsLog", format!("{e} | {e:?}"));
    let policy = Policy::new(log.clone(), vec![witness_v.clone()], 1, Span::from_secs(60)).unwrap();
    let e = policy.check(&broken, t(10)).unwrap_err();
    push("PolicyError::BadSignature", format!("{e} | {e:?}"));
    let e = policy.check(&note, t(10)).unwrap_err();
    push("PolicyError::QuorumNotMet", format!("{e} | {e:?}"));
    let mut w = Witness::new(Box::new(witness_key(0)), vec![log]).unwrap();
    let request = AddRequest::new(TreeSize(0), vec![Hash([1; 32])], note.clone()).unwrap();
    let e = w.add_checkpoint(&request, t(1)).unwrap_err();
    push("WitnessRefusal", format!("{e} | {e:?}"));
    push("Witness Debug", format!("{w:?}"));
    let e = AddRequest::parse(b"nonsense").unwrap_err();
    push("RequestError", format!("{e} | {e:?}"));
    let e = Checkpoint::parse_text("a\nb\n").unwrap_err();
    push("CheckpointError", format!("{e} | {e:?}"));
    let e = ReleaseRecord::parse(b"azlin-release-v2\n").unwrap_err();
    push("RecordError", format!("{e} | {e:?}"));
    let e = ProofBundle::parse(b"nonsense\n").unwrap_err();
    push("BundleError", format!("{e} | {e:?}"));
    let mut world = World::new(1);
    world.grow_to(2);
    let e = world
        .log
        .append(&{
            let mut twin = record(1);
            twin.security_fix = !twin.security_fix;
            twin
        })
        .unwrap_err();
    push("LogError", format!("{e} | {e:?}"));
    let monitor = Monitor::new(policy);
    push("Monitor Debug", format!("{monitor:?}"));
    let e = Alarm::NoCheckpoint;
    push("Alarm", format!("{e} | {e:?}"));
    for (what, text) in &shown {
        clean(what, text);
    }
    assert!(shown.len() >= 18);
    // the signature line of a note holds the public signature, never the seed: the whole note is public
    let bytes = String::from_utf8(note.to_bytes()).unwrap();
    clean("a note", &bytes);
}

/// A note text of exactly `n` bytes (a line of `a` and a newline).
fn text_of(n: usize) -> String {
    format!("{}\n", "a".repeat(n - 1))
}

fn one_sig() -> NoteSig {
    sign_checkpoint(
        &Checkpoint::new(Origin::azlin(), TreeSize(1), Hash([3; 32]), vec![]).unwrap(),
        &log_key(),
    )
    .unwrap()
    .sigs()[0]
        .clone()
}

fn many_sigs(n: usize) -> Vec<NoteSig> {
    (0..n)
        .map(|i| {
            NoteSig::new(
                kname(&format!("k{i}.example/key")),
                azlin_tlog::KeyHash::from_bytes([i as u8, 1, 2, 3]),
                vec![i as u8; 64],
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn t35_limits() {
    // a note: the text at most 16 KiB, at most 16 signatures, a signature of at most 4096 bytes
    assert!(Note::new(text_of(MAX_TEXT_BYTES), vec![one_sig()]).is_ok());
    assert_eq!(
        Note::new(text_of(MAX_TEXT_BYTES + 1), vec![one_sig()]),
        Err(NoteError::TooLarge)
    );
    let at_limit = Note::new(text_of(MAX_TEXT_BYTES), vec![one_sig()]).unwrap();
    assert_eq!(Note::parse(&at_limit.to_bytes()).unwrap(), at_limit);
    let over = {
        let mut bytes = at_limit.to_bytes();
        bytes.insert(0, b'a');
        bytes
    };
    assert_eq!(Note::parse(&over), Err(NoteError::TooLarge));
    assert!(Note::new(text_of(10), many_sigs(MAX_SIGNATURES)).is_ok());
    assert_eq!(
        Note::new(text_of(10), many_sigs(MAX_SIGNATURES + 1)),
        Err(NoteError::TooMany)
    );
    let sixteen = Note::new(text_of(10), many_sigs(MAX_SIGNATURES)).unwrap();
    assert_eq!(Note::parse(&sixteen.to_bytes()).unwrap(), sixteen);
    let seventeen = {
        let sigs = many_sigs(MAX_SIGNATURES + 1);
        let mut out = format!("{}\n", text_of(10));
        for sig in &sigs {
            let mut raw = sig.key_hash().as_bytes().to_vec();
            raw.extend_from_slice(sig.payload());
            out.push_str(&format!(
                "\u{2014} {} {}\n",
                sig.name(),
                STANDARD.encode(raw)
            ));
        }
        out.into_bytes()
    };
    assert_eq!(Note::parse(&seventeen), Err(NoteError::TooMany));
    assert!(NoteSig::new(
        kname("a"),
        azlin_tlog::KeyHash::from_bytes([0; 4]),
        vec![1; MAX_SIG_PAYLOAD]
    )
    .is_ok());
    assert_eq!(
        NoteSig::new(
            kname("a"),
            azlin_tlog::KeyHash::from_bytes([0; 4]),
            vec![1; MAX_SIG_PAYLOAD + 1]
        ),
        Err(NoteError::TooLarge)
    );
    assert_eq!(
        Note::parse(&vec![b'a'; MAX_NOTE_BYTES + 1]),
        Err(NoteError::TooLarge)
    );

    // names and origins: 255 bytes
    assert!(KeyName::new(&"n".repeat(MAX_NAME_BYTES)).is_ok());
    assert_eq!(
        KeyName::new(&"n".repeat(MAX_NAME_BYTES + 1)),
        Err(NameError::TooLong)
    );
    assert!(Origin::new(&"o".repeat(MAX_NAME_BYTES)).is_ok());
    assert_eq!(
        Origin::new(&"o".repeat(MAX_NAME_BYTES + 1)),
        Err(NameError::TooLong)
    );

    // a checkpoint: 8 extension lines of 256 bytes, no more; the tree at most 2^32 leaves
    let line = "e".repeat(MAX_EXTENSION_BYTES);
    let ok = |extensions: Vec<String>| {
        Checkpoint::new(Origin::azlin(), TreeSize(5), Hash([1; 32]), extensions)
    };
    assert!(ok(vec![line.clone(); MAX_EXTENSIONS]).is_ok());
    assert_eq!(
        ok(vec![line.clone(); MAX_EXTENSIONS + 1]).err(),
        Some(CheckpointError::TooManyExtensions)
    );
    assert!(ok(vec![line.clone()]).is_ok());
    assert_eq!(
        ok(vec![format!("{line}e")]).err(),
        Some(CheckpointError::ExtensionTooLong)
    );
    assert!(Checkpoint::new(Origin::azlin(), MAX_LEAVES, Hash([1; 32]), vec![]).is_ok());
    assert_eq!(
        Checkpoint::new(
            Origin::azlin(),
            TreeSize(MAX_LEAVES.0 + 1),
            Hash([1; 32]),
            vec![]
        )
        .err(),
        Some(CheckpointError::TooManyLeaves)
    );

    // a record: 4 KiB; a bundle: 64 KiB, 64 hashes of path
    assert_eq!(
        ReleaseRecord::parse(&vec![b'a'; MAX_RECORD_BYTES]),
        Err(RecordError::NoFinalNewline)
    );
    assert_eq!(
        ReleaseRecord::parse(&vec![b'a'; MAX_RECORD_BYTES + 1]),
        Err(RecordError::TooLarge)
    );
    assert_eq!(
        ProofBundle::parse(&vec![b'a'; MAX_BUNDLE_BYTES]),
        Err(BundleError::NoFinalNewline)
    );
    assert_eq!(
        ProofBundle::parse(&vec![b'a'; MAX_BUNDLE_BYTES + 1]),
        Err(BundleError::TooLarge)
    );
    let mut bundle = {
        let mut world = World::new(1);
        world.grow_to(3);
        let note = world.note();
        world.log.bundle(LeafIndex(1), &note).unwrap()
    };
    bundle.proof.path = vec![Hash([1; 32]); 64];
    assert!(bundle.to_text().is_ok());
    bundle.proof.path = vec![Hash([1; 32]); 65];
    assert_eq!(bundle.to_text(), Err(BundleError::TooManyPath));

    // a policy: 16 witnesses
    let witnesses = |n: usize| -> Vec<Verifier> {
        (0..n)
            .map(|i| witness_key(i).verifier(SigKind::Cosignature))
            .collect()
    };
    let log = || LogKey {
        origin: Origin::azlin(),
        verifier: log_key().verifier(SigKind::Ed25519),
    };
    assert!(Policy::new(log(), witnesses(MAX_WITNESSES), 1, Span::from_secs(1)).is_ok());
    assert_eq!(
        Policy::new(log(), witnesses(MAX_WITNESSES + 1), 1, Span::from_secs(1)).err(),
        Some(PolicyError::TooMany)
    );

    // a witness: 64 logs; a request: 63 proof lines, and the request as a whole
    let logs = |n: usize| -> Vec<LogKey> {
        (0..n)
            .map(|i| LogKey {
                origin: Origin::new(&format!("log{i}.example/v1")).unwrap(),
                verifier: log_key().verifier(SigKind::Ed25519),
            })
            .collect()
    };
    assert!(Witness::new(Box::new(witness_key(0)), logs(MAX_ORIGINS)).is_ok());
    assert_eq!(
        Witness::new(Box::new(witness_key(0)), logs(MAX_ORIGINS + 1)).err(),
        Some(WitnessError::TooMany)
    );
    let note = Note::new(text_of(10), vec![one_sig()]).unwrap();
    assert!(AddRequest::new(
        TreeSize(0),
        vec![Hash([1; 32]); MAX_PROOF_LINES],
        note.clone()
    )
    .is_ok());
    assert_eq!(
        AddRequest::new(TreeSize(0), vec![Hash([1; 32]); MAX_PROOF_LINES + 1], note).err(),
        Some(RequestError::TooMany)
    );
    assert_eq!(
        AddRequest::parse(&vec![b'a'; MAX_REQUEST_BYTES + 1]),
        Err(RequestError::TooLarge)
    );
    assert_ne!(
        AddRequest::parse(&vec![b'a'; MAX_REQUEST_BYTES]),
        Err(RequestError::TooLarge)
    );

    // the monitor's index: 1,000,000 records by default, a refusal and never an eviction one above the limit
    assert_eq!(MAX_INDEX_RECORDS, TreeSize(1_000_000));
    let mut world = World::new(1);
    world.grow_to(6);
    let note6 = world.note();
    let note6 = world.cosigned(&note6, &[(0, 10)]);
    let records6: Vec<Vec<u8>> = (0..6).map(|i| record(i).to_bytes()).collect();
    let mut at_limit = Monitor::with_limit(world.policy(1), TreeSize(6));
    at_limit.observe(&note6, t(100), None).unwrap();
    at_limit.ingest(&records6).unwrap();
    assert_eq!(at_limit.indexed(), Some(TreeSize(6)));
    let mut over_limit = Monitor::with_limit(world.policy(1), TreeSize(5));
    over_limit.observe(&note6, t(100), None).unwrap();
    assert_eq!(over_limit.ingest(&records6), Err(Alarm::TooMany));
    assert_eq!(over_limit.indexed(), None);
}
