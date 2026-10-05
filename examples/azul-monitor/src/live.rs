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
    words
        .iter()
        .map(|w| w.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The machine's disk traffic `(read, written)` since the previous reading:
/// the disks' counters when any counted something, else the processes'.
#[must_use]
pub fn disk_traffic(disks: (u64, u64), processes: (u64, u64)) -> (u64, u64) {
    if disks.0 > 0 || disks.1 > 0 {
        disks
    } else {
        processes
    }
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
    match sent {
        Some(true) if force => Ok(format!("Killed {name} ({pid})")),
        Some(true) => Ok(format!("Ended {name} ({pid})")),
        Some(false) => Err(format!(
            "{name} ({pid}) could not be ended: it belongs to another user (administrator rights \
             are needed) or it has just quit."
        )),
        None => Err(format!("{name} ({pid}) cannot be ended on this system.")),
    }
}

/// Whether an interface is the loopback (its traffic never leaves the
/// machine, so it is not network traffic).
#[must_use]
pub fn is_loopback(interface: &str) -> bool {
    let name = interface.trim().to_lowercase();
    if name.contains("loopback") {
        return true;
    }
    // `lo` (Linux), `lo0` (macOS, the BSDs).
    name.strip_prefix("lo")
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
}

/// A sysinfo percentage as a percentage: NaN (a first reading) is none.
fn percent(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.max(0.0)
    }
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
        let mut system = System::new();
        // `new` loads nothing: the CPU list first (the refreshes keep it).
        system.refresh_cpu_list(CpuRefreshKind::everything());
        Self {
            system,
            disks: Disks::new_with_refreshed_list_specifics(
                DiskRefreshKind::nothing().with_io_usage(),
            ),
            networks: Networks::new_with_refreshed_list(),
            users: Users::new_with_refreshed_list(),
            readings: 0,
        }
    }

    /// The name of the user `uid` stands for ("" = unknown).
    fn user_name(&self, process: &sysinfo::Process) -> String {
        process
            .user_id()
            .and_then(|uid| self.users.get_user_by_id(uid))
            .map(|u| u.name().to_string())
            .unwrap_or_default()
    }
}

impl Default for LiveMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for LiveMachine {
    fn read(&mut self, elapsed_ms: u64) -> Snapshot {
        // One refresh of each per reading: every "since the previous
        // refresh" count is then "since the previous reading".
        self.system
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        if self.readings % 30 == 0 {
            // The clock and the user list change rarely.
            self.system.refresh_cpu_frequency();
            if self.readings > 0 {
                self.users.refresh();
            }
        }
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::everything());
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_user(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet)
                // Linux lists every thread as a task of its process: a
                // process table lists processes.
                .without_tasks(),
        );
        self.disks
            .refresh_specifics(true, DiskRefreshKind::nothing().with_io_usage());
        self.networks.refresh(true);
        self.readings += 1;

        let mut by_processes = (0_u64, 0_u64);
        let processes: Vec<ProcSample> = self
            .system
            .processes()
            .values()
            .map(|p| {
                let io = p.disk_usage();
                by_processes.0 = by_processes.0.saturating_add(io.read_bytes);
                by_processes.1 = by_processes.1.saturating_add(io.written_bytes);
                ProcSample {
                    pid: p.pid().as_u32(),
                    parent: p.parent().map(Pid::as_u32),
                    name: p.name().to_string_lossy().into_owned(),
                    user: self.user_name(p),
                    command: join_command(p.cmd()),
                    status: p.status().to_string(),
                    cpu: percent(p.cpu_usage()),
                    memory: p.memory(),
                    disk_read: io.read_bytes,
                    disk_written: io.written_bytes,
                }
            })
            .collect();
        let by_disks = self.disks.list().iter().fold((0_u64, 0_u64), |acc, d| {
            let io = d.usage();
            (
                acc.0.saturating_add(io.read_bytes),
                acc.1.saturating_add(io.written_bytes),
            )
        });
        let (disk_read, disk_written) = disk_traffic(by_disks, by_processes);
        let (net_received, net_sent) = self
            .networks
            .iter()
            .filter(|(name, _)| !is_loopback(name))
            .fold((0_u64, 0_u64), |acc, (_, data)| {
                (
                    acc.0.saturating_add(data.received()),
                    acc.1.saturating_add(data.transmitted()),
                )
            });
        // The monitor runs as the user of its own process.
        let me = std::process::id();
        let user = processes
            .iter()
            .find(|p| p.pid == me)
            .map(|p| p.user.clone())
            .unwrap_or_default();
        let cpus = self.system.cpus();
        Snapshot {
            elapsed_ms,
            cpu_brand: cpus
                .first()
                .map(|c| c.brand().trim().to_string())
                .unwrap_or_default(),
            cpu_mhz: cpus.first().map_or(0, sysinfo::Cpu::frequency),
            cpu: percent(self.system.global_cpu_usage()).min(100.0),
            cores: cpus
                .iter()
                .map(|c| percent(c.cpu_usage()).min(100.0))
                .collect(),
            memory_used: self.system.used_memory(),
            memory_total: self.system.total_memory(),
            swap_used: self.system.used_swap(),
            swap_total: self.system.total_swap(),
            disk_read,
            disk_written,
            net_received,
            net_sent,
            uptime: System::uptime(),
            user,
            processes,
            notices: Vec::new(),
        }
    }

    fn end(&mut self, pid: u32, force: bool) -> Result<String, String> {
        let target = Pid::from_u32(pid);
        // That one process again: a process that quit meanwhile is gone.
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[target]), true);
        let Some(process) = self.system.process(target) else {
            return Err(format!("No process {pid} is running."));
        };
        let name = process.name().to_string_lossy().into_owned();
        let signal = if force { Signal::Kill } else { Signal::Term };
        // A platform without SIGTERM (Windows) can only kill.
        let sent = process.kill_with(signal).or_else(|| Some(process.kill()));
        end_outcome(&name, pid, force, sent)
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
