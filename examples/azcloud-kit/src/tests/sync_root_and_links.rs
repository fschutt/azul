//! What the app's session does to the synced folder itself: a delete or a rename the user asks
//! for reaches every name from the folder's root, never through a link below it, and a pass
//! whose folder is gone fails and says why. A pairing made before folders were marked takes
//! its marker only from a folder that still holds its files.

use std::{
    fs,
    sync::{atomic::AtomicBool, Arc},
};

use azul_storage::testing::TempDir;

use super::{
    fake_s3::S3Bucket,
    sync::{index_of, Device},
};
use crate::{
    store::RemoteStore,
    sync::{
        local::{hash_bytes, path_of},
        problems::Code,
        root,
        session::{SyncSession, SyncSetup},
    },
};

/// One device's pairing through an app's session: its folder and its state folder.
struct Pairing {
    folder: TempDir,
    /// Kept so the state folder lives as long as the session that writes into it.
    _state: TempDir,
    session: SyncSession,
}

impl Pairing {
    fn new(name: &str, store: &Arc<S3Bucket>) -> Pairing {
        let folder = TempDir::new(&format!("azcloud-rootlinks-{name}"));
        let state = TempDir::new(&format!("azcloud-rootlinks-state-{name}"));
        let setup = SyncSetup::new("d_test", "Documents/", folder.path());
        let remote: Arc<dyn RemoteStore> = store.clone();
        let session = SyncSession::plain(setup, state.path().to_path_buf(), name, remote);
        Pairing {
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

    fn read(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(path_of(self.folder.path(), key)).ok()
    }

    fn pass(&self) {
        let stop = AtomicBool::new(false);
        assert!(self.session.pass(&stop, &mut |_| {}).is_ok());
    }
}

#[cfg(unix)]
#[test]
fn a_delete_or_rename_the_user_asks_for_never_reaches_through_a_link() {
    let store = Arc::new(S3Bucket::new());
    let a = Pairing::new("links-a", &store);
    let b = Pairing::new("links-b", &store);
    b.write("inbox/x.txt", b"from b");
    b.pass();
    a.write("y.txt", b"y");
    a.pass();
    // a's inbox becomes a link to a folder of the user's that holds an x.txt of its own.
    let outside = TempDir::new("azcloud-rootlinks-outside");
    fs::write(outside.path().join("x.txt"), b"outside").unwrap();
    fs::remove_dir_all(a.folder.path().join("inbox")).unwrap();
    std::os::unix::fs::symlink(outside.path(), a.folder.path().join("inbox")).unwrap();
    assert!(a.session.delete(&[String::from("inbox/x.txt")]).is_err());
    assert!(a.session.rename("y.txt", "inbox/y.txt").is_err());
    assert_eq!(fs::read(outside.path().join("x.txt")).unwrap(), b"outside");
    assert!(!outside.path().join("y.txt").exists());
    assert_eq!(a.read("y.txt").as_deref(), Some(&b"y"[..]));
}

#[test]
fn a_pass_whose_folder_is_gone_fails_says_why_and_never_makes_it_again() {
    // An unplugged disk: the folder is gone. The pass changes nothing anywhere, and the app
    // says why instead of showing the folder as up to date.
    let store = Arc::new(S3Bucket::new());
    let a = Pairing::new("gone-a", &store);
    a.write("a.txt", b"a");
    a.write("b.txt", b"b");
    a.pass();
    let synced = a.session.states().last_pass;
    fs::remove_dir_all(a.folder.path()).unwrap();
    let stop = AtomicBool::new(false);
    assert!(
        a.session.pass(&stop, &mut |_| {}).is_err(),
        "a pass that synced nothing is no pass"
    );
    assert!(!a.folder.path().exists(), "the folder is never made again");
    let states = a.session.states();
    assert!(
        states
            .last_error
            .as_deref()
            .is_some_and(|why| why.contains("is gone")),
        "{:?}",
        states.last_error
    );
    assert_eq!(states.last_pass, synced, "not synced since");
    let b = Pairing::new("gone-b", &store);
    b.pass();
    assert_eq!(
        b.read("a.txt").as_deref(),
        Some(&b"a"[..]),
        "nothing deleted on the drive"
    );
}

/// Makes the pairing look like one made before folders were marked: no marker in its base,
/// none in its folder.
fn forget_marker(device: &Device) {
    let mut base: serde_json::Value =
        serde_json::from_slice(&fs::read(device.index()).unwrap()).unwrap();
    base.as_object_mut().unwrap().remove("root_id");
    fs::write(device.index(), serde_json::to_vec(&base).unwrap()).unwrap();
    let _ = fs::remove_file(device.path().join(root::SYNC_DIR).join(root::MARKER_FILE));
}

#[test]
fn a_folder_paired_before_markers_is_marked_only_while_it_holds_a_file_as_synced() {
    // Such a base names no marker, so only its files can tell its folder: an unplugged disk's
    // empty mount point, or a stale copy written under it, holds none of them as synced.
    let store = S3Bucket::new();
    let a = Device::new("premark-a");
    a.write("a.txt", b"a");
    a.write("b.txt", b"b");
    a.sync(&store);
    forget_marker(&a);
    let report = a.sync(&store);
    assert!(report.refused.is_none(), "{}", report.summary());
    assert!(
        root::read_marker(a.path()).is_some(),
        "its files are here: it is the folder"
    );
    forget_marker(&a);
    a.remove("a.txt");
    a.remove("b.txt");
    a.write("a.txt", b"a stale copy");
    let report = a.sync(&store);
    assert!(
        report.problems.iter().any(|p| p.code == Code::RootReplaced),
        "{:?}",
        report.problems
    );
    assert!(
        root::read_marker(a.path()).is_none(),
        "a folder that is not the pairing's is not marked"
    );
    let index = index_of(&store);
    assert_eq!(
        index.files["a.txt"].hash,
        hash_bytes(b"a"),
        "the stale copy never went up"
    );
    assert!(
        index.files.contains_key("b.txt"),
        "nothing deleted on the drive"
    );
}
