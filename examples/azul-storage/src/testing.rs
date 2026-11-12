//! Tests only (this crate's, and the apps' through the `testing` feature): a folder of its own
//! under the system's temporary folder, removed when dropped. The one copy: azul-pim
//! re-exports it, and AzMail, AzDrive, AzReader and azul-appkit had their own
//! (scripts/waves/wave9/SMALL_FIXES.md 5.6).

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
};

/// A folder of its own under the system's temporary folder, removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    /// A new empty folder (named after the process and a counter: two parallel test runs
    /// never share one).
    ///
    /// # Panics
    /// When the folder cannot be made.
    #[must_use]
    pub fn create() -> Self {
        Self::new("test")
    }

    /// A new empty folder whose name says what it is for (`what`, as in
    /// `azul-test-<what>-<process>-<counter>`).
    ///
    /// # Panics
    /// When the folder cannot be made.
    #[must_use]
    pub fn new(what: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "azul-test-{what}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // A folder of an earlier run with the same process id.
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a temporary folder");
        TempDir(path)
    }

    /// The folder.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Binds a Unix socket named `name` in the folder `dir`, however long the folder's path is: a
/// socket's address holds 104 bytes on macOS, and the folder of a test under `$TMPDIR` is longer.
/// The bind goes through a short link to the folder in `/tmp`, removed at once (the socket stays
/// where it was made).
///
/// # Errors
///
/// When the link or the socket cannot be made.
#[cfg(unix)]
pub fn bind_unix_socket(
    dir: &Path,
    name: &str,
) -> std::io::Result<std::os::unix::net::UnixListener> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let link = Path::new("/tmp").join(format!(
        "azs-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::os::unix::fs::symlink(dir, &link)?;
    let bound = std::os::unix::net::UnixListener::bind(link.join(name));
    // The link was only the short way to the folder.
    std::fs::remove_file(&link)?;
    bound
}

/// Makes the chain `levels` deep of folders called `name` under `top` with `openat` (the path of the
/// deep ones is past what a path-based call takes), calling `each(level, folder)` on the way down: a
/// test's tree for the walks that must not recurse. The folders are made by the walk that makes (the
/// crate's own `Dir::make_child`), which no code outside this crate and its tests can reach.
///
/// # Panics
///
/// When `top` cannot be opened or a folder of the chain cannot be made.
#[cfg(unix)]
pub fn make_folder_chain(
    top: &Path,
    name: &str,
    levels: usize,
    mut each: impl FnMut(usize, &crate::beneath::Dir),
) {
    let tree = crate::beneath::Root::open(top).expect("the top of the chain opens");
    let mut at = tree.top().expect("the root's own folder");
    for level in 1..=levels {
        at = at.make_child(name).expect("a folder of the chain");
        each(level, &at);
    }
}

/// A moment `days` days ago: how old a kept copy is made when "days go by" in a test.
#[must_use]
pub fn days_ago(days: u32) -> std::time::SystemTime {
    std::time::SystemTime::now() - std::time::Duration::from_secs(u64::from(days) * 86_400)
}

/// Sets the modification time of `path` - a file, or a folder - to `time`: what time does to a kept copy,
/// its record and its folder. A link is left alone (opening it would age what it points at).
///
/// # Panics
///
/// When `path` cannot be opened or its time cannot be set.
#[cfg(unix)]
pub fn age_to(path: &Path, time: std::time::SystemTime) {
    if !path.is_symlink() {
        std::fs::File::open(path)
            .expect("the path to age opens")
            .set_modified(time)
            .expect("its time is set");
    }
}

/// `path` and everything below it made `time` old: a kept folder is as old as the newest thing in it, so
/// a test that wants one old ages what is in it too.
///
/// # Panics
///
/// As [`age_to`], and when a folder of the tree cannot be read.
#[cfg(unix)]
pub fn age_tree_to(path: &Path, time: std::time::SystemTime) {
    if path.is_dir() && !path.is_symlink() {
        for entry in std::fs::read_dir(path).expect("the folder to age is read") {
            age_tree_to(&entry.expect("an entry of the folder").path(), time);
        }
    }
    age_to(path, time);
}

/// What is left to age once the copy `name` of the kept `folder` was noted as made (the caller's own
/// `Scratch::note_made`, AFTER everything in the folder was aged to `time` by [`age_tree_to`]: a piece of the
/// open-copy procedure belongs at the line that makes it, `scratch_gate`): its record and its folder - writing
/// the record changes the folder, which makes it young again - and the check that the copy is still the file
/// that was written. A record names the time of the file it describes, so a file aged after it was noted is
/// "another file" to it (`MadeState::Changed`) and its folder the user's work, which is what a sweep must see
/// only of a file somebody really wrote. The ONE helper of the sweep tests of storage, the kit and AzDrive
/// (X-TESTFIX-AZUL2-5: it was written four times, and the first versions of three of the tests aged the files
/// after the record and went red for it).
///
/// # Panics
///
/// As [`age_to`], and when the copy is not `Untouched` afterwards: the fixture is then no copy that time has
/// aged.
#[cfg(unix)]
pub fn age_record_to(folder: &Path, name: &str, time: std::time::SystemTime) {
    use crate::scratch::{MadeState, Scratch};

    age_to(&folder.join(".made.json"), time);
    age_to(folder, time);
    assert_eq!(
        Scratch::made_state(folder, name),
        MadeState::Untouched,
        "the copy is the file that was written, long ago"
    );
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
