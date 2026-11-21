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

// ==== A drive that only listens, and a call of every method (X-DELTA-STRUCT3-8) ====

/// One call a [`Probe`] heard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    /// The method the call arrived at.
    pub method: &'static str,
    /// The key it carried (the prefix of a listing), as it ARRIVED: a wrapper that rewrites keys shows here.
    pub key: String,
    /// The condition it carried: what a pinned read is pinned to.
    pub condition: Option<crate::Precondition>,
    /// The check it carried, for a checked upload (`put_file_checked`): what the file must still be when the upload
    /// completes. A wrapper that answers the call with `put_file` sends none (X-DELTA-DRIVEE-7).
    pub check: Option<crate::checked::FileCheck>,
}

/// A leaf drive that does nothing but listen: it records the calls that reach it - the method, the key as
/// it arrived, the condition, the check of a file - and answers what a drive nobody has written to answers (an empty listing,
/// no object), except a pinned read, which it answers with a few bytes. What a wrapper over it does with a
/// call is seen from below: the census of the wrappers checks that a method is NAMED, and a probe behind
/// each wrapper checks what its BODY does with it (a `get_range_if` that called `get_range` dropped the
/// condition, and passed every check of the names). A clone shares the log: the wrapper owns one, the test
/// listens through the other.
#[derive(Clone, Default)]
pub struct Probe {
    heard: std::sync::Arc<std::sync::Mutex<Vec<Heard>>>,
    /// The writes still to refuse, and with what ([`Probe::failing_first`]).
    failing: std::sync::Arc<std::sync::Mutex<(usize, Option<crate::DriveError>)>>,
}

impl Probe {
    /// A probe that has heard nothing.
    #[must_use]
    pub fn new() -> Probe {
        Probe::default()
    }

    /// A probe whose first `times` WRITES (a put, a conditional put, a batch's item, a delete, a move) answer `error` - after
    /// they were heard - and every later one is taken: what a wrapper that retries does with a call that failed for a
    /// moment is seen from here (a retry that sends a write again with another condition than the first try's turns an
    /// `If-Match` write into an overwrite of what another device just wrote).
    #[must_use]
    pub fn failing_first(self, times: usize, error: crate::DriveError) -> Probe {
        *self
            .failing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = (times, Some(error));
        self
    }

    /// What a write answers: the failure the probe was told to give while it has some left, else `taken`.
    fn written<T>(&self, taken: T) -> Result<T, crate::DriveError> {
        let mut failing = self
            .failing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match (failing.0, failing.1.clone()) {
            (left, Some(error)) if left > 0 => {
                failing.0 = left - 1;
                Err(error)
            }
            _ => Ok(taken),
        }
    }

    /// What it heard, in the order it heard it.
    #[must_use]
    pub fn heard(&self) -> Vec<Heard> {
        self.heard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn hear(&self, method: &'static str, key: &str, condition: Option<&crate::Precondition>) {
        self.note(Heard {
            method,
            key: key.to_string(),
            condition: condition.cloned(),
            check: None,
        });
    }

    /// A call that carried the check of a file.
    fn hear_check(&self, method: &'static str, key: &str, check: &crate::checked::FileCheck) {
        self.note(Heard {
            method,
            key: key.to_string(),
            condition: None,
            check: Some(check.clone()),
        });
    }

    fn note(&self, heard: Heard) {
        self.heard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(heard);
    }
}

impl crate::Drive for Probe {
    fn list(&self, request: &crate::ListRequest) -> Result<crate::ListPage, crate::DriveError> {
        self.hear("list", &request.prefix, None);
        Ok(crate::ListPage {
            folders: Vec::new(),
            objects: Vec::new(),
            next: None,
        })
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, crate::DriveError> {
        self.hear("get", key, None);
        Err(crate::DriveError::NotFound {
            key: key.to_string(),
        })
    }
    fn get_range(&self, key: &str, _range: crate::ByteRange) -> Result<Vec<u8>, crate::DriveError> {
        self.hear("get_range", key, None);
        Err(crate::DriveError::NotFound {
            key: key.to_string(),
        })
    }
    fn get_range_if(
        &self,
        key: &str,
        _range: crate::ByteRange,
        condition: &crate::Precondition,
    ) -> Result<Vec<u8>, crate::DriveError> {
        self.hear("get_range_if", key, Some(condition));
        Ok(b"pinned".to_vec())
    }
    fn put(&self, key: &str, _bytes: &[u8]) -> Result<(), crate::DriveError> {
        self.hear("put", key, None);
        self.written(())
    }
    fn delete(&self, key: &str) -> Result<(), crate::DriveError> {
        self.hear("delete", key, None);
        self.written(())
    }
    fn head(&self, key: &str) -> Result<crate::ObjectInfo, crate::DriveError> {
        self.hear("head", key, None);
        Err(crate::DriveError::NotFound {
            key: key.to_string(),
        })
    }
    // The writes that carry a CONDITION, each heard with it (X-DELTA-STRUCT4-4): a wrapper that answers `put_if` with
    // `put`, or sends it again with another condition, shows here. A batch is heard per item, under its own name.
    fn conditional_writes(&self) -> bool {
        true
    }
    fn put_if(
        &self,
        key: &str,
        _bytes: &[u8],
        condition: &crate::Precondition,
    ) -> Result<Option<String>, crate::DriveError> {
        self.hear("put_if", key, Some(condition));
        self.written(Some(String::from("tag-2")))
    }
    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn std::io::Read,
        condition: &crate::Precondition,
    ) -> Result<Option<String>, crate::DriveError> {
        let _ = std::io::copy(body, &mut std::io::sink());
        self.hear("put_from_if", key, Some(condition));
        self.written(Some(String::from("tag-2")))
    }
    fn put_file_if(
        &self,
        key: &str,
        _path: &std::path::Path,
        condition: &crate::Precondition,
        _ctl: &dyn crate::Ctl,
    ) -> Result<Option<String>, crate::DriveError> {
        self.hear("put_file_if", key, Some(condition));
        self.written(Some(String::from("tag-2")))
    }
    // A checked upload is heard with its check (X-DELTA-DRIVEE-7): a wrapper that answers it with `put_file` sends none, and
    // the probe then hears the plain write the trait's default makes of it. The file is not read: the probe takes the upload
    // as the check describes it.
    fn put_file_checked(
        &self,
        key: &str,
        _path: &std::path::Path,
        check: &crate::checked::FileCheck,
        _ctl: &dyn crate::Ctl,
    ) -> Result<crate::checked::CheckedUpload, crate::DriveError> {
        self.hear_check("put_file_checked", key, check);
        self.written(crate::checked::CheckedUpload::Sent(check.size))
    }
    fn put_many(
        &self,
        items: &[crate::BatchPut<'_>],
    ) -> Vec<Result<Option<String>, crate::DriveError>> {
        items
            .iter()
            .map(|item| {
                self.hear("put_many", item.key, item.condition);
                self.written(Some(String::from("tag-2")))
            })
            .collect()
    }
    fn put_from_many(
        &self,
        items: &mut [crate::BatchStream<'_>],
    ) -> Vec<Result<Option<String>, crate::DriveError>> {
        items
            .iter_mut()
            .map(|item| {
                let _ = std::io::copy(&mut *item.body, &mut std::io::sink());
                self.hear("put_from_many", item.key, item.condition);
                self.written(Some(String::from("tag-2")))
            })
            .collect()
    }
    fn delete_many(&self, items: &[crate::BatchDelete<'_>]) -> Vec<Result<(), crate::DriveError>> {
        items
            .iter()
            .map(|item| {
                let version = item
                    .version
                    .map(|etag| crate::Precondition::Matches(etag.to_string()));
                self.hear("delete_many", item.key, version.as_ref());
                self.written(())
            })
            .collect()
    }
    fn move_many(&self, items: &[crate::BatchMove<'_>]) -> Vec<Result<(), crate::DriveError>> {
        items
            .iter()
            .map(|item| {
                let version = item
                    .version
                    .map(|etag| crate::Precondition::Matches(etag.to_string()));
                self.hear("move_many", item.from, version.as_ref());
                self.written(())
            })
            .collect()
    }
}

/// What [`pinned_read_arrives`] found wrong with a wrapper.
#[derive(Debug, PartialEq, Eq)]
pub enum PinnedRead {
    /// The wrapper refused the pinned read with this error.
    Refused(crate::DriveError),
    /// What reached the probe is not ONE `get_range_if` carrying the condition: these calls did.
    NotPinned(Vec<Heard>),
}

/// Sends ONE pinned read - `get_range_if("object", 0..=3, Matches("tag-1"))` - through `wrapper`, which
/// stands over `probe`, and says whether it arrived as it was sent: one `get_range_if` call at the probe, with
/// the condition `Matches("tag-1")` and the key `arrives_as` (a wrapper that rewrites keys names the rewrite).
/// Anything the probe heard before the read (a drive that decides what it is from a listing first) is not
/// counted: only the calls that came after the first of them are asked about.
///
/// # Errors
///
/// [`PinnedRead`]: the wrapper refused the read, or what arrived is not that read.
pub fn pinned_read_arrives(
    wrapper: &dyn crate::Drive,
    probe: &Probe,
    arrives_as: &str,
) -> Result<(), PinnedRead> {
    let before = probe.heard().len();
    let condition = crate::Precondition::Matches(String::from("tag-1"));
    wrapper
        .get_range_if("object", crate::ByteRange::new(0, Some(3)), &condition)
        .map_err(PinnedRead::Refused)?;
    let heard: Vec<Heard> = probe
        .heard()
        .into_iter()
        .skip(before)
        .filter(|call| call.method == "get_range_if" || call.method == "get_range")
        .collect();
    let expected = Heard {
        method: "get_range_if",
        key: arrives_as.to_string(),
        condition: Some(condition),
        check: None,
    };
    if heard == [expected] {
        Ok(())
    } else {
        Err(PinnedRead::NotPinned(heard))
    }
}

/// What [`conditional_write_arrives`] and [`versioned_batches_arrive`] found wrong with a wrapper.
#[derive(Debug, PartialEq, Eq)]
pub enum ConditionalWrite {
    /// The wrapper refused the write with this error.
    Refused {
        /// The method that was called.
        call: &'static str,
        /// What the wrapper answered.
        error: crate::DriveError,
    },
    /// What reached the probe is not ONE write carrying the condition (or, for a checked upload, the check): these writes did
    /// (a write whose condition was dropped is a `put`, a retry that asked something else carries another condition, a checked
    /// upload answered with `put_file` is a `put`).
    NotConditional {
        /// The method that was called.
        call: &'static str,
        /// The writes the probe heard for it.
        heard: Vec<Heard>,
    },
}

/// What one write the probe heard must look like for the call that made it to count as arrived.
struct Arrival<'a> {
    call: &'static str,
    /// The methods at the probe that count: the call's own, and the single write the trait's default makes of it (a wrapper
    /// that does not write `put_many` itself reaches the probe with `put_if`: the condition travels the same way).
    methods: &'a [&'static str],
}

/// Sends `write` through the wrapper and says whether ONE write reached `probe` that is `arrival`'s, with `condition` and
/// the key `arrives_as`. What the probe heard from here on counts, writes only: a wrapper that looks before it writes (a
/// head, a listing) is not what is asked about.
fn arrives(
    probe: &Probe,
    arrival: &Arrival<'_>,
    condition: &crate::Precondition,
    arrives_as: &str,
    write: &mut dyn FnMut() -> Result<(), crate::DriveError>,
) -> Result<(), ConditionalWrite> {
    let before = probe.heard().len();
    write().map_err(|error| ConditionalWrite::Refused {
        call: arrival.call,
        error,
    })?;
    let heard = writes_since(probe, before);
    let arrived = matches!(
        heard.as_slice(),
        [one] if arrival.methods.contains(&one.method)
            && one.key == arrives_as
            && one.condition.as_ref() == Some(condition)
    );
    if arrived {
        Ok(())
    } else {
        Err(ConditionalWrite::NotConditional {
            call: arrival.call,
            heard,
        })
    }
}

/// What `probe` heard after its first `before` calls, writes only: a wrapper that looks before it writes (a head, a listing,
/// a read) is not what is asked about.
fn writes_since(probe: &Probe, before: usize) -> Vec<Heard> {
    probe
        .heard()
        .into_iter()
        .skip(before)
        .filter(|heard| {
            !matches!(
                heard.method,
                "list" | "get" | "get_range" | "get_range_if" | "head"
            )
        })
        .collect()
}

/// What a scan finds of `file`: its size, its modification time and its BLAKE3, and no device or inode (a scan that found
/// none). The check of a checked upload that the calls of this module send.
///
/// # Panics
///
/// When `file` cannot be read.
fn check_of(file: &Path) -> crate::checked::FileCheck {
    let meta = std::fs::metadata(file).expect("the file of the call");
    let opened = std::fs::File::open(file).expect("the file of the call opens");
    crate::checked::FileCheck {
        size: meta.len(),
        modified_ns: crate::checked::modified_ns(&meta),
        blake3: crate::checked::hash_file(opened).expect("the file of the call reads"),
        dev: 0,
        ino: 0,
    }
}

/// Sends a checked upload of `file` through the wrapper - `put_file_checked("object", file, check)` with the check a scan of
/// the file finds ([`check_of`]) - and says what is wrong with it: `None` when ONE `put_file_checked` reached `probe`, with the
/// key `arrives_as` and that very check. A wrapper that answers the call with `put_file` sends no check: an upload of a file
/// that changed while it was uploaded then completes, under the name of what the file was (X-DELTA-DRIVEE-7). An `Option` and no
/// `Result`: the error type is large (`clippy::result_large_err`), and the caller that returns it is the one place that has it.
fn check_arrives(
    wrapper: &dyn crate::Drive,
    probe: &Probe,
    file: &Path,
    arrives_as: &str,
) -> Option<ConditionalWrite> {
    let check = check_of(file);
    let before = probe.heard().len();
    if let Err(error) = wrapper.put_file_checked("object", file, &check, &crate::ctl::Quiet) {
        return Some(ConditionalWrite::Refused {
            call: "put_file_checked",
            error,
        });
    }
    let heard = writes_since(probe, before);
    let arrived = matches!(
        heard.as_slice(),
        [one] if one.method == "put_file_checked"
            && one.key == arrives_as
            && one.check.as_ref() == Some(&check)
    );
    (!arrived).then_some(ConditionalWrite::NotConditional {
        call: "put_file_checked",
        heard,
    })
}

/// Sends every write that carries a condition - `put_if`, `put_from_if`, `put_file_if`, and one item of `put_many` and
/// `put_from_many` - through `wrapper`, which stands over `probe`, with the condition `Matches("tag-1")`, and says whether
/// each arrived as it was sent: ONE write at the probe, with that condition and the key `arrives_as` (a wrapper that
/// rewrites keys names the rewrite), by the method it was sent by or by the single write the trait's default makes of it.
/// The last is the checked upload (`put_file_checked`): it arrives as ONE `put_file_checked` with the check of `file` as a
/// scan finds it. `file` is a file that exists (the one the upload of a file reads; the caller makes it, this module writes no
/// file). A wrapper that answers a conditional write with an unconditional one turns it into an overwrite of what another
/// device saved in the meantime (AZ-M1, X-DELTA-SYNC4-1), and one that answers a checked upload with `put_file` into an
/// upload of whatever the file has become: the census of the wrappers read their NAMES and the pinned read only; this reads
/// what each write does (X-DELTA-STRUCT4-4, X-DELTA-DRIVEE-7). The versions of a batch delete and a batch move are
/// [`versioned_batches_arrive`]'s.
///
/// # Errors
///
/// [`ConditionalWrite`]: the wrapper refused a write, or what arrived is not that write with its condition (its check).
pub fn conditional_write_arrives(
    wrapper: &dyn crate::Drive,
    probe: &Probe,
    file: &Path,
    arrives_as: &str,
) -> Result<(), ConditionalWrite> {
    use crate::{BatchPut, BatchStream, Precondition};

    let condition = Precondition::Matches(String::from("tag-1"));
    let quiet = crate::ctl::Quiet;
    let first = |answers: Vec<Result<Option<String>, crate::DriveError>>| {
        answers.into_iter().next().unwrap_or(Ok(None)).map(|_| ())
    };
    let put_if = Arrival {
        call: "put_if",
        methods: &["put_if"],
    };
    arrives(probe, &put_if, &condition, arrives_as, &mut || {
        wrapper.put_if("object", b"x", &condition).map(|_| ())
    })?;
    let put_from_if = Arrival {
        call: "put_from_if",
        methods: &["put_from_if", "put_if"],
    };
    arrives(probe, &put_from_if, &condition, arrives_as, &mut || {
        wrapper
            .put_from_if("object", &mut &b"x"[..], &condition)
            .map(|_| ())
    })?;
    let put_file_if = Arrival {
        call: "put_file_if",
        methods: &["put_file_if", "put_from_if", "put_if"],
    };
    arrives(probe, &put_file_if, &condition, arrives_as, &mut || {
        wrapper
            .put_file_if("object", file, &condition, &quiet)
            .map(|_| ())
    })?;
    let put_many = Arrival {
        call: "put_many",
        methods: &["put_many", "put_if"],
    };
    arrives(probe, &put_many, &condition, arrives_as, &mut || {
        first(wrapper.put_many(&[BatchPut::new("object", b"x").when(&condition)]))
    })?;
    let put_from_many = Arrival {
        call: "put_from_many",
        methods: &["put_from_many", "put_from_if", "put_if"],
    };
    arrives(probe, &put_from_many, &condition, arrives_as, &mut || {
        let mut body: &[u8] = b"x";
        first(wrapper.put_from_many(&mut [BatchStream::new("object", &mut body).when(&condition)]))
    })?;
    match check_arrives(wrapper, probe, file, arrives_as) {
        None => Ok(()),
        Some(wrong) => Err(wrong),
    }
}

/// Sends a batch delete and a batch move, each of one item with the version `tag-1`, through `wrapper`, which stands over
/// `probe`, and says whether each arrived as ONE call of the same method with that version and the key `arrives_as`. For
/// the wrappers that forward the batch methods themselves (the delegating ones, the pointers); a wrapper that leaves them to
/// the trait's defaults checks the version with a `head` first and has no such call to hear (X-DELTA-STRUCT4-4).
///
/// # Errors
///
/// [`ConditionalWrite`]: the wrapper refused a call, or what arrived is not that call with its version.
pub fn versioned_batches_arrive(
    wrapper: &dyn crate::Drive,
    probe: &Probe,
    arrives_as: &str,
) -> Result<(), ConditionalWrite> {
    use crate::{BatchDelete, BatchMove, Precondition};

    let condition = Precondition::Matches(String::from("tag-1"));
    let first_unit =
        |answers: Vec<Result<(), crate::DriveError>>| answers.into_iter().next().unwrap_or(Ok(()));
    let delete_many = Arrival {
        call: "delete_many",
        methods: &["delete_many"],
    };
    arrives(probe, &delete_many, &condition, arrives_as, &mut || {
        first_unit(wrapper.delete_many(&[BatchDelete::new("object").of_version("tag-1")]))
    })?;
    let move_many = Arrival {
        call: "move_many",
        methods: &["move_many"],
    };
    arrives(probe, &move_many, &condition, arrives_as, &mut || {
        first_unit(wrapper.move_many(&[BatchMove::new("object", "object-2").of_version("tag-1")]))
    })
}

/// What one call of [`every_method`] did.
#[derive(Debug)]
pub struct Called {
    /// The method's name, as `drive_methods!` writes it.
    pub name: &'static str,
    /// Whether it is a request to the drive (every method of the trait but the four questions about it: `site`,
    /// `local_path`, `resume_folder`, `conditional_writes`, which are answered and never refused).
    pub request: bool,
    /// What it answered; a batch's first item (the call asks for one), a yes or no or a path as `Ok`.
    pub outcome: Result<(), crate::DriveError>,
    /// What a question about the drive (`request: false`) answered, as `Debug` writes it: the place, the
    /// path, the folder, whether it writes conditionally. A drive that has nothing below it answers each of
    /// them with its "no". `None` for a request.
    pub answer: Option<String>,
}

#[doc(hidden)]
#[macro_export]
macro_rules! __probe_method_names {
    ( ; $( ($kind:ident, $required:ident) fn $name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty ; )* ) => {
        /// The name of every method of [`Drive`], from THE list (`drive_methods!`): what a call of every method
        /// ([`every_method`]) has to cover, and a method added to the trait is in it at once.
        pub const METHOD_NAMES: &[&str] = &[ $( stringify!($name) ),* ];
    };
}

crate::drive_methods!(crate::__probe_method_names;);

/// Calls every method of [`Drive`] once on `drive` with the smallest arguments that make a call (a batch of
/// one item, `file` for the uploads of a file, a reason for the calls that delete), in the order of the list,
/// and says what each answered. What a wrapper does with ALL its methods is seen from here: a stop flag that
/// must refuse every request, a drive that has nothing below it and must refuse every one. The names are
/// [`METHOD_NAMES`] and a test of this module keeps them so (a method the trait gets and this call does not
/// make fails it).
///
/// The CALLER makes what this module may not: `file` is a file of the one byte `x` that exists (a write
/// is the durable module's), and `keys` and `folder` make the reason a delete carries - the first for the
/// key `k` the calls delete, the second for the folder `f/` (a reason is for the key it names, and a test
/// item makes the reason it needs; this module is no job of a user's and names none).
///
/// # Panics
///
/// When `file` cannot be read.
#[must_use]
pub fn every_method<'a>(
    drive: &dyn crate::Drive,
    file: &Path,
    keys: impl Fn() -> crate::erase::Why<'a>,
    folder: impl Fn() -> crate::erase::Why<'a>,
) -> Vec<Called> {
    use crate::{
        BatchDelete, BatchMove, BatchPut, BatchStream, ByteRange, Limit, ListRequest, Precondition,
    };

    let check = check_of(file);
    let condition = Precondition::Matches(String::from("tag-1"));
    let quiet = crate::ctl::Quiet;
    let one = |called: Vec<Result<Option<String>, crate::DriveError>>| {
        called
            .into_iter()
            .next()
            .map_or(Ok(()), |first| first.map(|_| ()))
    };
    let first =
        |called: Vec<Result<(), crate::DriveError>>| called.into_iter().next().unwrap_or(Ok(()));
    let mut out = Vec::new();
    let mut call = |name: &'static str, request: bool, outcome: Result<(), crate::DriveError>| {
        out.push(Called {
            name,
            request,
            outcome,
            answer: None,
        });
    };
    call(
        "list",
        true,
        drive.list(&ListRequest::folder("")).map(|_| ()),
    );
    call("get", true, drive.get("k").map(|_| ()));
    call(
        "get_range",
        true,
        drive.get_range("k", ByteRange::new(0, Some(3))).map(|_| ()),
    );
    call(
        "get_range_if",
        true,
        drive
            .get_range_if("k", ByteRange::new(0, Some(3)), &condition)
            .map(|_| ()),
    );
    call(
        "get_limited",
        true,
        drive.get_limited("k", Limit::new("a call", 16)).map(|_| ()),
    );
    call("put", true, drive.put("k", b"x"));
    call("delete", true, drive.delete("k"));
    call("head", true, drive.head("k").map(|_| ()));
    call(
        "put_from",
        true,
        drive.put_from("k", &mut &b"x"[..]).map(|_| ()),
    );
    call(
        "put_if",
        true,
        drive.put_if("k", b"x", &Precondition::Absent).map(|_| ()),
    );
    call("copy", true, drive.copy("k", "k2"));
    call("create_folder", true, drive.create_folder("f/"));
    call("rename", true, drive.rename("k", "k2"));
    call("delete_folder", true, drive.delete_folder("f/"));
    call(
        "remove_empty_folder",
        true,
        drive.remove_empty_folder("f/").map(|_| ()),
    );
    let local_path = format!("{:?}", drive.local_path("k"));
    let site = format!("{:?}", drive.site());
    let resume_folder = format!("{:?}", drive.resume_folder());
    let conditional_writes = format!("{:?}", drive.conditional_writes());
    call("local_path", false, Ok(()));
    call("site", false, Ok(()));
    call("resume_folder", false, Ok(()));
    call("metadata", true, drive.metadata("k").map(|_| ()));
    call(
        "put_file",
        true,
        drive.put_file("k", file, &quiet).map(|_| ()),
    );
    call(
        "put_file_checked",
        true,
        drive
            .put_file_checked("k", file, &check, &quiet)
            .map(|_| ()),
    );
    call(
        "put_from_if",
        true,
        drive
            .put_from_if("k", &mut &b"x"[..], &Precondition::Absent)
            .map(|_| ()),
    );
    call("conditional_writes", false, Ok(()));
    call(
        "put_file_if",
        true,
        drive
            .put_file_if("k", file, &Precondition::Absent, &quiet)
            .map(|_| ()),
    );
    call(
        "put_many",
        true,
        one(drive.put_many(&[BatchPut::new("k", b"x")])),
    );
    call(
        "delete_many",
        true,
        first(drive.delete_many(&[BatchDelete::new("k")])),
    );
    call(
        "move_many",
        true,
        first(drive.move_many(&[BatchMove::new("k", "k2")])),
    );
    let mut body: &[u8] = b"x";
    call(
        "put_from_many",
        true,
        one(drive.put_from_many(&mut [BatchStream::new("k", &mut body)])),
    );
    call("delete_for", true, drive.delete_for("k", keys()));
    call(
        "delete_many_for",
        true,
        first(drive.delete_many_for(&[BatchDelete::new("k")], keys())),
    );
    call(
        "delete_folder_for",
        true,
        drive.delete_folder_for("f/", folder()),
    );
    for (name, answer) in [
        ("local_path", local_path),
        ("site", site),
        ("resume_folder", resume_folder),
        ("conditional_writes", conditional_writes),
    ] {
        if let Some(called) = out.iter_mut().find(|called| called.name == name) {
            called.answer = Some(answer);
        }
    }
    out
}
