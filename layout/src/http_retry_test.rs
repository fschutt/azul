//! A wait that a signal interrupts is started again until its deadline, and at the deadline it is the
//! timeout it would have been: a transport that says `Interrupted` a few times, or for ever, stands in
//! for a socket a signal hit.

use std::{
    collections::VecDeque,
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Instant,
};

use ureq::{
    unversioned::transport::{
        time::Duration, Buffers, Connector, LazyBuffers, NextTimeout, Transport,
    },
    Error, Timeout,
};

use super::*;

fn interrupted() -> Error {
    Error::Io(io::Error::from(io::ErrorKind::Interrupted))
}

fn after(ms: u64) -> NextTimeout {
    NextTimeout {
        after: Duration::from_millis(ms),
        reason: Timeout::Global,
    }
}

/// What the transport that is shown is told, and what it says.
#[derive(Debug)]
struct Script {
    reads: Mutex<VecDeque<Result<bool, Error>>>,
    writes: Mutex<VecDeque<Result<(), Error>>>,
    /// What every call says when the script is empty.
    otherwise: fn() -> Error,
    /// How long every call takes, ms (a wait that was a wait).
    takes_ms: u64,
    calls: AtomicUsize,
    /// The time each call was given, in the order of the calls.
    given: Mutex<Vec<Duration>>,
}

#[derive(Debug)]
struct Fake {
    buffers: LazyBuffers,
    script: Arc<Script>,
}

fn fake(
    reads: Vec<Result<bool, Error>>,
    writes: Vec<Result<(), Error>>,
    otherwise: fn() -> Error,
    takes_ms: u64,
) -> (Uninterrupted, Arc<Script>) {
    let script = Arc::new(Script {
        reads: Mutex::new(reads.into()),
        writes: Mutex::new(writes.into()),
        otherwise,
        takes_ms,
        calls: AtomicUsize::new(0),
        given: Mutex::new(Vec::new()),
    });
    let inner = Fake {
        buffers: LazyBuffers::new(1024, 1024),
        script: Arc::clone(&script),
    };
    (Uninterrupted(Box::new(inner)), script)
}

impl Fake {
    fn answer(&self, timeout: NextTimeout) {
        self.script.calls.fetch_add(1, Ordering::SeqCst);
        self.script
            .given
            .lock()
            .expect("the log")
            .push(timeout.after);
        if self.script.takes_ms > 0 {
            thread::sleep(std::time::Duration::from_millis(self.script.takes_ms));
        }
    }
}

impl Transport for Fake {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, _amount: usize, timeout: NextTimeout) -> Result<(), Error> {
        self.answer(timeout);
        self.script
            .writes
            .lock()
            .expect("the script")
            .pop_front()
            .unwrap_or_else(|| Err((self.script.otherwise)()))
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, Error> {
        self.answer(timeout);
        self.script
            .reads
            .lock()
            .expect("the script")
            .pop_front()
            .unwrap_or_else(|| Err((self.script.otherwise)()))
    }

    fn is_open(&mut self) -> bool {
        true
    }
}

#[test]
fn a_wait_a_signal_interrupted_once_is_started_again_and_gives_its_answer() {
    let (mut transport, script) = fake(vec![Err(interrupted()), Ok(true)], vec![], interrupted, 0);
    assert!(transport
        .await_input(after(5_000))
        .expect("the second try answers"));
    assert_eq!(script.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn a_wait_interrupted_many_times_is_started_again_each_time() {
    let mut reads: Vec<Result<bool, Error>> = (0..7).map(|_| Err(interrupted())).collect();
    reads.push(Ok(false));
    let (mut transport, script) = fake(reads, vec![], interrupted, 0);
    assert!(!transport
        .await_input(after(5_000))
        .expect("the eighth try answers"));
    assert_eq!(script.calls.load(Ordering::SeqCst), 8);
}

#[test]
fn a_write_that_a_signal_interrupted_is_started_again_too() {
    let (mut transport, script) = fake(
        vec![],
        vec![Err(interrupted()), Err(interrupted()), Ok(())],
        interrupted,
        0,
    );
    transport
        .transmit_output(10, after(5_000))
        .expect("the third try writes");
    assert_eq!(script.calls.load(Ordering::SeqCst), 3);
}

#[test]
fn at_the_deadline_a_wait_that_is_interrupted_for_ever_is_the_timeout_not_an_io_failure() {
    // every try takes 20 ms and ends in a signal: the deadline is 120 ms
    let (mut transport, script) = fake(vec![], vec![], interrupted, 20);
    let began = Instant::now();
    let error = transport
        .await_input(after(120))
        .expect_err("it never answers");
    assert!(
        matches!(error, Error::Timeout(Timeout::Global)),
        "the timeout of the deadline it was given, not {error:?}"
    );
    let took = began.elapsed();
    assert!(took >= std::time::Duration::from_millis(120), "{took:?}");
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
    assert!(script.calls.load(Ordering::SeqCst) >= 3);
}

#[test]
fn the_reason_of_the_deadline_is_the_one_it_was_given() {
    let (mut transport, _) = fake(vec![], vec![], interrupted, 10);
    let recv = NextTimeout {
        after: Duration::from_millis(40),
        reason: Timeout::RecvBody,
    };
    let error = transport.await_input(recv).expect_err("never");
    assert!(
        matches!(error, Error::Timeout(Timeout::RecvBody)),
        "{error:?}"
    );
}

#[test]
fn what_is_left_of_the_time_is_what_the_next_try_is_given() {
    let (mut transport, script) = fake(
        vec![Err(interrupted()), Err(interrupted()), Ok(true)],
        vec![],
        interrupted,
        30,
    );
    transport.await_input(after(10_000)).expect("answers");
    let given = script.given.lock().expect("the log").clone();
    assert_eq!(given.len(), 3);
    assert_eq!(given[0], Duration::from_millis(10_000));
    let ms = |d: &Duration| u64::try_from(d.as_millis()).expect("a time that is not for ever");
    assert!(
        ms(&given[1]) <= 10_000 - 30 && ms(&given[1]) > 9_000,
        "{given:?}"
    );
    assert!(
        ms(&given[2]) <= ms(&given[1]) - 30,
        "each is less than the one before: {given:?}"
    );
}

#[test]
fn a_wait_without_a_deadline_is_started_again_without_one() {
    let never = NextTimeout {
        after: Duration::NotHappening,
        reason: Timeout::Global,
    };
    let (mut transport, script) = fake(
        vec![Err(interrupted()), Err(interrupted()), Ok(true)],
        vec![],
        interrupted,
        0,
    );
    assert!(transport.await_input(never).expect("answers"));
    let given = script.given.lock().expect("the log").clone();
    assert_eq!(given, vec![Duration::NotHappening; 3]);
}

#[test]
fn a_storm_of_signals_without_a_deadline_ends_in_the_failure_it_is_not_in_a_wait_for_ever() {
    let never = NextTimeout {
        after: Duration::NotHappening,
        reason: Timeout::Global,
    };
    let (mut transport, script) = fake(vec![], vec![], interrupted, 0);
    let error = transport.await_input(never).expect_err("it never answers");
    assert!(
        matches!(&error, Error::Io(e) if e.kind() == io::ErrorKind::Interrupted),
        "{error:?}"
    );
    assert_eq!(
        script.calls.load(Ordering::SeqCst),
        MAX_RESTARTS as usize + 1
    );
}

#[test]
fn every_other_answer_is_passed_on_as_it_is_and_asked_once() {
    for (name, make) in [
        (
            "a reset",
            (|| Error::Io(io::Error::from(io::ErrorKind::ConnectionReset))) as fn() -> Error,
        ),
        ("a timeout", || Error::Timeout(Timeout::Resolve)),
        ("a bad uri", || Error::HostNotFound),
    ] {
        let (mut transport, script) = fake(vec![], vec![], make, 0);
        let error = transport.await_input(after(5_000)).expect_err(name);
        assert_eq!(format!("{error:?}"), format!("{:?}", make()), "{name}");
        assert_eq!(script.calls.load(Ordering::SeqCst), 1, "{name}");
    }
}

#[test]
fn what_the_inner_transport_says_of_itself_is_what_the_wrapper_says() {
    let (mut transport, _) = fake(vec![], vec![], interrupted, 0);
    assert!(transport.is_open());
    assert!(!transport.is_tls());
    assert!(
        !transport.buffers().can_use_input(),
        "an empty buffer has nothing to use"
    );
}

#[test]
fn the_connector_of_the_engine_makes_boxed_transports_like_the_default_one() {
    fn is_a_connector<C: Connector<(), Out = Box<dyn Transport>>>(_: &C) {}
    is_a_connector(&connector());
}
