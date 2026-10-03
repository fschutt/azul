//! The sampler: an azul `Thread` that reads the system (a [`Source`]: the
//! live machine or the sample machine) every `interval_ms` and hands each
//! reading to the window as a write-back. No callback ever waits on the OS.
//!
//! The window talks back through [`Shared`]: the interval (the settings'
//! update speed; 0 = paused), the processes to end (run at once on the
//! thread, which owns the source - the next reading follows at once, so an
//! ended process leaves the table without waiting a whole interval; what
//! was done rides on that reading's `notices`), and a stop flag. The thread
//! also ends when the window's side is gone (a write-back that cannot be
//! delivered) or azul asks it to terminate.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use azul::{
    callbacks::WriteBackCallbackType,
    prelude::*,
    task::{ThreadReceiveMsg, ThreadReceiver, ThreadSendMsg, ThreadSender, ThreadWriteBackMsg},
};

use crate::{
    live::LiveMachine,
    model::{Snapshot, Source},
    sample::SampleMachine,
};

/// The update speed a fresh window uses: one reading a second.
pub const DEFAULT_INTERVAL_MS: u64 = 1000;

/// The least time between two readings, also when one is asked for at once
/// (CPU usage is a difference of two counts: sysinfo needs 200 ms).
pub const MIN_GAP_MS: u64 = 250;

/// How often the thread looks at its commands and the clock.
const POLL_MS: u64 = 50;

/// What the window asks of the sampler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// End process `pid` (`force`: kill it).
    End { pid: u32, force: bool },
    /// Read now (the user pressed Refresh, or the update speed changed).
    ReadNow,
}

/// What the window and the sampler share.
#[derive(Debug)]
pub struct Shared {
    /// Milliseconds between readings; 0 = paused.
    pub interval_ms: AtomicU64,
    /// The window's requests, oldest first.
    pub commands: Mutex<Vec<Command>>,
    /// The window asked the sampler to stop.
    pub stop: AtomicBool,
}

impl Shared {
    /// Readings every `interval_ms` (0 = paused), nothing asked yet.
    #[must_use]
    pub fn new(interval_ms: u64) -> Self {
        Self {
            interval_ms: AtomicU64::new(interval_ms),
            commands: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
        }
    }

    /// Queues `command` for the sampler.
    pub fn ask(&self, command: Command) {
        if let Ok(mut queue) = self.commands.lock() {
            queue.push(command);
        }
    }

    /// The queued commands, oldest first (the queue is empty after).
    #[must_use]
    pub fn take_commands(&self) -> Vec<Command> {
        self.commands
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

/// Runs `commands` against `source`: what each came to (one sentence each)
/// and whether a reading is wanted at once.
pub fn run_commands(source: &mut dyn Source, commands: &[Command]) -> (Vec<String>, bool) {
    let _ = (source, commands);
    todo!("GREEN: run_commands")
}

/// Whether to read now: `since_ms` after the previous reading, readings
/// every `interval_ms` (0 = paused), `urgent` when one is wanted at once.
/// Never sooner than [`MIN_GAP_MS`].
#[must_use]
pub fn due(since_ms: u64, interval_ms: u64, urgent: bool) -> bool {
    let _ = (since_ms, interval_ms, urgent);
    todo!("GREEN: due")
}

/// One reading on its way to the window (the write-back's payload).
pub struct Reading {
    /// Taken by the window's write-back.
    pub snapshot: Option<Snapshot>,
}

/// What the sampler thread is handed.
pub struct SamplerInit {
    /// The sample machine instead of this computer.
    pub sample: bool,
    pub shared: Arc<Shared>,
    /// The window's write-back for each reading.
    pub on_reading: WriteBackCallbackType,
}

/// The sampler thread: a source, then readings until it is stopped.
pub extern "C" fn sampler_thread(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut receiver: ThreadReceiver,
) {
    let Some((sample, shared, on_reading)) = init
        .downcast_ref::<SamplerInit>()
        .map(|i| (i.sample, i.shared.clone(), i.on_reading))
    else {
        return;
    };
    // The source is made HERE, on the thread, and never leaves it.
    let mut source: Box<dyn Source> = if sample {
        Box::new(SampleMachine::new())
    } else {
        Box::new(LiveMachine::new())
    };
    let mut previous: Option<Instant> = None;
    let mut pending: Vec<String> = Vec::new();
    let mut urgent = false;
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            break;
        }
        let mut terminate = false;
        while let Some(message) = receiver.recv().into_option() {
            if matches!(message, ThreadSendMsg::TerminateThread) {
                terminate = true;
            }
        }
        if terminate {
            break;
        }
        let (notices, wanted) = run_commands(source.as_mut(), &shared.take_commands());
        pending.extend(notices);
        urgent |= wanted;
        let since_ms = previous.map_or(u64::MAX, |at| {
            u64::try_from(at.elapsed().as_millis()).unwrap_or(u64::MAX)
        });
        if previous.is_none() || due(since_ms, shared.interval_ms.load(Ordering::Relaxed), urgent) {
            let elapsed_ms = if previous.is_some() { since_ms } else { 0 };
            let mut snapshot = source.read(elapsed_ms);
            previous = Some(Instant::now());
            urgent = false;
            snapshot.notices.append(&mut pending);
            let delivered = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
                on_reading,
                RefAny::new(Reading {
                    snapshot: Some(snapshot),
                }),
            )));
            if !delivered {
                break; // the window is gone
            }
        }
        std::thread::sleep(Duration::from_millis(POLL_MS));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_is_due_after_the_interval() {
        assert!(!due(999, 1000, false));
        assert!(due(1000, 1000, false));
        assert!(due(2500, 2000, false));
    }

    #[test]
    fn a_paused_sampler_reads_only_when_asked() {
        assert!(!due(60_000, 0, false));
        assert!(due(60_000, 0, true));
    }

    #[test]
    fn an_urgent_reading_still_waits_for_the_least_gap() {
        assert!(!due(MIN_GAP_MS - 1, 1000, true));
        assert!(due(MIN_GAP_MS, 1000, true));
        // A very short interval is held to the least gap too.
        assert!(!due(100, 50, false));
    }

    #[test]
    fn ending_a_process_says_what_it_came_to_and_wants_a_reading() {
        let mut m = SampleMachine::new();
        let pipewire = m
            .read(0)
            .processes
            .iter()
            .find(|p| p.name == "pipewire")
            .map(|p| p.pid)
            .unwrap();
        let (notices, wanted) = run_commands(
            &mut m,
            &[
                Command::End {
                    pid: pipewire,
                    force: false,
                },
                Command::End {
                    pid: 702,
                    force: true,
                },
            ],
        );
        assert!(wanted);
        assert_eq!(notices.len(), 2);
        assert_eq!(notices[0], format!("Ended pipewire ({pipewire})"));
        assert!(notices[1].contains("administrator"), "{}", notices[1]);
        assert!(m.read(1000).processes.iter().all(|p| p.pid != pipewire));
    }

    #[test]
    fn read_now_wants_a_reading_and_says_nothing() {
        let mut m = SampleMachine::new();
        let (notices, wanted) = run_commands(&mut m, &[Command::ReadNow]);
        assert!(wanted);
        assert!(notices.is_empty());
        let (none, idle) = run_commands(&mut m, &[]);
        assert!(none.is_empty());
        assert!(!idle);
    }

    #[test]
    fn the_shared_queue_hands_each_command_over_once() {
        let shared = Shared::new(DEFAULT_INTERVAL_MS);
        shared.ask(Command::ReadNow);
        shared.ask(Command::End {
            pid: 7,
            force: false,
        });
        assert_eq!(
            shared.take_commands(),
            vec![
                Command::ReadNow,
                Command::End {
                    pid: 7,
                    force: false
                }
            ]
        );
        assert!(shared.take_commands().is_empty());
        assert_eq!(shared.interval_ms.load(Ordering::Relaxed), 1000);
    }
}
