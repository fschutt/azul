//! The live machine: this computer, read through the `sysinfo` crate (CPU,
//! memory, processes, disks, networks, users) - on Linux, macOS, Windows
//! and the BSDs alike.
//!
//! sysinfo counts a process' CPU, its disk traffic and every network
//! interface's traffic SINCE ITS PREVIOUS REFRESH, which is exactly the
//! model's "since the previous reading": one refresh per reading. The disk
//! traffic of the machine is the disks' own counters where the platform has
//! them, else the sum of the processes' (macOS and Windows count per
//! process, not per disk).
//!
//! Ending a process sends it SIGTERM (asks it to quit; Windows has no such
//! signal - it is then killed), "Kill" sends SIGKILL. A process of another
//! user is refused by the OS: the message says administrator rights are
//! needed.

use std::ffi::OsString;

use sysinfo::{
    CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks, Pid, ProcessRefreshKind,
    ProcessesToUpdate, Signal, System, UpdateKind, Users,
};

use crate::model::{ProcSample, Snapshot, Source};

/// A command line as one line: its words joined by spaces.
#[must_use]
pub fn join_command(words: &[OsString]) -> String {
    let _ = words;
    todo!("GREEN: join_command")
}

/// The machine's disk traffic `(read, written)` since the previous reading:
/// the disks' counters when any counted something, else the processes'.
#[must_use]
pub fn disk_traffic(disks: (u64, u64), processes: (u64, u64)) -> (u64, u64) {
    let _ = (disks, processes);
    todo!("GREEN: disk_traffic")
}

/// What ending `name` (`pid`) came to: `sent` is whether the signal went
/// out (`None`: the platform has no such signal).
///
/// # Errors
/// The signal was not delivered (another user's process, or gone).
pub fn end_outcome(
    name: &str,
    pid: u32,
    force: bool,
    sent: Option<bool>,
) -> Result<String, String> {
    let _ = (name, pid, force, sent);
    todo!("GREEN: end_outcome")
}

/// Whether an interface is the loopback (its traffic never leaves the
/// machine, so it is not network traffic).
#[must_use]
pub fn is_loopback(interface: &str) -> bool {
    let _ = interface;
    todo!("GREEN: is_loopback")
}

/// This computer.
pub struct LiveMachine {
    system: System,
    disks: Disks,
    networks: Networks,
    users: Users,
    /// Readings so far (the user list is re-read now and then).
    readings: u64,
}

impl LiveMachine {
    /// This computer, before its first reading.
    #[must_use]
    pub fn new() -> Self {
        todo!("GREEN: LiveMachine::new")
    }
}

impl Default for LiveMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for LiveMachine {
    fn read(&mut self, elapsed_ms: u64) -> Snapshot {
        let _ = elapsed_ms;
        todo!("GREEN: LiveMachine::read")
    }

    fn end(&mut self, pid: u32, force: bool) -> Result<String, String> {
        let _ = (pid, force);
        todo!("GREEN: LiveMachine::end")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_line_reads_as_its_words_joined_by_spaces() {
        let words = vec![
            OsString::from("cargo"),
            OsString::from("build"),
            OsString::from("--release"),
        ];
        assert_eq!(join_command(&words), "cargo build --release");
        assert_eq!(join_command(&[]), "");
    }

    #[test]
    fn the_disks_counters_win_when_they_counted_anything() {
        assert_eq!(disk_traffic((10, 20), (1, 2)), (10, 20));
        assert_eq!(disk_traffic((0, 5), (1, 2)), (0, 5));
        // A platform without per-disk counters: the processes' sum.
        assert_eq!(disk_traffic((0, 0), (1, 2)), (1, 2));
    }

    #[test]
    fn ending_says_what_was_done_or_why_not() {
        assert_eq!(
            end_outcome("cargo", 5102, false, Some(true)),
            Ok("Ended cargo (5102)".to_string())
        );
        assert_eq!(
            end_outcome("cargo", 5102, true, Some(true)),
            Ok("Killed cargo (5102)".to_string())
        );
        let refused = end_outcome("sshd", 702, false, Some(false));
        assert!(refused
            .as_ref()
            .is_err_and(|why| why.contains("sshd (702)") && why.contains("administrator")));
        assert!(end_outcome("x", 1, false, None).is_err());
    }

    #[test]
    fn the_loopback_is_not_network_traffic() {
        assert!(is_loopback("lo"));
        assert!(is_loopback("lo0"));
        assert!(is_loopback("Loopback Pseudo-Interface 1"));
        assert!(!is_loopback("en0"));
        assert!(!is_loopback("wlan0"));
        assert!(!is_loopback("eth0"));
    }

    #[test]
    fn the_live_machine_sees_this_process_its_cores_and_its_memory() {
        let mut m = LiveMachine::new();
        let s = m.read(0);
        assert!(!s.cores.is_empty());
        assert!(s.memory_total > 0);
        assert!(s.memory_used <= s.memory_total);
        let me = std::process::id();
        assert!(
            s.processes.iter().any(|p| p.pid == me),
            "this test's own process is not listed"
        );
        let second = m.read(250);
        assert_eq!(second.elapsed_ms, 250);
        assert!((0.0..=100.0).contains(&second.cpu));
    }

    #[test]
    fn ending_a_process_that_is_gone_says_so() {
        let mut m = LiveMachine::new();
        m.read(0);
        // PIDs are far below this on every OS.
        let gone = m.end(u32::MAX - 7, false);
        assert!(gone.is_err());
    }
}
