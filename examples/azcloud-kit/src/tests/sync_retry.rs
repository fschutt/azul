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
