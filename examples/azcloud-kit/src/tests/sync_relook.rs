//! The look a pass takes at the deletes it planned right before it commits them
//! (`hold_returned_deletes`) and the rules that hold a delete for a save in progress
//! (`keep_saves`): against what the pass itself already did to the same file on the drive, and
//! against disks that fold names (X-DELTA-SYNC4-28, -29, -3, -30).

use std::{
    collections::BTreeMap,
    fs,
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use azul_storage::{testing::TempDir, Drive};

use super::{
    fake_s3::{FakeS3, S3Bucket},
    sync::{index_of, Device},
    sync_named::{s3_drive, NamesHashes},
};
use crate::sync::{
    faults,
    local::{BaseEntry, Scan, Temp},
    merge::Action,
    problems::Code,
    session::{SyncSession, SyncSetup},
    sync_to, SyncReport, Target,
};

const PREFIX: &str = "relook/";

/// A folder synced to a drive BY NAME on the device `device`: its passes hear the fault seam.
fn named(device: &str, folder: &Path, state: &Path, drive: &Arc<dyn Drive>) -> SyncSession {
    let root = folder.to_path_buf();
    SyncSession::named(
        SyncSetup::new("d_relook", PREFIX, folder),
        state.to_path_buf(),
        device,
        Arc::clone(drive),
    )
    .with_points(Arc::new(move |name: &str| faults::point(&root, name)))
}

fn pass(session: &SyncSession) -> SyncReport {
    session
        .pass(&AtomicBool::new(false), &mut |_| {})
        .expect("the pass runs")
        .report
        .expect("a report")
}

/// The files of the folder by name, the sync's own folder left out.
fn tree(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let key = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if key == ".azlin" {
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                walk(&entry.path(), &format!("{key}/"), out);
            } else {
                out.insert(key, fs::read(entry.path()).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(folder, "", &mut out);
    out
}

/// The files of the folder's sync trash, by content.
fn trashed(folder: &Path) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut stack = vec![folder.join(".azlin").join("sync").join("trash")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                out.push(fs::read(entry.path()).unwrap());
            }
        }
    }
    out
}

/// What the drive holds under the prefix, by name.
fn on_drive(s3: &FakeS3) -> BTreeMap<String, Vec<u8>> {
    s3.keys()
        .into_iter()
        .filter(|key| !key.contains(".azlin"))
        .filter_map(|key| {
            let name = key.strip_prefix(PREFIX)?.to_string();
            s3.read(&key).map(|bytes| (name, bytes))
        })
        .collect()
}

/// Two devices on one named drive with a few files, every file of A on B too.
struct Pair {
    s3: Arc<FakeS3>,
    a: TempDir,
    _a_state: TempDir,
    b: TempDir,
    _b_state: TempDir,
    on_a: Arc<SyncSession>,
    on_b: SyncSession,
}

fn pair(name: &str) -> Pair {
    let s3 = FakeS3::new();
    let drive: Arc<dyn Drive> = Arc::new(NamesHashes(s3_drive(&s3)));
    let (a, a_state) = (
        TempDir::new(&format!("azcloud-relook-{name}-a")),
        TempDir::new(&format!("azcloud-relook-{name}-a-state")),
    );
    let (b, b_state) = (
        TempDir::new(&format!("azcloud-relook-{name}-b")),
        TempDir::new(&format!("azcloud-relook-{name}-b-state")),
    );
    let on_a = Arc::new(named("A", a.path(), a_state.path(), &drive));
    let on_b = named("B", b.path(), b_state.path(), &drive);
    for (key, bytes) in [
        ("one.txt", "the first"),
        ("two.txt", "the second"),
        ("three.txt", "the third"),
        ("report.pdf", "the report"),
    ] {
        fs::write(a.path().join(key), bytes).unwrap();
    }
    pass(&on_a);
    pass(&on_b);
    assert_eq!(tree(a.path()), tree(b.path()));
    Pair {
        s3,
        a,
        _a_state: a_state,
        b,
        _b_state: b_state,
        on_a,
        on_b,
    }
}

/// A's folder after `report.pdf` was renamed to `report-final.pdf` and a new small file was made
/// beside it: the pass of A moves the report on the drive first (one `move_many`), then sends the
/// new file in a batch and tells `after-blob` - the point where the move has landed on the drive and
/// the commit has not been made. Waits there and returns the thread of the pass.
fn pause_after_the_move(
    p: &Pair,
) -> std::thread::JoinHandle<crate::error::CloudResult<crate::sync::session::Pass>> {
    fs::rename(
        p.a.path().join("report.pdf"),
        p.a.path().join("report-final.pdf"),
    )
    .unwrap();
    fs::write(p.a.path().join("extra.txt"), "a new file").unwrap();
    faults::arm_pause(p.a.path(), "after-blob");
    let running = {
        let on_a = Arc::clone(&p.on_a);
        std::thread::spawn(move || on_a.pass(&AtomicBool::new(false), &mut |_| {}))
    };
    assert_eq!(
        faults::wait_paused(p.a.path(), Duration::from_secs(30)).as_deref(),
        Some("after-blob"),
        "the pass has sent the new file"
    );
    // The evidence the test rests on: the rename already happened on the drive.
    let held = on_drive(&p.s3);
    assert!(
        held.contains_key("report-final.pdf") && !held.contains_key("report.pdf"),
        "the pass moved the object before it commits: {:?}",
        held.keys()
    );
    running
}

/// X-DELTA-SYNC4-28, a REGRESSION of 623b73958 (X-DELTA-EDGE-5). On a drive synced by name a file
/// renamed here is MOVED on the drive by the upload phase, before the pass commits. The look at the
/// planned deletes right before the commit found the old name back (the user undid the rename while
/// the pass ran) and dropped the delete - of a file the drive no longer had: the commit then wrote
/// the new name into the base and kept the old one there, and the next pass read the old name as
/// "deleted on the drive" (a delete here, into the trash) and the new name as "deleted here" (a
/// delete there): both names gone, the bytes only in the sync trash. The delete the pass has done
/// already is no delete the look may take back. RED on the old code: the report is not in A's
/// folder after the next pass, its bytes are in A's trash and nowhere on the drive.
#[test]
fn x_delta_sync4_28_a_rename_the_pass_moved_on_the_drive_and_the_user_undid_loses_no_name() {
    let p = pair("s28");
    let running = pause_after_the_move(&p);
    // The undo, while the pass is paused: the old name is back, the new one is gone.
    fs::rename(
        p.a.path().join("report-final.pdf"),
        p.a.path().join("report.pdf"),
    )
    .unwrap();
    faults::release(p.a.path());
    let report = running
        .join()
        .expect("the pass ends")
        .expect("the pass succeeds")
        .report
        .expect("a report");
    assert!(report.errors.is_empty(), "{}", report.summary());

    // Two more passes on each device: the folder and the drive settle on what is here.
    for _ in 0..2 {
        pass(&p.on_a);
        pass(&p.on_b);
    }
    let here = tree(p.a.path());
    assert_eq!(
        here.get("report.pdf").map(Vec::as_slice),
        Some(&b"the report"[..]),
        "the file is where the user put it back: {:?}; the sync trash holds {:?}",
        here.keys().collect::<Vec<_>>(),
        trashed(p.a.path())
    );
    assert!(
        !here.contains_key("report-final.pdf"),
        "{:?}",
        here.keys().collect::<Vec<_>>()
    );
    assert!(trashed(p.a.path()).is_empty(), "nothing of A's was trashed");
    assert!(trashed(p.b.path()).is_empty(), "nothing of B's was trashed");
    let held = on_drive(&p.s3);
    assert_eq!(
        held.get("report.pdf").map(Vec::as_slice),
        Some(&b"the report"[..]),
        "the drive keeps the file under the name that is here: {:?}",
        held.keys().collect::<Vec<_>>()
    );
    assert_eq!(tree(p.b.path()), here, "B follows");
}

/// X-DELTA-SYNC4-28, the row of the crash table "killed after the move, before the commit": the
/// drive holds the new name only, the base the old one, and the note of what the pass sent says the
/// old name was to go. The next pass finishes the move as a move; no name is lost.
#[test]
fn x_delta_sync4_28_a_pass_killed_after_it_moved_the_file_on_the_drive_loses_no_name() {
    let p = pair("s28-kill");
    fs::rename(
        p.a.path().join("report.pdf"),
        p.a.path().join("report-final.pdf"),
    )
    .unwrap();
    fs::write(p.a.path().join("extra.txt"), "a new file").unwrap();
    faults::arm_crash(p.a.path(), "after-blob");
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let _ = p.on_a.pass(&AtomicBool::new(false), &mut |_| {});
    }))
    .expect_err("the pass is killed after the move");
    assert_eq!(
        faults::crashed_at(payload.as_ref()).as_deref(),
        Some("after-blob")
    );
    let held = on_drive(&p.s3);
    assert!(
        held.contains_key("report-final.pdf") && !held.contains_key("report.pdf"),
        "{:?}",
        held.keys()
    );
    for _ in 0..2 {
        pass(&p.on_a);
        pass(&p.on_b);
    }
    for folder in [p.a.path(), p.b.path()] {
        let here = tree(folder);
        assert_eq!(
            here.get("report-final.pdf").map(Vec::as_slice),
            Some(&b"the report"[..]),
            "{:?}",
            here.keys().collect::<Vec<_>>()
        );
        assert!(!here.contains_key("report.pdf"));
        assert!(trashed(folder).is_empty());
    }
    assert!(on_drive(&p.s3).contains_key("report-final.pdf"));
    faults::clear(p.a.path());
}

/// X-DELTA-SYNC4-28, the row "killed after the commit, before the base is saved", with the rename
/// undone while the pass ran: the commit holds the tombstone of the old name and the new name, the
/// base of this device is the old one. The next pass reads the note of what was sent and ends with
/// the file under the name that is here.
#[test]
fn x_delta_sync4_28_a_pass_killed_after_the_commit_of_an_undone_rename_loses_no_name() {
    let p = pair("s28-commit");
    let running = pause_after_the_move(&p);
    fs::rename(
        p.a.path().join("report-final.pdf"),
        p.a.path().join("report.pdf"),
    )
    .unwrap();
    faults::arm_crash(p.a.path(), "after-commit");
    faults::release(p.a.path());
    let payload = running
        .join()
        .expect_err("the pass is killed after its commit");
    assert_eq!(
        faults::crashed_at(payload.as_ref()).as_deref(),
        Some("after-commit")
    );
    for _ in 0..3 {
        pass(&p.on_a);
        pass(&p.on_b);
    }
    let here = tree(p.a.path());
    assert_eq!(
        here.get("report.pdf").map(Vec::as_slice),
        Some(&b"the report"[..]),
        "{:?}; trash {:?}",
        here.keys().collect::<Vec<_>>(),
        trashed(p.a.path())
    );
    assert!(trashed(p.a.path()).is_empty());
    assert!(trashed(p.b.path()).is_empty());
    assert_eq!(
        on_drive(&p.s3).get("report.pdf").map(Vec::as_slice),
        Some(&b"the report"[..])
    );
    assert_eq!(tree(p.b.path()), here);
    faults::clear(p.a.path());
}

/// Whether the problem list of `report` names `code` for `key`.
fn lists(report: &SyncReport, code: Code, key: &str) -> bool {
    report
        .problems
        .iter()
        .any(|p| p.code == code && p.key == key)
}

/// X-DELTA-SYNC4-29. `not_gone` (the look at the planned deletes right before the commit) opened a
/// folder by NAME: a disk that takes `Docs` and `docs` for one folder (APFS, NTFS, exFAT) opened the
/// renamed folder for the old spelling, found every file in it and held every `DeleteRemote(Docs/..)`
/// while the uploads under `docs/` went: the drive held both spellings and the next pass froze one
/// direction as a name collision. A folder counts as there only when its parent's listing names it
/// exactly as the key spells it. A disk without folder identity (the fault seam blanks it: FAT, exFAT,
/// SMB) makes the rename travel as file deletes and uploads. Played on a disk that folds case only
/// (it stops where this one does not). RED on the old code: the drive holds `Docs/a.txt` and
/// `docs/a.txt`.
#[test]
fn x_delta_sync4_29_a_folder_renamed_by_its_case_has_its_old_spelling_deleted_on_the_drive() {
    let store = S3Bucket::new();
    let a = Device::new("relook-case-a");
    faults::set_blank_folder_ids(a.path(), true);
    a.write("Docs/a.txt", b"the first file");
    a.write("Docs/b.txt", b"the second file");
    a.write("keep.txt", b"a file that stays");
    a.sync(&store);
    if !a.path().join("docs").exists() {
        // A disk that keeps `Docs` and `docs` apart cannot play this.
        faults::clear(a.path());
        return;
    }
    a.rename_folder("Docs", "docs");
    let report = a.sync(&store);
    faults::clear(a.path());
    let index = index_of(&store);
    let names: Vec<&str> = index.files.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        ["docs/a.txt", "docs/b.txt", "keep.txt"],
        "the old spelling is deleted on the drive, not held beside the new one: {}",
        report.summary()
    );
    assert!(
        report.changed_during_sync.is_empty(),
        "{:?}",
        report.changed_during_sync
    );
}

/// X-DELTA-SYNC4-3 (a). The window rule of `keep_saves` held every delete of a folder while ANY
/// rule-excluded file in it was written within the window: a database's `-wal` an app writes all day,
/// a `.DS_Store` Finder rewrites seconds after a delete, a log. None is a save of a document, and the
/// sentence ("an app is saving it") was false. Only a file an app saves through (a temp, a lock) is a
/// save in progress. RED on the old code: the delete is held, `deleted_there` is 0 and the problem
/// list names SaveInProgress.
#[test]
fn x_delta_sync4_3_a_database_side_file_and_the_systems_bookkeeping_hold_no_delete_of_the_folder() {
    let store = S3Bucket::new();
    let a = Device::new("relook-wal-a");
    a.write("old-report.pdf", b"the report");
    a.write("app.db", b"the database");
    a.sync(&store);
    // The app's journal is written all day, Finder has just rewritten its bookkeeping.
    a.write("app.db-wal", b"frames written a moment ago");
    a.write(".DS_Store", b"finder");
    a.remove("old-report.pdf");
    let mut opts = a.opts();
    opts.file_settle = Duration::from_secs(3600);
    let report = a.sync_with(&store, &opts).unwrap();
    assert!(
        !lists(&report, Code::SaveInProgress, "old-report.pdf"),
        "{:?}",
        report.problems
    );
    assert_eq!(report.deleted_there, 1, "{}", report.summary());
    assert!(!index_of(&store).files.contains_key("old-report.pdf"));
}

/// X-DELTA-SYNC4-3 (a), the sentence. A temp written moments ago holds the folder's deletes - the
/// app may be saving whatever it writes - and says so: the folder is not quiet, not "an app is
/// saving it". Green on the old code in effect, red on its words (the old text said an app saves
/// the deleted file).
#[test]
fn x_delta_sync4_3_a_hold_for_a_busy_folder_says_the_folder_is_not_quiet() {
    let store = S3Bucket::new();
    let a = Device::new("relook-busy-a");
    a.write("old-report.pdf", b"the report");
    a.sync(&store);
    a.write("~WRD0003.tmp", b"another file the app is writing");
    a.remove("old-report.pdf");
    let mut opts = a.opts();
    opts.file_settle = Duration::from_secs(3600);
    let report = a.sync_with(&store, &opts).unwrap();
    let held = report
        .problems
        .iter()
        .find(|p| p.code == Code::SaveInProgress && p.key == "old-report.pdf")
        .unwrap_or_else(|| panic!("the delete is held: {:?}", report.problems));
    assert!(
        held.detail.contains("quiet folder") && held.detail.contains("~WRD0003.tmp"),
        "{}",
        held.detail
    );
    assert_eq!(report.deleted_there, 0, "{}", report.summary());
}

/// X-DELTA-SYNC4-3 (a). The rule for a temp with the deleted file's size that cannot be read had
/// no age: the old version an app renamed aside and holds open - and a file nobody can read for
/// good (mode 000, another account's) held the delete for ever. Held while it changed within the
/// window or ten minutes, then the delete goes. The temp's times are those the scan recorded; its
/// bytes are unreadable here. RED on the old code (`keep_saves` knew no age): the stale temp holds.
#[cfg(unix)]
#[test]
fn x_delta_sync4_3_an_unreadable_same_size_temp_that_has_not_changed_for_ages_holds_no_longer() {
    use std::os::unix::fs::PermissionsExt;
    let dir = azul_storage::testing::TempDir::new("azcloud-relook-stale");
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    let held = dir.path().join("docs/~WRL0002.tmp");
    fs::write(&held, "0123456789").unwrap();
    fs::set_permissions(&held, fs::Permissions::from_mode(0o000)).unwrap();
    // A user who reads a file of mode 000 (root) cannot play this.
    if fs::read(&held).is_ok() {
        fs::set_permissions(&held, fs::Permissions::from_mode(0o644)).unwrap();
        return;
    }
    let mut base = BTreeMap::new();
    base.insert(
        String::from("docs/report.docx"),
        BaseEntry {
            size: 10,
            hash: String::from("not the temp's"),
            ..BaseEntry::default()
        },
    );
    let ns = |ago: Duration| crate::now_ns() - u64::try_from(ago.as_nanos()).unwrap();
    for (ago, holds) in [
        (Duration::from_secs(5), true),
        (Duration::from_secs(300), true),
        (Duration::from_secs(3 * 3600), false),
    ] {
        let mut scan = Scan::default();
        scan.temps.insert(
            String::from("docs/~WRL0002.tmp"),
            Temp {
                size: 10,
                mtime_ns: ns(ago),
                ctime_ns: ns(ago),
                saves: true,
            },
        );
        let mut actions = vec![Action::DeleteRemote {
            key: String::from("docs/report.docx"),
        }];
        let mut report = SyncReport::default();
        crate::sync::keep_saves(
            &mut actions,
            &scan,
            &base,
            dir.path(),
            Duration::from_secs(2),
            &mut report,
        );
        assert_eq!(
            actions.is_empty(),
            holds,
            "an unreadable temp of the size, last changed {ago:?} ago: {actions:?}"
        );
        assert_eq!(
            lists(&report, Code::SaveInProgress, "docs/report.docx"),
            holds,
            "{:?}",
            report.problems
        );
    }
    fs::set_permissions(&held, fs::Permissions::from_mode(0o644)).unwrap();
}

/// X-DELTA-SYNC4-30 (a), the disk's own rows. A copy name that the rows every system refuses accept
/// but this DISK refuses (a device name with a colon on an exFAT volume) was committed to the drive
/// and then failed its rename here: every device listed the copy as unreadable. The hold asks the
/// name policy of this disk. RED on the old code (`key_problem` only): the conflict goes on.
#[test]
fn x_delta_sync4_30_a_copy_name_this_disk_refuses_is_held_and_listed() {
    use crate::sync::{hold_unnameable_copies, names};
    let exfat = names::Policy::here(names::Volume {
        windows_names: true,
        ..names::Volume::default()
    });
    let key = String::from("notes/plan.md");
    let copy = String::from("notes/plan (conflict studio: B 2026-11-12).md");
    assert_eq!(
        names::key_problem(&copy),
        None,
        "no system refuses it outright"
    );
    let mut actions = vec![
        Action::Conflict {
            key: key.clone(),
            copy: copy.clone(),
        },
        Action::Upload {
            key: String::from("other.md"),
        },
    ];
    let mut report = SyncReport::default();
    // A disk that takes the name keeps the conflict.
    hold_unnameable_copies(
        &mut actions,
        &names::Policy::here(names::Volume::default()),
        &mut report,
    );
    assert_eq!(actions.len(), 2, "{actions:?}");
    // The exFAT volume refuses it: the conflict is held, the rest goes on.
    hold_unnameable_copies(&mut actions, &exfat, &mut report);
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert!(lists(&report, Code::BadName, &key), "{:?}", report.problems);
}

/// X-DELTA-SYNC4-30 (b), the order. The hold ran right after the plan, before a backup's one-way
/// answers: a file that changed here and on the drive is only an upload in a backup (this folder
/// wins) and needs no copy, but its conflict was dropped for its copy's name first and it was never
/// backed up. A device name with a control character makes every copy name a refused key. RED on the
/// old code: the drive keeps the old version and the problem list names a bad name.
#[test]
fn x_delta_sync4_30_a_backup_sends_the_file_whose_conflict_copy_would_have_no_name() {
    let store = S3Bucket::new();
    let a = Device::new("relook-backup-a");
    let b = Device::new("relook-backup-b");
    a.write("plan.md", b"the first version");
    a.sync(&store);
    b.sync(&store);
    // Another device changes it on the drive; this folder changes it too.
    b.write("plan.md", b"the other device's version");
    b.sync(&store);
    a.write("plan.md", b"this folder's version");
    let mut opts = a.opts();
    opts.device = String::from("studio\u{7}mac");
    let hooks = crate::sync::RunHooks {
        backup: true,
        ..crate::sync::RunHooks::default()
    };
    let report = sync_to(Target::Index(&store), &a.root(), &a.index(), &opts, &hooks).unwrap();
    assert!(
        !lists(&report, Code::BadName, "plan.md"),
        "a backup needs no copy: {:?}",
        report.problems
    );
    let index = index_of(&store);
    assert_eq!(
        index.files["plan.md"].hash,
        crate::sync::local::hash_bytes(b"this folder's version"),
        "the folder's version is backed up: {}",
        report.summary()
    );
}

/// A bucket that shows every time ten days behind this device's clock: a bucket whose clock is
/// off, or a device whose clock is far ahead.
struct TenDaysBehind(S3Bucket);

impl crate::store::RemoteStore for TenDaysBehind {
    fn get_unless(
        &self,
        key: &str,
        etag: Option<&str>,
    ) -> crate::error::CloudResult<crate::store::Conditional> {
        self.0.get_unless(key, etag)
    }
    fn fetch(&self, key: &str, size: u64) -> crate::error::CloudResult<Option<Vec<u8>>> {
        self.0.fetch(key, size)
    }
    fn put(&self, key: &str, data: &[u8]) -> crate::error::CloudResult<String> {
        self.0.put(key, data)
    }
    fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> crate::error::CloudResult<Option<String>> {
        self.0.put_if(key, data, if_match)
    }
    fn head(&self, key: &str) -> crate::error::CloudResult<Option<u64>> {
        self.0.head(key)
    }
    fn delete(&self, key: &str) -> crate::error::CloudResult<()> {
        self.0.delete(key)
    }
    fn list(&self, prefix: &str) -> crate::error::CloudResult<Vec<crate::store::RemoteObject>> {
        let mut listed = self.0.list(prefix)?;
        for object in &mut listed {
            object.modified = object.modified.map(|at| at - 10 * 86_400);
        }
        Ok(listed)
    }
}

/// X-DELTA-SYNC4-26, the missing index. `collect_garbage` read a drive whose index is not there (a
/// 404 from a proxy, a restore in progress) as an index that names nothing, and deleted every blob
/// older than the grace: the whole drive's content. A drive with no index is an error, and nothing is
/// deleted. RED on the old code: the call answers Ok and both blobs are gone.
#[test]
fn x_delta_sync4_26_a_drive_whose_index_is_missing_has_no_blob_collected() {
    let store = S3Bucket::new();
    let a = Device::new("relook-gc-missing");
    a.write("a.md", b"one");
    a.sync(&store);
    a.write("a.md", b"two");
    a.sync(&store);
    let blobs = |store: &S3Bucket| {
        store
            .keys()
            .into_iter()
            .filter(|k| k.contains("/blobs/"))
            .count()
    };
    assert_eq!(blobs(&store), 2, "{:?}", store.keys());
    // The index is not served now.
    let index_key = crate::sync::remote::index_key(PREFIX_GC);
    let held = store.read(&index_key).expect("an index");
    store.remove(&index_key);
    let refused = crate::sync::collect_garbage(&store, PREFIX_GC, -1, false);
    assert!(refused.is_err(), "no index, no collection: {refused:?}");
    assert_eq!(blobs(&store), 2, "nothing was deleted");
    // It is served again: the collection works as it did.
    store.write(&index_key, held);
    let report = crate::sync::collect_garbage(&store, PREFIX_GC, -1, false).unwrap();
    assert_eq!(report.deleted, 1, "{report:?}");
}

/// The prefix `Device::opts` syncs under.
const PREFIX_GC: &str = super::sync::PREFIX;

/// X-DELTA-SYNC4-26, the clock. The age of a blob was this device's `now` minus the bucket's time: a
/// device whose clock is days ahead of the bucket's (or a bucket whose clock is behind) made a blob
/// that was uploaded a moment ago and not committed yet look old, and the collection deleted it. The
/// age is the bucket's own: the newest write the listing shows is its "now". RED on the old code: the
/// blob a running sync uploaded is deleted.
#[test]
fn x_delta_sync4_26_a_blob_uploaded_a_moment_ago_is_young_by_the_buckets_own_clock() {
    let store = S3Bucket::new();
    let a = Device::new("relook-gc-clock");
    a.write("a.md", b"one");
    a.sync(&store);
    // A sync of another device uploaded this blob and has not committed it yet.
    let in_flight =
        crate::sync::remote::blob_key(PREFIX_GC, &crate::sync::local::hash_bytes(b"in flight"));
    store.write(&in_flight, b"in flight".to_vec());
    let behind = TenDaysBehind(store);
    let report = crate::sync::collect_garbage(&behind, PREFIX_GC, 86_400, false).unwrap();
    assert_eq!(report.deleted, 0, "{report:?}");
    assert!(behind.0.read(&in_flight).is_some(), "the blob stays");
}

/// A pairing on the plain index (the fake bucket) whose passes hear the fault seam.
fn plain(device: &str, folder: &Path, state: &Path, store: &Arc<S3Bucket>) -> Arc<SyncSession> {
    let remote: Arc<dyn crate::store::RemoteStore> = store.clone();
    let root = folder.to_path_buf();
    Arc::new(
        SyncSession::plain(
            SyncSetup::new("d_relook_plain", PREFIX, folder),
            state.to_path_buf(),
            device,
            remote,
        )
        .with_points(Arc::new(move |name: &str| faults::point(&root, name))),
    )
}

/// Runs a pass of `session` that stops at the point `at` of the pairing's folder, calls `meanwhile`
/// there and lets the pass go on; its report.
fn pass_with_a_step_at(
    session: &Arc<SyncSession>,
    folder: &Path,
    at: &str,
    meanwhile: impl FnOnce(),
) -> SyncReport {
    faults::arm_pause(folder, at);
    let running = {
        let session = Arc::clone(session);
        std::thread::spawn(move || session.pass(&AtomicBool::new(false), &mut |_| {}))
    };
    assert_eq!(
        faults::wait_paused(folder, Duration::from_secs(30)).as_deref(),
        Some(at),
        "the pass reaches {at}"
    );
    meanwhile();
    faults::release(folder);
    running
        .join()
        .expect("the pass ends")
        .expect("the pass succeeds")
        .report
        .expect("a report")
}

/// X-DELTA-SYNC4-22 (move_here). A file the drive renamed is renamed here by `move_here`, which looked
/// at the target name ("still free") and then called `Dir::rename` - a replacing rename: a file made
/// under the target name between the look and the rename (right before it, the point `before-rename`)
/// was overwritten with no trace. The rename is `rename_noreplace`; a taken name is "the next run takes
/// it". RED on the old code: the user's file holds the renamed file's bytes.
#[test]
fn x_delta_sync4_22_a_rename_made_here_never_replaces_a_file_that_appeared_at_the_target_name() {
    let store = Arc::new(S3Bucket::new());
    let (a, a_state) = (
        TempDir::new("azcloud-relook-s22-a"),
        TempDir::new("azcloud-relook-s22-a-state"),
    );
    let (b, b_state) = (
        TempDir::new("azcloud-relook-s22-b"),
        TempDir::new("azcloud-relook-s22-b-state"),
    );
    let on_a = plain("A", a.path(), a_state.path(), &store);
    let on_b = plain("B", b.path(), b_state.path(), &store);
    fs::write(a.path().join("a.txt"), "the renamed file's bytes").unwrap();
    for session in [&on_a, &on_b] {
        session
            .pass(&AtomicBool::new(false), &mut |_| {})
            .expect("the pass runs");
    }
    // A renames it; the drive has it under the new name.
    fs::rename(a.path().join("a.txt"), a.path().join("b.txt")).unwrap();
    on_a.pass(&AtomicBool::new(false), &mut |_| {})
        .expect("a sends the rename");
    // B's pass renames its copy; a file of its own appears under the new name right before.
    let b_path = b.path().to_path_buf();
    let report = pass_with_a_step_at(&on_b, b.path(), "before-rename", move || {
        fs::write(b_path.join("b.txt"), "a file B's user made under that name").unwrap();
    });
    assert_eq!(
        fs::read(b.path().join("b.txt")).unwrap(),
        b"a file B's user made under that name",
        "the file that appeared is not replaced: {}",
        report.summary()
    );
    faults::clear(b.path());
}

/// X-DELTA-SYNC4-22 (the conflict's copy). The conflict renames this side's file to the copy's name after
/// a look at the name ("must still be free"), with a replacing rename: a file that took the name since
/// (right before the rename, the point `conflict-before-rename`) was overwritten. RED on the old code:
/// the file holds this side's bytes.
#[test]
fn x_delta_sync4_22_a_conflict_copy_never_replaces_a_file_that_took_its_name_since_the_look() {
    let store = Arc::new(S3Bucket::new());
    let (a, a_state) = (
        TempDir::new("azcloud-relook-s22c-a"),
        TempDir::new("azcloud-relook-s22c-a-state"),
    );
    let (b, b_state) = (
        TempDir::new("azcloud-relook-s22c-b"),
        TempDir::new("azcloud-relook-s22c-b-state"),
    );
    let on_a = plain("A", a.path(), a_state.path(), &store);
    let on_b = plain("B", b.path(), b_state.path(), &store);
    fs::write(a.path().join("plan.txt"), "the first plan").unwrap();
    for session in [&on_a, &on_b] {
        session
            .pass(&AtomicBool::new(false), &mut |_| {})
            .expect("the pass runs");
    }
    // Both change it; A's change is on the drive first.
    fs::write(a.path().join("plan.txt"), "A's plan").unwrap();
    on_a.pass(&AtomicBool::new(false), &mut |_| {})
        .expect("a sends its plan");
    fs::write(b.path().join("plan.txt"), "B's plan, longer").unwrap();
    // The app holds the conflict as a question; the user keeps both.
    on_b.pass(&AtomicBool::new(false), &mut |_| {})
        .expect("b finds the conflict");
    on_b.resolve("plan.txt", crate::sync::session::Resolution::KeepBoth)
        .expect("the answer is taken");
    let today = crate::rfc3339(crate::now());
    let copy = crate::sync::merge::conflict_name("plan.txt", "B", &today[..10], &|_| false);
    let b_path = b.path().to_path_buf();
    let taken = copy.clone();
    let report = pass_with_a_step_at(&on_b, b.path(), "conflict-before-rename", move || {
        fs::write(b_path.join(&taken), "a file that took the copy's name").unwrap();
    });
    assert_eq!(
        fs::read(b.path().join(&copy)).unwrap(),
        b"a file that took the copy's name",
        "the file that took the name is not replaced: {}",
        report.summary()
    );
    assert!(
        report
            .changed_during_sync
            .iter()
            .any(|line| line.starts_with("plan.txt")),
        "said: {:?}",
        report.changed_during_sync
    );
    faults::clear(b.path());
}
