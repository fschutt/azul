//! What one drive leaves on this computer, as the stores azul-storage's `forget_drive` takes it
//! off with (ARCH28 RF53 / AR5b-4; LANE-AZ-2).
//!
//! "Remove drive" used to delete the drive's session and queue the deletes of its keys, and leave
//! the rest: the copy of its index and the search index with the PLAIN TEXT of its files, the local
//! copies of its objects, the files a picker fetched, its last listing, the period vault with the
//! paid months, the open calls, the encryption jobs' states, the sync pairing's state, and the
//! records in the settings. The drives-file entry went FIRST, so a delete the keyring refused left
//! entries nobody could find by the drive's id. Here every one of them is a store ([`forget`]),
//! the keyring's keys, the paid months and the calls that guard them go only with the evidence
//! that the recovery code was checked ([`RemovalProof`]), and the drives-file entry goes last and
//! only when every store before it is clean.
//!
//! The names of the files live where they are written; this module asks them (`files_of`,
//! `picked_dir`, `marks_file`, `drive_index_dir`, `drive_listing_file`, `notices_file`,
//! `nodes_file_in`, `drive_cache_dir`, the state files below) and spells none of its own.
//!
//! What stays, on purpose: the empty lock files of the keyring's entries
//! (`<config>/azlin/locks/<namespace>/<entry>.<hash>.lock`), because a lock file is never
//! unlinked (a waiter that opened it before would hold a lock of a file nobody locks any more);
//! and the files the drive's own folders hold, which are the user's.

use std::path::{Path, PathBuf};

use azcloud_kit::{sync::session::SyncSetup, Outbox, PeriodTokenStore};
use azul_storage::{
    footprint::{
        forget_drive, DrivesListing, Footprint, ForgetReport, KeyringFootprint, Paths,
        RemovalProof, ScratchFootprint,
    },
    keyring::KeyringStore,
    meta::MetaIndexProvider,
    scratch::Scratch,
};

/// The folder of drive `drive`'s copy of its index (named by a hash of its id) under `root`, the
/// drive indexes' folder (`<cache>/drive-index`): the copy itself, its `search/` (the plain text
/// of its files), its `objects/` and its `packs/` journal. `None` without a root (the copies are in
/// memory).
pub(crate) fn drive_cache_dir(root: Option<PathBuf>, drive: &str) -> Option<PathBuf> {
    MetaIndexProvider::drive_cache_dir_in(root.as_deref(), drive)
}

/// Where a migration keeps its state: `<state dir>/<drive>.migration.json`.
pub(crate) fn migration_state(dir: &Path, drive: &str) -> PathBuf {
    dir.join(format!("{drive}.migration.json"))
}

/// Where the recompression pass keeps its state.
pub(crate) fn recompress_state(dir: &Path, drive: &str) -> PathBuf {
    dir.join(format!("{drive}.recompress.json"))
}

/// Where "re-encrypt everything" keeps its state.
pub(crate) fn reencrypt_state(dir: &Path, drive: &str) -> PathBuf {
    dir.join(format!("{drive}.reencrypt.json"))
}

/// A damaged state file, set aside whole (`<name>.damaged`).
pub(crate) fn damaged_aside(file: &Path) -> PathBuf {
    let mut aside = file.as_os_str().to_owned();
    aside.push(".damaged");
    PathBuf::from(aside)
}

/// Every state file the encryption jobs keep for `drive` in `dir`, the damaged ones set aside
/// included.
pub(crate) fn job_state_files(dir: &Path, drive: &str) -> Vec<PathBuf> {
    [
        migration_state(dir, drive),
        recompress_state(dir, drive),
        reencrypt_state(dir, drive),
    ]
    .into_iter()
    .flat_map(|file| {
        let aside = damaged_aside(&file);
        [file, aside]
    })
    .collect()
}

/// Whether `drive` is one plain path component - what the writers that put an id into a file or
/// folder name unchecked take it for. For an id that is not (`..`, `a/b`) the files such a writer
/// would have made are not named: nothing is removed by a name that leaves its folder.
fn is_plain(drive: &str) -> bool {
    Path::new(drive)
        .file_name()
        .is_some_and(|name| name == std::ffi::OsStr::new(drive))
}

/// Where AzDrive keeps what it keeps of its drives on this computer, resolved from the window's
/// state once, so that a worker thread can take a drive off the computer without the window.
#[derive(Clone)]
pub(crate) struct Computer {
    /// The drives file (`None`: this run has none).
    pub drives_file: Option<PathBuf>,
    /// AzDrive's cache folder (`None`: this run has none).
    pub cache_dir: Option<PathBuf>,
    /// The folder of the drives' index copies, `<cache>/drive-index`.
    pub drive_index_root: Option<PathBuf>,
    /// The folder of the encryption jobs' states.
    pub state_dir: PathBuf,
    /// The folder the sync pairings' states are kept in (`<cache>/sync`).
    pub sync_root: Option<PathBuf>,
    /// The folder of the Azlin drives' node lists.
    pub nodes_root: PathBuf,
    /// The period tokens (the vault of the paid months) of this computer.
    pub period_tokens: PeriodTokenStore,
    /// The open calls of this computer's drives.
    pub outbox: Outbox,
    /// The scratch the picker's copies wait in when there is no cache folder.
    pub picked_scratch: Scratch,
    /// Where scratch lives (`None`: the process's own, [`azul_storage::scratch::root`]): a test's
    /// folder.
    pub scratch_root: Option<PathBuf>,
}

/// One store of a drive's files and folders on this computer.
#[derive(Clone)]
pub(crate) struct Spot {
    /// The store's name, for a report.
    pub store: &'static str,
    /// What the drive has in it: each a file, or a folder with everything in it.
    pub paths: Vec<PathBuf>,
    /// It goes only with a [`RemovalProof`], like the drive's keys.
    pub only_with_proof: bool,
}

impl Spot {
    fn free(store: &'static str, paths: Vec<PathBuf>) -> Spot {
        Spot {
            store,
            paths,
            only_with_proof: false,
        }
    }

    fn with_proof(store: &'static str, paths: Vec<PathBuf>) -> Spot {
        Spot {
            store,
            paths,
            only_with_proof: true,
        }
    }
}

/// Everything one drive has on this computer apart from the keyring, and the drives file its
/// entry is in.
#[derive(Clone)]
pub(crate) struct Places {
    /// The drives file (`None`: this run has none): the entry goes last.
    pub drives_file: Option<PathBuf>,
    pub spots: Vec<Spot>,
    /// The drive's own scratch of the copies handed to apps: what the program made goes, a copy
    /// the user edited and saved stays.
    pub open_copies: Scratch,
}

impl Places {
    /// The places of `drive` on `computer`; `pairings` are its sync pairings (the settings').
    pub(crate) fn of(computer: &Computer, drive: &str, pairings: &[SyncSetup]) -> Places {
        let plain = is_plain(drive);
        let mut spots = Vec::new();

        // What was paid for the drive goes with its keys, never before: the vault may be the only
        // place the months are, and the open calls are what lets a lost answer be asked for again.
        spots.push(Spot::with_proof(
            "period vault",
            computer.period_tokens.files_of(drive),
        ));
        spots.push(Spot::with_proof(
            "open calls",
            computer.outbox.files_of(drive),
        ));
        if plain {
            spots.push(Spot::free(
                "price notices",
                vec![crate::renewals::notices_file(
                    computer.period_tokens.dir(),
                    drive,
                )],
            ));
        }

        // The copy of the drive's index with its search index (the plain text of its files) and
        // its encrypted copies, the shared search index, its last listing, what a picker fetched,
        // its trash marks.
        let mut copies = Vec::new();
        copies.extend(drive_cache_dir(computer.drive_index_root.clone(), drive));
        if let Some(cache) = &computer.cache_dir {
            copies.push(crate::find::drive_index_dir(cache, drive));
            copies.push(crate::find::drive_listing_file(cache, drive));
            if plain {
                copies.push(crate::picker::picked_dir(cache, drive));
            }
            copies.extend(crate::trash::marks_file(cache, drive));
        }
        // The rescue copy's state names every key of the drive's copy: it goes with the drive
        // (X-DELTA-AZDRIVE-FILES-34), and so does the journal of the files that landed since its
        // last whole write, which a run that was cut off leaves and which names keys too
        // (X-DELTA-SYNC2-10). The copy's files are the user's own and stay.
        if let Some(file) = &computer.drives_file {
            if let Some(state) = crate::archive::rescue_file_of(file, drive) {
                copies.push(crate::archive::landed_dir(&state));
                copies.push(state);
            }
        }
        copies.push(computer.picked_scratch.kept(drive));
        spots.push(Spot::free("copies and indexes", copies));

        if azul_storage::key::safe_file_name(drive).is_some() {
            spots.push(Spot::free(
                "node list",
                vec![crate::nodes_file_in(&computer.nodes_root, drive)],
            ));
        }
        if plain {
            spots.push(Spot::free(
                "encryption state",
                job_state_files(&computer.state_dir, drive),
            ));
        }
        if let Some(root) = &computer.sync_root {
            // The state folders of the pairings the settings name, and every other one the drive
            // has under the root: the first try of a removal takes the pairing off the list, and a
            // retry after a failed delete of its state would find no pairing and never look again
            // (X-DELTA-AZDRIVE-KEYS-32).
            let mut dirs: Vec<PathBuf> = pairings
                .iter()
                .filter(|pairing| pairing.drive_id == drive)
                .map(|pairing| pairing.state_dir(root))
                .collect();
            for dir in SyncSetup::state_dirs_of(drive, root) {
                if !dirs.contains(&dir) {
                    dirs.push(dir);
                }
            }
            spots.push(Spot::free("sync state", dirs));
        }
        Places {
            drives_file: computer.drives_file.clone(),
            spots,
            open_copies: match &computer.scratch_root {
                Some(root) => Scratch::open_copies_under(root, drive),
                // The copies of the drive's cloud files handed to apps wait in one purpose folder per drive,
                // so that a removed drive takes its own with it (X-DELTA-AZDRIVE-KEYS-32).
                None => Scratch::open_copies_of(drive),
            },
        }
    }
}

/// Takes the drive `drive` off this computer: every store is tried, and the drives file's entry
/// goes last - only when no store failed ([`forget_drive`]). `keyring` is the drive's keys and
/// session (`None` for a drive that keeps none); `proof` lets the keys, the paid months and the
/// calls that guard them go - without one they stay and the report says so.
pub(crate) fn forget(
    drive: &str,
    places: &Places,
    keyring: Option<&dyn KeyringStore>,
    proof: Option<&RemovalProof>,
) -> ForgetReport {
    let mut stores: Vec<Box<dyn Footprint + '_>> = Vec::new();
    if let Some(keyring) = keyring {
        stores.push(Box::new(KeyringFootprint::new(keyring)));
    }
    for spot in &places.spots {
        let paths = spot.paths.clone();
        let store = Paths::new(spot.store, move |_| paths.clone());
        stores.push(Box::new(if spot.only_with_proof {
            store.only_with_proof()
        } else {
            store
        }));
    }
    stores.push(Box::new(ScratchFootprint::new(
        "open copies",
        places.open_copies.clone(),
    )));
    if let Some(file) = &places.drives_file {
        stores.push(Box::new(DrivesListing::new(file)));
    }
    let stores: Vec<&dyn Footprint> = stores.iter().map(|s| s.as_ref()).collect();
    forget_drive(&stores, drive, proof)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use azcloud_kit::{azlin_contract::IdempotentScope, TokenVault, VaultState};
    use azul_storage::{
        config::{DriveEntry, DriveLocation, DrivesFile},
        crypto::{
            device::{add_recovery_code, plan_drive_key, request_key, setup_new_drive},
            keys::RecoveryKdf,
            DriveKey,
        },
        durable::{self, Mode},
        footprint::{KeyEntry, StoreOutcome},
        keyring::{KeyringError, KeyringStore, MemoryKeyring},
        meta::MemoryBucket,
        testing::TempDir,
    };

    use super::*;

    const DRIVE: &str = "d_k3f9z";
    const OTHER: &str = "d_other";

    fn cheap() -> RecoveryKdf {
        RecoveryKdf::with_cost(64, 1, 1).unwrap()
    }

    /// A proof as the removal makes one: the key permit's Permit of a removal of `drive`, asked on a
    /// keyring that holds the drive's finished setup, with a recovery code typed back right and registered
    /// and no vault of its own to lose months from. It is not made from a name: the judge makes it.
    fn a_proof_for(drive: &str) -> RemovalProof {
        use azul_storage::crypto::{
            device::key_facts,
            keystate::{ask, Evidence, Op, Read, RecoveryRecord, TokenFacts, VaultFacts, Verdict},
        };

        struct Checked;
        impl RecoveryRecord for Checked {
            fn code_checked(&self) -> Read<u64> {
                Read::Found(1_000)
            }
            fn code_registered(&self) -> Read<bool> {
                Read::Found(true)
            }
        }

        let keyring = MemoryKeyring::new();
        setup_new_drive(&MemoryBucket::new(), &keyring, drive, cheap()).unwrap();
        let mut evidence = Evidence::new(drive, 2_000);
        evidence.keyring = key_facts(&keyring, drive);
        evidence.token = TokenFacts::of(&Checked);
        evidence.vault = VaultFacts::no_vault();
        match ask(Op::Remove, &evidence) {
            Verdict::Permit(permit) => {
                RemovalProof::from_permit(permit).expect("the permit of a removal")
            }
            Verdict::Refuse(reason) => panic!("{reason}"),
        }
    }

    fn entry(id: &str) -> DriveEntry {
        DriveEntry {
            id: id.to_string(),
            name: String::from("A drive"),
            location: DriveLocation::Local {
                root: String::from("/tmp/a-drive"),
            },
        }
    }

    fn pairing(drive: &str) -> SyncSetup {
        SyncSetup::new(drive, "", Path::new("/tmp/a-drive-synced"))
    }

    /// A computer: where the drives file, the cache, the states, the vault and the open calls are.
    struct Sandbox {
        dir: TempDir,
        computer: Computer,
        keyring: MemoryKeyring,
    }

    impl Sandbox {
        fn root(&self) -> &Path {
            self.dir.path()
        }

        fn places(&self, drive: &str) -> Places {
            Places::of(&self.computer, drive, &[pairing(drive), pairing(OTHER)])
        }
    }

    fn sandbox() -> Sandbox {
        let dir = TempDir::new("footprint-sweep");
        let root = dir.path().to_path_buf();
        let computer = Computer {
            drives_file: Some(root.join("config").join("drives.json")),
            cache_dir: Some(root.join("cache")),
            drive_index_root: Some(root.join("cache").join("drive-index")),
            state_dir: root.join("config").join("encryption"),
            sync_root: Some(root.join("cache").join("sync")),
            nodes_root: root.join("cache").join("nodes"),
            period_tokens: PeriodTokenStore::new(root.join("config").join("period-tokens")),
            outbox: Outbox::new(root.join("locks").join("outbox")),
            picked_scratch: Scratch::under(&root.join("cache").join("scratch"), "picked"),
            scratch_root: Some(root.join("cache").join("scratch")),
        };
        Sandbox {
            dir,
            computer,
            keyring: MemoryKeyring::new(),
        }
    }

    /// `drive` as the flows of a drive's life leave it on a computer: the keyring entries by the
    /// production flows (setup, a further code, a planned key, the recovery request), the vault
    /// and the calls by their writers, and every other file at the place the writer's own function
    /// names, with the drive's id in its text.
    fn leave_traces(sandbox: &Sandbox, drive: &str) {
        let computer = &sandbox.computer;
        let keyring = &sandbox.keyring;
        let bucket = MemoryBucket::new();
        setup_new_drive(&bucket, keyring, drive, cheap()).unwrap();
        add_recovery_code(&bucket, keyring, drive, cheap()).unwrap();
        plan_drive_key(keyring, drive, &DriveKey::generate().unwrap()).unwrap();
        request_key(keyring, drive).unwrap();
        for (entry, what) in [
            (KeyEntry::RotationPrevious, "previous"),
            (KeyEntry::RotationNext, "next"),
            (KeyEntry::Session, "session"),
            (KeyEntry::AzlinToken, "token"),
            (KeyEntry::DeviceKey, "device key"),
        ] {
            keyring
                .set(&entry.name(drive), &format!("{drive} {what}"))
                .unwrap();
        }

        // The vault of the paid months with its second copy and its lock, and the open calls:
        // written by the kit.
        let vault = computer.period_tokens.local(drive).unwrap();
        vault
            .join(&VaultState::with_spent(&format!("{drive}-nonce")))
            .unwrap();
        vault
            .second_copy()
            .join(&VaultState::with_spent(&format!("{drive}-nonce")))
            .unwrap();
        for scope in IdempotentScope::ALL {
            let _open_call = computer.outbox.open(scope, drive, "a call").unwrap();
        }

        let put = |path: PathBuf, text: String| {
            durable::replace(&path, text.as_bytes(), Mode::Private).unwrap();
        };
        // The copies and indexes, with the plain text of the files.
        let index = drive_cache_dir(computer.drive_index_root.clone(), drive).unwrap();
        put(
            index.join("search").join("segment"),
            format!("the plain text of a file of {drive}"),
        );
        put(
            index.join("objects").join("o1"),
            format!("ciphertext of {drive}"),
        );
        put(
            index.join("packs").join("journal"),
            format!("{drive} journal"),
        );
        let cache = computer.cache_dir.as_ref().unwrap();
        put(
            crate::find::drive_index_dir(cache, drive).join("meta"),
            format!("{drive} shared index"),
        );
        put(
            crate::find::drive_listing_file(cache, drive),
            format!("{drive}\tthe names of its files"),
        );
        put(
            crate::picker::picked_dir(cache, drive).join("copy.txt"),
            format!("{drive} picked"),
        );
        put(
            computer.picked_scratch.kept(drive).join("copy.txt"),
            format!("{drive} picked, no cache folder"),
        );
        put(
            crate::trash::marks_file(cache, drive).unwrap(),
            format!("{drive} trash marks"),
        );
        // The rescue copy's state lists every key of the drive's copy (X-DELTA-AZDRIVE-FILES-34),
        // and a run that was cut off left the journal of the files that landed since its last
        // whole write, one small file each, which names keys too (X-DELTA-SYNC2-10).
        let rescue =
            crate::archive::rescue_file_of(computer.drives_file.as_ref().unwrap(), drive).unwrap();
        put(
            crate::archive::landed_dir(&rescue).join("000000000001.json"),
            format!("{{\"key\": \"{drive}/a.txt\"}}"),
        );
        put(rescue, format!("{drive} the keys of its rescue copy"));
        put(
            crate::nodes_file_in(&computer.nodes_root, drive),
            format!("{drive} nodes"),
        );
        put(
            crate::renewals::notices_file(computer.period_tokens.dir(), drive),
            format!("{drive} notices"),
        );

        // The states of the encryption jobs, and the pairing's.
        for file in job_state_files(&computer.state_dir, drive) {
            put(file, format!("{{\"drive\": \"{drive}\"}}"));
        }
        put(
            pairing(drive)
                .state_dir(computer.sync_root.as_ref().unwrap())
                .join("state"),
            format!("{drive} sync state"),
        );
        // A pairing the settings no longer name (a first try of a removal took it off the list and
        // its state could not be deleted): found by the drive's name under the sync root.
        put(
            SyncSetup::new(drive, "Old/", Path::new("/tmp/a-drive-synced-before"))
                .state_dir(computer.sync_root.as_ref().unwrap())
                .join("state"),
            format!("{drive} sync state of a pairing the settings forgot"),
        );
        // The copies handed to apps (decrypted plain text): the drive's own scratch purpose.
        let opened = Scratch::tagged_under(computer.scratch_root.as_ref().unwrap(), "open", drive);
        let copy = opened.keep(&format!("{drive}/report.docx\0v1\05")).unwrap();
        put(
            copy.join("report.docx"),
            format!("{drive} the plain text of a file"),
        );
        Scratch::note_made(&copy, "report.docx").unwrap();
        DrivesFile::update(computer.drives_file.as_ref().unwrap(), |file| {
            file.add(entry(drive))
        })
        .unwrap();
    }

    /// Every file and folder below `dir` as (path below the root, text).
    fn walk(root: &Path, dir: &Path, into: &mut Vec<(String, String)>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap().display().to_string();
            if path.is_dir() {
                into.push((rel, String::new()));
                walk(root, &path, into);
            } else {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                into.push((rel, text));
            }
        }
    }

    /// What of the sandbox holds `id` in its name or its text, keyring entries included. The empty
    /// lock files stay on purpose (the module doc says so: a lock file is never unlinked, a
    /// waiter that opened it before would hold a lock of a file nobody locks any more) and are not
    /// traces: they hold nothing, and their names are `locks`' business.
    fn holding(sandbox: &Sandbox, id: &str) -> Vec<String> {
        let mut all = Vec::new();
        walk(sandbox.root(), sandbox.root(), &mut all);
        let mut found: Vec<String> = all
            .into_iter()
            .filter(|(path, _)| !path.ends_with(".lock"))
            .filter(|(path, text)| path.contains(id) || text.contains(id))
            .map(|(path, _)| path)
            .collect();
        found.extend(
            sandbox
                .keyring
                .names()
                .into_iter()
                .filter(|name| name.contains(id)),
        );
        found.sort();
        found
    }

    /// LANE-AZ-2: the sweep. A drive's life on a computer - set up, a further recovery code, a
    /// planned key, a recovery request, a rotation under way, a vault and open calls, its index
    /// copy and search index, its listing, a picker's copies, the states of its jobs, a pairing -
    /// and then "Remove drive" with the evidence that its recovery code was checked: the sandbox
    /// is walked, and no keyring entry, file or folder NAME or TEXT holds the drive's id. Another
    /// drive's traces stay.
    #[test]
    fn lane_az_2_a_removed_drive_leaves_no_trace() {
        let sandbox = sandbox();
        leave_traces(&sandbox, DRIVE);
        leave_traces(&sandbox, OTHER);
        // Every kind of trace is there before: the sweep is not walking an empty sandbox.
        let before = holding(&sandbox, DRIVE);
        for kind in [
            "azul-storage/drive-key/",
            "azul-storage/member-key/",
            "azul-storage/recovery-request/",
            "period-tokens",
            "outbox",
            "drive-index",
            "listings",
            "picked",
            "trash",
            "rescue.json.landed/",
            "rescue.json",
            "nodes",
            "notices",
            "encryption",
            "sync",
            "drives.json",
            "azul-storage/s3/",
            "open-",
        ] {
            assert!(
                before.iter().any(|trace| trace.contains(kind)),
                "no trace of the kind {kind} before: {before:?}"
            );
        }
        let others = holding(&sandbox, OTHER);

        let proof = a_proof_for(DRIVE);
        let keyring: &dyn KeyringStore = &sandbox.keyring;
        let report = forget(DRIVE, &sandbox.places(DRIVE), Some(keyring), Some(&proof));
        assert!(report.complete(), "{report:?}");
        assert!(report.kept().is_empty(), "{report:?}");
        assert_eq!(
            holding(&sandbox, DRIVE),
            Vec::<String>::new(),
            "the drive's id is still somewhere"
        );

        // The listing is gone from the drives file, and the other drive's entry is not.
        let drives = DrivesFile::load(sandbox.computer.drives_file.as_ref().unwrap()).unwrap();
        assert!(drives.get(DRIVE).is_none());
        assert!(drives.get(OTHER).is_some());
        // Everything of the other drive is still there, in the same folders.
        assert_eq!(
            holding(&sandbox, OTHER),
            others,
            "another drive's traces stay"
        );
        assert!(matches!(
            report.stores.last().map(|s| &s.outcome),
            Some(StoreOutcome::Done(_))
        ));
        assert_eq!(report.stores.last().map(|s| s.store), Some("drives file"));
    }

    /// LANE-AZ-2: without the evidence that the recovery code was checked, the keys, the paid
    /// months in the vault and the calls that guard them stay - and say so - and everything else
    /// of the drive goes, the drives-file entry too.
    #[test]
    fn lane_az_2_without_the_proof_the_keys_the_paid_months_and_the_calls_stay() {
        let sandbox = sandbox();
        leave_traces(&sandbox, DRIVE);

        let keyring: &dyn KeyringStore = &sandbox.keyring;
        let places = sandbox.places(DRIVE);
        let held: Vec<KeyEntry> = KeyEntry::ALL
            .into_iter()
            .filter(|entry| sandbox.keyring.get(&entry.name(DRIVE)).unwrap().is_some())
            .collect();
        assert!(
            held.contains(&KeyEntry::DeviceKey) && held.contains(&KeyEntry::DriveKey),
            "the drive's life left its device key and its keys: {held:?}"
        );
        let report = forget(DRIVE, &places, Some(keyring), None);
        assert!(report.complete(), "{report:?}");
        let kept: Vec<String> = report.kept().iter().map(|k| k.what.clone()).collect();
        for entry in held {
            if entry.goes_with_proof() {
                assert!(
                    kept.contains(&entry.name(DRIVE)),
                    "{} is kept: {kept:?}",
                    entry.name(DRIVE)
                );
            } else {
                assert!(
                    sandbox.keyring.get(&entry.name(DRIVE)).unwrap().is_none(),
                    "{} is a credential and goes",
                    entry.name(DRIVE)
                );
            }
        }
        assert!(report.keys_kept());
        for spot in places.spots.iter().filter(|spot| spot.only_with_proof) {
            let there: Vec<&PathBuf> = spot.paths.iter().filter(|p| p.exists()).collect();
            assert!(
                !there.is_empty(),
                "{}: the paid months and the calls stay",
                spot.store
            );
            assert!(
                there
                    .iter()
                    .all(|p| kept.contains(&p.display().to_string())),
                "{}: what stays is said: {kept:?}",
                spot.store
            );
        }
        // The copies, the indexes, the states and the pairing's folder go.
        for spot in places.spots.iter().filter(|spot| !spot.only_with_proof) {
            assert!(
                spot.paths.iter().all(|p| !p.exists()),
                "{}: {:?}",
                spot.store,
                spot.paths
            );
        }
        let drives = DrivesFile::load(sandbox.computer.drives_file.as_ref().unwrap()).unwrap();
        assert!(drives.get(DRIVE).is_none(), "the drive is off the list");
    }

    /// X-DELTA-AZDRIVE-KEYS-32 (b): the first try of a removal takes the pairing off the settings'
    /// list; a retry after a failed delete of its state is asked for with no pairing at all, and
    /// used to find no state folder to look in. The folders of the drive are found by name.
    #[test]
    fn x_delta_azdrive_keys_32_a_retry_with_no_pairing_still_finds_the_state_folders() {
        let sandbox = sandbox();
        leave_traces(&sandbox, DRIVE);
        leave_traces(&sandbox, OTHER);
        let root = sandbox.computer.sync_root.clone().unwrap();
        let before = SyncSetup::state_dirs_of(DRIVE, &root);
        assert_eq!(before.len(), 2, "its two pairings' folders: {before:?}");

        let places = Places::of(&sandbox.computer, DRIVE, &[]);
        let proof = a_proof_for(DRIVE);
        let keyring: &dyn KeyringStore = &sandbox.keyring;
        let report = forget(DRIVE, &places, Some(keyring), Some(&proof));
        assert!(report.complete(), "{report:?}");
        assert!(
            SyncSetup::state_dirs_of(DRIVE, &root).is_empty(),
            "no state folder of the drive is left"
        );
        assert_eq!(
            SyncSetup::state_dirs_of(OTHER, &root).len(),
            2,
            "another drive's stay"
        );
    }

    /// X-DELTA-AZDRIVE-KEYS-32 (a): the device key is a key of the drive; it goes with the proof
    /// and not before (it is what gets a locked drive back to its owner).
    #[test]
    fn x_delta_azdrive_keys_32_the_device_key_waits_for_the_proof_like_the_keys() {
        let sandbox = sandbox();
        leave_traces(&sandbox, DRIVE);
        let name = KeyEntry::DeviceKey.name(DRIVE);
        let keyring: &dyn KeyringStore = &sandbox.keyring;
        let places = sandbox.places(DRIVE);

        let report = forget(DRIVE, &places, Some(keyring), None);
        assert!(report.complete(), "{report:?}");
        assert!(
            sandbox.keyring.get(&name).unwrap().is_some(),
            "kept without a proof"
        );
        assert!(
            report.kept().iter().any(|kept| kept.what == name),
            "and said"
        );

        let proof = a_proof_for(DRIVE);
        let report = forget(DRIVE, &places, Some(keyring), Some(&proof));
        assert!(report.complete(), "{report:?}");
        assert!(
            sandbox.keyring.get(&name).unwrap().is_none(),
            "gone with the proof"
        );
    }

    /// X-DELTA-AZDRIVE-KEYS-32 (c): the decrypted copies of the drive's files handed to apps go with
    /// it - they were one folder for every drive, found by nothing but their age -, a copy the
    /// user edited stays (and is no "keys kept"), another drive's copies stay.
    #[cfg(unix)]
    #[test]
    fn x_delta_azdrive_keys_32_the_open_copies_go_with_the_drive_and_the_users_edit_stays() {
        let sandbox = sandbox();
        leave_traces(&sandbox, DRIVE);
        leave_traces(&sandbox, OTHER);
        let root = sandbox.computer.scratch_root.clone().unwrap();
        let mine = Scratch::tagged_under(&root, "open", DRIVE);
        let theirs = Scratch::tagged_under(&root, "open", OTHER);
        // the user saved an edit beside a copy of the drive
        let edited = mine.keep(&format!("{DRIVE}/notes.txt\0v1\05")).unwrap();
        durable::replace(&edited.join("notes.txt"), b"cloud bytes", Mode::Private).unwrap();
        Scratch::note_made(&edited, "notes.txt").unwrap();
        durable::replace(
            &edited.join("notes (edited).txt"),
            b"my words",
            Mode::Private,
        )
        .unwrap();

        let proof = a_proof_for(DRIVE);
        let keyring: &dyn KeyringStore = &sandbox.keyring;
        let report = forget(DRIVE, &sandbox.places(DRIVE), Some(keyring), Some(&proof));
        assert!(report.complete(), "{report:?}");
        assert!(
            !report.keys_kept(),
            "a file of the user's is not keys kept: {report:?}"
        );
        assert!(report
            .kept()
            .iter()
            .all(|kept| kept.because == azul_storage::footprint::KeptBecause::UsersWork));
        assert_eq!(
            std::fs::read(edited.join("notes (edited).txt")).unwrap(),
            b"my words",
            "the user's work is not the program's to delete"
        );
        assert_eq!(
            std::fs::read_dir(mine.dir()).unwrap().count(),
            1,
            "only the folder with the user's edit is left of the drive's copies"
        );
        assert!(
            std::fs::read_dir(theirs.dir()).unwrap().count() >= 1,
            "another drive's copies stay"
        );
    }

    /// A keyring that refuses to delete one entry until it is told it may.
    struct Stubborn {
        inner: MemoryKeyring,
        refuses: String,
        allowed: AtomicBool,
    }

    impl KeyringStore for Stubborn {
        fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
            self.inner.get(key)
        }
        fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
            self.inner.set(key, secret)
        }
        fn delete(&self, key: &str) -> Result<(), KeyringError> {
            if key == self.refuses && !self.allowed.load(Ordering::SeqCst) {
                return Err(KeyringError::Denied);
            }
            self.inner.delete(key)
        }
    }

    /// LANE-AZ-2: a keyring that refuses a delete leaves the drive on the list - its other
    /// entries and files are gone, the refused one is findable by the drive's id - and removing it
    /// again, once the keyring lets go, finishes the job.
    #[test]
    fn lane_az_2_a_keyring_that_refuses_leaves_the_drive_listed_and_the_next_try_finishes() {
        let sandbox = sandbox();
        leave_traces(&sandbox, DRIVE);
        let stubborn = Stubborn {
            inner: MemoryKeyring::new(),
            refuses: KeyEntry::DriveKey.name(DRIVE),
            allowed: AtomicBool::new(false),
        };
        for name in sandbox.keyring.names() {
            let secret = sandbox.keyring.get(&name).unwrap().unwrap();
            stubborn.inner.set(&name, &secret).unwrap();
        }
        let places = sandbox.places(DRIVE);
        let proof = a_proof_for(DRIVE);
        let keyring: &dyn KeyringStore = &stubborn;

        let report = forget(DRIVE, &places, Some(keyring), Some(&proof));
        assert!(!report.complete(), "{report:?}");
        let drives = DrivesFile::load(sandbox.computer.drives_file.as_ref().unwrap()).unwrap();
        assert!(
            drives.get(DRIVE).is_some(),
            "the drive stays listed for another try"
        );
        assert_eq!(
            report.stores.last().map(|s| &s.outcome),
            Some(&StoreOutcome::Skipped)
        );
        assert!(stubborn
            .inner
            .get(&KeyEntry::DriveKey.name(DRIVE))
            .unwrap()
            .is_some());
        assert!(
            places
                .spots
                .iter()
                .filter(|spot| !spot.only_with_proof)
                .all(|spot| spot.paths.iter().all(|p| !p.exists())),
            "every other store was tried"
        );

        stubborn.allowed.store(true, Ordering::SeqCst);
        let report = forget(DRIVE, &places, Some(keyring), Some(&proof));
        assert!(report.complete(), "{report:?}");
        assert!(stubborn
            .inner
            .names()
            .iter()
            .all(|name| !name.contains(DRIVE)));
        let drives = DrivesFile::load(sandbox.computer.drives_file.as_ref().unwrap()).unwrap();
        assert!(drives.get(DRIVE).is_none());
    }

    /// LANE-AZ-2: a drive that is on a computer with no keyring (nothing of its keys) and no
    /// drives file (a run that has none) is taken off all the same; one with no cache folder
    /// takes its picked copies out of scratch.
    #[test]
    fn lane_az_2_a_drive_with_no_keyring_no_drives_file_and_no_cache_folder_is_taken_off_all_the_same(
    ) {
        let mut sandbox = sandbox();
        sandbox.computer.drives_file = None;
        sandbox.computer.cache_dir = None;
        sandbox.computer.drive_index_root = None;
        let copy = sandbox.computer.picked_scratch.kept(DRIVE).join("copy.txt");
        durable::replace(&copy, b"a copy", Mode::Private).unwrap();
        let places = sandbox.places(DRIVE);
        let report = forget(DRIVE, &places, None, None);
        assert!(report.complete(), "{report:?}");
        assert!(!copy.exists(), "the picker's copies in scratch go too");
    }

    /// LANE-AZ-2: an id that is no plain name has no files to be found by name: nothing is
    /// named that leaves the folder it is looked for in.
    #[test]
    fn lane_az_2_an_id_that_is_a_path_names_no_file_outside_its_folders() {
        let sandbox = sandbox();
        for drive in ["../escape", "a/b", "..", ""] {
            for spot in sandbox.places(drive).spots {
                for path in &spot.paths {
                    assert!(
                        !path
                            .components()
                            .any(|c| matches!(c, std::path::Component::ParentDir)),
                        "{drive:?}: {} names {}",
                        spot.store,
                        path.display()
                    );
                    assert!(
                        path.strip_prefix(sandbox.root()).is_ok(),
                        "{drive:?}: {} names {}",
                        spot.store,
                        path.display()
                    );
                }
            }
        }
    }
    /// ARCH29 C7: the registry of what a drive key touches (azul-storage `keystate::CONSUMERS`) and the
    /// stores of a removal say the same of this side of the computer. The vault of the paid months
    /// goes only with a proof - the consumer that owns the key that opens it goes with a proof too -,
    /// and the index copies and the encryption jobs' states are stores of their own that need none.
    #[test]
    fn the_registry_and_the_places_of_a_removal_name_the_same_stores_of_a_drive_key() {
        use azul_storage::crypto::keystate::{owner, Owner, CONSUMERS};

        let places = sandbox().places(DRIVE);
        let named: Vec<(&str, bool)> = places
            .spots
            .iter()
            .map(|spot| (spot.store, spot.only_with_proof))
            .collect();
        let vault = CONSUMERS
            .iter()
            .find(|consumer| consumer.name == "PERIOD_VAULT")
            .expect("the registry lists the vault");
        assert!(!vault.entries.is_empty());
        for entry in vault.entries {
            assert!(
                entry.goes_with_proof(),
                "{entry:?} opens the vault: it goes with a proof"
            );
            assert_eq!(owner(*entry), Owner::Consumer("PERIOD_VAULT"));
        }
        assert!(
            named.contains(&("period vault", true)),
            "the vault's files go with its key, only with a proof: {named:?}"
        );
        for (consumer, store) in [
            ("INDEX", "copies and indexes"),
            ("REENCRYPT_STATE", "encryption state"),
        ] {
            assert!(
                CONSUMERS.iter().any(|c| c.name == consumer),
                "{consumer} is in the registry"
            );
            assert!(
                named.contains(&(store, false)),
                "{consumer}'s files on this computer are the store {store:?}: {named:?}"
            );
        }
        // Every drive of this computer has the open calls guarded like the keys.
        assert!(named.contains(&("open calls", true)), "{named:?}");
    }
}
