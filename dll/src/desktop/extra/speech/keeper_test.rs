//! The microphone kept open between calls: opened once, found warm by the next call with what came in
//! between (up to two seconds), closed after its time, at once on a release, when it breaks, when the
//! call that had it ends badly, and when its owner goes. Driven by a scripted microphone whose sound the
//! test pushes in from outside, and real (short) times: the thread that keeps it is a real one.

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use super::{Lease, MicKeeper, PREROLL_MS};
use super::super::{
    builtin::{Microphone, MicrophoneSource},
    recognizer::Outcome,
};

const RATE: u32 = 16_000;

/// What a test sees of its microphones, and pushes into them.
#[derive(Clone, Default)]
struct Room {
    opened: Arc<AtomicU32>,
    closed: Arc<AtomicU32>,
    /// What the microphone delivers next, a chunk per read.
    sound: Arc<Mutex<VecDeque<Vec<f32>>>>,
    /// The microphone fails at its next read.
    break_it: Arc<Mutex<bool>>,
    /// The microphone cannot be opened, and says so.
    refuse: Arc<Mutex<Option<(Outcome, String)>>>,
}

impl Room {
    fn push(&self, chunk: Vec<f32>) {
        self.sound.lock().unwrap().push_back(chunk);
    }

    /// Waits until the keeper has read everything pushed (the thread that keeps the microphone does).
    fn wait_until_read(&self) {
        let began = Instant::now();
        while !self.sound.lock().unwrap().is_empty() {
            assert!(began.elapsed() < Duration::from_secs(5), "the keeper never read what came in");
            std::thread::sleep(Duration::from_millis(5));
        }
        // And the chunk it holds is in the buffer.
        std::thread::sleep(Duration::from_millis(60));
    }

    fn opened(&self) -> u32 {
        self.opened.load(Ordering::SeqCst)
    }

    fn closed(&self) -> u32 {
        self.closed.load(Ordering::SeqCst)
    }

    /// Waits up to two seconds for `closed()` to reach `count`.
    fn wait_closed(&self, count: u32) -> bool {
        let began = Instant::now();
        while self.closed() < count {
            if began.elapsed() > Duration::from_secs(2) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }
}

struct RoomSource(Room);

impl MicrophoneSource for RoomSource {
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
        if let Some(refusal) = self.0.refuse.lock().unwrap().clone() {
            return Err(refusal);
        }
        self.0.opened.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(RoomMicrophone(self.0.clone())))
    }
}

struct RoomMicrophone(Room);

impl Microphone for RoomMicrophone {
    fn rate(&self) -> u32 {
        RATE
    }

    fn read(&mut self, out: &mut Vec<f32>, _wait_ms: u32) -> Result<(), String> {
        if *self.0.break_it.lock().unwrap() {
            return Err("the device went away".to_string());
        }
        if let Some(chunk) = self.0.sound.lock().unwrap().pop_front() {
            out.extend(chunk);
        }
        Ok(())
    }
}

impl Drop for RoomMicrophone {
    fn drop(&mut self) {
        self.0.closed.fetch_add(1, Ordering::SeqCst);
    }
}

fn keeper(room: &Room, keep_ms: u32) -> MicKeeper {
    let keeper = MicKeeper::new(Box::new(RoomSource(room.clone())));
    keeper.set_keep_ms(keep_ms);
    keeper
}

fn samples(from: usize, count: usize) -> Vec<f32> {
    (from..from + count).map(|i| i as f32).collect()
}

#[test]
fn a_microphone_that_is_not_kept_is_opened_for_each_call_and_closed_at_its_end() {
    let room = Room::default();
    let keeper = keeper(&room, 0);
    assert_eq!(keeper.keep_ms(), 0);
    let lease = keeper.acquire().expect("a microphone");
    assert!(!lease.warm && lease.preroll.is_empty());
    assert_eq!(room.opened(), 1);
    lease.finish(true);
    assert_eq!((room.closed(), keeper.is_held()), (1, false), "nothing is kept when the time is zero");
    let lease = keeper.acquire().expect("a microphone");
    assert!(!lease.warm);
    assert_eq!(room.opened(), 2);
}

#[test]
fn a_kept_microphone_is_found_warm_by_the_next_call_with_what_came_in_between() {
    let room = Room::default();
    let keeper = keeper(&room, 5_000);
    let lease = keeper.acquire().expect("a microphone");
    assert!(!lease.warm);
    lease.finish(true);
    assert!(keeper.is_held());
    assert_eq!(room.closed(), 0, "kept open, not closed");
    // Speech begins in the gap between two calls.
    room.push(samples(0, 1_600));
    room.push(samples(1_600, 1_600));
    room.wait_until_read();
    let again = keeper.acquire().expect("the kept microphone");
    assert!(again.warm, "no microphone was opened for the second call");
    assert_eq!(room.opened(), 1);
    assert_eq!(again.preroll, samples(0, 3_200), "what came in between, oldest first");
    assert!(!keeper.is_held(), "a call that has the microphone is not the keeper's");
    again.finish(true);
    assert!(keeper.is_held());
}

#[test]
fn what_came_in_while_nobody_listened_is_kept_to_two_seconds_with_the_newest_last() {
    let room = Room::default();
    let keeper = keeper(&room, 5_000);
    keeper.acquire().expect("a microphone").finish(true);
    // Five seconds in chunks of a tenth of a second, each sample numbered.
    for chunk in 0..50 {
        room.push(samples(chunk * 1_600, 1_600));
    }
    room.wait_until_read();
    let lease = keeper.acquire().expect("the kept microphone");
    let cap = (RATE as usize) * (PREROLL_MS as usize) / 1000;
    assert_eq!(lease.preroll.len(), cap, "two seconds, no more");
    assert_eq!(*lease.preroll.last().unwrap(), (50 * 1_600 - 1) as f32, "the newest sample is the last");
    assert_eq!(lease.preroll[0], (50 * 1_600 - cap) as f32, "the oldest were let go");
}

#[test]
fn a_kept_microphone_is_closed_after_its_time_even_when_nobody_asks() {
    let room = Room::default();
    let keeper = keeper(&room, 80);
    keeper.acquire().expect("a microphone").finish(true);
    assert!(keeper.is_held());
    assert!(room.wait_closed(1), "the microphone stayed open past its time");
    assert!(!keeper.is_held());
    // The next call opens a new one.
    let lease = keeper.acquire().expect("a new microphone");
    assert!(!lease.warm);
    assert_eq!(room.opened(), 2);
}

#[test]
fn a_release_and_a_time_of_zero_close_a_kept_microphone_at_once() {
    let room = Room::default();
    let keeper = keeper(&room, 5_000);
    keeper.acquire().expect("a microphone").finish(true);
    assert!(keeper.is_held());
    keeper.release();
    assert_eq!((room.closed(), keeper.is_held()), (1, false));
    keeper.release();
    assert_eq!(room.closed(), 1, "releasing nothing does nothing");
    keeper.acquire().expect("a microphone").finish(true);
    keeper.set_keep_ms(0);
    assert_eq!((room.closed(), keeper.is_held()), (2, false), "a time of zero gives it up now");
}

#[test]
fn a_microphone_that_breaks_while_kept_is_dropped_and_the_next_call_opens_another() {
    let room = Room::default();
    let keeper = keeper(&room, 5_000);
    keeper.acquire().expect("a microphone").finish(true);
    *room.break_it.lock().unwrap() = true;
    assert!(room.wait_closed(1), "the broken microphone was not given up");
    *room.break_it.lock().unwrap() = false;
    let lease = keeper.acquire().expect("a new microphone");
    assert!(!lease.warm);
    assert_eq!(room.opened(), 2);
}

#[test]
fn a_call_that_ends_badly_or_is_dropped_closes_the_microphone_instead_of_keeping_it() {
    let room = Room::default();
    let keeper = keeper(&room, 5_000);
    // Cancelled, or failed: finish(false).
    keeper.acquire().expect("a microphone").finish(false);
    assert_eq!((room.closed(), keeper.is_held()), (1, false));
    // A call that returned early without saying anything: the lease is dropped.
    let lease = keeper.acquire().expect("a microphone");
    drop(lease);
    assert_eq!((room.closed(), keeper.is_held()), (2, false));
}

#[test]
fn a_microphone_that_cannot_be_opened_is_the_sources_refusal() {
    let room = Room::default();
    *room.refuse.lock().unwrap() = Some((Outcome::Denied, "the microphone is turned off for Terminal".to_string()));
    let keeper = keeper(&room, 5_000);
    let Err((outcome, message)) = keeper.acquire() else {
        panic!("a refused microphone gave a lease");
    };
    assert_eq!(outcome, Outcome::Denied);
    assert!(message.contains("Terminal"), "{message}");
    assert!(!keeper.is_held());
}

#[test]
fn when_the_keeper_goes_the_kept_microphone_closes() {
    let room = Room::default();
    let keeper = keeper(&room, 60_000);
    keeper.acquire().expect("a microphone").finish(true);
    let other_handle = keeper.clone();
    drop(keeper);
    assert_eq!(room.closed(), 0, "a handle is left: it is still the session's");
    drop(other_handle);
    assert!(room.wait_closed(1), "the last handle went and the microphone stayed open");
}

/// A microphone that waits for sound the way a device does: a read takes its time when there is none.
struct SlowMicrophone(Room);

impl Microphone for SlowMicrophone {
    fn rate(&self) -> u32 {
        RATE
    }

    fn read(&mut self, out: &mut Vec<f32>, wait_ms: u32) -> Result<(), String> {
        std::thread::sleep(Duration::from_millis(u64::from(wait_ms)));
        out.extend(samples(0, 16));
        Ok(())
    }
}

impl Drop for SlowMicrophone {
    fn drop(&mut self) {
        self.0.closed.fetch_add(1, Ordering::SeqCst);
    }
}

struct SlowSource(Room);

impl MicrophoneSource for SlowSource {
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
        self.0.opened.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(SlowMicrophone(self.0.clone())))
    }
}

/// The thread that reads the kept microphone must not keep a call that wants it waiting: with a
/// microphone whose reads take their time, a call got it only after hundreds of milliseconds.
#[test]
fn a_call_that_wants_the_kept_microphone_is_not_kept_waiting_by_the_thread_that_reads_it() {
    let room = Room::default();
    let keeper = MicKeeper::new(Box::new(SlowSource(room.clone())));
    keeper.set_keep_ms(10_000);
    let mut slowest = Duration::ZERO;
    let mut lease = keeper.acquire().expect("a microphone");
    for _ in 0..25 {
        lease.finish(true);
        std::thread::sleep(Duration::from_millis(37));
        let asked = Instant::now();
        lease = keeper.acquire().expect("the kept microphone");
        slowest = slowest.max(asked.elapsed());
        assert!(lease.warm);
    }
    assert!(slowest < Duration::from_millis(40), "a call waited {slowest:?} for the kept microphone");
    assert_eq!(room.opened(), 1);
}

#[test]
fn the_time_is_kept_to_a_minute() {
    let room = Room::default();
    let keeper = keeper(&room, 4_000_000_000);
    assert_eq!(keeper.keep_ms(), 60_000);
}

fn _lease_is_send(lease: Lease) -> impl Send {
    lease
}
