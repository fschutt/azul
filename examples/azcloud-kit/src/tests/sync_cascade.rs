//! Conflicts between devices that were offline, run until every device is quiet: the copies stop at
//! N - 1, a copy of a copy is a name a folder holds, the date in a copy's name is the device's own
//! clock's, and a file in endless conflict is frozen after its third copy in an hour (EDGE.md E2a,
//! E2b, E2c; X-DELTA-EDGE-4).

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use azul_storage::testing::TempDir;

use super::fake_s3::S3Bucket;
use crate::{
    store::RemoteStore,
    sync::{
        engine::{Engine, PassSummary},
        local::path_of,
        names,
        problems::Code,
        quarantine::OwnDevices,
        remote::{index_key, RemoteIndex},
        session::{Resolution, SyncSession, SyncSetup},
        sync_to, LocalRoot, RunHooks, SyncOptions, SyncReport, Target,
    },
};

const PREFIX: &str = "cascade/";

/// One device that syncs the way the command line does: through the engine, no conflict held for
/// the user.
struct Node {
    name: &'static str,
    folder: TempDir,
    state: TempDir,
    /// Seconds this device's clock is off (the date in its copies' names reads it).
    offset: Cell<i64>,
}

impl Node {
    fn new(name: &'static str) -> Node {
        Node {
            name,
            folder: TempDir::new(&format!("cascade-{name}")),
            state: TempDir::new(&format!("cascade-state-{name}")),
            offset: Cell::new(0),
        }
    }

    fn engine(&self) -> Engine {
        Engine::new(
            self.state.path(),
            self.folder.path(),
            Box::new(OwnDevices(vec![self.name.to_string()])),
        )
    }

    /// One pass; what it did and what the engine published.
    fn pass(&self, store: &dyn RemoteStore) -> (SyncReport, PassSummary) {
        let root = LocalRoot::folder(self.folder.path());
        let mut opts = SyncOptions::new(PREFIX, "d-cascade", self.name).unwrap();
        opts.clock_offset = self.offset.get();
        let index = self.state.path().join("index.json");
        let (result, summary) = self.engine().pass(RunHooks::default(), |hooks| {
            sync_to(Target::Index(store), &root, &index, &opts, hooks)
        });
        (result.expect("the pass succeeds"), summary)
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let path = path_of(self.folder.path(), key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn read(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(path_of(self.folder.path(), key)).ok()
    }

    fn tree(&self) -> BTreeMap<String, Vec<u8>> {
        tree_of(self.folder.path())
    }
}

/// Every file of `folder` by key, the bookkeeping aside.
fn tree_of(folder: &Path) -> BTreeMap<String, Vec<u8>> {
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

/// Whether a pass changed anything: the drive or a file here.
fn wrote(report: &SyncReport) -> bool {
    report.index_written
        || report.files_up > 0
        || report.files_down > 0
        || report.deleted_here > 0
        || report.deleted_there > 0
        || !report.conflicts.is_empty()
}

/// Rounds of every device's pass, in order, until a round changes nothing anywhere; how many rounds
/// that took, the quiet one counted. Fails when `max_rounds` are not enough.
fn settle_all(nodes: &[Node], store: &dyn RemoteStore, max_rounds: usize) -> usize {
    for round in 1..=max_rounds {
        let mut quiet = true;
        for node in nodes {
            let (report, _) = node.pass(store);
            quiet &= !wrote(&report);
        }
        if quiet {
            return round;
        }
    }
    panic!("the devices did not settle within {max_rounds} rounds");
}

fn drive_index(store: &S3Bucket) -> RemoteIndex {
    RemoteIndex::parse(&store.read(&index_key(PREFIX)).expect("an index")).unwrap()
}

/// The conflict copies in `tree`: key -> bytes.
fn copies(tree: &BTreeMap<String, Vec<u8>>) -> BTreeMap<&String, &Vec<u8>> {
    tree.iter()
        .filter(|(k, _)| k.contains("(conflict "))
        .collect()
}

/// Three devices synced on `notes/a.md`, each then edits it offline: `bytes` per device.
fn three_edit(nodes: &[Node; 3], bytes: [&[u8]; 3]) {
    for (node, bytes) in nodes.iter().zip(bytes) {
        node.write("notes/a.md", bytes);
    }
}

/// X-DELTA-EDGE (E2a, T3, the unattended path). Three devices edit one file offline, with different
/// bytes, and sync in turn through the engine (the command line's path: no conflict is held). The
/// first to commit keeps the name, the other two each add a copy of their own, and the devices are
/// quiet in the third round: nothing is copied again, nothing is sent, the drive's index does not
/// grow. A characterization test: green on the first run.
#[test]
fn a_conflict_between_offline_devices_settles_in_two_rounds_and_stays_settled() {
    let store = S3Bucket::new();
    let nodes = [Node::new("A"), Node::new("B"), Node::new("C")];
    nodes[0].write("notes/a.md", b"base");
    for node in &nodes {
        node.pass(&store);
    }
    assert!(nodes
        .iter()
        .all(|n| n.read("notes/a.md").as_deref() == Some(&b"base"[..])));
    three_edit(&nodes, [b"a's edit", b"b's edit", b"c's edit"]);

    let rounds = settle_all(&nodes, &store, 6);
    assert!(rounds <= 3, "settled in {rounds} rounds");
    let trees: Vec<_> = nodes.iter().map(Node::tree).collect();
    assert_eq!(trees[0], trees[1], "A and B hold the same files");
    assert_eq!(trees[1], trees[2], "B and C hold the same files");
    assert_eq!(
        trees[0]["notes/a.md"], b"a's edit",
        "the first to commit keeps the name"
    );
    let made = copies(&trees[0]);
    assert_eq!(made.len(), 2, "N - 1 copies: {:?}", made.keys());
    let by: BTreeSet<&str> = made
        .keys()
        .map(|k| {
            ["B", "C"]
                .into_iter()
                .find(|d| k.starts_with(&format!("notes/a (conflict {d} ")) && k.ends_with(").md"))
                .unwrap_or_else(|| panic!("a copy under a name of the shape: {k}"))
        })
        .collect();
    assert_eq!(by, BTreeSet::from(["B", "C"]), "one copy each");
    assert_eq!(
        made.values().map(|v| v.as_slice()).collect::<BTreeSet<_>>(),
        BTreeSet::from([&b"b's edit"[..], b"c's edit"]),
        "each copy holds the bytes of the device that made it"
    );

    // Two more rounds: nothing is written, nothing sent, the drive does not grow.
    let before = drive_index(&store);
    let size = |i: &RemoteIndex| i.files.values().map(|f| f.size).sum::<u64>();
    store.clear_log();
    for _ in 0..2 {
        for node in &nodes {
            let (report, _) = node.pass(&store);
            assert!(!report.index_written, "{}", report.summary());
            assert_eq!((report.files_up, report.files_down), (0, 0));
            assert!(report.conflicts.is_empty());
        }
    }
    let after = drive_index(&store);
    assert_eq!(
        (after.files.len(), size(&after)),
        (before.files.len(), size(&before))
    );
    assert_eq!(after.generation, before.generation);
    assert_eq!(store.count("PUT"), 0, "no byte went up");
}

/// X-DELTA-EDGE (E2a, T3, the date). `today()` read the system's clock and ignored the device's
/// clock offset, so a device a day ahead named its copies with the other devices' day. Two waves of
/// edits of one file: the second with C's clock a day ahead. C's second copy carries another date
/// than its first, no copy is named twice, and the devices settle again in three rounds.
#[test]
fn a_copy_made_by_a_device_a_day_ahead_carries_that_devices_date() {
    let store = S3Bucket::new();
    let nodes = [Node::new("A"), Node::new("B"), Node::new("C")];
    nodes[0].write("notes/a.md", b"base");
    for node in &nodes {
        node.pass(&store);
    }
    three_edit(&nodes, [b"a1", b"b1", b"c1"]);
    assert!(settle_all(&nodes, &store, 6) <= 3);
    nodes[2].offset.set(86_400);
    three_edit(&nodes, [b"a2", b"b2", b"c2"]);
    let rounds = settle_all(&nodes, &store, 6);
    assert!(rounds <= 3, "settled in {rounds} rounds");
    let trees: Vec<_> = nodes.iter().map(Node::tree).collect();
    assert_eq!(trees[0], trees[1]);
    assert_eq!(trees[1], trees[2]);
    let made = copies(&trees[0]);
    assert_eq!(made.len(), 4, "B twice, C twice: {:?}", made.keys());
    let dates_of = |device: &str| -> BTreeSet<String> {
        made.keys()
            .filter_map(|k| k.strip_prefix(&format!("notes/a (conflict {device} ")))
            .map(|rest| rest[..10].to_string())
            .collect()
    };
    assert_eq!(
        dates_of("B").len(),
        1,
        "B's clock is the system's: {:?}",
        dates_of("B")
    );
    assert_eq!(
        dates_of("C").len(),
        2,
        "C's second copy carries the date of its clock, a day on: {:?}",
        made.keys()
    );
}

/// An app's pairing: the session holds a conflict for its user.
struct Paired {
    folder: TempDir,
    /// Kept as long as the session, which keeps its state in it.
    _state: TempDir,
    session: SyncSession,
}

impl Paired {
    fn new(name: &str, store: &Arc<S3Bucket>) -> Paired {
        let folder = TempDir::new(&format!("cascade-app-{name}"));
        let state = TempDir::new(&format!("cascade-app-state-{name}"));
        let remote: Arc<dyn RemoteStore> = store.clone();
        let session = SyncSession::plain(
            SyncSetup::new("d_cascade", PREFIX, folder.path()),
            state.path().to_path_buf(),
            name,
            remote,
        );
        Paired {
            folder,
            _state: state,
            session,
        }
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let path = path_of(self.folder.path(), key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn pass(&self) -> crate::sync::session::Pass {
        self.session
            .pass(&AtomicBool::new(false), &mut |_| {})
            .expect("the pass succeeds")
    }
}

/// X-DELTA-EDGE (E2a, T3, the app path). The same three devices through `SyncSession`: the second and
/// the third hold the conflict, the user answers "keep both" on each, and the next pass makes the
/// copy. The copies stop at N - 1, the trees are equal, and a pass after the answers writes nothing.
#[test]
fn three_devices_that_keep_both_end_with_two_copies_and_are_quiet() {
    let store = Arc::new(S3Bucket::new());
    let devices = [
        Paired::new("A", &store),
        Paired::new("B", &store),
        Paired::new("C", &store),
    ];
    devices[0].write("notes/a.md", b"base");
    for d in &devices {
        d.pass();
    }
    for (d, bytes) in devices
        .iter()
        .zip([&b"a's edit"[..], b"b's edit", b"c's edit"])
    {
        d.write("notes/a.md", bytes);
    }
    devices[0].pass();
    for d in &devices[1..] {
        let pass = d.pass();
        assert_eq!(pass.states.conflicts().len(), 1, "the conflict is held");
        d.session
            .resolve("notes/a.md", Resolution::KeepBoth)
            .unwrap();
        d.pass();
    }
    // Everyone gets everyone's copies.
    for _ in 0..2 {
        for d in &devices {
            d.pass();
        }
    }
    let trees: Vec<_> = devices.iter().map(|d| tree_of(d.folder.path())).collect();
    assert_eq!(trees[0], trees[1]);
    assert_eq!(trees[1], trees[2]);
    assert_eq!(trees[0]["notes/a.md"], b"a's edit");
    assert_eq!(copies(&trees[0]).len(), 2, "{:?}", copies(&trees[0]).keys());
    store.clear_log();
    for d in &devices {
        let pass = d.pass();
        assert!(pass.states.conflicts().is_empty());
    }
    assert_eq!(
        store.count("PUT"),
        0,
        "a pass after the answers sends nothing"
    );
}

/// X-DELTA-EDGE-4 (E2b, T4, the pass). A copy of a copy added its label to the first copy's, 33 to 60
/// bytes a level, and nothing cut it: the copy's key was committed to the drive BEFORE its local
/// rename, and a name past 255 bytes fails that rename - the copy is on the drive under a name no
/// device can hold, every device lists it as unreadable, `cut_conflicts` cannot finish it, and the
/// conflict is stuck with an error every pass. A file with a 200-byte stem conflicts with itself ten
/// times over (each level's copy is edited on both devices): no pass has an error, every key on the
/// drive is a key a folder holds, the other device lists no bad name, and the devices are quiet.
#[test]
fn a_conflict_copy_of_a_conflict_copy_gets_a_valid_name_and_a_long_name_never_commits_a_copy_no_device_can_hold(
) {
    let store = S3Bucket::new();
    let (a, b) = (Node::new("A"), Node::new("B"));
    let mut key = format!("big/{}.md", "s".repeat(200));
    a.write(&key, b"base");
    a.pass(&store);
    b.pass(&store);
    for level in 0..10 {
        a.write(&key, format!("a at {level}").as_bytes());
        b.write(&key, format!("b at {level}").as_bytes());
        a.pass(&store);
        let (report, summary) = b.pass(&store);
        assert!(
            report.errors.is_empty(),
            "level {level}: {:?}",
            report.errors
        );
        assert_eq!(
            report.conflicts.len(),
            1,
            "level {level}: {:?}",
            report.conflicts
        );
        let copy = report.conflicts[0]
            .rsplit_once(" -> ")
            .unwrap()
            .1
            .to_string();
        assert_ne!(copy, key);
        assert!(
            !summary
                .problems
                .problems
                .iter()
                .any(|p| matches!(p.code, Code::BadRemoteName | Code::InvalidHere)),
            "level {level}: {:?}",
            summary.problems.problems
        );
        let (report, summary) = a.pass(&store);
        assert!(
            report.errors.is_empty(),
            "level {level}: {:?}",
            report.errors
        );
        assert!(
            !summary
                .problems
                .problems
                .iter()
                .any(|p| matches!(p.code, Code::BadRemoteName | Code::InvalidHere)),
            "level {level}: the other device lists a name it cannot hold: {:?}",
            summary.problems.problems
        );
        key = copy;
    }
    for held in drive_index(&store).files.keys() {
        assert_eq!(names::key_problem(held), None, "{held}");
    }
    assert_eq!(a.tree(), b.tree());
    for node in [&a, &b] {
        let (report, _) = node.pass(&store);
        assert!(!wrote(&report), "{}", report.summary());
    }
}

/// X-DELTA-EDGE (E2c, T5; the owner's decision taken as recommended: at most three copies of one key
/// in an hour, then the key is listed LoopFrozen and the folder goes on with everything else). Two
/// devices alternate "edit both offline, A syncs, B syncs" six times on one key through the engine
/// (no conflict held): B made a copy each time and nothing bounded them. Now the fourth conflict is
/// frozen and listed, further passes make no copy and change nothing, other files go on syncing, and
/// the user's answer lets the key go.
#[test]
fn a_file_in_endless_conflict_is_frozen_after_its_third_copy_in_an_hour() {
    let store = S3Bucket::new();
    let (a, b) = (Node::new("A"), Node::new("B"));
    a.write("notes/a.md", b"base");
    a.write("notes/other.md", b"other");
    a.pass(&store);
    b.pass(&store);
    let mut frozen_at = None;
    let mut made = 0;
    for round in 0..6 {
        a.write("notes/a.md", format!("a {round}").as_bytes());
        b.write("notes/a.md", format!("b {round}").as_bytes());
        a.pass(&store);
        let (report, summary) = b.pass(&store);
        made += report.conflicts.len();
        let listed = summary
            .problems
            .problems
            .iter()
            .any(|p| p.code == Code::LoopFrozen && p.key == "notes/a.md");
        if listed && frozen_at.is_none() {
            frozen_at = Some(round);
        }
    }
    assert_eq!(made, 3, "three copies, then none");
    assert_eq!(frozen_at, Some(3), "the fourth conflict is the frozen one");
    let copies_on_drive = drive_index(&store)
        .files
        .keys()
        .filter(|k| k.contains("(conflict B "))
        .count();
    assert_eq!(copies_on_drive, 3);

    // A frozen key changes nothing: not here, not on the drive.
    let (here, drive) = (b.tree(), drive_index(&store));
    for _ in 0..2 {
        let (report, summary) = b.pass(&store);
        assert!(!wrote(&report), "{}", report.summary());
        assert!(summary
            .problems
            .problems
            .iter()
            .any(|p| p.code == Code::LoopFrozen && p.key == "notes/a.md"));
    }
    assert_eq!(b.tree(), here);
    assert_eq!(drive_index(&store).generation, drive.generation);
    // The rest of the folder goes on syncing.
    a.write("notes/new.md", b"written while a.md is frozen");
    a.pass(&store);
    b.pass(&store);
    assert_eq!(
        b.read("notes/new.md").as_deref(),
        Some(&b"written while a.md is frozen"[..])
    );

    // The user's answer lets the key go: the next conflict is copied again.
    b.engine().release_loop("notes/a.md").unwrap();
    let (report, _) = b.pass(&store);
    assert_eq!(report.conflicts.len(), 1, "{}", report.summary());
}

/// X-DELTA-EDGE (E2c, T5, the answer): the bound on copies is for the ones nobody asked for. Two
/// devices through `SyncSession`, B's user answers "keep both" at each of five conflicts of one file
/// within the hour: all five copies are made - `resolve` lets the key go (`Engine::release_loop`,
/// which had no caller) - and the list names no frozen file. Without the release the fourth answer
/// is frozen out.
#[test]
fn an_answer_to_a_conflict_is_never_frozen_out_by_the_bound_on_unattended_copies() {
    let store = Arc::new(S3Bucket::new());
    let (a, b) = (Paired::new("A", &store), Paired::new("B", &store));
    a.write("notes/a.md", b"base");
    a.pass();
    b.pass();
    for round in 0..5 {
        a.write("notes/a.md", format!("a {round}").as_bytes());
        b.write("notes/a.md", format!("b {round}").as_bytes());
        a.pass();
        let held = b.pass();
        assert_eq!(held.states.conflicts().len(), 1, "round {round}: held");
        b.session
            .resolve("notes/a.md", Resolution::KeepBoth)
            .unwrap();
        b.pass();
    }
    let made = tree_of(b.folder.path());
    assert_eq!(copies(&made).len(), 5, "{:?}", copies(&made).keys());
    assert!(
        !b.session
            .problems()
            .problems
            .iter()
            .any(|p| p.code == Code::LoopFrozen),
        "{:?}",
        b.session.problems().problems
    );
}
