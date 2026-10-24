//! What the app's session does to the synced folder itself: a delete or a rename the user asks
//! for reaches every name from the folder's root, never through a link below it.

use std::{
    fs,
    sync::{atomic::AtomicBool, Arc},
};

use azul_storage::testing::TempDir;

use super::fake_s3::S3Bucket;
use crate::{
    store::RemoteStore,
    sync::{
        local::path_of,
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
