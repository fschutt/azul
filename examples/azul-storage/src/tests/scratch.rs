//! Scratch (ARCH28 RF48 / AR5a-8; AZ-C6, AZ-W4, AZ-U14, X-AR5a-23): what a program keeps for a
//! while and nobody asked for lives in one private place, under names that say which process
//! made them, and what a killed program left is swept - never in the system's shared temp folder
//! under a fixed name with the default mode.

use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use super::TempDir;
use crate::scratch::{self, Scratch};

/// A pid no process has (`kill(pid, 0)` says "no such process" on every system we run on).
#[cfg(unix)]
const DEAD_PID: u32 = i32::MAX as u32;

/// The horizon the sweep tests give: one day.
#[cfg(unix)]
const DAY: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// `path` (a file, or a folder) made `days` old.
#[cfg(unix)]
fn age(path: &Path, days: u32) {
    let when = std::time::SystemTime::now() - DAY * days;
    fs::File::open(path).unwrap().set_modified(when).unwrap();
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(read) => read
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

/// A file of the module's naming (`<purpose>-<pid>-<run>-<n>`) in `dir`, as a process with that
/// pid and run id made it. (The sweep tests that plant one ask whether a process runs, which only
/// Unix can be asked.)
#[cfg(unix)]
fn planted(dir: &Path, purpose: &str, pid: u32, run: u64, n: u64) -> String {
    let name = format!("{purpose}-{pid}-{run:x}-{n}");
    fs::write(dir.join(&name), b"left behind").unwrap();
    name
}

/// AZ-C6: a streamed encrypted write above its memory limit went to
/// `<system temp>/azul-storage-<hex>.azl1-spool` - a shared folder, the default mode, a kill
/// left it for good. It is a scratch file now: private, named after its process, gone with the
/// spool.
#[cfg(feature = "encryption")]
#[test]
fn az_c6_a_spool_above_its_memory_limit_is_a_private_scratch_file_that_goes_with_it() {
    use crate::encrypted::Spool;

    let root = TempDir::new("scratch-spool");
    let scratch = Scratch::under(root.path(), "spool");
    let data: Vec<u8> = (0..(9u32 << 20)).map(|i| (i % 251) as u8).collect();

    // Below the limit: in memory, no folder made.
    let mut small = Spool::in_scratch(scratch.clone());
    small.write_all(&data[..1 << 20]).unwrap();
    assert!(!small.is_in_file());
    assert!(
        names(scratch.dir()).is_empty(),
        "a small spool touches no disk"
    );
    drop(small);

    // Above it: the first 8 MiB move to a file, the rest follows, and it reads back whole.
    let mut spool = Spool::in_scratch(scratch.clone());
    spool.write_all(&data).unwrap();
    assert!(spool.is_in_file());
    let held = names(scratch.dir());
    assert_eq!(held.len(), 1, "{held:?}");
    assert!(
        held[0].starts_with(&format!("spool-{}-", std::process::id())),
        "{held:?}"
    );
    #[cfg(unix)]
    {
        assert_eq!(mode_of(&scratch.dir().join(&held[0])), 0o600);
        assert_eq!(mode_of(scratch.dir()), 0o700);
        assert_eq!(
            mode_of(root.path()),
            0o700,
            "the root is closed to the others, too"
        );
    }
    spool.seek(SeekFrom::Start(0)).unwrap();
    let mut back = Vec::new();
    spool.read_to_end(&mut back).unwrap();
    assert!(back == data, "the spool reads back what was written");

    drop(spool);
    assert!(
        names(scratch.dir()).is_empty(),
        "a dropped spool leaves no file: {:?}",
        names(scratch.dir())
    );
}

/// AZ-W4 / AZ-U14: the folder of a purpose is `rwx------` and checked on every use. A 0755
/// folder somebody (an older build, a umask) made is closed; a link or a file at its name is
/// refused - the old `/tmp/AzDrive-open/<drive>` followed a link another user had planted.
#[cfg(unix)]
#[test]
fn az_w4_scratch_folders_are_private_and_a_link_or_a_file_in_their_place_is_refused() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let base = TempDir::new("scratch-private");
    let root = base.path().join("root");

    // Made: the root and the purpose, both closed to everybody else.
    let open = Scratch::under(&root, "open");
    open.ensure().unwrap();
    assert_eq!(mode_of(&root), 0o700);
    assert_eq!(mode_of(open.dir()), 0o700);

    // A folder others can enter is closed when it is ours.
    let loose = Scratch::under(&root, "loose");
    fs::create_dir(loose.dir()).unwrap();
    fs::set_permissions(loose.dir(), fs::Permissions::from_mode(0o755)).unwrap();
    loose.ensure().unwrap();
    assert_eq!(mode_of(loose.dir()), 0o700);

    // A link at the purpose's name is not followed, whatever it points to.
    let target = base.path().join("elsewhere");
    fs::create_dir(&target).unwrap();
    let linked = Scratch::under(&root, "linked");
    symlink(&target, linked.dir()).unwrap();
    let refused = linked.ensure().unwrap_err();
    assert_eq!(
        refused.kind(),
        std::io::ErrorKind::PermissionDenied,
        "{refused}"
    );
    assert!(
        linked.file().is_err(),
        "no file is made through a link: {:?}",
        names(&target)
    );
    assert!(names(&target).is_empty(), "nothing went through the link");

    // A file at the name of the root is no folder.
    let blocked = base.path().join("blocked");
    fs::write(&blocked, b"a file").unwrap();
    let under_file = Scratch::under(&blocked, "open");
    assert!(under_file.ensure().is_err());
}

/// X-AR5a-23: a program that has no cache folder gets one folder per user in the temp folder,
/// not the temp folder itself, with a name the user's number makes unlike another user's.
#[test]
fn x_ar5a_23_without_a_cache_folder_scratch_is_one_private_folder_per_user() {
    let root = scratch::root();
    assert!(root.starts_with(std::env::temp_dir()), "{root:?}");
    let leaf = root.file_name().unwrap().to_string_lossy().into_owned();
    assert!(leaf.starts_with("azul-scratch-"), "{leaf}");
    assert_ne!(root, std::env::temp_dir(), "never the shared folder itself");
}

/// AZ-C6 / U15: every file is new and has its own name, `<purpose>-<pid>-<run>-<n>`; it is
/// readable and writable by its maker, and gone when it drops.
#[test]
fn az_c6_a_scratch_file_is_new_private_named_after_its_process_and_removed_on_drop() {
    let root = TempDir::new("scratch-file");
    let scratch = Scratch::under(root.path(), "Spool");
    assert!(
        scratch.dir().ends_with("spool"),
        "the purpose is a lower-case word"
    );

    let mut a = scratch.file().unwrap();
    let mut b = scratch.file().unwrap();
    assert_ne!(a.path(), b.path());
    a.write_all(b"ciphertext").unwrap();
    b.write_all(b"more").unwrap();
    assert_eq!(a.len().unwrap(), 10);
    assert!(!b.is_empty().unwrap());
    a.seek(SeekFrom::Start(0)).unwrap();
    let mut back = String::new();
    a.read_to_string(&mut back).unwrap();
    assert_eq!(back, "ciphertext");

    let held = names(scratch.dir());
    assert_eq!(held.len(), 2);
    for name in &held {
        assert!(
            name.starts_with(&format!("spool-{}-", std::process::id())),
            "{name}"
        );
    }
    #[cfg(unix)]
    assert_eq!(mode_of(&a.path()), 0o600);

    let gone = a.path();
    drop(a);
    assert!(!gone.exists());
    assert_eq!(names(scratch.dir()).len(), 1);
    drop(b);
    assert!(names(scratch.dir()).is_empty());
}

/// X-AR5a-23 / S21 / U15: a sweep removes what a process that is gone left and what is old, and
/// nothing else: not the file of this run (its owner removes it), not another purpose's, not a
/// name that is not the module's, not a young file of a process that runs.
#[cfg(unix)]
#[test]
fn x_ar5a_23_a_sweep_removes_a_dead_processs_files_and_nothing_a_live_one_holds() {
    let root = TempDir::new("scratch-sweep");
    let scratch = Scratch::under(root.path(), "spool");
    let mine = scratch.file().unwrap();
    let dir = scratch.dir().to_path_buf();
    let parent = std::os::unix::process::parent_id();

    // The leftovers of a program that was killed, and of an earlier run with our pid.
    let dead = planted(&dir, "spool", DEAD_PID, 7, 0);
    let earlier_run = planted(&dir, "spool", std::process::id(), 7, 1);
    // A live process's file: young stays, old goes.
    let young_live = planted(&dir, "spool", parent, 7, 2);
    let old_live = planted(&dir, "spool", parent, 7, 3);
    age(&dir.join(&old_live), 3);
    // Not the module's: another purpose's file, a name of no scheme, a folder.
    let other_purpose = planted(&dir, "open", DEAD_PID, 7, 4);
    fs::write(dir.join("notes.txt"), b"mine").unwrap();
    fs::create_dir(dir.join("folder")).unwrap();
    // This run's file is never swept, however old it looks.
    age(&mine.path(), 30);

    let removed = scratch.sweep(DAY);
    assert_eq!(removed, 3, "{:?}", names(&dir));
    let left = names(&dir);
    for gone in [&dead, &earlier_run, &old_live] {
        assert!(!left.contains(gone), "{gone} should be swept: {left:?}");
    }
    for kept in [&young_live, &other_purpose] {
        assert!(left.contains(kept), "{kept} should stay: {left:?}");
    }
    assert!(left.contains(&"notes.txt".to_string()));
    assert!(left.contains(&"folder".to_string()));
    assert!(mine.path().exists(), "the file of this run stays");
}

/// The first scratch file of a purpose in a process sweeps what dead processes left in its
/// folder: a program nobody tells to clean up (the mail export, the command line) cleans up.
#[cfg(unix)]
#[test]
fn x_ar5a_23_the_first_file_of_a_purpose_sweeps_what_a_dead_process_left() {
    let root = TempDir::new("scratch-first");
    let scratch = Scratch::under(root.path(), "spool");
    scratch.ensure().unwrap();
    let dead = planted(scratch.dir(), "spool", DEAD_PID, 9, 0);
    let _file = scratch.file().unwrap();
    assert!(
        !names(scratch.dir()).contains(&dead),
        "{:?}",
        names(scratch.dir())
    );
}

/// `sweep_all_in` sweeps every purpose below the root, and only folders that are purposes.
#[cfg(unix)]
#[test]
fn x_ar5a_23_sweep_all_visits_every_purpose_folder() {
    let root = TempDir::new("scratch-all");
    let spool = Scratch::under(root.path(), "spool");
    let open = Scratch::under(root.path(), "open");
    spool.ensure().unwrap();
    open.ensure().unwrap();
    planted(spool.dir(), "spool", DEAD_PID, 1, 0);
    planted(open.dir(), "open", DEAD_PID, 1, 0);
    fs::write(root.path().join("stray.txt"), b"not a purpose").unwrap();
    assert_eq!(scratch::sweep_all_in(root.path(), DAY), 2);
    assert!(names(spool.dir()).is_empty() && names(open.dir()).is_empty());
    assert!(root.path().join("stray.txt").exists());
}

/// AZ-U13: a copy kept for a viewer is one folder per tag (a file and its version), private; it
/// goes, read-only files and all, once it is old.
#[cfg(unix)]
#[test]
fn az_u13_a_kept_folder_is_one_per_tag_private_and_swept_when_old() {
    let root = TempDir::new("scratch-keep");
    let open = Scratch::under(root.path(), "open");
    let a = open.keep("docs/a.docx\0v1").unwrap();
    let again = open.keep("docs/a.docx\0v1").unwrap();
    let other_version = open.keep("docs/a.docx\0v2").unwrap();
    assert_eq!(a, again, "the same version opens the same copy");
    assert_ne!(a, other_version);
    assert_eq!(mode_of(&a), 0o700);
    assert!(a.starts_with(open.dir()));

    let copy = a.join("a.docx");
    fs::write(&copy, b"cloud bytes").unwrap();
    scratch::make_read_only(&copy).unwrap();
    assert_eq!(mode_of(&copy), 0o400, "a save fails loudly");

    // Young: stays. Old: the folder and the read-only file in it go; the other tag stays.
    assert_eq!(open.sweep(DAY), 0);
    assert!(copy.exists());
    age(&a, 3);
    assert_eq!(open.sweep(DAY), 1);
    assert!(!a.exists());
    assert!(other_version.exists());
}

/// LANE-AZ-2: what takes a kept folder away before its age does (a removed drive's copies) asks
/// where it is: `kept` is the path `keep` makes, and makes nothing itself.
#[test]
fn lane_az_2_kept_names_the_folder_keep_makes_and_makes_nothing() {
    let root = TempDir::new("scratch-kept");
    let picked = Scratch::under(root.path(), "picked");
    let named = picked.kept("d_1");
    assert!(!root.path().join("picked").exists(), "nothing was made");
    assert!(named.starts_with(picked.dir()));
    assert_ne!(named, picked.kept("d_2"), "one folder a tag");
    assert_eq!(picked.keep("d_1").unwrap(), named);
}

/// AZ-U13: the product call is a read-only copy; on a system without modes it is the read-only
/// attribute.
#[test]
fn az_u13_make_read_only_closes_the_file_to_writes() {
    let root = TempDir::new("scratch-readonly");
    let path = root.path().join("copy.txt");
    fs::write(&path, b"bytes").unwrap();
    assert!(!fs::metadata(&path).unwrap().permissions().readonly());
    scratch::make_read_only(&path).unwrap();
    assert!(fs::metadata(&path).unwrap().permissions().readonly());
    assert_eq!(fs::read(&path).unwrap(), b"bytes", "it still reads");
}

/// The purpose is a folder name and the front of file names: nothing else gets through.
#[test]
fn a_purpose_cannot_leave_its_folder_or_be_empty() {
    let root = TempDir::new("scratch-purpose");
    for purpose in ["../escape", "a/b", "", "...", "Open Copy"] {
        let scratch = Scratch::under(root.path(), purpose);
        assert_eq!(
            scratch.dir().parent().unwrap(),
            root.path(),
            "{purpose:?} stays directly below the root: {:?}",
            scratch.dir()
        );
    }
    assert!(Scratch::under(root.path(), "").dir().ends_with("scratch"));
    assert!(Scratch::under(root.path(), "Open Copy")
        .dir()
        .ends_with("opencopy"));
}
