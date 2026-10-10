//! A drive's space: stored bytes against the quota, the original size as extra information.

use crate::usage::{size_text, Level, Usage};

const GB: u64 = 1_000_000_000;

/// The user's example: the quota counts what arrives - the compressed bytes.
#[test]
fn the_usage_line_counts_stored_bytes_and_shows_the_original_size_as_extra_information() {
    let usage = Usage {
        used: 62 * GB,
        quota: 100 * GB,
        original: Some(99 * GB),
    };
    assert_eq!(
        usage.text(),
        "62 GB used of 100 GB, your files are 99 GB before compression"
    );
    assert_eq!(usage.available(), 38 * GB);
    // Without the original size (not known yet), or when nothing compressed: stored only.
    let stored_only = Usage {
        original: None,
        ..usage
    };
    assert_eq!(stored_only.text(), "62 GB used of 100 GB");
    let incompressible = Usage {
        original: Some(62 * GB),
        ..usage
    };
    assert_eq!(incompressible.text(), "62 GB used of 100 GB");
}

#[test]
fn a_drive_is_nearly_full_by_its_stored_bytes_not_its_files_size() {
    // 120 GB of files that compress to 80 GB: not nearly full.
    let fine = Usage {
        used: 80 * GB,
        quota: 100 * GB,
        original: Some(120 * GB),
    };
    assert_eq!(fine.level(), Level::Fine);
    assert_eq!(fine.warning(), None);
    let nearly = Usage {
        used: 93 * GB,
        ..fine
    };
    assert_eq!(nearly.level(), Level::NearlyFull);
    let warning = nearly.warning().unwrap();
    assert!(warning.contains("93 GB of 100 GB"), "{warning}");
    let full = Usage {
        used: 100 * GB,
        ..fine
    };
    assert_eq!(full.level(), Level::Full);
    assert_eq!(full.available(), 0);
    assert!(full.warning().unwrap().contains("full"));
}

#[test]
fn sizes_read_as_the_tiers_are_sold() {
    assert_eq!(size_text(512 * 1_000_000), "512 MB");
    assert_eq!(size_text(1_500_000_000), "1.5 GB");
    assert_eq!(size_text(62 * GB), "62 GB");
    assert_eq!(size_text(1_000 * GB), "1 TB");
    assert_eq!(size_text(1_600 * GB), "1.6 TB");
}
