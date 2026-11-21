//! The plan of a transfer (ARCH28 RF43 / AR5a-2): the names the target holds and the names the
//! plan itself reserves, so that nothing in a batch replaces another file of it.

use std::collections::BTreeSet;

use super::TempDir;
use crate::{
    names,
    xfer::{plan, volume_of, Choice, Item, TargetSet},
    Drive, LocalDrive,
};

fn file(key: &str) -> Item {
    Item {
        key: key.to_string(),
        is_folder: false,
        size: None,
    }
}

fn folder(key: &str) -> Item {
    Item {
        key: key.to_string(),
        is_folder: true,
        size: None,
    }
}

/// The disk's own idea of "one name": the keys as the target's disk compares them.
fn folded(tmp: &TempDir, keys: impl IntoIterator<Item = String>) -> Vec<String> {
    let volume = volume_of(tmp.path());
    keys.into_iter().map(|k| names::fold(&k, &volume)).collect()
}

/// AZ-U3: a search for "invoice.pdf" finds twelve files in twelve folders; Cut, Paste in
/// Invoices/. Each had the same target and none was a conflict: each copy renamed over the last
/// and all twelve sources were deleted.
#[test]
fn az_u3_twelve_files_of_one_name_in_one_batch_get_twelve_names() {
    let tmp = TempDir::new("u3");
    let home = LocalDrive::new(tmp.path().join("home"));
    let mut items = Vec::new();
    for i in 0..12 {
        home.put(&format!("client{i}/invoice.pdf"), format!("{i}").as_bytes())
            .unwrap();
        items.push(file(&format!("client{i}/invoice.pdf")));
    }
    let other = LocalDrive::without_manifest(tmp.path().join("other"));
    other.create_folder("Invoices/").unwrap();
    for removes_source in [false, true] {
        let plan = plan(&home, &items, &other, "Invoices/", removes_source, "Copy").unwrap();
        assert_eq!(plan.files.len(), 12);
        assert!(
            plan.conflicts().is_empty(),
            "nothing is at the target yet: no conflict to ask about"
        );
        let keys: Vec<String> = plan.files.iter().map(|f| f.target_key.clone()).collect();
        assert_eq!(keys[0], "Invoices/invoice.pdf", "the first keeps its name");
        assert_eq!(keys[1], "Invoices/invoice (2).pdf");
        assert_eq!(keys[11], "Invoices/invoice (12).pdf");
        let distinct: BTreeSet<String> = folded(&tmp, keys.clone()).into_iter().collect();
        assert_eq!(
            distinct.len(),
            12,
            "twelve names as the disk compares them: {keys:?}"
        );
        assert_eq!(
            plan.tops, keys,
            "what is selected afterwards are the names they got"
        );
    }
}

/// The same batch inside one drive: a rename in place each, each to a name of its own.
#[test]
fn az_u3_a_move_within_one_drive_gives_each_file_of_one_name_its_own_target() {
    let tmp = TempDir::new("u3-moves");
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    for dir in ["a", "b", "c"] {
        home.put(&format!("{dir}/x.txt"), dir.as_bytes()).unwrap();
    }
    home.create_folder("all/").unwrap();
    let items = [file("a/x.txt"), file("b/x.txt"), file("c/x.txt")];
    let plan = plan(&home, &items, &home, "all/", true, "Copy").unwrap();
    assert!(plan.same_drive);
    assert_eq!(
        plan.moves,
        vec![
            (String::from("a/x.txt"), String::from("all/x.txt")),
            (String::from("b/x.txt"), String::from("all/x (2).txt")),
            (String::from("c/x.txt"), String::from("all/x (3).txt")),
        ]
    );
    assert!(plan.files.is_empty());
}

/// X-AR5a-11: "Keep both" took the first free name among the CONFLICTS, so `a (2).txt` was chosen
/// for `a.txt` while another file of the batch, `a (2).txt`, was planned for that very name: the
/// second replaced the first.
#[test]
fn x_ar5a_11_keep_both_never_takes_a_name_a_file_of_the_batch_has() {
    let tmp = TempDir::new("x11");
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    home.put("s/a.txt", b"new a").unwrap();
    home.put("s/a (2).txt", b"new a2").unwrap();
    let other = LocalDrive::without_manifest(tmp.path().join("other"));
    other.put("t/a.txt", b"old a").unwrap();
    let items = [file("s/a.txt"), file("s/a (2).txt")];
    let mut plan = plan(&home, &items, &other, "t/", false, "Copy").unwrap();
    assert_eq!(plan.conflicts(), vec![0], "only a.txt is there already");
    let kept = plan.files[0].keep_as.clone().expect("a name for Keep both");
    assert_eq!(kept, "t/a (3).txt", "a (2).txt is a file of this batch");
    plan.choose(0, Choice::KeepBoth);
    let keys: BTreeSet<&str> = plan.files.iter().map(|f| f.target_key.as_str()).collect();
    assert_eq!(keys.len(), 2, "two files, two names: {keys:?}");
    assert_eq!(plan.files[0].target_key, "t/a (3).txt");
    assert_eq!(plan.files[1].target_key, "t/a (2).txt");
    assert_eq!(
        plan.tops[0], "t/a (3).txt",
        "the selection follows the new name"
    );
}

/// A name that something at the target holds is not taken by Keep both either: the plan asked the
/// target, not only its own list.
#[test]
fn x_ar5a_11_keep_both_never_takes_a_name_the_target_has() {
    let tmp = TempDir::new("x11-target");
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    home.put("a.txt", b"new").unwrap();
    let other = LocalDrive::without_manifest(tmp.path().join("other"));
    other.put("a.txt", b"old").unwrap();
    other.put("a (2).txt", b"older").unwrap();
    let mut plan = plan(&home, &[file("a.txt")], &other, "", false, "Copy").unwrap();
    assert_eq!(plan.conflicts(), vec![0]);
    plan.choose(0, Choice::KeepBoth);
    assert_eq!(plan.files[0].target_key, "a (3).txt");
}

/// AZ-W5: the folder copy kept the names it saw as the disk spells them, so on APFS and NTFS
/// `Report.txt` landed on `report.txt` without a question. Whatever this disk does with case, the
/// plan's conflicts and the disk agree.
#[test]
fn az_w5_a_name_the_disk_reads_as_another_is_a_conflict() {
    let tmp = TempDir::new("w5");
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    home.put("pics/Report.txt", b"new").unwrap();
    let other = LocalDrive::without_manifest(tmp.path().join("other"));
    other.put("pics/report.txt", b"old").unwrap();
    // Does this disk take the two names for one file?
    let one_file = tmp
        .path()
        .join("other")
        .join("pics")
        .join("Report.txt")
        .exists();
    let plan = plan(&home, &[folder("pics/")], &other, "", false, "Copy").unwrap();
    assert_eq!(plan.files.len(), 1);
    assert_eq!(
        plan.files[0].conflict, one_file,
        "a conflict exactly when the disk reads Report.txt as report.txt"
    );
}

/// Two names of one batch that a disk takes for one.
#[test]
fn az_w5_two_files_of_a_batch_that_the_disk_reads_as_one_get_two_names() {
    let tmp = TempDir::new("w5-batch");
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    home.put("x/Report.txt", b"one").unwrap();
    home.put("y/report.txt", b"two").unwrap();
    let other = LocalDrive::without_manifest(tmp.path().join("other"));
    other.create_folder("in/").unwrap();
    let plan = plan(
        &home,
        &[file("x/Report.txt"), file("y/report.txt")],
        &other,
        "in/",
        false,
        "Copy",
    )
    .unwrap();
    let keys: Vec<String> = plan.files.iter().map(|f| f.target_key.clone()).collect();
    let distinct: BTreeSet<String> = folded(&tmp, keys.clone()).into_iter().collect();
    assert_eq!(
        distinct.len(),
        2,
        "two files, two names as this disk compares them: {keys:?}"
    );
}

/// The set itself: a fold per volume, planned and existing apart, a trailing slash no
/// difference on a disk.
#[test]
fn a_target_set_folds_as_its_volume_does() {
    let strong = names::Volume {
        case_insensitive: true,
        norm_insensitive: true,
        ..names::Volume::default()
    };
    let mut set = TargetSet::new(strong, true);
    set.note_existing("in/report.txt");
    assert!(set.is_existing("in/REPORT.txt"));
    assert!(set.is_taken("in/Report.txt"));
    set.reserve("in/Docs/");
    assert!(
        set.is_taken("in/docs"),
        "a file and a folder of one name are one name on a disk"
    );
    let mut exact = TargetSet::new(names::Volume::default(), false);
    exact.note_existing("in/report.txt");
    assert!(
        !exact.is_taken("in/Report.txt"),
        "names in a bucket are exact"
    );
    exact.reserve("in/docs/");
    assert!(
        !exact.is_taken("in/docs"),
        "a file and a folder are two keys there"
    );
    // Keep both: the first free other name, from the whole set, folder keys keep their slash.
    set.note_existing("in/a (2).txt");
    assert_eq!(set.free_key("in/a.txt", &mut |_| false), "in/a (3).txt");
    assert_eq!(set.free_key("in/photos/", &mut |_| false), "in/photos (2)/");
    assert_eq!(
        set.free_key("in/a.txt", &mut |candidate| candidate == "in/a (3).txt"),
        "in/a (4).txt",
        "what the target holds that the set was not told of is asked for"
    );
}

/// A folder that goes into a folder of itself through another drive over the same tree is refused,
/// and a folder moved onto its own place does nothing (the planner asks the sites).
#[test]
fn a_folder_cannot_go_into_itself_and_a_move_onto_itself_is_nothing() {
    let tmp = TempDir::new("into-itself");
    let home = LocalDrive::new(tmp.path().join("home"));
    home.put("docs/sub/b.txt", b"beta").unwrap();
    let again = LocalDrive::new(tmp.path().join("home"));
    assert!(plan(&home, &[folder("docs/")], &again, "docs/sub/", true, "Copy").is_err());
    assert!(plan(
        &home,
        &[folder("docs/")],
        &again,
        "docs/sub/",
        false,
        "Copy"
    )
    .is_err());
    let nothing = plan(&home, &[folder("docs/")], &again, "", true, "Copy").unwrap();
    assert!(nothing.is_empty(), "docs/ is already where it would land");
    let copy = plan(&home, &[folder("docs/")], &again, "", false, "Copy").unwrap();
    assert_eq!(copy.tops, vec![String::from("docs - Copy/")]);
}

/// X-DELTA-AZDRIVE-FILES-32: a letter that changes into two (`ß` is `SS` in capitals) is no probe of
/// a disk's case: its "other spelling" is not the one a disk that ignores case looks up, and
/// `Straße` answered "this disk tells case apart".
#[test]
fn x_delta_azdrive_files_32_a_letter_that_changes_into_two_is_no_probe_of_the_disks_case() {
    use crate::xfer::targets::swapped_case;
    assert_eq!(swapped_case("Report.txt"), Some(String::from("rEPORT.TXT")));
    assert_eq!(swapped_case("12345"), None, "no letter to swap");
    assert_eq!(swapped_case("Straße"), None);
    assert_eq!(swapped_case("ǆ").as_deref(), Some("Ǆ"));
}

/// X-DELTA-AZDRIVE-FILES-32: the name of a mount point is looked up in the folder above it, which
/// is another disk (an exFAT stick under `/Volumes` answered for `/Volumes`' disk). The disk is then
/// asked through a name inside it - and the answer is the one a name of the folder itself gives.
#[test]
fn x_delta_azdrive_files_32_a_disk_is_asked_through_a_name_inside_it() {
    use crate::xfer::targets::{folds_case, folds_case_inside};
    let tmp = TempDir::new("dx32-inside");
    std::fs::write(tmp.path().join("Probe.txt"), b"x").unwrap();
    let inside = folds_case_inside(tmp.path());
    assert!(inside.is_some(), "a file with letters is a probe");
    assert_eq!(inside, folds_case(&tmp.path().join("Probe.txt")));
    // An empty folder has nothing to ask through.
    let empty = TempDir::new("dx32-inside-empty");
    assert_eq!(folds_case_inside(empty.path()), None);
}
