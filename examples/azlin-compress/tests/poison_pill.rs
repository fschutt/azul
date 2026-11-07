//! The poison pill (ARCH28 RF50 / AR5b-5): `panic = "abort"` makes a panic inside the analysis of a
//! file the end of the program, and the next pass reads the same file again. The analysis leaves a
//! marker (the file's BLAKE3, with a count) while it runs; a marker found when it is about to
//! start means the last one did not come back, and a file whose analyses did not come back three
//! times is stored as it is (a quit or a kill is one try and no verdict, X-DELTA-AZDRIVE-KEYS-44).

use azlin_compress::{transform, Kind, Options, TransformId};

const BASELINE: &[u8] = include_bytes!("fixtures/baseline.jpg");

fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("azlin-poison-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn x_ar5b_5_a_file_whose_analysis_did_not_come_back_is_not_analysed_again() {
    let dir = folder("pill");
    let options = Options {
        threads: 1,
        ..Options::idle()
    }
    .with_poison_dir(Some(dir.clone()));

    // An analysis that returns leaves no marker, and the transform applies.
    let transform::Forward::Applied(t) = transform::forward(BASELINE, Kind::Jpeg, &options) else {
        panic!("the transform applies");
    };
    assert_eq!(t.id, TransformId::Jpeg);
    let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    assert!(
        left.is_empty(),
        "no marker after an analysis that returned: {left:?}"
    );

    // The marker of analyses that ended the program (three tries on record): the file is stored
    // as it is.
    let marker = dir.join(format!("{}.analysing", blake3::hash(BASELINE).to_hex()));
    std::fs::write(&marker, b"analysing 3\n").unwrap();
    match transform::forward(BASELINE, Kind::Jpeg, &options) {
        transform::Forward::NotApplied(why) => {
            assert!(why.contains("ended the program"), "{why}");
        }
        transform::Forward::Applied(_) => {
            panic!("a file with three tries on record is analysed again")
        }
    }
    assert!(
        marker.exists(),
        "the marker stays: the file is out of the analysis from now on"
    );

    // X-DELTA-AZDRIVE-KEYS-44: one try on record - a quit, a kill, a shutdown during the
    // analysis - is no verdict: the file is analysed again, and the analysis that comes back
    // erases the record. (The old build marked a file for ever at the first.) So is the record
    // the first build wrote, which held no count.
    for record in [&b"analysing 1\n"[..], &b"analysing"[..]] {
        std::fs::write(&marker, record).unwrap();
        assert!(
            matches!(
                transform::forward(BASELINE, Kind::Jpeg, &options),
                transform::Forward::Applied(_)
            ),
            "a file that was only interrupted is analysed again"
        );
        assert!(!marker.exists(), "it came back: the record goes");
    }

    // Another file is not affected.
    assert!(matches!(
        transform::forward(&[0xffu8; 10], Kind::Binary, &options),
        transform::Forward::NotApplied(_)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn x_ar5b_5_without_a_folder_there_is_no_marker_and_the_analysis_runs() {
    let options = Options {
        threads: 1,
        ..Options::idle()
    };
    assert!(options.poison_dir.is_none());
    assert!(matches!(
        transform::forward(BASELINE, Kind::Jpeg, &options),
        transform::Forward::Applied(_)
    ));
}

// ==== X-DELTA-AZDRIVE-KEYS-15: the READ path has a pill too ====

/// The marker the decode of `stream` leaves while it runs.
fn decoding_marker(dir: &std::path::Path, stream: &[u8]) -> std::path::PathBuf {
    dir.join(format!("{}.decoding", blake3::hash(stream).to_hex()))
}

/// X-DELTA-AZDRIVE-KEYS-15: Lepton's decoder and the preflate rebuild run over bytes any member of
/// the drive wrote, `catch_unwind` around them is dead under `panic = "abort"`, and the pill
/// bracketed the ENCODE only: a malformed stream ended every device that read it, the sync read it
/// again after each restart. A decode leaves its marker (with a count); one that returned leaves
/// none; a stream that has ended the program three times is refused by name.
#[test]
fn x_delta_azdrive_keys_15_a_stream_whose_decode_did_not_come_back_is_not_decoded_again() {
    let dir = folder("decode");
    let analysis = Options {
        threads: 1,
        ..Options::idle()
    };
    let transform::Forward::Applied(t) = transform::forward(BASELINE, Kind::Jpeg, &analysis) else {
        panic!("the transform applies");
    };
    let limits = Options::default().limits;
    let marker = decoding_marker(&dir, &t.bytes);

    // A decode that returns leaves no marker, and the file comes back bit for bit.
    let mut out = Vec::new();
    transform::inverse_guarded(
        TransformId::Jpeg,
        &t.bytes,
        &mut out,
        &limits,
        1,
        Some(&dir),
    )
    .expect("the stream decodes");
    assert_eq!(out, BASELINE);
    assert!(!marker.exists(), "no marker after a decode that returned");

    // Two decodes that did not come back (the process ended inside them) are counted, and the
    // third try is still let in - an app that was only quit meanwhile reads its file - and clears it.
    std::fs::write(&marker, b"decoding 2\n").unwrap();
    let mut out = Vec::new();
    transform::inverse_guarded(
        TransformId::Jpeg,
        &t.bytes,
        &mut out,
        &limits,
        1,
        Some(&dir),
    )
    .expect("the third try is let in");
    assert_eq!(out, BASELINE);
    assert!(!marker.exists(), "a decode that came back clears the count");

    // Three that did not: refused by name, nothing decoded, the marker stays.
    std::fs::write(&marker, b"decoding 3\n").unwrap();
    let mut out = Vec::new();
    let refused = transform::inverse_guarded(
        TransformId::Jpeg,
        &t.bytes,
        &mut out,
        &limits,
        1,
        Some(&dir),
    );
    assert!(
        matches!(&refused, Err(azlin_compress::Error::Unsupported(why)) if why.contains("ended the program")),
        "{refused:?}"
    );
    assert!(out.is_empty(), "the decoder was not entered");
    assert!(marker.exists());

    // No folder, no marker; no transform, no marker.
    let mut out = Vec::new();
    transform::inverse_guarded(TransformId::Jpeg, &t.bytes, &mut out, &limits, 1, None).unwrap();
    assert_eq!(out, BASELINE);
    let mut copied = Vec::new();
    transform::inverse_guarded(
        TransformId::None,
        b"plain",
        &mut copied,
        &limits,
        1,
        Some(&dir),
    )
    .unwrap();
    assert_eq!(copied, b"plain");
    assert!(!decoding_marker(&dir, b"plain").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// X-DELTA-AZDRIVE-KEYS-15: the pack members are containers decoded by `decode`; the same pill
/// brackets them when the options name a folder.
#[test]
fn x_delta_azdrive_keys_15_a_container_with_a_transform_is_decoded_under_the_pill_too() {
    let dir = folder("decode-container");
    let options = Options {
        threads: 1,
        ..Options::idle()
    }
    .with_poison_dir(Some(dir.clone()));
    let encoded = azlin_compress::encode(BASELINE, &options).expect("encodes");
    assert_ne!(
        encoded.report.transform,
        TransformId::None,
        "the JPEG is transformed"
    );
    let container = encoded.bytes.to_vec();

    let plain = azlin_compress::decode(&container, &options).expect("decodes");
    assert_eq!(&plain[..], BASELINE);
    assert!(
        !decoding_marker(&dir, &container).exists(),
        "no marker after a decode that returned"
    );

    std::fs::write(decoding_marker(&dir, &container), b"decoding 3\n").unwrap();
    let refused = azlin_compress::decode(&container, &options);
    assert!(
        matches!(&refused, Err(azlin_compress::Error::Unsupported(why)) if why.contains("ended the program")),
        "{refused:?}"
    );

    // A container without a transform enters no decoder and takes no marker.
    let stored =
        azlin_compress::encode(b"just some text, no transform", &options).expect("encodes");
    let text = azlin_compress::decode(&stored.bytes, &options).expect("decodes");
    assert_eq!(&text[..], b"just some text, no transform");
    assert!(!decoding_marker(&dir, &stored.bytes).exists());
    let _ = std::fs::remove_dir_all(&dir);
}
