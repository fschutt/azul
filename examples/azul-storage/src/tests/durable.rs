//! The one durable write path ([`crate::durable`], ARCH28 RF41): the order of its steps and what
//! a cut at each one leaves, what the name must still hold, the bits and the time of the new
//! file, a stop between chunks, the old file kept in a slot, a name taken in one step.

use std::{
    cell::RefCell,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use super::TempDir;
#[cfg(unix)]
use crate::beneath::{Kind, Tri};
use crate::{
    beneath::{Dir, Fingerprint, Root},
    ctl::{Ctl, Quiet},
    durable::{
        publish, publish_probed, publish_staged, replace, replace_cache, replace_keeping,
        replace_probed, stage, Blob, Durability, Expect, Mode, Probe, Publish, PublishError, Step,
        TrashSlot,
    },
};

/// Records every step and cuts one by failing it; can make a file at the name just before the
/// rename, as another process would.
struct Order {
    seen: RefCell<Vec<Step>>,
    cut: Option<Step>,
    /// What the cut step answers: "cut" (an I/O error) or, for a file system that cannot sync a
    /// folder, `Unsupported`.
    cut_kind: io::ErrorKind,
    make_at_rename: Option<PathBuf>,
}

impl Order {
    fn new() -> Order {
        Order {
            seen: RefCell::new(Vec::new()),
            cut: None,
            cut_kind: io::ErrorKind::Other,
            make_at_rename: None,
        }
    }

    fn cut_at(step: Step) -> Order {
        Order {
            cut: Some(step),
            ..Order::new()
        }
    }

    /// The step is refused as a file system that does not do it refuses (`ENOTSUP`).
    fn refused_at(step: Step) -> Order {
        Order {
            cut: Some(step),
            cut_kind: io::ErrorKind::Unsupported,
            ..Order::new()
        }
    }

    fn seen(&self) -> Vec<Step> {
        self.seen.borrow().clone()
    }
}

impl Probe for Order {
    fn before(&self, step: Step) -> io::Result<()> {
        self.seen.borrow_mut().push(step);
        if step == Step::Rename {
            if let Some(path) = &self.make_at_rename {
                fs::write(path, b"made by someone else").unwrap();
            }
        }
        if self.cut == Some(step) {
            return Err(io::Error::new(self.cut_kind, "cut"));
        }
        Ok(())
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn shared() -> Publish<'static> {
    Publish::new(Expect::Any, Mode::Shared, &Quiet)
}

#[test]
fn the_steps_of_a_publish_come_in_the_order_stage_write_sync_check_rename_syncdir() {
    let tmp = TempDir::new("durable-order");
    let dir = Dir::open_following(tmp.path()).unwrap();
    let probe = Order::new();
    let landed = publish_probed(&dir, "doc.json", &mut &b"{}"[..], shared(), &probe).unwrap();
    assert_eq!(
        probe.seen(),
        vec![
            Step::Stage,
            Step::Write,
            Step::SyncFile,
            Step::Check,
            Step::Rename,
            Step::SyncDir
        ]
    );
    assert_eq!(landed.len, 2);
    assert_eq!(
        landed.blake3,
        Some(*blake3::hash(b"{}").as_bytes()),
        "the receipt names the bytes that were written"
    );
    assert_eq!(fs::read(tmp.path().join("doc.json")).unwrap(), b"{}");
    assert_eq!(names(tmp.path()), vec!["doc.json"]);
    // A cache takes the rename only: no sync of the file, none of the folder.
    let probe = Order::new();
    publish_probed(
        &dir,
        "cache.json",
        &mut &b"[]"[..],
        shared().cached(),
        &probe,
    )
    .unwrap();
    assert_eq!(
        probe.seen(),
        vec![Step::Stage, Step::Write, Step::Check, Step::Rename]
    );
}

#[test]
fn a_publish_cut_at_any_step_before_the_rename_leaves_the_old_file_and_no_temp_file() {
    for cut in [
        Step::Stage,
        Step::Write,
        Step::SyncFile,
        Step::Check,
        Step::Rename,
    ] {
        let tmp = TempDir::new("durable-cut");
        fs::write(tmp.path().join("doc.json"), b"old").unwrap();
        let dir = Dir::open_following(tmp.path()).unwrap();
        let probe = Order::cut_at(cut);
        let done = publish_probed(&dir, "doc.json", &mut &b"new"[..], shared(), &probe);
        assert!(done.is_err(), "cut at {cut:?}");
        assert_eq!(
            fs::read(tmp.path().join("doc.json")).unwrap(),
            b"old",
            "the old file stays when {cut:?} fails"
        );
        assert_eq!(
            names(tmp.path()),
            vec!["doc.json"],
            "no temporary file is left when {cut:?} fails"
        );
    }
}

#[test]
fn a_folder_that_cannot_be_synced_is_no_failure_of_a_write_that_landed() {
    let tmp = TempDir::new("durable-syncdir");
    fs::write(tmp.path().join("doc.json"), b"old").unwrap();
    let dir = Dir::open_following(tmp.path()).unwrap();
    // A file system that does not sync a folder at all (FAT on macOS) refuses it as unsupported.
    let probe = Order::refused_at(Step::SyncDir);
    let landed = publish_probed(&dir, "doc.json", &mut &b"new"[..], shared(), &probe);
    assert!(landed.is_ok(), "{landed:?}");
    assert_eq!(fs::read(tmp.path().join("doc.json")).unwrap(), b"new");
    assert_eq!(names(tmp.path()), vec!["doc.json"]);
}

/// X-DELTA-AZDRIVE-FILES-14. `Landed` was made whatever the folder sync found: a failing USB disk
/// that answers EIO to the sync after the rename reported the file landed, and a Move deleted its
/// source on that word while the target's entry could be lost with the disk's cache. Only a file
/// system that cannot sync a folder at all (`Unsupported`, `EINVAL`, `ENOTSUP`) is no failure; an
/// I/O error is - the file is in place and stays, the publish says it was not made durable.
#[test]
fn x_delta_azdrive_files_14_a_folder_sync_that_answers_an_io_error_is_a_failure_of_the_publish() {
    let tmp = TempDir::new("durable-syncdir-eio");
    fs::write(tmp.path().join("doc.json"), b"old").unwrap();
    let dir = Dir::open_following(tmp.path()).unwrap();
    let probe = Order::cut_at(Step::SyncDir);
    let landed = publish_probed(&dir, "doc.json", &mut &b"new"[..], shared(), &probe);
    assert!(
        matches!(landed, Err(PublishError::Io(_))),
        "an I/O error of the folder sync was taken for 'landed': {landed:?}"
    );
    assert_eq!(
        names(tmp.path()),
        vec!["doc.json"],
        "no temporary file is left"
    );
    // The same for a replace that keeps the old file in a slot.
    let tmp = TempDir::new("durable-syncdir-eio-slot");
    fs::write(tmp.path().join("doc.json"), b"old").unwrap();
    fs::create_dir(tmp.path().join("slots")).unwrap();
    let dir = Dir::open_following(tmp.path()).unwrap();
    let slots = Dir::open_following(&tmp.path().join("slots")).unwrap();
    let staged = stage(&dir, "doc.json", &mut &b"new"[..], &shared()).unwrap();
    let probe = Order::cut_at(Step::SyncDir);
    let kept = crate::durable::replace_keeping_probed(
        &dir,
        "doc.json",
        staged,
        &TrashSlot {
            dir: &slots,
            name: "old-doc",
        },
        Expect::Any,
        &probe,
    );
    assert!(matches!(kept, Err(PublishError::Io(_))), "{kept:?}");
    assert_eq!(fs::read(tmp.path().join("doc.json")).unwrap(), b"new");
    assert_eq!(
        fs::read(tmp.path().join("slots").join("old-doc")).unwrap(),
        b"old",
        "the old file is kept"
    );
}

/// A probe that looks at the folder at the moment the old file has its place in the slot and the new
/// one is about to take its name - where a kill or a power cut would leave the world.
struct AtReplace {
    folder: PathBuf,
    /// `(the name's bytes, the slot's bytes, the names in the folder)` as they were then.
    seen: RefCell<Option<(Option<Vec<u8>>, Option<Vec<u8>>, Vec<String>)>>,
    /// Another writer puts this at the name right then.
    third: Option<&'static [u8]>,
}

impl Probe for AtReplace {
    fn before(&self, step: Step) -> io::Result<()> {
        if step == Step::Replace {
            *self.seen.borrow_mut() = Some((
                fs::read(self.folder.join("doc")).ok(),
                fs::read(self.folder.join("trash").join("doc.kept")).ok(),
                names(&self.folder),
            ));
            if let Some(bytes) = self.third {
                let other = self.folder.join("someone-elses");
                fs::write(&other, bytes).unwrap();
                fs::rename(&other, self.folder.join("doc")).unwrap();
            }
        }
        Ok(())
    }
}

/// X-DELTA-AZDRIVE-FILES-26. `replace_keeping` was two renames: the old file out of its place into
/// the slot, then the new one into the free name. A kill or a power cut between them left the name
/// EMPTY, the staged file beside it and the old one in the slot, and the next sync pass read (gone
/// here, unchanged on the drive) as a delete - AZ-S4's failure at the width of two renames. The
/// old file now gets a second name in the slot first (a hard link); the new version is renamed
/// over the first. Where the world would stop between the steps the old file is at its name.
#[cfg(unix)]
#[test]
fn x_delta_azdrive_files_26_the_name_is_never_empty_between_the_old_file_and_the_new_one() {
    let tmp = TempDir::new("durable-keeping-gap");
    fs::create_dir(tmp.path().join("trash")).unwrap();
    fs::write(tmp.path().join("doc"), b"old").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let dir = root.top().unwrap();
    let trash = root.dir("trash", false).unwrap();
    let staged = stage(&dir, "doc", &mut &b"the new version"[..], &shared()).unwrap();
    let slot = TrashSlot {
        dir: &trash,
        name: "doc.kept",
    };
    let probe = AtReplace {
        folder: tmp.path().to_path_buf(),
        seen: RefCell::new(None),
        third: None,
    };
    crate::durable::replace_keeping_probed(&dir, "doc", staged, &slot, Expect::Any, &probe)
        .unwrap();
    let (at_name, in_slot, names_then) = probe.seen.borrow().clone().expect("the step was taken");
    assert_eq!(
        at_name.as_deref(),
        Some(&b"old"[..]),
        "between the steps the name was empty"
    );
    assert_eq!(
        in_slot.as_deref(),
        Some(&b"old"[..]),
        "the slot already held it"
    );
    assert!(
        names_then.len() == 3,
        "the staged file waits beside the name: {names_then:?}"
    );
    assert_eq!(
        fs::read(tmp.path().join("doc")).unwrap(),
        b"the new version"
    );
    assert_eq!(fs::read(tmp.path().join("trash/doc.kept")).unwrap(), b"old");
}

/// X-DELTA-AZDRIVE-FILES-26. The name must still be the file that was linked into the slot when the
/// new version is renamed over it: a file another writer put there since is not replaced, and the
/// old file - whose only name is the slot's now - is not unlinked.
#[cfg(unix)]
#[test]
fn x_delta_azdrive_files_26_a_file_another_writer_put_at_the_name_is_not_replaced() {
    let tmp = TempDir::new("durable-keeping-third");
    fs::create_dir(tmp.path().join("trash")).unwrap();
    fs::write(tmp.path().join("doc"), b"old").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let dir = root.top().unwrap();
    let trash = root.dir("trash", false).unwrap();
    let staged = stage(&dir, "doc", &mut &b"new"[..], &shared()).unwrap();
    let slot = TrashSlot {
        dir: &trash,
        name: "doc.kept",
    };
    let probe = AtReplace {
        folder: tmp.path().to_path_buf(),
        seen: RefCell::new(None),
        third: Some(b"saved by another process"),
    };
    let done =
        crate::durable::replace_keeping_probed(&dir, "doc", staged, &slot, Expect::Any, &probe);
    assert!(matches!(done, Err(PublishError::Changed)), "{done:?}");
    assert_eq!(
        fs::read(tmp.path().join("doc")).unwrap(),
        b"saved by another process",
        "the other writer's file was replaced"
    );
    assert_eq!(
        fs::read(tmp.path().join("trash/doc.kept")).unwrap(),
        b"old",
        "the old file, which has no other name, is kept"
    );
    assert_eq!(
        names(tmp.path()),
        vec!["doc", "trash"],
        "the staged file is gone"
    );
}

/// The kinds of answer a folder sync gives that are no failure: the volume cannot do it.
#[cfg(unix)]
#[test]
fn x_delta_azdrive_files_14_only_a_refusal_to_sync_a_folder_is_no_failure() {
    use crate::beneath::folder_sync_refused;
    for refused in [
        io::Error::from(io::ErrorKind::Unsupported),
        io::Error::from_raw_os_error(libc::EINVAL),
        io::Error::from_raw_os_error(libc::ENOTSUP),
        io::Error::from_raw_os_error(libc::EOPNOTSUPP),
        io::Error::from_raw_os_error(libc::ENOSYS),
    ] {
        assert!(folder_sync_refused(&refused), "{refused:?}");
    }
    for failed in [
        io::Error::from_raw_os_error(libc::EIO),
        io::Error::from_raw_os_error(libc::ENOSPC),
        io::Error::from_raw_os_error(libc::EDQUOT),
        io::Error::other("cut"),
    ] {
        assert!(!folder_sync_refused(&failed), "{failed:?}");
    }
    // A real folder syncs: Ok.
    let tmp = TempDir::new("durable-syncdir-real");
    let dir = Dir::open_following(tmp.path()).unwrap();
    assert!(dir.sync_checked().is_ok());
}

#[test]
fn a_file_that_changed_since_it_was_looked_at_is_not_replaced() {
    let tmp = TempDir::new("durable-expect-is");
    let path = tmp.path().join("doc.json");
    fs::write(&path, b"old").unwrap();
    let dir = Dir::open_following(tmp.path()).unwrap();
    let print = Fingerprint::of(&dir.lstat("doc.json").unwrap());
    // Written to meanwhile (another size, so on any disk): the stale fingerprint is refused and
    // nothing is written.
    fs::write(&path, b"changed meanwhile").unwrap();
    let done = publish(
        &dir,
        "doc.json",
        &mut &b"new"[..],
        Publish::new(Expect::Is(print), Mode::Shared, &Quiet),
    );
    assert!(matches!(done, Err(PublishError::Changed)), "{done:?}");
    assert_eq!(fs::read(&path).unwrap(), b"changed meanwhile");
    assert_eq!(names(tmp.path()), vec!["doc.json"]);
    // Still the file that was looked at: it goes.
    let print = Fingerprint::of(&dir.lstat("doc.json").unwrap());
    let done = publish(
        &dir,
        "doc.json",
        &mut &b"new"[..],
        Publish::new(Expect::Is(print), Mode::Shared, &Quiet),
    );
    assert!(done.is_ok(), "{done:?}");
    assert_eq!(fs::read(&path).unwrap(), b"new");
}

#[cfg(unix)]
#[test]
fn a_file_whose_identity_is_not_known_is_never_the_one_that_was_expected() {
    let tmp = TempDir::new("durable-expect-unknown");
    let path = tmp.path().join("doc.json");
    fs::write(&path, b"old").unwrap();
    let dir = Dir::open_following(tmp.path()).unwrap();
    let st = dir.lstat("doc.json").unwrap();
    // Everything that can be told matches; the inode and the device were not recorded.
    let print = Fingerprint::from_parts(Kind::File, st.size, st.mtime_ns, st.ctime_ns, 0, 0);
    assert_eq!(print.matches(&st), Tri::Unknown);
    let done = publish(
        &dir,
        "doc.json",
        &mut &b"new"[..],
        Publish::new(Expect::Is(print), Mode::Shared, &Quiet),
    );
    assert!(matches!(done, Err(PublishError::Changed)), "{done:?}");
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert_eq!(names(tmp.path()), vec!["doc.json"]);
}

#[test]
fn a_name_that_must_be_free_is_taken_in_the_one_step_of_the_rename() {
    let tmp = TempDir::new("durable-expect-absent");
    let dir = Dir::open_following(tmp.path()).unwrap();
    let absent = || Publish::new(Expect::Absent, Mode::Shared, &Quiet);
    let landed = publish(&dir, "doc.json", &mut &b"first"[..], absent());
    assert!(landed.is_ok(), "{landed:?}");
    // Taken: the second is refused and the first stays.
    let again = publish(&dir, "doc.json", &mut &b"second"[..], absent());
    assert!(matches!(again, Err(PublishError::Changed)), "{again:?}");
    assert_eq!(fs::read(tmp.path().join("doc.json")).unwrap(), b"first");
    // Made by someone between the look and the rename: the rename itself refuses.
    let racer = Order {
        make_at_rename: Some(tmp.path().join("raced.json")),
        ..Order::new()
    };
    let raced = publish_probed(&dir, "raced.json", &mut &b"mine"[..], absent(), &racer);
    assert!(matches!(raced, Err(PublishError::Changed)), "{raced:?}");
    assert_eq!(
        fs::read(tmp.path().join("raced.json")).unwrap(),
        b"made by someone else",
        "the file that turned up is not replaced"
    );
    assert_eq!(names(tmp.path()), vec!["doc.json", "raced.json"]);
}

#[cfg(unix)]
#[test]
fn the_new_file_has_the_bits_and_the_time_that_were_asked_for() {
    use std::os::unix::fs::PermissionsExt;
    let mode_of = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o7777;
    let tmp = TempDir::new("durable-bits");
    let dir = Dir::open_following(tmp.path()).unwrap();
    let put = |name: &str, mode: Mode| {
        publish(
            &dir,
            name,
            &mut &b"x"[..],
            Publish::new(Expect::Any, mode, &Quiet),
        )
        .unwrap();
        mode_of(&tmp.path().join(name))
    };
    assert_eq!(put("private", Mode::Private), 0o600);
    assert_eq!(
        put("source", Mode::FromSource(0o4755)),
        0o755,
        "a copy never keeps setuid, setgid or sticky"
    );
    assert_eq!(
        put("shared", Mode::Shared) & 0o600,
        0o600,
        "a new file is readable and writable by its owner"
    );
    assert_eq!(
        put("shared", Mode::Shared) & 0o111,
        0,
        "and executable by no one"
    );
    // The bits of the file that is replaced.
    fs::write(tmp.path().join("kept"), b"old").unwrap();
    fs::set_permissions(tmp.path().join("kept"), fs::Permissions::from_mode(0o640)).unwrap();
    assert_eq!(put("kept", Mode::Keep), 0o640);
    // A sync's rule: rw-r--r-- for a new file, the executable bit where the drive's is.
    assert_eq!(put("plain", Mode::Synced { exec: false }), 0o644);
    assert_eq!(put("script", Mode::Synced { exec: true }), 0o755);
    fs::write(tmp.path().join("mine"), b"old").unwrap();
    fs::set_permissions(tmp.path().join("mine"), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        put("mine", Mode::Synced { exec: true }),
        0o700,
        "the user's own choice of who reads it stands; the executable bit follows the read bit"
    );
    // The modification time.
    let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    publish(
        &dir,
        "old-news",
        &mut &b"x"[..],
        Publish::new(Expect::Any, Mode::Shared, &Quiet).with_mtime(then),
    )
    .unwrap();
    assert_eq!(
        fs::metadata(tmp.path().join("old-news"))
            .unwrap()
            .modified()
            .unwrap(),
        then
    );
}

/// Stops once it has heard `limit` bytes.
struct StopAfter {
    heard: AtomicU64,
    limit: u64,
}

impl Ctl for StopAfter {
    fn bytes(&self, done: u64) {
        self.heard.store(done, Ordering::SeqCst);
    }

    fn cancelled(&self) -> bool {
        self.heard.load(Ordering::SeqCst) >= self.limit
    }
}

#[test]
fn a_cancel_between_chunks_leaves_the_old_file_and_no_temp_file() {
    let tmp = TempDir::new("durable-cancel");
    let path = tmp.path().join("big.bin");
    fs::write(&path, b"old").unwrap();
    let dir = Dir::open_following(tmp.path()).unwrap();
    let ctl = StopAfter {
        heard: AtomicU64::new(0),
        limit: 1,
    };
    let mut body = io::repeat(7).take(3 << 20);
    let done = publish(
        &dir,
        "big.bin",
        &mut body,
        Publish::new(Expect::Any, Mode::Shared, &ctl),
    );
    assert!(matches!(done, Err(PublishError::Cancelled)), "{done:?}");
    let heard = ctl.heard.load(Ordering::SeqCst);
    assert!(
        heard > 0 && heard < 3 << 20,
        "stopped part of the way: {heard}"
    );
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert_eq!(names(tmp.path()), vec!["big.bin"]);
}

#[test]
fn a_closure_that_hears_the_progress_is_a_ctl_that_never_stops() {
    let tmp = TempDir::new("durable-progress");
    let dir = Dir::open_following(tmp.path()).unwrap();
    let last = AtomicU64::new(0);
    let hear = |done: u64| last.store(done, Ordering::SeqCst);
    let mut body = io::repeat(1).take(600_000);
    publish(
        &dir,
        "n.bin",
        &mut body,
        Publish::new(Expect::Any, Mode::Shared, &hear),
    )
    .unwrap();
    assert_eq!(last.load(Ordering::SeqCst), 600_000);
    assert_eq!(
        fs::metadata(tmp.path().join("n.bin")).unwrap().len(),
        600_000
    );
}

#[test]
fn replace_writes_a_document_makes_its_folders_and_leaves_nothing_behind() {
    let tmp = TempDir::new("durable-replace");
    let path = tmp.path().join("a").join("b").join("state.json");
    replace(&path, b"{\"n\":1}", Mode::Private).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"{\"n\":1}");
    replace(&path, b"{}", Mode::Private).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"{}");
    assert_eq!(names(&tmp.path().join("a").join("b")), vec!["state.json"]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bits = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(bits(&path), 0o600, "a private document");
        assert_eq!(
            bits(&tmp.path().join("a").join("b")),
            0o700,
            "in a private folder"
        );
        assert_eq!(
            bits(&tmp.path().join("a")),
            0o700,
            "and the folders on the way"
        );
    }
    // A cache document is the rename only, and still leaves nothing behind.
    let cache = tmp.path().join("cache").join("nodes.json");
    replace_cache(&cache, b"[1]").unwrap();
    replace_cache(&cache, b"[1,2]").unwrap();
    assert_eq!(fs::read(&cache).unwrap(), b"[1,2]");
    assert_eq!(names(&tmp.path().join("cache")), vec!["nodes.json"]);
    // No name in the path: nothing to write.
    assert!(replace(Path::new("/"), b"x", Mode::Shared).is_err());
}

#[test]
fn rename_noreplace_takes_a_free_name_and_never_a_taken_one() {
    let tmp = TempDir::new("durable-noreplace");
    let dir = Dir::open_following(tmp.path()).unwrap();
    fs::write(tmp.path().join("a"), b"A").unwrap();
    fs::write(tmp.path().join("b"), b"B").unwrap();
    let refused = dir.rename_noreplace("a", &dir, "b");
    assert!(
        matches!(&refused, Err(e) if e.is_already_exists()),
        "{refused:?}"
    );
    assert_eq!(fs::read(tmp.path().join("a")).unwrap(), b"A");
    assert_eq!(fs::read(tmp.path().join("b")).unwrap(), b"B");
    dir.rename_noreplace("a", &dir, "c").unwrap();
    assert_eq!(fs::read(tmp.path().join("c")).unwrap(), b"A");
    assert_eq!(names(tmp.path()), vec!["b", "c"]);
    let missing = dir.rename_noreplace("nothing", &dir, "d");
    assert!(
        matches!(&missing, Err(e) if e.is_not_found()),
        "{missing:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_folder_the_user_linked_is_opened_through_its_link_and_what_is_below_it_is_not() {
    let tmp = TempDir::new("durable-following");
    let real = tmp.path().join("real");
    fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, tmp.path().join("link")).unwrap();
    // The user's own link is the folder they chose.
    let dir = Dir::open_following(&tmp.path().join("link")).unwrap();
    publish(&dir, "doc", &mut &b"in the real one"[..], shared()).unwrap();
    assert_eq!(fs::read(real.join("doc")).unwrap(), b"in the real one");
    // A link below it is none that is followed.
    std::os::unix::fs::symlink(tmp.path(), real.join("up")).unwrap();
    assert!(dir.child("up", false).is_err());
}

#[test]
fn replace_keeping_puts_the_new_version_in_place_and_the_old_one_in_the_slot() {
    let tmp = TempDir::new("durable-keeping");
    fs::create_dir(tmp.path().join("trash")).unwrap();
    fs::write(tmp.path().join("doc"), b"old").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let dir = root.top().unwrap();
    let trash = root.dir("trash", false).unwrap();
    let staged = stage(
        &dir,
        "doc",
        &mut &b"the new version"[..],
        &Publish::new(Expect::Any, Mode::Keep, &Quiet),
    )
    .unwrap();
    // The new version is whole beside the old one before the old one leaves its place.
    assert_eq!(fs::read(tmp.path().join("doc")).unwrap(), b"old");
    let slot = TrashSlot {
        dir: &trash,
        name: "doc.20261105",
    };
    let landed = replace_keeping(&dir, "doc", staged, &slot, Expect::Any).unwrap();
    assert_eq!(landed.len, 15);
    assert_eq!(
        fs::read(tmp.path().join("doc")).unwrap(),
        b"the new version"
    );
    assert_eq!(
        fs::read(tmp.path().join("trash/doc.20261105")).unwrap(),
        b"old"
    );
    assert_eq!(names(tmp.path()), vec!["doc", "trash"]);
}

#[test]
fn replace_keeping_puts_the_old_one_back_when_the_new_one_cannot_go_in() {
    let tmp = TempDir::new("durable-keeping-back");
    fs::create_dir(tmp.path().join("trash")).unwrap();
    fs::write(tmp.path().join("doc"), b"old").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let dir = root.top().unwrap();
    let trash = root.dir("trash", false).unwrap();
    let staged = stage(&dir, "doc", &mut &b"new"[..], &shared()).unwrap();
    // The staged file vanishes before it can be renamed in.
    fs::remove_file(tmp.path().join(staged.name())).unwrap();
    let slot = TrashSlot {
        dir: &trash,
        name: "doc.kept",
    };
    let done = replace_keeping(&dir, "doc", staged, &slot, Expect::Any);
    assert!(done.is_err(), "{done:?}");
    assert_eq!(
        fs::read(tmp.path().join("doc")).unwrap(),
        b"old",
        "the old version is back in its place"
    );
    assert_eq!(names(&tmp.path().join("trash")), Vec::<String>::new());
    assert_eq!(names(tmp.path()), vec!["doc", "trash"]);
}

#[test]
fn replace_keeping_of_a_file_that_changed_moves_nothing_and_drops_the_staged_file() {
    let tmp = TempDir::new("durable-keeping-changed");
    fs::create_dir(tmp.path().join("trash")).unwrap();
    let path = tmp.path().join("doc");
    fs::write(&path, b"old").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let dir = root.top().unwrap();
    let trash = root.dir("trash", false).unwrap();
    let print = Fingerprint::of(&dir.lstat("doc").unwrap());
    fs::write(&path, b"the user edited it meanwhile").unwrap();
    let staged = stage(&dir, "doc", &mut &b"new"[..], &shared()).unwrap();
    let slot = TrashSlot {
        dir: &trash,
        name: "doc.kept",
    };
    let done = replace_keeping(&dir, "doc", staged, &slot, Expect::Is(print));
    assert!(matches!(done, Err(PublishError::Changed)), "{done:?}");
    assert_eq!(fs::read(&path).unwrap(), b"the user edited it meanwhile");
    assert_eq!(
        names(tmp.path()),
        vec!["doc", "trash"],
        "the staged file is gone"
    );
    assert!(names(&tmp.path().join("trash")).is_empty());
}

#[test]
fn a_blob_is_renamed_in_with_its_bits_and_left_alone_when_it_does_not_land() {
    let tmp = TempDir::new("durable-blob");
    fs::create_dir(tmp.path().join("spool")).unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let dir = root.top().unwrap();
    let spool = root.dir("spool", false).unwrap();
    fs::write(tmp.path().join("spool/blob-1"), b"downloaded").unwrap();
    let landed = publish_staged(
        &dir,
        "doc",
        Blob {
            dir: &spool,
            name: "blob-1",
        },
        Publish::new(Expect::Absent, Mode::Synced { exec: false }, &Quiet),
    )
    .unwrap();
    assert_eq!(landed.len, 10);
    assert_eq!(fs::read(tmp.path().join("doc")).unwrap(), b"downloaded");
    assert!(
        names(&tmp.path().join("spool")).is_empty(),
        "the blob moved into place"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(tmp.path().join("doc"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
    }
    // The name is taken now: the second blob is refused and stays where it is.
    fs::write(tmp.path().join("spool/blob-2"), b"another").unwrap();
    let again = publish_staged(
        &dir,
        "doc",
        Blob {
            dir: &spool,
            name: "blob-2",
        },
        Publish::new(Expect::Absent, Mode::Shared, &Quiet),
    );
    assert!(matches!(again, Err(PublishError::Changed)), "{again:?}");
    assert_eq!(names(&tmp.path().join("spool")), vec!["blob-2"]);
    assert_eq!(fs::read(tmp.path().join("doc")).unwrap(), b"downloaded");
}

#[test]
fn az_s19_az_p20_a_private_document_is_synced_before_the_rename_and_its_folder_after() {
    // The base and the key an outbox call carries were renamed in without the folder synced: a
    // power cut after the rename could bring the old directory entry back, or none.
    let tmp = TempDir::new("durable-s19");
    let probe = Order::new();
    replace_probed(
        &tmp.path().join("state").join("outbox.json"),
        b"{\"key\":1}",
        Mode::Private,
        Durability::State,
        &probe,
    )
    .unwrap();
    let seen = probe.seen();
    let at = |step: Step| seen.iter().position(|s| *s == step);
    assert!(
        at(Step::SyncFile).unwrap() < at(Step::Rename).unwrap(),
        "{seen:?}"
    );
    assert!(
        at(Step::Rename).unwrap() < at(Step::SyncDir).unwrap(),
        "{seen:?}"
    );
    assert_eq!(seen.last(), Some(&Step::SyncDir), "{seen:?}");
    // Cut at the file's sync: nothing is in place, nothing is left behind.
    let cut = Order::cut_at(Step::SyncFile);
    let path = tmp.path().join("state").join("other.json");
    assert!(replace_probed(&path, b"x", Mode::Private, Durability::State, &cut).is_err());
    assert!(!path.exists());
    assert_eq!(names(&tmp.path().join("state")), vec!["outbox.json"]);
}

#[test]
fn az_u15_a_sweep_removes_the_temp_files_of_dead_processes_and_old_partial_downloads() {
    use crate::durable::sweep_temps;
    let tmp = TempDir::new("durable-sweep");
    let dir = Dir::open_following(tmp.path()).unwrap();
    // A process that cannot exist: its temporary file is a leftover, young as it is.
    let dead = ".a.txt.azul-storage-2000000000-0.tmp";
    // This process's own, still being written.
    let mine = format!(".b.txt.azul-storage-{}-0.tmp", std::process::id());
    for name in [
        dead,
        mine.as_str(),
        ".c.bin.download.part",
        ".d.bin.download.part",
    ] {
        fs::write(tmp.path().join(name), b"x").unwrap();
    }
    fs::write(tmp.path().join(".c.bin.download-state.part"), b"{}").unwrap();
    fs::write(tmp.path().join("e.txt"), b"mine").unwrap();
    fs::write(tmp.path().join(".hidden"), b"mine too").unwrap();
    let ten_days_ago = SystemTime::now() - Duration::from_secs(10 * 24 * 3600);
    for old in [".c.bin.download.part", ".c.bin.download-state.part"] {
        fs::File::options()
            .write(true)
            .open(tmp.path().join(old))
            .unwrap()
            .set_modified(ten_days_ago)
            .unwrap();
    }
    let removed = sweep_temps(&dir, Duration::from_secs(6 * 24 * 3600));
    assert_eq!(
        removed, 3,
        "the dead process's temp file and the old partial with its state"
    );
    let mut kept = vec![
        mine,
        String::from(".d.bin.download.part"),
        String::from(".hidden"),
        String::from("e.txt"),
    ];
    kept.sort();
    assert_eq!(names(tmp.path()), kept);
    // Nothing more to sweep: a second sweep removes nothing.
    assert_eq!(sweep_temps(&dir, Duration::from_secs(6 * 24 * 3600)), 0);
}
