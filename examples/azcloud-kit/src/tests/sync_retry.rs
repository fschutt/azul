//! Bounded retries: backoff with jitter and a ceiling, Retry-After honoured, no retry of what
//! the user must fix.

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use azul_storage::{DriveError, ServiceError};

use crate::{
    error::CloudError,
    sync::retry::{classify, with_retries, Gate, PassBackoff, RetryPolicy, Verdict},
};

fn service(status: u16, retry_after: Option<u64>) -> CloudError {
    CloudError::Drive(DriveError::Service(ServiceError {
        status,
        code: String::from("E"),
        retry_after,
        ..ServiceError::default()
    }))
}

const FAST: RetryPolicy = RetryPolicy {
    attempts: 4,
    base: Duration::from_millis(1),
    ceiling: Duration::from_millis(4),
    max_wait: Duration::from_secs(5),
};

#[test]
fn the_backoff_doubles_up_to_its_ceiling_and_the_jitter_scales_it() {
    let p = RetryPolicy::REQUEST;
    assert_eq!(p.delay(1, 1.0), Duration::from_millis(100));
    assert_eq!(p.delay(2, 1.0), Duration::from_millis(200));
    assert_eq!(p.delay(30, 1.0), p.ceiling);
    assert_eq!(p.delay(3, 0.5), Duration::from_millis(200));
    assert_eq!(p.delay(3, 0.0), Duration::ZERO);
}

#[test]
fn server_errors_and_lost_connections_pass_and_what_the_user_must_fix_does_not() {
    assert_eq!(classify(&service(503, None)), Verdict::Retry(None));
    assert_eq!(
        classify(&service(429, Some(7))),
        Verdict::Retry(Some(Duration::from_secs(7)))
    );
    assert_eq!(
        classify(&CloudError::Drive(DriveError::Transport(String::from("reset")))),
        Verdict::Retry(None)
    );
    assert_eq!(classify(&service(401, None)), Verdict::GiveUp);
    assert_eq!(classify(&service(507, None)), Verdict::GiveUp);
    assert_eq!(classify(&service(400, None)), Verdict::GiveUp);
}

#[test]
fn a_request_that_fails_twice_goes_through_on_its_third_try() {
    let gate = Gate::new();
    let tries = Cell::new(0);
    let out = with_retries(&FAST, &gate, &|| 0.5, || {
        tries.set(tries.get() + 1);
        if tries.get() < 3 {
            Err(service(503, None))
        } else {
            Ok(42)
        }
    });
    assert_eq!(out.unwrap(), 42);
    assert_eq!(tries.get(), 3);
}

#[test]
fn a_request_that_keeps_failing_stops_after_its_tries() {
    let gate = Gate::new();
    let tries = Cell::new(0);
    let out: Result<(), _> = with_retries(&FAST, &gate, &|| 0.5, || {
        tries.set(tries.get() + 1);
        Err(service(500, None))
    });
    assert!(out.is_err());
    assert_eq!(tries.get(), FAST.attempts);
}

#[test]
fn refused_credentials_are_tried_once() {
    let gate = Gate::new();
    let tries = Cell::new(0);
    let out: Result<(), _> = with_retries(&FAST, &gate, &|| 0.5, || {
        tries.set(tries.get() + 1);
        Err(service(401, None))
    });
    assert!(out.is_err());
    assert_eq!(tries.get(), 1);
}

#[test]
fn after_a_429_no_request_goes_out_before_its_retry_after() {
    let gate = Gate::new();
    let sent: std::cell::RefCell<Vec<Instant>> = std::cell::RefCell::new(Vec::new());
    let start = Instant::now();
    let out = with_retries(&FAST, &gate, &|| 0.5, || {
        sent.borrow_mut().push(Instant::now());
        if sent.borrow().len() == 1 {
            Err(service(429, Some(1)))
        } else {
            Ok(())
        }
    });
    assert!(out.is_ok());
    let sent = sent.borrow();
    assert_eq!(sent.len(), 2);
    assert!(sent[1].duration_since(start) >= Duration::from_secs(1), "asked early");
    // Another request of the same pass after it waits for nothing more.
    assert!(gate.remaining().is_none());
}

#[test]
fn a_retry_after_longer_than_a_pass_waits_fails_the_request_without_sending_it() {
    let gate = Gate::new();
    gate.hold_for(Duration::from_secs(3600));
    let tries = Cell::new(0);
    let out: Result<(), _> = with_retries(&FAST, &gate, &|| 0.5, || {
        tries.set(tries.get() + 1);
        Ok(())
    });
    assert!(out.is_err());
    assert_eq!(tries.get(), 0, "nothing is sent while the drive said to wait");
}

#[test]
fn passes_are_spaced_out_after_failures_and_a_retry_after_outlives_the_process() {
    let mut b = PassBackoff::default();
    b.failed(&service(503, None), 1000.0, 1.0);
    let first = b.wait(1000.0);
    assert!(first >= PassBackoff::BASE && first <= PassBackoff::BASE * 2, "{first:?}");
    b.failed(&service(503, None), 1000.0, 1.0);
    assert!(b.wait(1000.0) > first);
    b.failed(&service(429, Some(120)), 2000.0, 0.0);
    assert_eq!(b.wait(2000.0), Duration::from_secs(120));
    let dir = azul_storage::testing::TempDir::new("retry");
    b.save(dir.path()).unwrap();
    assert_eq!(PassBackoff::load(dir.path()), b);
    b.succeeded();
    assert_eq!(b.wait(2000.0), Duration::ZERO);
}

/// A bucket that answers each request with the next planned failure, if any, and otherwise
/// as the local drive below it does; it notes when each request came and which keys a batch
/// held.
struct Flaky {
    inner: azul_storage::LocalDrive,
    planned: std::sync::Mutex<std::collections::VecDeque<Option<DriveError>>>,
    sent: std::sync::Mutex<Vec<Instant>>,
    batches: std::sync::Mutex<Vec<Vec<String>>>,
}

impl Flaky {
    fn new(dir: &azul_storage::testing::TempDir, planned: Vec<Option<DriveError>>) -> Flaky {
        Flaky {
            inner: azul_storage::LocalDrive::new(dir.path().to_path_buf()),
            planned: std::sync::Mutex::new(planned.into()),
            sent: std::sync::Mutex::new(Vec::new()),
            batches: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn answer(&self) -> Result<(), DriveError> {
        self.sent.lock().unwrap().push(Instant::now());
        match self.planned.lock().unwrap().pop_front().flatten() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

impl azul_storage::Drive for Flaky {
    fn list(
        &self,
        request: &azul_storage::ListRequest,
    ) -> Result<azul_storage::ListPage, DriveError> {
        self.answer()?;
        self.inner.list(request)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.answer()?;
        self.inner.get(key)
    }

    fn get_range(&self, key: &str, range: azul_storage::ByteRange) -> Result<Vec<u8>, DriveError> {
        self.answer()?;
        self.inner.get_range(key, range)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.answer()?;
        self.inner.put(key, bytes)
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.answer()?;
        self.inner.delete(key)
    }

    fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
        self.answer()?;
        self.inner.head(key)
    }

    fn put_many(
        &self,
        items: &[azul_storage::BatchPut<'_>],
    ) -> Vec<Result<Option<String>, DriveError>> {
        self.batches
            .lock()
            .unwrap()
            .push(items.iter().map(|i| i.key.to_string()).collect());
        items
            .iter()
            .map(|i| self.put(i.key, i.bytes).map(|()| None))
            .collect()
    }
}

fn drive_service(status: u16, retry_after: Option<u64>) -> Option<DriveError> {
    match service(status, retry_after) {
        CloudError::Drive(e) => Some(e),
        _ => None,
    }
}

fn retrying(flaky: Flaky) -> (std::sync::Arc<Flaky>, crate::sync::retry::RetryingDrive) {
    let flaky = std::sync::Arc::new(flaky);
    let mut drive = crate::sync::retry::RetryingDrive::new(flaky.clone());
    drive.policy = FAST;
    (flaky, drive)
}

#[test]
fn a_bucket_request_that_fails_for_a_moment_is_sent_again_and_lands() {
    use azul_storage::Drive;
    let dir = azul_storage::testing::TempDir::new("retry-drive");
    let (flaky, drive) = retrying(Flaky::new(
        &dir,
        vec![drive_service(503, None), drive_service(500, None)],
    ));
    drive.put("a.txt", b"a").unwrap();
    assert_eq!(flaky.sent.lock().unwrap().len(), 3);
    assert_eq!(drive.get("a.txt").unwrap(), b"a");
    // What the user must fix is answered at once.
    flaky.planned.lock().unwrap().push_back(drive_service(403, None));
    assert!(drive.get("a.txt").is_err());
    assert_eq!(flaky.sent.lock().unwrap().len(), 5);
}

#[test]
fn after_a_429_the_bucket_gets_nothing_before_its_retry_after() {
    use azul_storage::Drive;
    let dir = azul_storage::testing::TempDir::new("retry-drive-429");
    let (flaky, drive) = retrying(Flaky::new(&dir, vec![drive_service(429, Some(1))]));
    let start = Instant::now();
    drive.put("a.txt", b"a").unwrap();
    drive.head("a.txt").unwrap();
    let sent = flaky.sent.lock().unwrap();
    assert_eq!(sent.len(), 3);
    assert!(sent[1].duration_since(start) >= Duration::from_secs(1), "asked early");
}

#[test]
fn a_batch_sends_again_only_the_items_that_failed_for_a_moment() {
    use azul_storage::{BatchPut, Drive};
    let dir = azul_storage::testing::TempDir::new("retry-drive-batch");
    // The second item's put fails once.
    let (flaky, drive) = retrying(Flaky::new(&dir, vec![None, drive_service(503, None)]));
    let items = [
        BatchPut::new("a.txt", b"a"),
        BatchPut::new("b.txt", b"b"),
        BatchPut::new("c.txt", b"c"),
    ];
    let out = drive.put_many(&items);
    assert!(out.iter().all(Result::is_ok), "{out:?}");
    assert_eq!(
        *flaky.batches.lock().unwrap(),
        vec![
            vec![String::from("a.txt"), String::from("b.txt"), String::from("c.txt")],
            vec![String::from("b.txt")],
        ]
    );
    assert_eq!(drive.get("b.txt").unwrap(), b"b");
}

#[test]
fn a_streamed_body_gets_one_try_and_its_429_still_holds_every_request_back() {
    use azul_storage::Drive;
    let dir = azul_storage::testing::TempDir::new("retry-drive-stream");
    let (flaky, drive) = retrying(Flaky::new(&dir, vec![drive_service(429, Some(1))]));
    let mut body: &[u8] = b"streamed";
    assert!(drive.put_from("s.txt", &mut body).is_err());
    assert_eq!(flaky.sent.lock().unwrap().len(), 1, "a read body is not sent twice");
    assert!(drive.gate.remaining().is_some());
}

#[test]
fn rf48_a_cancel_is_given_up_at_once_never_retried_and_never_shown_as_a_service_error() {
    use azul_storage::Drive;
    assert_eq!(
        classify(&CloudError::Drive(DriveError::Cancelled)),
        Verdict::GiveUp
    );
    assert_eq!(
        crate::answer::of_drive_error(&DriveError::Cancelled),
        crate::answer::Outcome::Done
    );
    assert!(crate::user_errors::UserError::from_drive_error(&DriveError::Cancelled).is_none());
    let dir = azul_storage::testing::TempDir::new("retry-drive-cancel");
    let (flaky, drive) = retrying(Flaky::new(&dir, vec![Some(DriveError::Cancelled)]));
    assert_eq!(drive.get("a.txt"), Err(DriveError::Cancelled));
    assert_eq!(flaky.sent.lock().unwrap().len(), 1, "one try, no second");
}

/// X-DELTA-AZDRIVE-FILES-2. Home and every folder added as a drive are a `RetryingDrive` over a
/// `LocalDrive`, and the wrapper forwarded neither `site()` nor `remove_empty_folder()`: its place
/// was its own address (two slots over one tree were two unknown places, so a folder moved into
/// itself was not refused) and the trait's default tried to delete the emptied FOLDER as a file
/// (`it names a folder, not a file`), so every folder Move out of a local slot ended "failed"
/// with the folder left behind.
#[test]
fn x_delta_azdrive_files_2_a_retrying_drive_over_a_folder_is_that_folder_and_removes_an_emptied_one(
) {
    use azul_storage::{xfer::Site, Drive, LocalDrive};
    let dir = azul_storage::testing::TempDir::new("dx2-retry-local");
    let inner = std::sync::Arc::new(LocalDrive::new(dir.path().to_path_buf()));
    let drive = crate::sync::retry::RetryingDrive::new(inner.clone());
    // The place is the folder's own, not the wrapper's address.
    assert!(
        matches!(drive.site(), Site::Disk { .. }),
        "the wrapper's place: {:?}",
        drive.site()
    );
    assert_eq!(drive.site(), inner.site());
    // Two wrappers over one folder are provably one place.
    let second = crate::sync::retry::RetryingDrive::new(std::sync::Arc::new(LocalDrive::new(
        dir.path().to_path_buf(),
    )));
    assert!(drive.site().same_place(&second.site()));
    // An emptied folder is removed (rmdir), where the default tried a file's delete.
    drive.create_folder("docs/").unwrap();
    assert_eq!(drive.remove_empty_folder("docs/"), Ok(true));
    assert!(
        !dir.path().join("docs").exists(),
        "the emptied folder stays"
    );
    // A folder with a file in it stays, and says so: nothing in it is deleted.
    drive.put("keep/a.txt", b"a").unwrap();
    assert_eq!(drive.remove_empty_folder("keep/"), Ok(false));
    assert_eq!(drive.get("keep/a.txt").unwrap(), b"a");
}

/// ARCH29 C6e. `RetryingDrive`'s `impl Drive` is made by `delegate_drive!` from its
/// `DriveDelegate`: what it does not write is forwarded to the drive below, the questions about the
/// drive included. Over a folder the file of a key is the folder's file (the trait's default is
/// `None`, and every Open of a local file would be a copy). Green on the old code too: it pins what
/// the conversion must not change.
#[test]
fn c6e_a_retrying_drive_over_a_folder_forwards_the_path_of_a_key_as_the_folder_names_it() {
    use azul_storage::{Drive, LocalDrive};
    let dir = azul_storage::testing::TempDir::new("c6e-retry-forwards");
    let inner = std::sync::Arc::new(LocalDrive::new(dir.path().to_path_buf()));
    let drive = crate::sync::retry::RetryingDrive::new(inner.clone());
    drive.put("docs/a.txt", b"a").unwrap();
    let path = drive.local_path("docs/a.txt");
    assert_eq!(path, inner.local_path("docs/a.txt"));
    assert!(
        path.as_deref().is_some_and(std::path::Path::is_file),
        "the file of a key of a folder: {path:?}"
    );
    assert_eq!(drive.resume_folder(), inner.resume_folder());
    assert_eq!(drive.conditional_writes(), inner.conditional_writes());
}

/// X-DELTA-AZDRIVE-FILES-2, the cache below an encrypted Azlin drive: it keeps ciphertext copies
/// and adds no place; a batch of small files goes to the drive below as a batch, and a copy it
/// holds of a key is dropped as `put` and `put_if` drop it.
#[test]
fn x_delta_azdrive_files_2_an_object_cache_is_the_place_of_the_drive_below_and_batches_through() {
    use azul_storage::{xfer::Site, BatchPut, Drive, LocalDrive, Precondition};
    let below = azul_storage::testing::TempDir::new("dx2-cache-below");
    let kept = azul_storage::testing::TempDir::new("dx2-cache-kept");
    let inner: std::sync::Arc<dyn Drive> =
        std::sync::Arc::new(LocalDrive::new(below.path().to_path_buf()));
    let cache = crate::sync::objects::ObjectCache::new(inner.clone(), kept.path().to_path_buf());
    assert_eq!(cache.site(), inner.site());
    assert!(matches!(cache.site(), Site::Disk { .. }));
    // The batch lands, and the copy of an unconditional write is dropped before it.
    inner.put("a.txt", b"old").unwrap();
    cache.fill("a.txt").unwrap();
    assert!(cache.has("a.txt"));
    let items = [BatchPut::new("a.txt", b"new"), BatchPut::new("b.txt", b"b")];
    let out = cache.put_many(&items);
    assert!(out.iter().all(Result::is_ok), "{out:?}");
    assert!(!cache.has("a.txt"), "the copy of an overwritten key stays");
    assert_eq!(cache.get("a.txt").unwrap(), b"new");
    assert_eq!(cache.get("b.txt").unwrap(), b"b");
    // An emptied folder is the drive below's to remove.
    cache.create_folder("gone/").unwrap();
    assert_eq!(cache.remove_empty_folder("gone/"), Ok(true));
    assert!(!below.path().join("gone").exists());
    // A conditional write that lost leaves the copy.
    cache.fill("b.txt").unwrap();
    let lost = [BatchPut::new("b.txt", b"x").when(&Precondition::Absent)];
    let refused = cache.put_many(&lost);
    assert!(refused[0].is_err(), "{refused:?}");
    assert_eq!(inner.get("b.txt").unwrap(), b"b");
    assert!(
        cache.has("b.txt"),
        "a write that did not take dropped the copy"
    );
}

/// ARCH29 C6e. `ObjectCache`'s `impl Drive` is made by `delegate_drive!` from its `DriveDelegate`:
/// what the cache does not keep goes to the drive below as it is - a listing, a head, the metadata,
/// the path of a key, whether it writes conditionally - and keeps no copy. Green on the old code too:
/// it pins what the conversion must not change.
#[test]
fn c6e_an_object_cache_forwards_what_it_does_not_keep_to_the_drive_below_as_it_is() {
    use azul_storage::{Drive, ListRequest, LocalDrive};
    let below = azul_storage::testing::TempDir::new("c6e-cache-below");
    let kept = azul_storage::testing::TempDir::new("c6e-cache-kept");
    let inner = std::sync::Arc::new(LocalDrive::new(below.path().to_path_buf()));
    let cache = crate::sync::objects::ObjectCache::new(inner.clone(), kept.path().to_path_buf());
    inner.put("docs/a.txt", b"a").unwrap();
    let listed = cache.list(&ListRequest::folder("docs/"));
    assert!(
        listed.as_ref().is_ok_and(|page| page.objects.len() == 1),
        "{listed:?}"
    );
    assert_eq!(listed, inner.list(&ListRequest::folder("docs/")));
    assert_eq!(cache.head("docs/a.txt"), inner.head("docs/a.txt"));
    assert_eq!(cache.metadata("docs/a.txt"), inner.metadata("docs/a.txt"));
    assert_eq!(
        cache.local_path("docs/a.txt"),
        inner.local_path("docs/a.txt")
    );
    assert!(cache.local_path("docs/a.txt").is_some());
    assert_eq!(cache.resume_folder(), inner.resume_folder());
    assert_eq!(cache.conditional_writes(), inner.conditional_writes());
    assert!(
        !cache.has("docs/a.txt"),
        "a listing and a head keep no copy"
    );
}

/// A stop that is pressed from the outside.
struct Pressed(std::sync::atomic::AtomicBool);

impl azul_storage::Ctl for Pressed {
    fn bytes(&self, _done: u64) {}

    fn cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// X-DELTA-AZDRIVE-FILES-13 (AZ-U10). The backoff between the tries of a file's upload never asked
/// the stop: a Cancel pressed while the drive waited for its next try went unheard for the whole
/// wait, and the upload was tried again after it. Now the wait is cut short, the file is not sent
/// again, and the answer is Cancelled - not the 503 of the last try.
#[test]
fn x_delta_azdrive_files_13_a_cancel_is_heard_during_the_backoff_between_the_tries_of_an_upload() {
    use azul_storage::Drive;
    let dir = azul_storage::testing::TempDir::new("dx13-backoff");
    let file = dir.path().join("f.bin");
    std::fs::write(&file, b"x").unwrap();
    let (flaky, mut drive) = retrying(Flaky::new(&dir, vec![drive_service(503, None); 6]));
    // Thirty seconds between tries: the cancel must not wait for them.
    drive.policy = RetryPolicy {
        attempts: 4,
        base: Duration::from_secs(30),
        ceiling: Duration::from_secs(30),
        max_wait: Duration::from_secs(60),
    };
    let cancel = std::sync::Arc::new(Pressed(std::sync::atomic::AtomicBool::new(false)));
    let presser = {
        let cancel = std::sync::Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            cancel.0.store(true, std::sync::atomic::Ordering::SeqCst);
        })
    };
    let started = Instant::now();
    let result = drive.put_file("f.bin", &file, &*cancel);
    presser.join().unwrap();
    assert_eq!(result, Err(DriveError::Cancelled), "{result:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the cancel waited for the backoff: {:?}",
        started.elapsed()
    );
    assert!(
        flaky.sent.lock().unwrap().len() <= 2,
        "the file was sent again after the cancel"
    );
}
