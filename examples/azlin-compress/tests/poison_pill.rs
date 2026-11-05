//! The poison pill (ARCH28 RF50 / AR5b-5): `panic = "abort"` makes a panic inside the analysis of a
//! file the end of the program, and the next pass reads the same file again. The analysis leaves a
//! marker (the file's BLAKE3) while it runs; a marker found when it is about to start means the
//! last one did not come back, and the file is stored as it is.

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

    // The marker of an analysis that ended the program: the file is stored as it is.
    let marker = dir.join(format!("{}.analysing", blake3::hash(BASELINE).to_hex()));
    std::fs::write(&marker, b"analysing").unwrap();
    match transform::forward(BASELINE, Kind::Jpeg, &options) {
        transform::Forward::NotApplied(why) => {
            assert!(why.contains("did not come back"), "{why}");
        }
        transform::Forward::Applied(_) => panic!("a file with a marker is analysed again"),
    }
    assert!(
        marker.exists(),
        "the marker stays: the file is out of the analysis from now on"
    );

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
