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
    fake_s3::FakeS3,
    sync_named::{s3_drive, NamesHashes},
};
use crate::sync::{
    faults,
    session::{SyncSession, SyncSetup},
    SyncReport,
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
