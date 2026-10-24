//! The name policy of a synced folder: what can never be a file, what one system cannot hold,
//! what misleads a reader, and how a disk folds names.

use crate::sync::names::{
    display_risks, escaped, fold, here_problem, key_problem, nfc, portability, segment_problem,
    windows_problem, System, Volume,
};

#[test]
fn a_name_that_leaves_the_folder_or_names_its_bookkeeping_is_no_file_anywhere() {
    for bad in [
        "",
        "/etc/passwd",
        "//server/share/x",
        "\\\\server\\share",
        "C:x",
        "c:/Windows/x",
        "a/../b",
        "./a",
        "a//b",
        "a/",
        "a\\b",
        "nul\0byte",
        "bell\u{7}",
        "del\u{7f}",
        "c1\u{85}",
        "non\u{fdd0}char",
        "end\u{fffe}",
        ".azlin/sync/root-id",
        ".azlin",
    ] {
        assert!(key_problem(bad).is_some(), "{bad:?}");
    }
    assert!(
        key_problem(&"n".repeat(256)).is_some(),
        "a name over 255 bytes"
    );
    for fine in [
        "a.txt",
        "dir/sub/a b.txt",
        "résumé.pdf",
        "x/.azlin/y",
        "日本語.txt",
    ] {
        assert_eq!(key_problem(fine), None, "{fine:?}");
    }
    assert_eq!(
        segment_problem(".."),
        Some(String::from(
            "a \"..\" in its path climbs out of its folder"
        ))
    );
}

#[test]
fn windows_refuses_its_characters_device_names_trailing_dots_and_long_paths() {
    for bad in [
        "a:b.txt",
        "dir/C:x",
        "q?.txt",
        "star*.txt",
        "pipe|.txt",
        "quote\".txt",
        "lt<.txt",
        "CON",
        "con.txt",
        "nul.tar.gz",
        "Aux",
        "COM1.log",
        "lpt9",
        "COM\u{b9}",
        "dot.",
        "space ",
        "dir./x",
    ] {
        assert!(windows_problem(bad).is_some(), "{bad:?}");
    }
    assert!(windows_problem(&format!("{}/x.txt", "d".repeat(230))).is_some());
    for fine in ["console.txt", "COM10", "conx", "a.b.c", "LPT"] {
        assert_eq!(windows_problem(fine), None, "{fine:?}");
    }
}

#[test]
fn a_name_valid_here_and_not_elsewhere_is_named_for_the_systems_that_cannot_hold_it() {
    let warned = portability("a:b.txt", System::Linux);
    assert_eq!(warned.len(), 1);
    assert_eq!(warned[0].0, System::Windows);
    assert!(portability("plain.txt", System::Linux).is_empty());
    // On Windows itself it is invalid here, not a warning.
    assert!(here_problem("a:b.txt", System::Windows, &Volume::default()).is_some());
    assert!(here_problem("a:b.txt", System::Linux, &Volume::default()).is_none());
    // A FAT or exFAT disk refuses the same names on any system.
    let fat = Volume {
        windows_names: true,
        ..Volume::default()
    };
    assert!(here_problem("a:b.txt", System::Macos, &fat).is_some());
}

#[test]
fn names_that_mislead_a_reader_are_flagged_and_always_shown_escaped() {
    let reversed = "invoice\u{202e}fdp.exe";
    assert!(!display_risks(reversed).is_empty());
    assert_eq!(escaped(reversed), "invoice<U+202E>fdp.exe");
    assert!(
        !display_risks("pay\u{200b}pal.txt").is_empty(),
        "an invisible character"
    );
    assert!(
        !display_risks("invoice\u{2024}pdf").is_empty(),
        "a look-alike dot"
    );
    assert!(
        !display_risks("p\u{430}ypal.txt").is_empty(),
        "latin and cyrillic in one name"
    );
    assert!(!display_risks(" lead.txt").is_empty());
    assert!(display_risks("plain.txt").is_empty());
    assert!(display_risks("Москва.txt").is_empty(), "one script is fine");
    assert_eq!(escaped("tab\there"), "tab<U+0009>here");
}

#[test]
fn a_disk_that_ignores_case_or_normalization_folds_names_together() {
    let composed = "caf\u{e9}.txt";
    let decomposed = "cafe\u{301}.txt";
    assert_ne!(composed, decomposed);
    assert_eq!(nfc(decomposed), composed);
    let apfs = Volume {
        case_insensitive: true,
        norm_insensitive: true,
        windows_names: false,
    };
    assert_eq!(fold(decomposed, &apfs), fold(composed, &apfs));
    assert_eq!(fold("Report.TXT", &apfs), fold("report.txt", &apfs));
    let ext4 = Volume::default();
    assert_ne!(fold(decomposed, &ext4), fold(composed, &ext4));
    assert_ne!(fold("Report.txt", &ext4), fold("report.txt", &ext4));
    let casefold_ext4 = Volume {
        case_insensitive: true,
        ..Volume::default()
    };
    assert_eq!(
        fold("Report.txt", &casefold_ext4),
        fold("report.txt", &casefold_ext4)
    );
}
